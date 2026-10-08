/* Host floor cases share construction, but fatal observations always use a
 * fresh process. Retire each case with the runtime mechanism it exercises. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static int probe_fault, probe_active, probe_control = -1;
#if defined(WF_FLOOR_TEST_HOOKS)
static _Thread_local uintptr_t probe_alt_low, probe_alt_high;
#endif
static pthread_t probe_initial_thread;
static const char *probe_mode;
static size_t probe_offset;

#if defined(WF_FLOOR_TEST_HOOKS)
/* Only the floor's calls are substituted; success forwards to the host. */
static void *probe_mmap(void *p, size_t n, int prot, int flags, int fd, off_t offset) {
    if (probe_active && probe_fault == 1) { errno = ENOMEM; return MAP_FAILED; }
    return mmap(p, n, prot, flags, fd, offset);
}
static int probe_sigaltstack(const stack_t *stack, stack_t *old) {
    if (probe_active && probe_fault == 2) { errno = ENOMEM; return -1; }
    int result = sigaltstack(stack, old);
    if (!result && stack) {
        probe_alt_low = (uintptr_t)stack->ss_sp;
        probe_alt_high = probe_alt_low + stack->ss_size;
    }
    return result;
}
static int probe_sigaction(int signo, const struct sigaction *action, struct sigaction *old) {
    if (probe_active && ((probe_fault == 3 && signo == SIGSEGV)
        || (probe_fault == 4 && signo == SIGBUS))) { errno = ENOMEM; return -1; }
    return sigaction(signo, action, old);
}
static long probe_sysconf(int name) {
    if (probe_active && probe_fault == 5 && name == _SC_PAGESIZE) { errno = EINVAL; return -1; }
    return sysconf(name);
}
#if defined(__APPLE__)
static void *probe_stackaddr(pthread_t thread) {
    return probe_active && probe_fault == 6 ? NULL : pthread_get_stackaddr_np(thread);
}
static size_t probe_stacksize(pthread_t thread) {
    return probe_active && probe_fault == 7 ? 0 : pthread_get_stacksize_np(thread);
}
#define pthread_get_stackaddr_np probe_stackaddr
#define pthread_get_stacksize_np probe_stacksize
#else
static int probe_getattr(pthread_t thread, pthread_attr_t *attributes) {
    return probe_active && probe_fault == 6 ? ENOMEM : pthread_getattr_np(thread, attributes);
}
static int probe_getstack(const pthread_attr_t *attributes, void **base, size_t *size) {
    return probe_active && probe_fault == 7 ? EINVAL : pthread_attr_getstack(attributes, base, size);
}
#define pthread_getattr_np probe_getattr
#define pthread_attr_getstack probe_getstack
#endif
static int probe_setstacksize(pthread_attr_t *attr, size_t size) {
    return probe_fault == 8 ? EINVAL : pthread_attr_setstacksize(attr, size);
}
static int probe_attr_init(pthread_attr_t *attr) {
    return probe_fault == 10 ? ENOMEM : pthread_attr_init(attr);
}
static int probe_create(pthread_t *t, const pthread_attr_t *attr, void *(*run)(void *), void *arg) {
    return probe_fault == 9 ? EAGAIN : pthread_create(t, attr, run, arg);
}
static int probe_pause(void) {
    if (probe_control >= 0) {
        char marker;
        uintptr_t here = (uintptr_t)&marker;
        marker = here >= probe_alt_low && here < probe_alt_high ? 'L' : 'X';
        if (write(probe_control, &marker, 1) != 1) _Exit(81);
        probe_control = -1;
    }
    return pause();
}
#define mmap probe_mmap
#define sigaltstack probe_sigaltstack
#define sigaction(...) probe_sigaction(__VA_ARGS__)
#define sysconf probe_sysconf
#define pthread_attr_setstacksize probe_setstacksize
#define pthread_attr_init probe_attr_init
#define pthread_create probe_create
#define pause probe_pause
#endif
#include "wf_floor.c"
#if defined(WF_FLOOR_TEST_HOOKS)
#undef mmap
#undef sigaltstack
#undef sigaction
#undef sysconf
#undef pthread_attr_setstacksize
#undef pthread_attr_init
#undef pthread_create
#undef pause
#if defined(__APPLE__)
#undef pthread_get_stackaddr_np
#undef pthread_get_stacksize_np
#else
#undef pthread_getattr_np
#undef pthread_attr_getstack
#endif
#endif
#include "sched/prim.h"
#include "runtime_test_guard.h"
extern void *wf__par_acquire_lane(uint64_t);
extern void wf__par_publish(void *, void (*)(void *));
extern void wf__par_join(void *);
extern void wf__par_release(void *);

static uintptr_t stack_low(size_t *size) {
#if defined(__APPLE__)
    *size = pthread_get_stacksize_np(pthread_self());
    return (uintptr_t)pthread_get_stackaddr_np(pthread_self()) - *size;
#else
    pthread_attr_t attributes;
    void *base;
    if (pthread_getattr_np(pthread_self(), &attributes)
        || pthread_attr_getstack(&attributes, &base, size)) _Exit(82);
    pthread_attr_destroy(&attributes);
    return (uintptr_t)base;
#endif
}
static _Atomic unsigned worker_done;
static void measure_worker(void *unused) {
    (void)unused;
    size_t size;
    (void)stack_low(&size);
    if (pthread_equal(pthread_self(), probe_initial_thread)
        || size < (size_t)1024 * 1024 * 1024) _Exit(83);
    atomic_store_explicit(&worker_done, 1, memory_order_release);
}
static void *attach_worker(void *unused) {
    (void)unused;
    wf__floor_attach_thread();
    atomic_store(&worker_done, 1);
    return NULL;
}
static char *reservation;
#define PROBE_PAD ((size_t)16 * 1024 * 1024 + 65536)
#define PROBE_STACK ((size_t)1024 * 1024)
static void *offset_fault(void *unused) {
    (void)unused;
    size_t size;
    wf__floor_attach_thread();
    uintptr_t low = stack_low(&size);
    if (low != (uintptr_t)(reservation + PROBE_PAD) || size != PROBE_STACK) {
        fprintf(stderr, "owned stack: base delta=%lld bytes=%zu\n", (long long)(low - (uintptr_t)(reservation + PROBE_PAD)), size);
        _Exit(84);
    }
    if (munmap(reservation, PROBE_PAD)) _Exit(85);
    /* No allocation occurs after opening this hole. */
    *(volatile uint32_t *)(low - probe_offset) = 1;
    _Exit(86);
}
static void run_offset_fault(void) {
    pthread_attr_t attr;
    pthread_t thread;
    reservation = mmap(NULL, PROBE_PAD + PROBE_STACK, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (reservation == MAP_FAILED || mprotect(reservation + PROBE_PAD, PROBE_STACK, PROT_READ | PROT_WRITE)
        || pthread_attr_init(&attr) || pthread_attr_setstack(&attr, reservation + PROBE_PAD, PROBE_STACK)
        || pthread_create(&thread, &attr, offset_fault, NULL)) _Exit(87);
    pthread_attr_destroy(&attr);
    pthread_join(thread, NULL);
    _Exit(88);
}

int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    if (!strcmp(probe_mode, "wild")) {
        *(volatile int *)(uintptr_t)0xdeadb000 = 1;
        return 89;
    }
    if (!strcmp(probe_mode, "external")) {
        /* Delivery to the attached entry thread is explicit. */
        if (pthread_kill(pthread_self(), SIGBUS)) return 90;
        if (write(1, "SURVIVED\n", 9) != 9) return 90;
        return 91;
    }
    if (!strcmp(probe_mode, "offset")) run_offset_fault();
    if (!strcmp(probe_mode, "latch")) {
        *wf__floor_record_latch() = 1;
        run_offset_fault();
    }
    if (!strcmp(probe_mode, "setup-worker")) {
        pthread_t thread;
        probe_active = 1;
        if (pthread_create(&thread, NULL, attach_worker, NULL) || pthread_join(thread, NULL)
            || !atomic_load(&worker_done)) return 92;
    }
    if (!strcmp(probe_mode, "provision")) {
        size_t size;
        (void)stack_low(&size);
        if (pthread_equal(pthread_self(), probe_initial_thread)
            || size < (size_t)1024 * 1024 * 1024) {
            fprintf(stderr, "entry stack: bytes=%zu original=%d\n", size, pthread_equal(pthread_self(), probe_initial_thread));
            return 93;
        }
        probe_initial_thread = pthread_self();
        void *frame = wf__par_acquire_lane(8);
        if (!frame) return 94;
        wf__par_publish(frame, measure_worker);
        uint64_t deadline = wf_prim_monotonic_us() + 5000000;
        while (!atomic_load_explicit(&worker_done, memory_order_acquire)) {
            if (wf_prim_monotonic_us() >= deadline) return 95;
            sched_yield();
        }
        wf__par_join(frame);
        wf__par_release(frame);
    }
    if (write(1, "ran\n", 4) != 4) return 97;
    return 73;
}

/* Capture exact channels and wait disposition. A latch loser is stopped only
 * after its real signal handler announces entry to pause on the alternate
 * stack; elapsed time is never accepted as evidence of that state. */
static void run_case(const char *self, const char *mode, unsigned value, int expected_signal,
                     int expected_exit, const char *expected_out, const char *expected_err, int latch) {
    int out[2], err[2], control[2];
    if (pipe(out) || pipe(err) || pipe(control)) abort();
    char number[32], fd[32];
    snprintf(number, sizeof(number), "%u", value);
    snprintf(fd, sizeof(fd), "%d", control[1]);
    pid_t child = fork();
    if (child < 0) abort();
    if (!child) {
        close(out[0]); close(err[0]); close(control[0]);
        if (dup2(out[1], 1) < 0 || dup2(err[1], 2) < 0) _Exit(98);
        close(out[1]); close(err[1]);
        execl(self, self, "child", mode, number, fd, (char *)NULL);
        _Exit(99);
    }
    close(out[1]); close(err[1]); close(control[1]);
    char stdout_bytes[256] = {0}, stderr_bytes[256] = {0};
    if (latch) {
        struct pollfd wait = {control[0], POLLIN, 0};
        char event = 0;
        if (poll(&wait, 1, 5000) != 1 || read(control[0], &event, 1) != 1 || event != 'L') {
            fprintf(stderr, "floor %s: missing alternate-stack latch loser\n", mode);
            kill(child, SIGKILL); waitpid(child, NULL, 0); exit(1);
        }
        if (kill(child, SIGTERM)) abort();
    }
    int status;
    if (waitpid(child, &status, 0) != child) abort();
    ssize_t nout = read(out[0], stdout_bytes, sizeof(stdout_bytes) - 1);
    ssize_t nerr = read(err[0], stderr_bytes, sizeof(stderr_bytes) - 1);
    close(out[0]); close(err[0]); close(control[0]);
    if (nout < 0 || nerr < 0 || (expected_signal ? !WIFSIGNALED(status) || WTERMSIG(status) != expected_signal
        : !WIFEXITED(status) || WEXITSTATUS(status) != expected_exit)
        || nout != (ssize_t)strlen(expected_out) || memcmp(stdout_bytes, expected_out, (size_t)nout)
        || nerr != (ssize_t)strlen(expected_err) || memcmp(stderr_bytes, expected_err, (size_t)nerr)) {
        fprintf(stderr, "floor %s %u: status=%d stdout=[%s] stderr=[%s]\n", mode, value, status, stdout_bytes, stderr_bytes);
        exit(1);
    }
}
int main(int argc, char **argv) {
    struct rlimit no_core = {0, 0};
    if (setrlimit(RLIMIT_CORE, &no_core)) return 2;
    if (argc == 5 && !strcmp(argv[1], "child")) {
        alarm(10);
        probe_mode = argv[2];
        probe_fault = atoi(argv[3]);
        probe_offset = (size_t)strtoul(argv[3], NULL, 10);
        probe_control = !strcmp(probe_mode, "latch") ? atoi(argv[4]) : -1;
        probe_active = !strcmp(probe_mode, "setup-entry");
        probe_initial_thread = pthread_self();
        return wf__floor_run(argc, argv);
    }
    if (argc != 1 || wf__floor_stack_bytes() != (size_t)1024 * 1024 * 1024) return 2;
    wf_test_guard_start(60);
    wf_test_guard_phase("floor host cases");
#if defined(WF_FLOOR_TEST_HOOKS)
    const char *setup_error = "whitefoot floor: stack exhaustion protection could not be installed\n";
    for (unsigned worker = 0; worker < 2; ++worker) {
        for (unsigned fault = 0; fault <= 7; ++fault) {
            if (worker && fault >= 3 && fault <= 5) continue;
            run_case(argv[0], worker ? "setup-worker" : "setup-entry", fault, fault ? SIGABRT : 0,
                     73, fault ? "" : "ran\n", fault ? setup_error : "", 0);
        }
    }
    /* Running the entry after a launch failure would leave the original
     * thread unavailable for stop reception. No body output is permitted. */
    const char *launch_error =
        "whitefoot floor: stop receiver or program entry thread could not be started\n";
    for (unsigned fault = 8; fault <= 10; ++fault)
        run_case(argv[0], "launch", fault, SIGABRT, 0, "", launch_error, 0);
    run_case(argv[0], "latch", (unsigned)sysconf(_SC_PAGESIZE), SIGTERM, 0, "", "", 1);
#else
    run_case(argv[0], "provision", 0, 0, 73, "ran\n", "", 0);
    run_case(argv[0], "wild", 0, SIGSEGV, 0, "", "", 0);
    run_case(argv[0], "external", 0, SIGBUS, 0, "", "", 0);
    unsigned page = (unsigned)sysconf(_SC_PAGESIZE);
    unsigned offsets[] = {page / 2, page, page + 128, page + 132, 4 * page, 65536, 16 * 1024 * 1024};
    for (unsigned i = 0; i < sizeof(offsets) / sizeof(offsets[0]); ++i) {
        int duplicate = 0;
        for (unsigned j = 0; j < i; ++j) duplicate |= offsets[i] == offsets[j];
        if (duplicate) continue;
        int inside = offsets[i] <= page + 128;
        run_case(argv[0], "offset", offsets[i], inside ? SIGABRT : SIGSEGV, 0, "",
                 inside ? "{\"resource\":\"stack\"}\n" : "", 0);
    }
#endif
    wf_test_guard_finish();
    puts("floor host cases: PASS");
    return 0;
}
