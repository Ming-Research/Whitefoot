#define _GNU_SOURCE
#include "fork_capsule.h"

#if defined(__linux__) && defined(__x86_64__) && defined(__LP64__) && defined(__GLIBC__)
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <poll.h>
#include <signal.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/prctl.h>
#include <sys/wait.h>
#include <unistd.h>

#define WF_CAPSULE_STATUS_MAGIC UINT32_C(0x57464331)
/* Four fixed words, no padding, comfortably below PIPE_BUF. */
typedef struct wf_capsule_status {
    uint32_t magic;
    uint32_t setup_error;
    uint32_t encoder_error;
    uint32_t reserved;
} wf_capsule_status;

void wf_fork_capsule_dispose(wf_fork_capsule *capsule) {
    if (capsule == NULL) return;
    if (capsule->armed) {
        (void)wf_fork_capsule_raw(SYS_rt_sigprocmask, SIG_SETMASK,
            (long)&capsule->parent_signal_mask, 0, sizeof(uint64_t), 0, 0);
        capsule->armed = 0;
    }
    if (capsule->scratch != NULL) {
        (void)munmap(capsule->scratch, capsule->scratch_size);
        capsule->scratch = NULL;
    }
    if (capsule->result_read >= 0) (void)close(capsule->result_read);
    if (capsule->result_write >= 0) (void)close(capsule->result_write);
    capsule->result_read = -1;
    capsule->result_write = -1;
    capsule->prepared = 0;
}

int wf_fork_capsule_prepare(
    wf_fork_capsule *capsule, const int *outputs, size_t output_count,
    size_t scratch_size
) {
    int channel[2];
    int probe;
    int error;
    if (capsule == NULL) return EINVAL;
    memset(capsule, 0, sizeof(*capsule));
    capsule->result_read = -1;
    capsule->result_write = -1;
    if (output_count > WF_FORK_CAPSULE_MAX_OUTPUTS
        || (output_count != 0 && outputs == NULL) || scratch_size == 0) {
        return EINVAL;
    }
    struct sigaction child_action;
    if (sigaction(SIGCHLD, NULL, &child_action) != 0) return errno;
    if (child_action.sa_handler == SIG_IGN
        || (child_action.sa_flags & SA_NOCLDWAIT) != 0) return ENOTSUP;
    capsule->parent = getpid();
    /* Refuse unsupported kernels before creating any child. If the later
     * pidfd_open fails, the created child still has exactly one reaper. */
    probe = (int)syscall(SYS_pidfd_open, capsule->parent, 0u);
    if (probe < 0) return errno;
    (void)close(probe);
    for (size_t i = 0; i < output_count; ++i) {
        if (outputs[i] < 0 || fcntl(outputs[i], F_GETFD) < 0) return EBADF;
        capsule->outputs[i] = outputs[i];
        capsule->keep[i] = outputs[i];
    }
    capsule->output_count = output_count;
    capsule->scratch_size = scratch_size;
    capsule->scratch = mmap(NULL, scratch_size, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (capsule->scratch == MAP_FAILED) {
        capsule->scratch = NULL;
        return errno;
    }
    if (pipe2(channel, O_CLOEXEC | O_NONBLOCK) != 0) {
        error = errno;
        wf_fork_capsule_dispose(capsule);
        return error;
    }
    capsule->result_read = channel[0];
    capsule->result_write = channel[1];
    capsule->keep[output_count] = channel[1];
    /* Sort and deduplicate outside capture; outputs retain encoder order. */
    for (size_t i = 1; i <= output_count; ++i) {
        int descriptor = capsule->keep[i];
        size_t j = i;
        while (j != 0 && capsule->keep[j - 1] > descriptor) {
            capsule->keep[j] = capsule->keep[j - 1];
            --j;
        }
        capsule->keep[j] = descriptor;
    }
    for (size_t i = 0; i <= output_count; ++i) {
        if (capsule->keep_count == 0
            || capsule->keep[i] != capsule->keep[capsule->keep_count - 1]) {
            capsule->keep[capsule->keep_count++] = capsule->keep[i];
        }
    }
    capsule->prepared = 1;
    return 0;
}

static int wf_capsule_keeps(const wf_fork_capsule *capsule, int fd) {
    for (size_t i = 0; i < capsule->keep_count; ++i) {
        if (capsule->keep[i] == fd) return 1;
    }
    return 0;
}

/* close_range predates neither pidfd nor every supported host kernel. Its
 * fallback loops over actual /proc descriptors with raw getdents64/close:
 * RLIMIT_NOFILE alone misses existing fds above a subsequently lowered limit.
 * No allocator, dirent library or parent-prepared fd ceiling is used. */
static int wf_capsule_close_loop(const wf_fork_capsule *capsule) {
    struct wf_linux_dirent64 {
        uint64_t inode;
        int64_t offset;
        unsigned short length;
        unsigned char type;
        char name[];
    };
    _Alignas(uint64_t) unsigned char buffer[1024];
    long directory = wf_fork_capsule_raw(SYS_openat, AT_FDCWD,
        (long)"/proc/self/fd", O_RDONLY | O_DIRECTORY | O_CLOEXEC, 0, 0, 0);
    if (directory < 0) return (int)-directory;
    int error = 0;
    for (;;) {
        long bytes = wf_fork_capsule_raw(SYS_getdents64, directory,
            (long)buffer, sizeof(buffer), 0, 0, 0);
        if (bytes == -EINTR) continue;
        if (bytes <= 0) {
            if (bytes < 0) error = (int)-bytes;
            break;
        }
        for (size_t at = 0; at < (size_t)bytes;) {
            const struct wf_linux_dirent64 *entry =
                (const struct wf_linux_dirent64 *)(buffer + at);
            size_t prefix = offsetof(struct wf_linux_dirent64, name);
            if ((size_t)bytes - at <= prefix || entry->length <= prefix
                || entry->length > (size_t)bytes - at) {
                error = EIO;
                break;
            }
            unsigned fd = 0;
            size_t n = 0;
            while (prefix + n < entry->length
                   && entry->name[n] >= '0' && entry->name[n] <= '9') {
                unsigned digit = (unsigned)(entry->name[n] - '0');
                if (fd > ((unsigned)INT_MAX - digit) / 10u) break;
                fd = fd * 10u + digit;
                ++n;
            }
            if (n != 0 && prefix + n < entry->length && entry->name[n] == '\0'
                && (int)fd != directory && !wf_capsule_keeps(capsule, (int)fd)) {
                long closed = wf_fork_capsule_raw(SYS_close, fd, 0, 0, 0, 0, 0);
                /* Linux releases the fd even when close reports EINTR. */
                if (closed < 0 && closed != -EINTR && closed != -EBADF) {
                    error = (int)-closed;
                    break;
                }
            }
            at += entry->length;
        }
        if (error != 0) break;
    }
    (void)wf_fork_capsule_raw(SYS_close, directory, 0, 0, 0, 0, 0);
    return error;
}

static int wf_capsule_close_gaps(const wf_fork_capsule *capsule) {
    unsigned first = 0;
    for (size_t i = 0; i <= capsule->keep_count; ++i) {
        unsigned last = i == capsule->keep_count
            ? UINT_MAX : (unsigned)capsule->keep[i] - 1u;
        if (i == capsule->keep_count || first < (unsigned)capsule->keep[i]) {
#if defined(WF_FORK_CAPSULE_TEST_NO_CLOSE_RANGE)
            (void)last;
            long closed = -ENOSYS;
#else
            long closed = wf_fork_capsule_raw(SYS_close_range,
                first, last, 0, 0, 0, 0);
#endif
            if (closed == -ENOSYS) return wf_capsule_close_loop(capsule);
            if (closed < 0) return (int)-closed;
        }
        if (i < capsule->keep_count) first = (unsigned)capsule->keep[i] + 1u;
    }
    return 0;
}

static int wf_capsule_reset_signals(void) {
    /* Kernel rt_sigaction layout and sigset size on Linux x86-64; deliberately
     * not glibc's struct sigaction or sigset_t (which are different layouts). */
    const struct {
        unsigned long handler, flags, restorer;
        uint64_t mask;
    } action = {0, 0, 0, 0};
    const uint64_t all = UINT64_MAX;
    long result = wf_fork_capsule_raw(SYS_rt_sigprocmask, SIG_SETMASK,
        (long)&all, 0, sizeof(all), 0, 0);
    if (result < 0) return (int)-result;
    for (int signal = 1; signal <= 64; ++signal) {
        if (signal == SIGKILL || signal == SIGSTOP) continue;
        result = wf_fork_capsule_raw(SYS_rt_sigaction, signal,
            (long)&action, 0, sizeof(all), 0, 0);
        if (result < 0) return (int)-result;
    }
    return 0;
}

static _Noreturn void wf_capsule_child(
    const wf_fork_capsule *capsule, const void *captured,
    wf_fork_capsule_encoder encoder
) {
    wf_capsule_status status;
    status.magic = WF_CAPSULE_STATUS_MAGIC;
    status.reserved = 0;
    status.encoder_error = 0;
    status.setup_error = (uint32_t)wf_capsule_reset_signals();
    if (status.setup_error == 0) {
        long result = wf_fork_capsule_raw(SYS_prctl, PR_SET_PDEATHSIG,
            SIGKILL, 0, 0, 0, 0);
        if (result < 0) status.setup_error = (uint32_t)-result;
        else if (wf_fork_capsule_raw(SYS_getppid, 0, 0, 0, 0, 0, 0)
                 != capsule->parent) status.setup_error = ECHILD;
    }
    if (status.setup_error == 0) {
        status.setup_error = (uint32_t)wf_capsule_close_gaps(capsule);
    }
    if (status.setup_error == 0) {
        int error = encoder(captured, capsule->scratch, capsule->scratch_size,
                            capsule->outputs, capsule->output_count);
        status.encoder_error = error >= 0 ? (uint32_t)error : EIO;
    }
    long written;
    do {
        written = wf_fork_capsule_raw(SYS_write, capsule->result_write,
            (long)&status, sizeof(status), 0, 0, 0);
    } while (written == -EINTR);
    int failed = status.setup_error != 0 || status.encoder_error != 0
        || written != (long)sizeof(status);
    /* Raw _exit equivalent: no inherited atexit handlers, stdio or TLS. */
    (void)wf_fork_capsule_raw(SYS_exit_group, failed ? 1 : 0, 0, 0, 0, 0, 0);
    __builtin_unreachable();
}

int wf_fork_capsule_arm(wf_fork_capsule *capsule) {
    const uint64_t all = UINT64_MAX;
    if (capsule == NULL || !capsule->prepared || capsule->armed) {
        return EINVAL;
    }
    /* Block before fork as well: blocking only in the child leaves a window
     * in which an inherited handler can reenter vanished runtime threads. */
    long masked = wf_fork_capsule_raw(SYS_rt_sigprocmask, SIG_SETMASK,
        (long)&all, (long)&capsule->parent_signal_mask, sizeof(all), 0, 0);
    if (masked < 0) return (int)-masked;
    capsule->armed = 1;
    return 0;
}

pid_t wf_fork_capsule_capture_held(
    wf_fork_capsule *capsule, const void *captured,
    wf_fork_capsule_encoder encoder
) {
    if (capsule == NULL || !capsule->prepared || !capsule->armed
        || capsule->capture_pid != 0 || encoder == NULL) return -EINVAL;
    pid_t pid = fork(); /* glibc malloc atfork ordering; never raw SYS_fork. */
    if (pid == 0) wf_capsule_child(capsule, captured, encoder);
    capsule->capture_pid = pid < 0 ? (pid_t)-errno : pid;
    return capsule->capture_pid;
}

int wf_fork_capsule_parent_start(wf_fork_capsule *capsule, wf_fork_capsule_job *job) {
    if (capsule == NULL || !capsule->armed || capsule->capture_pid == 0
        || job == NULL) return EINVAL;
    pid_t pid = capsule->capture_pid;
    (void)wf_fork_capsule_raw(SYS_rt_sigprocmask, SIG_SETMASK,
        (long)&capsule->parent_signal_mask, 0, sizeof(uint64_t), 0, 0);
    capsule->armed = 0;
    capsule->capture_pid = 0;
    if (pid < 0) return (int)-pid;
    job->pid = pid;
    job->result_read = capsule->result_read;
    capsule->result_read = -1;
    (void)close(capsule->result_write);
    capsule->result_write = -1;
    capsule->prepared = 0;
    job->pidfd = (int)syscall(SYS_pidfd_open, pid, 0u);
    job->pidfd_error = job->pidfd < 0 ? errno : 0;
    return 0;
}

int wf_fork_capsule_start(
    wf_fork_capsule *capsule, const void *captured,
    wf_fork_capsule_encoder encoder, wf_fork_capsule_job *job
) {
    if (encoder == NULL || job == NULL) return EINVAL;
    int error = wf_fork_capsule_arm(capsule);
    if (error != 0) return error;
    pid_t pid = wf_fork_capsule_capture_held(capsule, captured, encoder);
    if (pid == -EINVAL && capsule->capture_pid == 0) {
        /* Invalid sequencing, no fork attempted: retain preparation but undo
         * the mask, just as a caller cancelling an armed capture would. */
        (void)wf_fork_capsule_raw(SYS_rt_sigprocmask, SIG_SETMASK,
            (long)&capsule->parent_signal_mask, 0, sizeof(uint64_t), 0, 0);
        capsule->armed = 0;
        return EINVAL;
    }
    return wf_fork_capsule_parent_start(capsule, job);
}

int wf_fork_capsule_finish(
    wf_fork_capsule_job *job, wf_fork_capsule_result *result
) {
    siginfo_t info;
    int wait_status;
    int waited;
    wf_capsule_status status;
    if (job == NULL || job->pid <= 0 || result == NULL) return EINVAL;
    memset(result, 0, sizeof(*result));
    result->exit_code = -1;
    if (job->pidfd >= 0) {
        struct pollfd readiness = {job->pidfd, POLLIN, 0};
        do { waited = poll(&readiness, 1, -1); } while (waited < 0 && errno == EINTR);
        if (waited < 0) return errno;
        if ((readiness.revents & POLLIN) == 0) return EIO;
        memset(&info, 0, sizeof(info));
        do { waited = waitid(P_PIDFD, (id_t)job->pidfd, &info, WEXITED); }
        while (waited < 0 && errno == EINTR);
        if (waited == 0) {
            if (info.si_code == CLD_EXITED) result->exit_code = info.si_status;
            else result->signal_number = info.si_status;
        } else if (errno != EINVAL && errno != ENOSYS) return errno;
    } else waited = -1;
    if (waited < 0) {
        do { waited = waitpid(job->pid, &wait_status, 0); }
        while (waited < 0 && errno == EINTR);
        if (waited < 0) return errno;
        if (WIFEXITED(wait_status)) result->exit_code = WEXITSTATUS(wait_status);
        else if (WIFSIGNALED(wait_status)) result->signal_number = WTERMSIG(wait_status);
    }
    /* Nonblocking result pipe: even a malformed encoder retaining another
     * writer cannot strand the parent after it has confirmed exit. */
    ssize_t bytes;
    do { bytes = read(job->result_read, &status, sizeof(status)); }
    while (bytes < 0 && errno == EINTR);
    if (bytes == (ssize_t)sizeof(status) && status.magic == WF_CAPSULE_STATUS_MAGIC
        && status.reserved == 0 && status.setup_error <= INT_MAX
        && status.encoder_error <= INT_MAX) {
        unsigned char extra;
        ssize_t tail;
        do { tail = read(job->result_read, &extra, 1); }
        while (tail < 0 && errno == EINTR);
        if (tail == 0) {
            result->status_valid = 1;
            result->setup_error = (int)status.setup_error;
            result->encoder_error = (int)status.encoder_error;
        }
    }
    (void)close(job->result_read);
    if (job->pidfd >= 0) (void)close(job->pidfd);
    job->pid = 0;
    job->pidfd = -1;
    job->result_read = -1;
    return 0;
}
#else
/* No capsule implementation before this target's native closure is qualified. */
typedef int wf_fork_capsule_unqualified_target;
#endif
