#ifndef WHITEFOOT_FORK_CAPSULE_H
#define WHITEFOOT_FORK_CAPSULE_H

#if defined(__linux__)
#include <features.h>
#endif

#if defined(__linux__) && defined(__x86_64__) && defined(__LP64__) && defined(__GLIBC__)
/* Restricted native capsule; this is not a Whitefoot callback ABI. Every
 * encoder and its transitive calls must be audited: private COW read-only
 * captured data, stack/scratch writes, raw syscalls on designated outputs,
 * no inherited allocators, TLS, locks, runtime helpers or cleanup. Outputs
 * must be fresh unpublished descriptors, with no parent offset/flag mutation.
 * In the native witness workflow one thread owns prepare/start/finish and
 * outlives the child. Source reserves/jobs may move; the physical executor
 * that performs their fork still outlives the child. The
 * parent must leave children waitable and have no competing child reaper.
 * Capture runs glibc's fork, so the process must register no pthread_atfork
 * handler: a handler runs in the child before the capsule resets anything,
 * and in the parent inside the hold. The runtime registers none; glibc's own
 * fork handling takes only its internal locks, never held across a hold wait.
 * AArch64 and other libcs need separate child-closure qualification. */
#include <stddef.h>
#include <stdint.h>
#include <sys/syscall.h>
#include <sys/types.h>

#define WF_FORK_CAPSULE_MAX_OUTPUTS 64u

typedef int (*wf_fork_capsule_encoder)(
    const void *captured,
    void *scratch,
    size_t scratch_size,
    const int *outputs,
    size_t output_count
);

/* Runtime-supplied source encoder, never a Whitefoot callback. It emits its
 * own status protocol, including setup failure, and returns an exit code. */
typedef int (*wf_fork_capsule_source_encoder)(const void *, void *, size_t,
                                             int, int, int);

typedef struct wf_fork_capsule {
    void *scratch;
    size_t scratch_size;
    int outputs[WF_FORK_CAPSULE_MAX_OUTPUTS];
    size_t output_count;
    int keep[WF_FORK_CAPSULE_MAX_OUTPUTS + 1u];
    size_t keep_count;
    int result_read;
    int result_write;
    pid_t parent;
    unsigned prepared;
    uint64_t parent_signal_mask;
    unsigned armed;
    pid_t capture_pid;
    void *source_stack;
    size_t source_stack_size;
    wf_fork_capsule_source_encoder source_encoder;
} wf_fork_capsule;

typedef struct wf_fork_capsule_job {
    pid_t pid;
    int pidfd;
    /* A post-fork pidfd failure still creates a job, reaped by waitpid. */
    int pidfd_error;
    int result_read;
} wf_fork_capsule_job;

typedef struct wf_fork_capsule_result {
    int exit_code;                  /* -1 for signal death */
    int signal_number;              /* zero for normal exit */
    unsigned status_valid;          /* absent/malformed status is not success */
    int setup_error;
    int encoder_error;
} wf_fork_capsule_result;

/* Parent only, outside dataset holds. Outputs remain caller-owned; this unit
 * owns only scratch and its result pipe. On error no resources are retained.
 * Prepare requires fresh capsule storage. Start requires a prepared capsule
 * and fresh job storage, never an already running job. */
int wf_fork_capsule_prepare(
    wf_fork_capsule *capsule,
    const int *outputs,
    size_t output_count,
    size_t scratch_size
);
int wf_fork_capsule_prepare_source(wf_fork_capsule *, int, size_t, void *,
                                   size_t, wf_fork_capsule_source_encoder);
/* Bookkeeping only: the ordinary atomic entry has already blocked signals
 * and owns their restoration, independently of the supplied reserve. */
int wf_fork_capsule_adopt_blocked(wf_fork_capsule *, uint64_t);
/* Held-capture route: arm outside the dataset hold, acquire the hold, call
 * capture_held, release the hold immediately on return, then parent_start.
 * Arm blocks this calling thread's signals, excluding inherited handlers at
 * child entry. capture_held performs only glibc fork and return bookkeeping
 * in the parent: no allocation, descriptor work or coroutine suspension.
 * Its positive return is a created child; its negative return is -errno.
 * parent_start must follow either result, restoring the calling thread's mask
 * and transferring every created child to a job before disposal/reuse. */
int wf_fork_capsule_arm(wf_fork_capsule *capsule);
pid_t wf_fork_capsule_capture_held(
    wf_fork_capsule *capsule,
    const void *captured,
    wf_fork_capsule_encoder encoder
);
int wf_fork_capsule_parent_start(wf_fork_capsule *capsule, wf_fork_capsule_job *job);
/* Outside holds; no pidfd and no mask restoration. */
int wf_fork_capsule_parent_start_owned(wf_fork_capsule *, wf_fork_capsule_job *);
/* Termination only, leaving source status collection and descriptors owned.
 * On error the child remains owned; success clears its pid exactly once. */
int wf_fork_capsule_reap_owned(wf_fork_capsule_job *, wf_fork_capsule_result *);
/* Convenience composition of those three steps, OUTSIDE dataset holds.
 * Returns an errno only when no child exists; otherwise returns zero and a
 * job with pid/pidfd. No ring/driver/pool state is consulted. */
int wf_fork_capsule_start(
    wf_fork_capsule *capsule,
    const void *captured,
    wf_fork_capsule_encoder encoder,
    wf_fork_capsule_job *job
);
/* Parent only, outside holds: poll pidfd then reap and read the bounded status.
 * Zero means reaped, not encoder success. On error the job remains owned and
 * must be retried. No SIGCHLD handler or process-wide signal-mask change. */
int wf_fork_capsule_finish(
    wf_fork_capsule_job *job,
    wf_fork_capsule_result *result
);
/* Parent only; may run after start because the child has its private image.
 * Never closes designated outputs. Must not run on an active prepared object
 * concurrently with start. An armed capture needs parent_start first; an arm
 * cancelled before capture may be disposed (restoring the thread's mask).
 * Safe also after a failed prepare or repeated call. */
void wf_fork_capsule_dispose(wf_fork_capsule *capsule);

/* Linux x86-64 kernel ABI, returning negative errno directly. Child code uses
 * this inline stub instead of libc syscall/errno or lazy dynamic linking. */
static inline long wf_fork_capsule_raw(
    long number, long a1, long a2, long a3, long a4, long a5, long a6
) {
    register long r10 __asm__("r10") = a4;
    register long r8 __asm__("r8") = a5;
    register long r9 __asm__("r9") = a6;
    long result;
    __asm__ __volatile__(
        "syscall"
        : "=a"(result)
        : "a"(number), "D"(a1), "S"(a2), "d"(a3),
          "r"(r10), "r"(r8), "r"(r9)
        : "rcx", "r11", "memory", "cc"
    );
    return result;
}
#endif
#endif
