#ifndef WHITEFOOT_FORK_CAPSULE_H
#define WHITEFOOT_FORK_CAPSULE_H

#if defined(__linux__)
#include <features.h>
#endif

#if defined(__linux__) && defined(__x86_64__) && defined(__LP64__) && defined(__GLIBC__)
/* Native witness surface only; this is not a Whitefoot callback ABI. Every
 * encoder and its transitive calls must be audited: private COW read-only
 * captured data, stack/scratch writes, raw syscalls on designated outputs,
 * no inherited allocators, TLS, locks, runtime helpers or cleanup. Outputs
 * must be fresh unpublished descriptors, with no parent offset/flag mutation.
 * One calling thread owns prepare/start/finish and outlives the child. The
 * parent must leave children waitable and have no competing child reaper.
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
