/*
 * The bounded file adapter: the queue, the helpers, the claim of one's own
 * queued record, and the in-place progress pass.
 *
 * One implementation for every platform.  The one thing a platform differs in
 * is the host call each request kind makes, which is `wf_file_execute_direct`
 * and lives in one leaf per platform: `file_posix.c` and `file_windows.c`.
 * Nothing in this unit makes a host call of its own -- its threads come from
 * `wf_prim_thread_start` and its lock and condition from the platform's one
 * wait set -- so there is no `#if` here and no header of a host in it.
 */

#include "file_adapter.h"

#include <errno.h>
#include <stdint.h>
#include <string.h>
#include <time.h>

int wf_file_request_valid(const wf_file_request *request) {
    if (request == NULL) {
        return 0;
    }
    switch (request->kind) {
    case WF_FILE_OPEN_AT:
        return request->operation.open_at.path != NULL
            && request->operation.open_at.expected_kind
                <= WF_FILE_EXPECT_DIRECTORY;
    case WF_FILE_READ:
        return request->operation.read.buffer != NULL
            || request->operation.read.count == 0;
    case WF_FILE_WRITE:
        return request->operation.write.buffer != NULL
            || request->operation.write.count == 0;
    case WF_FILE_PREAD:
        return request->operation.pread.buffer != NULL
            || request->operation.pread.count == 0;
    case WF_FILE_CLOSE:
        return 1;
    /* A listen and a connect name an address and create their own socket, so
     * there is no descriptor and no buffer to check; the address is a value
     * the emitter built and every combination of its operands is one address
     * (ordinary native library). */
    case WF_FILE_SOCKET_LISTEN:
    case WF_FILE_SOCKET_CONNECT:
        return 1;
    case WF_FILE_SOCKET_ACCEPT:
        return request->operation.accept.descriptor >= 0;
    case WF_FILE_SOCKET_RECEIVE:
        return request->operation.receive.buffer != NULL
            || request->operation.receive.count == 0;
    case WF_FILE_SOCKET_SEND:
        return request->operation.send.buffer != NULL
            || request->operation.send.count == 0;
    case WF_FILE_SOCKET_SHUTDOWN:
        return request->operation.shutdown.descriptor >= 0
            && (request->operation.shutdown.direction
                    == WF_SOCKET_DIRECTION_RECEIVE
                || request->operation.shutdown.direction
                    == WF_SOCKET_DIRECTION_SEND);
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
    case WF_FILE_DIRECTORY_NEXT:
        /* The position cell is required on both families even though only
         * Darwin's facility writes it: one validity rule keeps a request
         * record that is well formed on one host well formed on the other. */
        return (request->operation.directory_next.buffer != NULL
                || request->operation.directory_next.count == 0)
            && request->operation.directory_next.position != NULL;
#endif
    default:
        return 0;
    }
}

/* Whether this request's kind waits on a peer; see the header for what the
 * four kinds have in common and why a listen and a shutdown are not among
 * them.
 *
 * It is a switch over the kind and nothing else.  A wait this adapter cannot
 * end is a property of what the request asks the host for, so knowing it needs
 * no clock, no history and no host call, and the answer is the same on the
 * first operation of a program as on its thousandth. */
int wf_file_request_is_peer_bound(const wf_file_request *request) {
    if (request == NULL) {
        return 0;
    }
    switch (request->kind) {
    case WF_FILE_SOCKET_ACCEPT:
    case WF_FILE_SOCKET_RECEIVE:
    case WF_FILE_SOCKET_CONNECT:
    case WF_FILE_SOCKET_SEND:
        return 1;
    default:
        return 0;
    }
}

/* The two-count of every connection descriptor, one byte each: zero when
 * neither direction has completed its shutdown and one when exactly one has. See
 * `wf_file_connection_release` in the header for what it promises and why it
 * is a table rather than something a record could carry.
 *
 * It is `_Atomic` because the two directions of one connection are two places
 * that may be released on two threads, and because a helper and a joining
 * scheduler both reach this from `wf_file_execute_direct`. */
static _Atomic unsigned char
    wf_file_connection_halves[WF_FILE_CONNECTION_DESCRIPTORS];

int wf_file_connection_release(int descriptor) {
    unsigned char previous;
    if (descriptor < 0
        || (unsigned)descriptor >= WF_FILE_CONNECTION_DESCRIPTORS) {
        return 0;
    }
    previous = atomic_exchange_explicit(
        &wf_file_connection_halves[descriptor],
        1u,
        memory_order_acq_rel
    );
    if (previous == 0u) {
        return 0;
    }
    /* This release closes the target's object, so the byte goes back to zero
     * before the close: the descriptor is still this program's until the
     * close returns, and a host that hands the same number out afterwards
     * finds the count where a fresh connection expects it. */
    atomic_store_explicit(
        &wf_file_connection_halves[descriptor],
        0u,
        memory_order_release
    );
    return 1;
}

/* The measured host-call time above which handing an operation to another
 * thread is worth the handoff.
 *
 * Below it the operation does not wait: the host answers from memory, and a
 * helper can only add a queue crossing and two thread wakes to work that was
 * already done by the time the wake arrived.  This is the number that decides
 * whether the pool grows at all, and it is a measurement of the program's own
 * operations rather than an assumption about the host.
 *
 * Twenty microseconds is about ten times what one handoff costs on the host
 * this was measured on — a wake on the submitting thread, a wake-up on the
 * helper, and a queue crossing.  Sizing it at the handoff itself would admit
 * every operation whose overlap merely breaks even, and a pool that breaks
 * even still costs the CPU it spends.  Every population this project has
 * measured falls clearly on one side: a warm 4 KiB read is about 1 us, a warm
 * 64 KiB read 5 to 7, and a read that reaches the device 130 or more. */
#define WF_FILE_OVERLAP_WAIT_NS 20000u

/* How often an execution is timed for the average above: one in this many.
 * The clock is read twice per sampled execution and not at all otherwise. */
#define WF_FILE_EXECUTE_SAMPLE_INTERVAL 16u

/* Records one host call's duration into the smoothed average, from one in
 * every WF_FILE_EXECUTE_SAMPLE_INTERVAL executions.  Two threads that sample
 * at once may lose one another's contribution; the average is a policy hint
 * and no operation's outcome depends on it. */
static void wf_file_note_execution(wf_file_adapter *adapter, uint64_t started) {
    uint64_t mean;
    uint64_t sample = wf_file_monotonic_ns();
    sample = sample > started ? sample - started : 0u;
    mean = atomic_load_explicit(&adapter->mean_execute_ns, memory_order_relaxed);
    mean = mean == 0 ? sample : mean - mean / 4u + sample / 4u;
    atomic_store_explicit(&adapter->mean_execute_ns, mean, memory_order_relaxed);
}

static int wf_file_execution_should_be_timed(wf_file_adapter *adapter) {
    uint64_t tick = atomic_fetch_add_explicit(
        &adapter->execute_ticks,
        1,
        memory_order_relaxed
    );
    return tick % WF_FILE_EXECUTE_SAMPLE_INTERVAL == 0;
}

/* Removes one record from the pending list, with the queue lock held.
 *
 * `previous` is the record before it, or NULL when it is the head.  The three
 * places that take a record off this list -- the scheduler's progress pass,
 * the joining thread's claim of its own, and a shutdown's drain through the
 * first of those -- all unlink through here, so the head, the tail and the
 * count are maintained in exactly one place. */
static void wf_file_unlink_locked(
    wf_file_adapter *adapter,
    wf_completion_record *previous,
    wf_completion_record *record
) {
    if (previous == NULL) {
        adapter->queue_head = record->next;
    } else {
        previous->next = record->next;
    }
    if (adapter->queue_tail == record) {
        adapter->queue_tail = previous;
    }
    record->next = NULL;
    atomic_store_explicit(
        &adapter->queue_count,
        atomic_load_explicit(&adapter->queue_count, memory_order_relaxed) - 1u,
        memory_order_seq_cst
    );
}

/* Takes one queued record off this adapter's list for a scheduler thread.
 *
 * A scheduler thread is not a helper: it has a program's own continuations to
 * run, and a host call it makes here holds all of them.  So while this adapter
 * has a helper pool to grow at all -- `helper_cap` above zero -- this pass
 * leaves every peer-bound request alone and takes a queued request no peer
 * can hold up, from the end `from_head` names.  A peer-bound wait is ended by another
 * program, so a scheduler thread inside one is a scheduler thread another
 * program has taken, and the four peers of `tests/programs/tcp_fanout.wf` are
 * exactly that: three workers each inside a receive whose peer is silent, and
 * a fourth connection nobody is left to accept.
 *
 * With `helper_cap` at zero this takes anything, because nothing else could
 * run it: a pinned zero-helper pool is an explicit policy that makes the
 * waiting thread the queue's only engine, and refusing a request there would
 * leave it with no engine at all.
 *
 * `from_head` chooses which end, among the requests this pass may take.  A
 * joining thread's visit takes the newest, so a blocked earlier host
 * facility cannot hide every later free operation behind it, and helper
 * threads take the oldest, so several target contexts spread across both ends
 * of the list.
 *
 * The list is singly linked through each record's own `next`, so both the walk
 * to the newest and the walk past the peer-bound entries are bounded by what
 * this program has outstanding, which is bounded by the frames that hold the
 * records, and cost nothing at all in the common case of one queued operation.
 * Nothing is copied out: the record is the submitting frame's and outlives the
 * operation, so the executing thread runs it in place (design §5, §7).
 */
/* Whether a queued request is left to the helpers: its kind waits on a peer,
 * or the program runs contexts that may be its peer.  The caller holds the
 * queue lock, under which `hold_for_contexts` is written. */
static int wf_file_request_held_locked(
    const wf_file_adapter *adapter,
    const wf_file_request *request
) {
    return adapter->hold_for_contexts != 0 || wf_file_request_is_peer_bound(request);
}

static wf_completion_record *wf_file_take_work(
    wf_file_adapter *adapter,
    int from_head
) {
    wf_completion_record *previous = NULL;
    wf_completion_record *taken = NULL;
    wf_completion_wait_lock(&adapter->queue_wait);
    if (adapter->helper_cap != 0) {
        wf_completion_record *before = NULL;
        wf_completion_record *scan = adapter->queue_head;
        while (scan != NULL) {
            if (!wf_file_request_held_locked(adapter, &scan->request)) {
                previous = before;
                taken = scan;
                if (from_head != 0) {
                    break;
                }
            }
            before = scan;
            scan = scan->next;
        }
    } else if (adapter->queue_head != NULL) {
        if (from_head != 0 || adapter->queue_head == adapter->queue_tail) {
            taken = adapter->queue_head;
        } else {
            previous = adapter->queue_head;
            while (previous->next != adapter->queue_tail) {
                previous = previous->next;
            }
            taken = adapter->queue_tail;
        }
    }
    if (taken != NULL) {
        wf_file_unlink_locked(adapter, previous, taken);
    }
    wf_completion_wait_unlock(&adapter->queue_wait);
    return taken;
}

/* Whether this adapter's record has been published, read the way a thread
 * that ran no initialization of its own has to read it.
 *
 * The direct execution route reaches `wf_file_execute_timed` straight from a
 * generated call, with no once-control of its own, while another thread may
 * be inside `wf_file_adapter_init`.  Pairing this acquire load with the
 * release store init ends on is what makes the rest of the record safe to
 * read for a thread that finds it set, and makes the read itself a
 * synchronizing one rather than a race. */
static int wf_file_adapter_initialized(const wf_file_adapter *adapter) {
    return adapter != NULL
        && atomic_load_explicit(&adapter->initialized, memory_order_acquire)
            != 0;
}

/* Records that one operation this adapter owned has finished its host work.
 *
 * A refused open asks the process-wide ledger, not this adapter, whether a
 * descriptor came back, so every operation reports there as well as into this
 * adapter's own execution counts.  That is the whole of what makes the
 * exhaustion rule one rule: an open refused on the kernel ring can see a read
 * finishing on a helper thread here, and this open can see that ring's close.
 *
 * The report says both things the rule asks for: that this operation has ended,
 * and whether its ending put a descriptor back.  A read finishing here ends
 * without returning one, so it is not a reason for an open refused on the ring
 * to spend its single re-attempt.
 */
static void wf_file_finish_execution(
    wf_file_adapter *adapter,
    int helper
) {
    atomic_fetch_add_explicit(
        helper != 0 ? &adapter->stat_helper_executions
                    : &adapter->stat_scheduler_executions,
        1,
        memory_order_relaxed
    );
}


wf_file_result wf_file_execute_timed(
    wf_file_adapter *adapter,
    wf_file_request *request
) {
    int timed;
    uint64_t started;
    wf_file_result result;
    if (!wf_file_adapter_initialized(adapter)) {
        return wf_file_execute_direct(request);
    }
    /* A peer-bound execution is not part of the population this average is
     * about.  The average answers one question -- whether this program's own
     * host calls wait long enough for a handoff to be worth its cost -- and a
     * receive that sits for seconds because its peer is quiet answers a
     * different one: it measures the peer, not the host.  Letting one in would
     * make the verdict LONG for every file operation that followed it, and the
     * verdict is what decides whether an ordinary read is queued or executed
     * where it was stated.  The kind test comes first, so a peer-bound
     * execution does not advance the sample counter either: the sample is one
     * in sixteen of the executions the average is about. */
    timed = !wf_file_request_is_peer_bound(request)
        && wf_file_execution_should_be_timed(adapter);
    started = timed ? wf_file_monotonic_ns() : 0u;
    result = wf_file_execute_direct(request);
    if (timed) {
        wf_file_note_execution(adapter, started);
    }
    return result;
}

enum wf_file_wait_verdict wf_file_adapter_wait_verdict(
    const wf_file_adapter *adapter
) {
    uint64_t mean;
    if (!wf_file_adapter_initialized(adapter)) {
        return WF_FILE_WAIT_UNMEASURED;
    }
    mean = atomic_load_explicit(
        &adapter->mean_execute_ns,
        memory_order_relaxed
    );
    if (mean == 0) {
        return WF_FILE_WAIT_UNMEASURED;
    }
    return mean >= WF_FILE_OVERLAP_WAIT_NS ? WF_FILE_WAIT_LONG
                                           : WF_FILE_WAIT_SHORT;
}

int wf_file_adapter_transfer_runs_on_caller(const wf_file_adapter *adapter) {
    if (wf_file_adapter_wait_verdict(adapter) != WF_FILE_WAIT_SHORT
        || wf_file_adapter_helper_count(adapter) != 0) {
        return 0;
    }
    return wf_file_adapter_queued(adapter) == 0;
}

/* Publishing DONE is the engine's last access to the caller's record. */
void wf_file_complete_record(
    wf_completion_record *record,
    const wf_file_result *result
) {
    record->result = result->head;
    wf_completion_record_complete(record);
}

static void wf_file_run_work(
    wf_file_adapter *adapter,
    wf_completion_record *record,
    int helper
) {
    wf_file_result result = wf_file_execute_timed(adapter, &record->request);
    wf_file_finish_execution(adapter, helper);
    wf_file_complete_record(record, &result);
}

static void wf_file_helper_main(void *context) {
    wf_file_adapter *adapter = context;
    for (;;) {
        wf_completion_record *record;
        wf_completion_wait_lock(&adapter->queue_wait);
        while (adapter->queue_head == NULL && adapter->stopping == 0) {
            adapter->blocked_helpers += 1;
            (void)wf_completion_wait_sleep(&adapter->queue_wait, UINT32_MAX);
            adapter->blocked_helpers -= 1;
        }
        if (adapter->queue_head == NULL && adapter->stopping != 0) {
            /* The last act of a helper, under the lock a shutdown is waiting
             * on: drop the live count and wake whoever is waiting for it to
             * reach zero.  Shutdown's own wait returns holding this lock, so
             * by the time it can tear the wait set down every helper has
             * already released it. */
            adapter->live_helpers -= 1u;
            wf_completion_wait_wake(&adapter->queue_wait, 1);
            wf_completion_wait_unlock(&adapter->queue_wait);
            return;
        }
        record = adapter->queue_head;
        adapter->queue_head = record->next;
        if (adapter->queue_head == NULL) {
            adapter->queue_tail = NULL;
        }
        record->next = NULL;
        atomic_store_explicit(
            &adapter->queue_count,
            atomic_load_explicit(&adapter->queue_count, memory_order_relaxed)
                - 1u,
            memory_order_seq_cst
        );
        wf_completion_wait_unlock(&adapter->queue_wait);
        wf_file_run_work(adapter, record, 1);
    }
}

/* Waits, with the queue lock held, until every helper that started has
 * returned.  Called by a shutdown and by a failed start, which are the two
 * places a pool has to be gone before its wait set can be torn down.
 *
 * There is no join by handle here because there is no handle: every thread
 * this runtime starts is detached (`wf_prim_thread_start`).  What replaces it
 * is a count a helper drops as its last act under this same lock, so a wait
 * that returns holding the lock has seen every helper release it. */
static void wf_file_await_helpers(wf_file_adapter *adapter) {
    while (adapter->live_helpers != 0u) {
        (void)wf_completion_wait_sleep(&adapter->queue_wait, UINT32_MAX);
    }
}

/* Starts one helper, with the queue lock held.  The live count goes up before
 * the thread exists, so a start that fails puts it back rather than leaving a
 * shutdown waiting for a thread that was never made. */
static int wf_file_start_helper_locked(wf_file_adapter *adapter) {
    size_t held = atomic_load_explicit(
        &adapter->helper_count,
        memory_order_relaxed
    );
    if (held >= WF_FILE_MAX_HELPERS) {
        return 1;
    }
    adapter->live_helpers += 1u;
    if (wf_prim_thread_start(
            &adapter->helper_threads[held],
            wf_file_helper_main,
            adapter,
            0
        ) != 0) {
        adapter->live_helpers -= 1u;
        return 1;
    }
    return 0;
}

int wf_file_adapter_init(
    wf_file_adapter *adapter,
    wf_completion_runtime *runtime,
    size_t helper_capacity,
    size_t helper_count
) {
    size_t created = 0;
    int error;

    if (adapter == NULL || runtime == NULL || helper_count > helper_capacity
        || helper_capacity > WF_FILE_MAX_HELPERS) {
        return EINVAL;
    }
    /* Cleared before anything else is written, so that a record left half
     * built by a failure below is never mistaken for a usable one, and set
     * again only when every field is in place.
     *
     * That pair of stores is what makes the record safe to read, and it is the
     * only thing that does.  The field-by-field assignment below is not:
     * `atomic_init` is a plain write by definition, and adjacent plain writes
     * may be merged.  What excludes the race is that no reader touches any of
     * these fields without first passing `wf_file_adapter_initialized`, whose
     * acquire load pairs with the release store this function ends on.
     *
     * A reader can therefore see one of these writes only if the record it
     * already holds is initialized *again* underneath it, which this adapter's
     * contract does not allow and the bridge never does: it initializes once,
     * under a once-control, with the flag still zero. */
    atomic_store_explicit(&adapter->initialized, 0, memory_order_release);
    adapter->runtime = runtime;
    adapter->queue_head = NULL;
    adapter->queue_tail = NULL;
    atomic_init(&adapter->queue_count, 0);
    atomic_init(&adapter->helper_count, 0);
    atomic_init(&adapter->mean_execute_ns, 0);
    atomic_init(&adapter->execute_ticks, 0);
    adapter->blocked_helpers = 0;
    adapter->live_helpers = 0;
    adapter->stopping = 0;
    adapter->hold_for_contexts = 0;
    adapter->helper_capacity = helper_capacity;
    adapter->helper_cap = helper_count;
    atomic_init(&adapter->stat_submissions, 0);
    atomic_init(&adapter->stat_helper_executions, 0);
    atomic_init(&adapter->stat_scheduler_executions, 0);

    error = wf_completion_wait_init(&adapter->queue_wait);
    if (error != 0) {
        return error;
    }
    /* Every field above is published by this one store, and nothing reads the
     * record without the acquire load that pairs with it. */
    atomic_store_explicit(&adapter->initialized, 1, memory_order_release);

    for (created = 0; created < helper_count; ++created) {
        wf_completion_wait_lock(&adapter->queue_wait);
        error = wf_file_start_helper_locked(adapter);
        if (error != 0) {
            adapter->stopping = 1;
            wf_completion_wait_wake(&adapter->queue_wait, 1);
            wf_file_await_helpers(adapter);
            wf_completion_wait_unlock(&adapter->queue_wait);
            (void)wf_completion_wait_destroy(&adapter->queue_wait);
            atomic_store_explicit(
                &adapter->initialized,
                0,
                memory_order_release
            );
            return EAGAIN;
        }
        wf_completion_wait_unlock(&adapter->queue_wait);
        atomic_store_explicit(
            &adapter->helper_count,
            created + 1,
            memory_order_release
        );
    }
    return 0;
}

int wf_file_adapter_hold_for_contexts(wf_file_adapter *adapter) {
    if (!wf_file_adapter_initialized(adapter)) {
        return EINVAL;
    }
    wf_completion_wait_lock(&adapter->queue_wait);
    adapter->hold_for_contexts = 1u;
    wf_completion_wait_unlock(&adapter->queue_wait);
    return 0;
}

int wf_file_adapter_set_helper_cap(wf_file_adapter *adapter, size_t cap) {
    if (!wf_file_adapter_initialized(adapter)) {
        return EINVAL;
    }
    /* The policy asks for a ceiling; the caller's storage decides how much of
     * it exists.  A cap above the bound init was given would let the pool grow
     * past the number of helpers this adapter was told it may ever hold. */
    if (cap > adapter->helper_capacity) {
        cap = adapter->helper_capacity;
    }
    wf_completion_wait_lock(&adapter->queue_wait);
    adapter->helper_cap = cap;
    wf_completion_wait_unlock(&adapter->queue_wait);
    return 0;
}

size_t wf_file_adapter_helper_count(const wf_file_adapter *adapter) {
    if (!wf_file_adapter_initialized(adapter)) {
        return 0;
    }
    return atomic_load_explicit(&adapter->helper_count, memory_order_acquire);
}

/* Adds one helper when the queue holds more accepted requests than there are
 * helpers to take them **and** this program's operations wait long enough for
 * another thread to be worth its handoff.
 *
 * Queue depth alone is not enough, and measuring it was what made this pool
 * cost more than it saved.  Depth says a program stated width; it does not say
 * the width is over anything.  A program reading a warm page cache states the
 * same eight independent reads as one reading a device and exposes the same
 * queue, but each of its reads is a memory copy that is finished before a
 * woken helper is scheduled — so every helper the depth rule added there
 * bought a wake and a queue crossing and overlapped nothing.  The measured
 * host-call time is the missing half of the condition, and it is a
 * measurement of the program's own operations rather than a guess about the
 * host.
 *
 * A rule that instead grew whenever a submission found no helper *waiting*
 * was tried and measured worse: a helper that has been signalled but not yet
 * scheduled still counts as waiting, so a run of consecutive submissions sees
 * one available helper every time and the pool never grows at all. On the
 * quiet macOS host that left the four-wide program at 919 ms against 625 ms
 * for the depth rule.  Queue depth is a lagging signal, but it is a true one.
 *
 * Growth starts from no helpers at all, because a program whose operations do
 * not wait wants none: with an empty pool the waiting scheduler is the queue's
 * engine and the completion path costs a queue crossing and nothing else.  It
 * runs on the submitting thread with the queue lock already held, so at most
 * one helper appears per submission and the count never passes the policy's
 * cap. A pinned helper policy sets the cap equal to the initial count, making
 * this a no-op.
 *
 * A peer-bound submission takes neither of those two terms, and that is the
 * rule this pool needs for sockets.  Both terms are about a wait this adapter
 * has already seen: the verdict is a measurement of executions that have
 * finished, and the depth is a queue that has already outrun the pool.
 * Neither can see the wait a receive is about to make, because that wait has
 * not started and is ended by another program.  The kind says it outright, so
 * a peer-bound submission grows a helper whenever there is room in the pool
 * for one, and `n` connections open at once get `min(n, helper_cap)` helpers
 * to wait in.
 *
 * The depth term in particular would defeat the whole rule: the second
 * concurrent accept finds one helper held and one entry queued, so depth would
 * refuse it and the two accepts would run one after the other -- on a pool
 * whose one helper is inside the first of them, waiting for a peer that may
 * speak last.  The cost of dropping both terms is a pool that can reach its
 * cap for a program whose sockets are used strictly one at a time; those
 * helpers sleep on the queue condition and the cap is `WF_BRIDGE_MAX_HELPERS`,
 * so it is a bounded and idle cost against a class of program that otherwise
 * cannot make progress at all. */
static void wf_file_grow_helpers_locked(
    wf_file_adapter *adapter,
    int peer_bound
) {
    size_t held = atomic_load_explicit(
        &adapter->helper_count,
        memory_order_relaxed
    );
    if (held >= adapter->helper_cap) {
        return;
    }
    if (peer_bound == 0) {
        if (adapter->queue_count <= held) {
            return;
        }
        if (wf_file_adapter_wait_verdict(adapter) != WF_FILE_WAIT_LONG) {
            return;
        }
    }
    if (wf_file_start_helper_locked(adapter) != 0) {
        return;
    }
    atomic_store_explicit(
        &adapter->helper_count,
        held + 1,
        memory_order_release
    );
}

/* Appends one record to the pending list and grows the pool when the list has
 * outrun it.  The caller holds the queue lock.
 *
 * There is no capacity here to answer: the list is threaded through the
 * records, each of which is a block of the frame that submitted it, so the
 * only bound is how many operations the program's own frames can hold
 * (design §7).
 *
 * Returns whether a sleeping helper has to be woken for it.  The wake itself
 * is the caller's, after the lock: a signal issued while holding the queue
 * lock wakes a helper whose very next act is to block on that same lock, so
 * the submission pays a system call to start a thread it then stalls. */
static int wf_file_enqueue_locked(
    wf_file_adapter *adapter,
    wf_completion_record *record
) {
    int wake;
    record->next = NULL;
    if (adapter->queue_tail == NULL) {
        adapter->queue_head = record;
    } else {
        adapter->queue_tail->next = record;
    }
    adapter->queue_tail = record;
    atomic_store_explicit(
        &adapter->queue_count,
        atomic_load_explicit(&adapter->queue_count, memory_order_relaxed) + 1u,
        memory_order_seq_cst
    );
    atomic_fetch_add_explicit(
        &adapter->stat_submissions,
        1,
        memory_order_relaxed
    );
    /* One newly queued request needs exactly one helper woken, and only a
     * helper that is actually asleep needs a host wake at all.  Announcing to
     * every helper would cost a wake per helper per submission — a thundering
     * herd rather than progress.  This lock is the one a sleeper counts itself
     * under, so the answer read here is exact rather than a guess; a helper
     * that wakes on its own between here and the caller's signal only makes
     * that signal a spurious one, which its predicate loop already tolerates. */
    wake = adapter->blocked_helpers != 0;
    wf_file_grow_helpers_locked(
        adapter,
        wf_file_request_held_locked(adapter, &record->request)
    );
    return wake;
}

enum wf_file_submit_result wf_file_adapter_submit(
    wf_file_adapter *adapter,
    wf_completion_record *record
) {
    int wake;

    if (!wf_file_adapter_initialized(adapter) || record == NULL
        || !wf_file_request_valid(&record->request)) {
        return WF_FILE_SUBMIT_INVALID;
    }
    record->route = WF_COMPLETION_ROUTE_FILE_ADAPTER;
    wf_completion_wait_lock(&adapter->queue_wait);
    if (adapter->stopping != 0) {
        wf_completion_wait_unlock(&adapter->queue_wait);
        return WF_FILE_ADAPTER_STOPPING;
    }
    wake = wf_file_enqueue_locked(adapter, record);
    wf_completion_wait_unlock(&adapter->queue_wait);
    if (wake != 0) {
        wf_completion_wait_wake(&adapter->queue_wait, 0);
    }
    return WF_FILE_TARGET_OWNS;
}

/* Takes this exact record off the queue for the thread that is waiting on it.
 *
 * This is the one visit to the queue a joining thread may make while helpers
 * exist: the rule that otherwise keeps it out is about *unrelated* work, which
 * could block the very frame waiting on a completion a helper has already
 * published.  Its own record cannot be that, because this thread has nothing
 * left to do until that record is DONE, and running it here is the same host
 * call the helper would have made without a queue crossing and without waiting
 * behind whatever the helpers are already inside.
 *
 * Answering 0 covers every other case with no special reasoning: a record the
 * ring owns, one a helper has already taken, and one already published are all
 * simply not on this list.
 *
 * A claimed record is the caller's to run, and the caller must run it: it is
 * off the queue and no other engine will publish it. */
int wf_file_adapter_claim_own(
    wf_file_adapter *adapter,
    wf_completion_record *record
) {
    int claimed = 0;
    if (!wf_file_adapter_initialized(adapter) || record == NULL) {
        return 0;
    }
    wf_completion_wait_lock(&adapter->queue_wait);
    if (adapter->queue_head == record) {
        wf_file_unlink_locked(adapter, NULL, record);
        claimed = 1;
    } else {
        wf_completion_record *previous = adapter->queue_head;
        while (previous != NULL && previous->next != record) {
            previous = previous->next;
        }
        if (previous != NULL) {
            wf_file_unlink_locked(adapter, previous, record);
            claimed = 1;
        }
    }
    wf_completion_wait_unlock(&adapter->queue_wait);
    return claimed;
}

/* Runs a record `wf_file_adapter_claim_own` answered 1 for, and publishes it.
 *
 * It is the same execution a helper would have made, timed by the same
 * adapter, so the measurement of what this program's operations cost keeps
 * running on the route a joining thread took. */
void wf_file_adapter_run_claimed(
    wf_file_adapter *adapter,
    wf_completion_record *record
) {
    wf_file_run_work(adapter, record, 0);
}

/* Executes at most `budget` queued requests on the calling thread, taking
 * only what `wf_file_take_work` will hand a scheduler thread.
 *
 * There is no second attempt for what it leaves behind.  A peer-bound request
 * this pass skips waits in the queue for a helper, and nothing runs it on a
 * scheduler thread when the pool is already at its cap: a fallback that did
 * would put a scheduler thread back inside a wait another program ends, which
 * is the whole defect this rule removes, and it would do it at exactly the
 * moment the program is widest.
 *
 * So on a host with no kernel completion ring, the number of socket operations
 * this route can have waiting at once is `WF_BRIDGE_MAX_HELPERS`, and a
 * program that needs more of them in flight than that waits.  That bound is a
 * property of an engine built out of blocking host calls and threads to make
 * them on.  The engine that removes it is a readiness-driven adapter -- one
 * `poll`, `kqueue` or `WSAPoll` over the descriptors of every queued request,
 * made inside the park a thread with nothing to run already enters, so waiting
 * for any number of peers costs one thread and no host call per peer.  That is
 * the ring-less host's proper engine and it is the open design item here; it
 * is not this change, which is the kind rule that makes the present engine
 * correct within its bound. */
size_t wf_file_adapter_progress(wf_file_adapter *adapter, size_t budget) {
    size_t executed = 0;
    if (!wf_file_adapter_initialized(adapter)) {
        return 0;
    }
    while (executed < budget) {
        wf_completion_record *record = wf_file_take_work(adapter, 0);
        if (record == NULL) {
            break;
        }
        wf_file_run_work(adapter, record, 0);
        executed += 1;
    }
    return executed;
}

size_t wf_file_adapter_queued(const wf_file_adapter *adapter) {
    if (!wf_file_adapter_initialized(adapter)) {
        return 0;
    }
    return atomic_load_explicit(&adapter->queue_count, memory_order_seq_cst);
}

int wf_file_adapter_shutdown(wf_file_adapter *adapter) {
    size_t held;
    int first_error = 0;
    if (!wf_file_adapter_initialized(adapter)) {
        return EINVAL;
    }
    wf_completion_wait_lock(&adapter->queue_wait);
    adapter->stopping = 1;
    /* Growth stops with admission, so the count read below is final. */
    adapter->helper_cap = 0;
    wf_completion_wait_wake(&adapter->queue_wait, 1);
    wf_completion_wait_unlock(&adapter->queue_wait);

    held = atomic_load_explicit(&adapter->helper_count, memory_order_acquire);
    if (held == 0) {
        while (wf_file_adapter_progress(adapter, 1) != 0) {
        }
    }
    wf_completion_wait_lock(&adapter->queue_wait);
    wf_file_await_helpers(adapter);
    wf_completion_wait_unlock(&adapter->queue_wait);
    /* The record stops answering "usable" before the lock behind that answer
     * is destroyed, not after.
     *
     * Every reader of this adapter passes `wf_file_adapter_initialized` first
     * and then uses storage this teardown destroys: the queue's wait set.  The
     * queue count is not among them — it is
     * an atomic this teardown never writes, which is what lets
     * `wf_file_adapter_queued` be read with no lock at all, for the check
     * `wf_file_adapter_transfer_runs_on_caller` makes.  Storing zero after the
     * destroy would leave a window in which that guard still answers yes and
     * the wait set it guards no longer exists, so a reader admitted through it
     * locks destroyed storage.  Storing zero first closes the window to the
     * ordinary one this function's precondition already covers: a reader that
     * passed the guard before this store.
     *
     * It is stored whatever the wait for the helpers and the destroy reported.
     * A record whose wait set has been destroyed is not usable, and saying so
     * is not conditional on the teardown having been clean. */
    atomic_store_explicit(&adapter->initialized, 0, memory_order_release);
    {
        int error = wf_completion_wait_destroy(&adapter->queue_wait);
        if (first_error == 0 && error != 0) {
            first_error = error;
        }
    }
    return first_error;
}

wf_file_adapter_statistics wf_file_adapter_statistics_snapshot(
    const wf_file_adapter *adapter
) {
    wf_file_adapter_statistics statistics = {0};
    if (adapter == NULL) {
        return statistics;
    }
    statistics.submissions = atomic_load_explicit(
        &adapter->stat_submissions,
        memory_order_relaxed
    );
    statistics.helper_executions = atomic_load_explicit(
        &adapter->stat_helper_executions,
        memory_order_relaxed
    );
    statistics.scheduler_executions = atomic_load_explicit(
        &adapter->stat_scheduler_executions,
        memory_order_relaxed
    );
    return statistics;
}
