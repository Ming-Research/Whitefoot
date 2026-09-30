#ifndef WHITEFOOT_WINDOWS_IOCP_H
#define WHITEFOOT_WINDOWS_IOCP_H

#if defined(_WIN32)

/*
 * The Windows kernel completion ring, in the same shape as `linux_io_uring.h`.
 *
 * It finds the record by address: the `OVERLAPPED` a request is issued with is
 * embedded in the frame's own completion record, so the completion packet's
 * `OVERLAPPED` pointer *is* the record's identity, exactly as a CQE's
 * `user_data` is on Linux (design
 * `research/investigations/io-model/PARK-ON-MISS.md` §7, "What survives, and
 * the one change inside it").  There is no entry pool, no token, no
 * generation, and therefore no capacity this ring can refuse for.
 *
 * Its wait is the one park the bridge supplies through the
 * `wf__sched_host_epoch` / `_park` / `_wake` seam on Windows (§7, platform item
 * 2): the completion port itself, with a wake delivered as a
 * `PostQueuedCompletionStatus` packet, so a ready stack and an I/O completion
 * arrive on one queue. New park calls use the same epoch's condition variable
 * until an older notified cohort has finished consuming its port packets.
 */

#include "contract.h"

#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <windows.h>

#if defined(__cplusplus)
extern "C" {
#endif

enum wf_windows_iocp_submit_result {
    WF_WINDOWS_IOCP_TARGET_OWNS = 0,
    WF_WINDOWS_IOCP_SUBMIT_INVALID = 1,
    WF_WINDOWS_IOCP_UNAVAILABLE = 2
};

typedef struct wf_windows_iocp_statistics {
    uint64_t submissions;
    /* Requests the kernel answered synchronously, published on the submitting
     * thread because the association asked for no packet in that case. */
    uint64_t inline_completions;
    uint64_t dequeued_completions;
    uint64_t completions;
    uint64_t kernel_waits;
    uint64_t host_wake_posts;
} wf_windows_iocp_statistics;

/* Target-private protocol state.  The completion port is not Whitefoot memory
 * and never crosses the generated-code ABI.  A request buffer crosses this
 * boundary only inside a `wf_completion_record`, which is a block of the frame
 * that submitted the operation and outlives it. */
typedef struct wf_windows_iocp_adapter {
    wf_completion_runtime *runtime;
    HANDLE port;
    ULONG_PTR wake_key;
    ULONG_PTR file_key;
    /* Under runtime->wait: native sleepers and the subset already notified.
     * Each node belongs to one active park call, not to a writer frame. */
    struct wf_windows_iocp_waiter *waiters;
    unsigned notified_waiters;
    _Atomic size_t in_flight;
    _Atomic int progress_error;
    unsigned initialized;

    _Atomic uint64_t stat_submissions;
    _Atomic uint64_t stat_inline_completions;
    _Atomic uint64_t stat_dequeued_completions;
    _Atomic uint64_t stat_completions;
    _Atomic uint64_t stat_kernel_waits;
    _Atomic uint64_t stat_host_wake_posts;
} wf_windows_iocp_adapter;

/* Creates the one completion port this process's ring uses.  `concurrency` is
 * the port's own concurrency hint; zero asks the kernel for its default. */
int wf_windows_iocp_init(
    wf_windows_iocp_adapter *adapter,
    wf_completion_runtime *runtime,
    DWORD concurrency
);

/* Associates one file handle with the port, once, and asks the kernel not to
 * queue a packet for a request it answers synchronously -- that request is
 * already complete, so the submitting thread publishes it instead of paying
 * for a port round trip. */
int wf_windows_iocp_associate(
    wf_windows_iocp_adapter *adapter,
    HANDLE handle
);

/* Whether this ring has a form for one record's request kind.  Asked by the
 * bridge before the record is handed over, so a kind the ring does not carry
 * reaches the bounded adapter or the inline executor instead of being refused
 * after the operation was already the ring's. */
int wf_windows_iocp_carries(const wf_completion_record *record);

/* The descriptor this record's request will be issued on, or -1.
 *
 * Every kind but the connect already has one.  A connect names none at submit,
 * because it creates its own socket; this makes it, and binds it to the
 * wildcard address of its family because `ConnectEx` requires a bound socket.
 * It is a separate call from the submit below because the port has to take the
 * handle before the request is issued on it, and taking it happens under the
 * descriptor table's own lock (`../windows_runtime.h`,
 * `wf__windows_completion_ring_handle`).
 *
 * The socket it creates is undone by `wf_windows_iocp_withdraw` when the port
 * refuses the handle, so the record reaches the bounded adapter exactly as it
 * was offered. */
int wf_windows_iocp_issue_descriptor(wf_completion_record *record);

/* Undoes what `wf_windows_iocp_issue_descriptor` created for a record the
 * ring did not take.  Nothing was published and nothing is left half-owned. */
void wf_windows_iocp_withdraw(wf_completion_record *record);

/* Asks the kernel to give up the operation a record names, once its deadline
 * has passed [PRE-2]: `CancelIoEx` on the handle and `OVERLAPPED` it was
 * issued with.  The record then completes with ERROR_OPERATION_ABORTED, or
 * with its own result when that came first. */
void wf_windows_iocp_cancel(wf_completion_record *record);

/* Hands one record to the ring, on a handle the port has taken.  It cannot
 * answer capacity: the record is the frame's and the port holds no pool. */
enum wf_windows_iocp_submit_result wf_windows_iocp_submit(
    wf_windows_iocp_adapter *adapter,
    wf_completion_record *record,
    HANDLE handle
);

/* Dequeues at most `budget` ready packets without waiting and publishes the
 * terminal completions among them. Wake packets are put back while a notified
 * native cohort remains and dropped after that cohort drains. */
int wf_windows_iocp_progress(
    wf_windows_iocp_adapter *adapter,
    size_t budget,
    size_t *published
);

/* Announces any completion-core epoch change to the port under runtime->wait.
 * Each native sleeper keeps one outstanding notification until it leaves the
 * kernel wait. Repeated notifications do not multiply its wake packets. */
void wf_windows_iocp_notify(void *context);

/* Parks one thread on the port.  `observed_epoch` is rechecked under the
 * runtime's own wait lock before the kernel wait, closing
 * completion-before-park without a polling timeout. New sleepers wait on the
 * runtime condition while an older notified cohort drains, so they cannot
 * consume that cohort's packets. UINT32_MAX means no deadline. */
int wf_windows_iocp_park(
    wf_windows_iocp_adapter *adapter,
    uint64_t observed_epoch,
    uint32_t timeout_milliseconds
);

/* A nonzero value is a sticky target-owned progress failure.  Once an
 * operation has been accepted, the bridge must fail-stop on this condition. */
int wf_windows_iocp_progress_error(const wf_windows_iocp_adapter *adapter);

size_t wf_windows_iocp_in_flight(const wf_windows_iocp_adapter *adapter);

/* Refuses with EBUSY until every accepted operation has produced and published
 * its packet. */
int wf_windows_iocp_destroy(wf_windows_iocp_adapter *adapter);

wf_windows_iocp_statistics wf_windows_iocp_statistics_snapshot(
    const wf_windows_iocp_adapter *adapter
);

#if defined(__cplusplus)
}
#endif

#endif

#endif
