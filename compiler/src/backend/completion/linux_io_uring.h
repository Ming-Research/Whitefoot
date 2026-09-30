#ifndef WHITEFOOT_LINUX_IO_URING_H
#define WHITEFOOT_LINUX_IO_URING_H

#if defined(__linux__)

#include "contract.h"
/* The typed file vocabulary — expected kind, open outcome, and the one rule
 * that decides them — is shared with the POSIX host leaf of the bounded
 * adapter so that one open states one contract whichever POSIX engine ran it.
 * Only the header is used: this adapter calls no function defined in
 * `file_posix.c`. */
#include "file_posix.h"

#include <linux/io_uring.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>

#if defined(__cplusplus)
extern "C" {
#endif

/* The ring's depth: how many submission entries the kernel keeps for this
 * process, asked for once at `io_uring_setup`.
 *
 * It is a throughput parameter of the ring and nothing else.  It used to be
 * the completion slot count, and through that a bound on how many operations
 * the process could have in flight; with the record a block of the submitting
 * frame there is no such bound, and a full submission queue is emptied by the
 * submitting call's own `io_uring_enter` rather than by a wait on an event
 * (design §7).  The number is the one the slot count used to supply, so this
 * change moves no measurement.  The bridge passes it at
 * `wf_linux_io_uring_init`; nothing else sets it. */
#define WF_LINUX_IO_URING_DEPTH 64u

/* The completion queue's size, which is the number of completions the kernel
 * can post before it has to keep the rest on its own overflow list.  With
 * IORING_FEAT_NODROP nothing is lost past it, but an overflowed completion
 * reaches the queue only through an `io_uring_enter`, so the reaper flushes
 * the overflow whenever the kernel raises IORING_SQ_CQ_OVERFLOW.  This is
 * sized so the network control test's 1024 connections in flight, one
 * receive each, never reach that path: twice the lane's slot count, at
 * sixteen bytes an entry.  The bridge passes it at `wf_linux_io_uring_init`;
 * the native adapter probe passes a small one to reach the overflow path on
 * purpose. */
#define WF_LINUX_IO_URING_COMPLETIONS 2048u

enum wf_linux_io_uring_submit_result {
    WF_LINUX_IO_URING_TARGET_OWNS = 0,
    WF_LINUX_IO_URING_SUBMIT_INVALID = 1,
    WF_LINUX_IO_URING_UNAVAILABLE = 2,
    /* The submitting call could not empty the submission queue itself.  It is
     * a target-runtime failure and never an operation outcome: the caller
     * fail-stops on it. */
    WF_LINUX_IO_URING_SUBMIT_FAILED = 3
};

typedef struct wf_linux_io_uring_statistics {
    uint64_t submissions;
    /* `io_uring_enter` calls that carried staged submissions to the kernel.
     * With the doorbell deferred this is far below `submissions`, and the
     * distance between the two is the whole of what deferring buys. */
    uint64_t submission_enters;
    uint64_t completions;
    uint64_t enter_failures;
    uint64_t kernel_waits;
    uint64_t kernel_wakes;
    uint64_t host_wake_writes;
    /* `io_uring_enter` calls made only to move completions off the kernel's
     * overflow list into the queue.  Zero in a program whose in-flight count
     * stayed under the queue's size. */
    uint64_t overflow_flushes;
} wf_linux_io_uring_statistics;

/* Target-private protocol state.  The mmap pointers below name shared
 * kernel/runtime queue pages; they are never Whitefoot buffers and never
 * cross the generated-code ABI.  A request buffer crosses this boundary only
 * inside a `wf_completion_record`, which is a block of the frame that
 * submitted the operation and outlives it. */
typedef struct wf_linux_io_uring_adapter {
    wf_completion_runtime *runtime;
    size_t depth;
    _Atomic size_t in_flight;

    int ring_descriptor;
    int wait_descriptor;
    int wake_descriptor;
    void *submission_mapping;
    size_t submission_mapping_size;
    void *completion_mapping;
    size_t completion_mapping_size;
    struct io_uring_sqe *submission_entries;
    size_t submission_entries_size;

    unsigned *submission_head;
    unsigned *submission_tail;
    unsigned *submission_mask;
    unsigned *submission_count;
    unsigned *submission_array;
    /* The kernel's flags word on the submission ring, read for
     * IORING_SQ_CQ_OVERFLOW: completions the kernel is holding on its
     * overflow list because the completion queue was full. */
    unsigned *submission_flags;
    unsigned *completion_head;
    unsigned *completion_tail;
    unsigned *completion_mask;
    unsigned *completion_count;
    unsigned *completion_overflow;
    struct io_uring_cqe *completion_entries;

    pthread_mutex_t submission_lock;
    pthread_mutex_t completion_lock;
    unsigned initialized;

    _Atomic uint64_t stat_submissions;
    _Atomic uint64_t stat_submission_enters;
    _Atomic uint64_t stat_completions;
    _Atomic uint64_t stat_enter_failures;
    _Atomic uint64_t stat_kernel_waits;
    _Atomic uint64_t stat_kernel_wakes;
    _Atomic uint64_t stat_host_wake_writes;
    _Atomic uint64_t stat_overflow_flushes;
    _Atomic int progress_error;
} wf_linux_io_uring_adapter;

/* `depth` is the ring's submission-queue depth, a throughput parameter and
 * not a bound on operations in flight; `completions` is the completion
 * queue's size, at least `depth`, past which the kernel holds completions on
 * its overflow list until the reaper flushes them.  Initialization requires
 * IORING_FEAT_NODROP so an accepted operation can never silently lose its
 * unique CQE.  Any setup or qualification failure occurs before a record is
 * touched. */
int wf_linux_io_uring_init(
    wf_linux_io_uring_adapter *adapter,
    wf_completion_runtime *runtime,
    size_t depth,
    size_t completions
);

/* Whether this adapter has a ring form for the record's request kind.  Asked
 * by the bridge before it hands the record over, so a kind the ring does not
 * carry reaches the bounded POSIX adapter or the inline executor instead of
 * being refused after the operation was already the ring's. */
int wf_linux_io_uring_carries(const wf_completion_record *record);

/* Hands one record to the ring.  It cannot answer capacity: a full submission
 * queue is emptied by this call's own `io_uring_enter`. */
enum wf_linux_io_uring_submit_result wf_linux_io_uring_submit(
    wf_linux_io_uring_adapter *adapter,
    wf_completion_record *record
);

/* Kicks staged SQEs, then completes at most `budget` CQEs.  If `wait` is
 * nonzero and no CQE is ready, one thread may wait in io_uring_enter;
 * submissions use a different lock and remain possible while it sleeps.
 * Returns zero or an errno value and always reports the number completed. */
int wf_linux_io_uring_progress(
    wf_linux_io_uring_adapter *adapter,
    size_t budget,
    unsigned wait,
    size_t *published
);

/* Announces any completion-core epoch change to the Linux wait set.  This is
 * the callback installed with wf_completion_set_wake_callback; it only writes
 * the bounded eventfd counter and never runs a continuation. */
void wf_linux_io_uring_notify(void *context);

/* Parks one thread on the union of the ring CQ and the completion core's
 * epoch endpoint.  `observed_epoch` is rechecked before the kernel wait,
 * closing completion-before-park without a polling timeout.  UINT32_MAX means
 * no deadline. */
int wf_linux_io_uring_park(
    wf_linux_io_uring_adapter *adapter,
    uint64_t observed_epoch,
    uint32_t timeout_milliseconds
);

/* A nonzero value is a sticky target-owned progress failure.  Once an
 * operation has been accepted, the bridge must fail-stop on this condition;
 * it may neither fall back nor leave the operation waiting silently. */
int wf_linux_io_uring_progress_error(
    const wf_linux_io_uring_adapter *adapter
);

size_t wf_linux_io_uring_in_flight(
    const wf_linux_io_uring_adapter *adapter
);

/* The ring's submission-queue depth as the kernel gave it.  Zero before
 * initialization succeeds.  It is a throughput parameter, not a bound on how
 * many operations may be outstanding. */
size_t wf_linux_io_uring_capacity(
    const wf_linux_io_uring_adapter *adapter
);

/* Submits every staged entry the deferred doorbell has left in the submission
 * queue.  Returns zero or an errno value.  A caller that is about to block
 * outside this adapter — a join, a park, or a blocking direct host call — must
 * flush first, or the kernel never learns of work it could already be doing.
 */
int wf_linux_io_uring_flush(wf_linux_io_uring_adapter *adapter);

/* Asks the kernel to give up the operation a record names, once its deadline
 * has passed [PRE-2]: an IORING_OP_ASYNC_CANCEL whose own completion names no
 * record.  The record then completes with -ECANCELED, or with its own result
 * when that came first.  Returns zero, or the error that stopped the ring. */
int wf_linux_io_uring_cancel(
    wf_linux_io_uring_adapter *adapter,
    struct wf_completion_record *record
);

/* Refuses with EBUSY until every accepted operation has produced and
 * published its CQE. */
int wf_linux_io_uring_destroy(wf_linux_io_uring_adapter *adapter);

wf_linux_io_uring_statistics wf_linux_io_uring_statistics_snapshot(
    const wf_linux_io_uring_adapter *adapter
);

#if defined(__cplusplus)
}
#endif

#endif

#endif
