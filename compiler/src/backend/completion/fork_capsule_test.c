#define _GNU_SOURCE
#include "fork_capsule.h"
#include "linux_io_uring.h"

#if defined(__linux__)
/* Like the scheduler smoke/deque probes, inspect the actual native core's
 * private lanes, without exporting a test-only runtime interface. Also link
 * it on unqualified Linux targets, whose main below returns unavailable. */
#include "../sched/core.c"
void wf_completion_record_complete(wf_completion_record *record) {
    wf_completion_record_publish(record);
}
#endif

#if defined(__linux__) && defined(__x86_64__) && defined(__LP64__) && defined(__GLIBC__)
#include <errno.h>
#include <fcntl.h>
#include <gnu/libc-version.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <sys/utsname.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../runtime_test_guard.h"

#define CHECK(condition) do {                                                 \
    if (!(condition)) {                                                       \
        fprintf(stderr, "fork capsule: %s:%d: %s\n",                          \
                __FILE__, __LINE__, #condition);                              \
        exit(1);                                                              \
    }                                                                         \
} while (0)

typedef struct witness_data {
    const unsigned char *bytes;
    size_t size;
    int extra_fd;
    int ring_fd;
    int wait_fd;
    int wake_fd;
    const volatile unsigned char *forbidden;
} witness_data;

/* These helpers and encoders form the entire test child closure. No libc
 * calls, globals, inherited locks, TLS or allocation are reachable here. */
static int child_write(int fd, const void *bytes, size_t count) {
    const unsigned char *at = bytes;
    while (count != 0) {
        long sent = wf_fork_capsule_raw(SYS_write, fd, (long)at, count, 0, 0, 0);
        if (sent == -EINTR) continue;
        if (sent < 0) return (int)-sent;
        if (sent == 0) return EIO;
        at += sent;
        count -= (size_t)sent;
    }
    return 0;
}

static int fixed_encoder(
    const void *captured, void *scratch, size_t scratch_size,
    const int *outputs, size_t output_count
) {
    const witness_data *data = captured;
    unsigned char *encoded = scratch;
    const unsigned char marker = 0x7e;
    if (output_count != 1 || scratch_size < data->size + 1u) return EINVAL;
    /* The output pipe is full at fork. Parent mutation/free must precede
     * draining it, so this first write gates every captured-data read below. */
    int error = child_write(outputs[0], &marker, 1);
    if (error != 0) return error;
    const int closed[] = {data->extra_fd, data->ring_fd, data->wait_fd,
                          data->wake_fd, 0, 1, 2};
    for (size_t i = 0; i < sizeof(closed) / sizeof(closed[0]); ++i) {
        if (wf_fork_capsule_raw(SYS_fcntl, closed[i], F_GETFD, 0, 0, 0, 0)
            != -EBADF) return EBADF;
    }
    uint64_t mask = 0;
    long queried = wf_fork_capsule_raw(SYS_rt_sigprocmask, SIG_SETMASK, 0,
                                      (long)&mask, sizeof(mask), 0, 0);
    uint64_t unblockable = (UINT64_C(1) << (SIGKILL - 1))
        | (UINT64_C(1) << (SIGSTOP - 1));
    if (queried != 0 || mask != (UINT64_MAX & ~unblockable)) return EPROTO;
    encoded[0] = 1; /* Unrelated descriptors were all closed. */
    for (size_t i = 0; i < data->size; ++i) encoded[i + 1] = data->bytes[i];
    return child_write(outputs[0], encoded, data->size + 1u);
}

static int forbidden_encoder(
    const void *captured, void *scratch, size_t scratch_size,
    const int *outputs, size_t output_count
) {
    const witness_data *data = captured;
    (void)scratch;
    (void)scratch_size;
    if (output_count != 1) return EINVAL;
    unsigned char byte = *data->forbidden;
    return child_write(outputs[0], &byte, 1);
}

static int error_encoder(
    const void *captured, void *scratch, size_t scratch_size,
    const int *outputs, size_t output_count
) {
    (void)captured;
    (void)scratch;
    (void)scratch_size;
    if (output_count != 1) return EINVAL;
    const unsigned char byte = 42;
    return child_write(outputs[0], &byte, 1);
}

/* An inherited SIGSEGV handler would turn the fault into exit 88. The control
 * must observe SIGSEGV, thereby also checking the raw disposition reset. */
static void inherited_fault_handler(int signal) {
    (void)signal;
    _exit(88);
}

static void parent_read(int fd, void *bytes, size_t count) {
    unsigned char *at = bytes;
    while (count != 0) {
        ssize_t got = read(fd, at, count);
        if (got < 0 && errno == EINTR) continue;
        CHECK(got > 0);
        at += got;
        count -= (size_t)got;
    }
}

static void check_reaped(const wf_fork_capsule_job *job, pid_t pid) {
    CHECK(job->pid == 0 && job->pidfd == -1 && job->result_read == -1);
    CHECK(waitpid(pid, NULL, WNOHANG) == -1 && errno == ECHILD);
}

static void ring_round_trip(wf_linux_io_uring_adapter *adapter) {
    const unsigned char seed[] = {3, 1, 4, 1, 5, 9};
    unsigned char bytes[sizeof(seed)] = {0};
    int file = (int)syscall(SYS_memfd_create, "fork-parent-ring", 0u);
    CHECK(file >= 0);
    CHECK(pwrite(file, seed, sizeof(seed), 0) == (ssize_t)sizeof(seed));
    wf_completion_record record;
    memset(&record, 0, sizeof(record));
    wf_completion_record_init(&record);
    record.request.kind = WF_FILE_PREAD;
    record.request.operation.pread.descriptor = file;
    record.request.operation.pread.buffer = bytes;
    record.request.operation.pread.count = sizeof(bytes);
    record.request.operation.pread.offset = 0;
    record.opened_descriptor = -1;
    CHECK(wf_linux_io_uring_submit(adapter, &record) == WF_LINUX_IO_URING_TARGET_OWNS);
    while (atomic_load_explicit(&record.state, memory_order_acquire)
           != WF_COMPLETION_DONE) {
        size_t published;
        CHECK(wf_linux_io_uring_progress(adapter, 1, 1, &published) == 0);
    }
    CHECK(record.result.error_code == 0);
    CHECK(record.result.value == (int64_t)sizeof(seed));
    CHECK(memcmp(bytes, seed, sizeof(seed)) == 0);
    CHECK(close(file) == 0);
}

static _Atomic unsigned lane_locked;
static _Atomic unsigned release_lane;
static void *hold_compute_wait(void *unused) {
    (void)unused;
    wf_prim_wait_lock(&wf__par_lanes[1].wait);
    atomic_store_explicit(&lane_locked, 1, memory_order_release);
    while (!atomic_load_explicit(&release_lane, memory_order_acquire)) {
        const struct timespec pause = {0, 1000000};
        (void)nanosleep(&pause, NULL);
    }
    wf_prim_wait_unlock(&wf__par_lanes[1].wait);
    return NULL;
}

static void compute_increment(void *frame) { ++*(unsigned *)frame; }

static void snapshot_case(wf_linux_io_uring_adapter *adapter) {
    /* Serial oracle, independent of the child's copy loop. Include N and two
     * related records in the captured bytes; no WF map/library semantics are
     * claimed by this native-only witness. */
    const unsigned char expected[] = {7, 0, 0, 0, 'a', 11, 'b', 22};
    unsigned char *bytes = malloc(sizeof(expected));
    witness_data *data = malloc(sizeof(*data));
    CHECK(bytes != NULL && data != NULL);
    memcpy(bytes, expected, sizeof(expected));
    int output[2];
    CHECK(pipe2(output, O_CLOEXEC) == 0);
    int extra = fcntl(adapter->ring_descriptor, F_DUPFD_CLOEXEC, 4096);
    CHECK(extra >= 4096);
    *data = (witness_data){bytes, sizeof(expected), extra,
        adapter->ring_descriptor, adapter->wait_descriptor,
        adapter->wake_descriptor, NULL};
    int capacity = fcntl(output[1], F_GETPIPE_SZ);
    CHECK(capacity > 0);
    unsigned char padding[4096] = {0};
    for (size_t left = (size_t)capacity; left != 0;) {
        size_t count = left < sizeof(padding) ? left : sizeof(padding);
        CHECK(write(output[1], padding, count) == (ssize_t)count);
        left -= count;
    }
    wf_fork_capsule capsule;
    wf_fork_capsule_job job;
    wf_fork_capsule_result result;
    /* Duplicate allowlist entries must be deduplicated without closing the
     * one output; encoder still receives the designated list's order. The
     * normal encoder uses a single descriptor, so test dedup via prepare then
     * dispose before preparing the actual capture. */
    const int duplicate[] = {output[1], output[1]};
    CHECK(wf_fork_capsule_prepare(&capsule, duplicate, 2, 4096) == 0);
    CHECK(capsule.keep_count == 2); /* one unique output plus result writer */
    wf_fork_capsule_dispose(&capsule);
    CHECK(fcntl(output[1], F_GETFD) >= 0);
    CHECK(wf_fork_capsule_prepare(&capsule, &output[1], 1, 4096) == 0);
    /* Preserve an existing high fd above a lowered limit: the fallback must
     * enumerate actual fds rather than stop at the current RLIMIT_NOFILE. */
    struct rlimit original, lowered;
    CHECK(getrlimit(RLIMIT_NOFILE, &original) == 0);
    lowered = original;
    lowered.rlim_cur = 4096;
    CHECK(setrlimit(RLIMIT_NOFILE, &lowered) == 0);
    uint64_t mask_before = 0, mask_after = 0;
    CHECK(wf_fork_capsule_raw(SYS_rt_sigprocmask, SIG_SETMASK, 0,
          (long)&mask_before, sizeof(mask_before), 0, 0) == 0);
    pthread_mutex_t capture_hold = PTHREAD_MUTEX_INITIALIZER;
    CHECK(wf_fork_capsule_arm(&capsule) == 0);
    CHECK(pthread_mutex_lock(&capture_hold) == 0);
    pid_t captured_pid = wf_fork_capsule_capture_held(&capsule, data, fixed_encoder);
    CHECK(pthread_mutex_unlock(&capture_hold) == 0);
    CHECK(captured_pid > 0);
    CHECK(wf_fork_capsule_parent_start(&capsule, &job) == 0);
    CHECK(job.pid == captured_pid);
    CHECK(pthread_mutex_destroy(&capture_hold) == 0);
    CHECK(job.pid > 0 && job.pidfd >= 0 && job.pidfd_error == 0);
    pid_t pid = job.pid;
    CHECK(setrlimit(RLIMIT_NOFILE, &original) == 0);
    CHECK(wf_fork_capsule_raw(SYS_rt_sigprocmask, SIG_SETMASK, 0,
          (long)&mask_after, sizeof(mask_after), 0, 0) == 0);
    CHECK(mask_before == mask_after);
    memset(bytes, 0xcc, sizeof(expected));
    memset(data, 0xdd, sizeof(*data));
    free(bytes);
    free(data);
    wf_fork_capsule_dispose(&capsule);
    CHECK(close(output[1]) == 0);
    for (size_t left = (size_t)capacity; left != 0;) {
        size_t count = left < sizeof(padding) ? left : sizeof(padding);
        parent_read(output[0], padding, count);
        for (size_t i = 0; i < count; ++i) CHECK(padding[i] == 0);
        left -= count;
    }
    unsigned char actual[sizeof(expected) + 2];
    parent_read(output[0], actual, sizeof(actual));
    CHECK(actual[0] == 0x7e && actual[1] == 1);
    CHECK(memcmp(actual + 2, expected, sizeof(expected)) == 0);
    CHECK(wf_fork_capsule_finish(&job, &result) == 0);
    CHECK(result.status_valid && result.setup_error == 0 && result.encoder_error == 0);
    CHECK(result.exit_code == 0 && result.signal_number == 0);
    check_reaped(&job, pid);
    unsigned char tail;
    CHECK(read(output[0], &tail, 1) == 0);
    CHECK(close(output[0]) == 0);
    CHECK(close(extra) == 0); /* Parent's unrelated fd was retained. */
    CHECK(atomic_load_explicit(&release_lane, memory_order_acquire) == 0);
    ring_round_trip(adapter); /* Real parent I/O while the compute lock is held. */
}

static void mapping_controls(wf_linux_io_uring_adapter *adapter) {
    const void *mappings[] = {adapter->submission_mapping,
        adapter->completion_mapping, adapter->submission_entries};
    for (size_t i = 0; i < sizeof(mappings) / sizeof(mappings[0]); ++i) {
        int output[2];
        CHECK(pipe2(output, O_CLOEXEC) == 0);
        witness_data data = {0};
        data.forbidden = mappings[i];
#if defined(WF_LINUX_IO_URING_TEST_SKIP_DONTFORK)
        unsigned char expected = *data.forbidden;
#endif
        wf_fork_capsule capsule;
        wf_fork_capsule_job job;
        wf_fork_capsule_result result;
        CHECK(wf_fork_capsule_prepare(&capsule, &output[1], 1, 4096) == 0);
        CHECK(wf_fork_capsule_start(&capsule, &data, forbidden_encoder, &job) == 0);
        CHECK(job.pidfd >= 0);
        pid_t pid = job.pid;
        wf_fork_capsule_dispose(&capsule);
        CHECK(close(output[1]) == 0);
        CHECK(wf_fork_capsule_finish(&job, &result) == 0);
#if defined(WF_LINUX_IO_URING_TEST_SKIP_DONTFORK)
        unsigned char actual;
        parent_read(output[0], &actual, 1);
        CHECK(actual == expected);
        CHECK(result.status_valid && result.exit_code == 0 && result.signal_number == 0);
        CHECK(result.setup_error == 0 && result.encoder_error == 0);
#else
        CHECK(result.signal_number == SIGSEGV && result.exit_code == -1);
        CHECK(!result.status_valid);
#endif
        unsigned char tail;
        CHECK(read(output[0], &tail, 1) == 0);
        CHECK(close(output[0]) == 0);
        check_reaped(&job, pid);
        ring_round_trip(adapter);
    }
}

static void refusal_and_output_failure(void) {
    wf_fork_capsule capsule;
    int invalid = -1;
    CHECK(wf_fork_capsule_prepare(&capsule, &invalid, 1, 4096) == EBADF);
    wf_fork_capsule_dispose(&capsule);
    CHECK(wf_fork_capsule_prepare(&capsule, NULL, 0, 0) == EINVAL);
    wf_fork_capsule_dispose(&capsule);
    /* An ignored SIGCHLD auto-reaps children, defeating pidfd/wait ownership.
     * Refuse it during preparation rather than create an unreapable job. */
    struct sigaction original, ignored;
    CHECK(sigaction(SIGCHLD, NULL, &original) == 0);
    memset(&ignored, 0, sizeof(ignored));
    ignored.sa_handler = SIG_IGN;
    CHECK(sigemptyset(&ignored.sa_mask) == 0);
    CHECK(sigaction(SIGCHLD, &ignored, NULL) == 0);
    CHECK(wf_fork_capsule_prepare(&capsule, NULL, 0, 4096) == ENOTSUP);
    wf_fork_capsule_dispose(&capsule);
    CHECK(sigaction(SIGCHLD, &original, NULL) == 0);
    int output[2];
    CHECK(pipe2(output, O_CLOEXEC) == 0);
    CHECK(close(output[0]) == 0);
    CHECK(wf_fork_capsule_prepare(&capsule, &output[1], 1, 4096) == 0);
    wf_fork_capsule_job job;
    wf_fork_capsule_result result;
    CHECK(wf_fork_capsule_start(&capsule, NULL, error_encoder, &job) == 0);
    CHECK(job.pidfd >= 0);
    pid_t pid = job.pid;
    wf_fork_capsule_dispose(&capsule);
    CHECK(close(output[1]) == 0);
    CHECK(wf_fork_capsule_finish(&job, &result) == 0);
    CHECK(result.status_valid && result.setup_error == 0 && result.encoder_error == EPIPE);
    CHECK(result.exit_code == 1 && result.signal_number == 0);
    check_reaped(&job, pid);
}

#if defined(WF_LINUX_IO_URING_MADVISE)
static unsigned advice_calls;
static unsigned refuse_advice;
int WF_LINUX_IO_URING_MADVISE(void *mapping, size_t length, int advice) {
    CHECK(advice == MADV_DONTFORK);
    ++advice_calls;
    if (advice_calls == refuse_advice) {
        errno = EIO;
        return -1;
    }
    return madvise(mapping, length, advice);
}

static void advice_failure_cases(wf_completion_runtime *runtime, unsigned count) {
    for (unsigned fail = 1; fail <= count; ++fail) {
        wf_linux_io_uring_adapter adapter;
        advice_calls = 0;
        refuse_advice = fail;
        CHECK(wf_linux_io_uring_init(&adapter, runtime, 8, 16) == EIO);
        CHECK(advice_calls == fail);
        CHECK(adapter.ring_descriptor == -1 && !adapter.initialized);
        CHECK(adapter.submission_mapping == NULL && adapter.completion_mapping == NULL);
        CHECK(adapter.submission_entries == NULL);
        CHECK(adapter.wait_descriptor == -1 && adapter.wake_descriptor == -1);
    }
    refuse_advice = 0;
}
#endif

int main(void) {
    struct utsname host;
    CHECK(uname(&host) == 0);
    printf("fork capsule host: %s %s %s; glibc %s\n",
           host.sysname, host.release, host.machine, gnu_get_libc_version());
    CHECK(setenv("WF_WORKERS", "2", 1) == 0);
    wf_test_guard_start(120);
    wf_completion_runtime runtime;
    wf_linux_io_uring_adapter adapter;
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    int error = wf_linux_io_uring_init(&adapter, &runtime, 8, 16);
    if (error != 0) {
        fprintf(stderr, "fork capsule: native ring unavailable: %s\n", strerror(error));
        CHECK(wf_completion_runtime_destroy(&runtime) == 0);
        wf_test_guard_finish();
        return 77;
    }
#if defined(WF_LINUX_IO_URING_MADVISE)
    unsigned expected_calls = adapter.submission_mapping == adapter.completion_mapping ? 2 : 3;
    CHECK(advice_calls == expected_calls); /* single-mapping alias advised once */
    advice_failure_cases(&runtime, expected_calls);
#endif
    CHECK(signal(SIGSEGV, inherited_fault_handler) != SIG_ERR);
    void *frame = wf__par_acquire_lane(sizeof(unsigned));
    CHECK(frame != NULL && wf__par_lane_count >= 2);
    wf__par_release(frame);
    pthread_t holder;
    CHECK(pthread_create(&holder, NULL, hold_compute_wait, NULL) == 0);
    while (!atomic_load_explicit(&lane_locked, memory_order_acquire)) {
        const struct timespec pause = {0, 1000000};
        (void)nanosleep(&pause, NULL);
    }
    wf_test_guard_phase("captured bytes and descriptor isolation with held compute lock");
    ring_round_trip(&adapter);
    snapshot_case(&adapter);
    wf_test_guard_phase("SQ/CQ/SQE forbidden-access controls and parent ring survival");
    mapping_controls(&adapter);
    wf_test_guard_phase("refusal and output failure result/reap");
    refusal_and_output_failure();
    atomic_store_explicit(&release_lane, 1, memory_order_release);
    CHECK(pthread_join(holder, NULL) == 0);
    frame = wf__par_acquire_lane(sizeof(unsigned));
    CHECK(frame != NULL);
    *(unsigned *)frame = 41;
    wf__par_publish(frame, compute_increment);
    wf__par_join(frame);
    CHECK(*(unsigned *)frame == 42);
    wf__par_release(frame);
    CHECK(wf_linux_io_uring_destroy(&adapter) == 0);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    wf_test_guard_finish();
    puts("fork capsule: PASS");
    return 0;
}
#else
#include <stdio.h>
#if defined(__linux__) && defined(WF_LINUX_IO_URING_MADVISE)
#include <sys/mman.h>
/* The unqualified image still links the ring, whose named advice hook needs
 * a definition. No capsule runs here; main reports unavailable below. */
int WF_LINUX_IO_URING_MADVISE(void *mapping, size_t length, int advice) {
    return madvise(mapping, length, advice);
}
#endif
int main(void) {
    fputs("fork capsule: requires Linux x86-64 glibc qualification\n", stderr);
    return 77;
}
#endif
