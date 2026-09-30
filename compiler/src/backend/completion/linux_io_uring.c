#define _GNU_SOURCE

#include "linux_io_uring.h"

#if defined(__linux__)

#include <errno.h>
#include <limits.h>
#include <netinet/tcp.h>
#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/epoll.h>
#include <sys/eventfd.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <unistd.h>

static unsigned wf_linux_load_acquire(const unsigned *value) {
    return __atomic_load_n(value, __ATOMIC_ACQUIRE);
}

static unsigned wf_linux_load_relaxed(const unsigned *value) {
    return __atomic_load_n(value, __ATOMIC_RELAXED);
}

static void wf_linux_store_release(unsigned *target, unsigned value) {
    __atomic_store_n(target, value, __ATOMIC_RELEASE);
}

static int wf_linux_enter(
    int ring_descriptor,
    unsigned submissions,
    unsigned minimum_completions,
    unsigned flags
) {
    long result = syscall(
        __NR_io_uring_enter,
        ring_descriptor,
        submissions,
        minimum_completions,
        flags,
        NULL,
        0u
    );
    if (result < 0) {
        return -errno;
    }
    return (int)result;
}

static void wf_linux_unmap(wf_linux_io_uring_adapter *adapter) {
    if (adapter->submission_entries != MAP_FAILED
        && adapter->submission_entries != NULL) {
        (void)munmap(
            adapter->submission_entries,
            adapter->submission_entries_size
        );
    }
    if (adapter->completion_mapping != MAP_FAILED
        && adapter->completion_mapping != NULL
        && adapter->completion_mapping != adapter->submission_mapping) {
        (void)munmap(
            adapter->completion_mapping,
            adapter->completion_mapping_size
        );
    }
    if (adapter->submission_mapping != MAP_FAILED
        && adapter->submission_mapping != NULL) {
        (void)munmap(
            adapter->submission_mapping,
            adapter->submission_mapping_size
        );
    }
    adapter->submission_entries = NULL;
    adapter->submission_mapping = NULL;
    adapter->completion_mapping = NULL;
}

enum wf_linux_wait_source {
    WF_LINUX_WAIT_WAKE = 1,
    WF_LINUX_WAIT_CQ = 2
};

static void wf_linux_record_progress_error(
    wf_linux_io_uring_adapter *adapter,
    int error
) {
    int expected = 0;
    if (error == 0) {
        return;
    }
    (void)atomic_compare_exchange_strong_explicit(
        &adapter->progress_error,
        &expected,
        error,
        memory_order_release,
        memory_order_relaxed
    );
    /* The error is target progress too. Advancing the core epoch closes an
     * error-before-park race, while the core emits an eventfd write only when
     * a scheduler has actually announced sleep. */
    wf_completion_notify_target(adapter->runtime);
}

static void wf_linux_close_wait_set(wf_linux_io_uring_adapter *adapter) {
    if (adapter->wake_descriptor >= 0) {
        (void)close(adapter->wake_descriptor);
        adapter->wake_descriptor = -1;
    }
    if (adapter->wait_descriptor >= 0) {
        (void)close(adapter->wait_descriptor);
        adapter->wait_descriptor = -1;
    }
}

static int wf_linux_init_wait_set(wf_linux_io_uring_adapter *adapter) {
    struct epoll_event event;
    adapter->wait_descriptor = epoll_create1(EPOLL_CLOEXEC);
    if (adapter->wait_descriptor < 0) {
        return errno;
    }
    adapter->wake_descriptor = eventfd(0, EFD_CLOEXEC | EFD_NONBLOCK);
    if (adapter->wake_descriptor < 0) {
        int error = errno;
        wf_linux_close_wait_set(adapter);
        return error;
    }

    memset(&event, 0, sizeof(event));
    event.events = EPOLLIN;
    event.data.u32 = WF_LINUX_WAIT_WAKE;
    if (epoll_ctl(
            adapter->wait_descriptor,
            EPOLL_CTL_ADD,
            adapter->wake_descriptor,
            &event
        ) != 0) {
        int error = errno;
        wf_linux_close_wait_set(adapter);
        return error;
    }
    memset(&event, 0, sizeof(event));
    event.events = EPOLLIN;
    event.data.u32 = WF_LINUX_WAIT_CQ;
    if (epoll_ctl(
            adapter->wait_descriptor,
            EPOLL_CTL_ADD,
            adapter->ring_descriptor,
            &event
        ) != 0) {
        int error = errno;
        wf_linux_close_wait_set(adapter);
        return error;
    }
    return 0;
}

static void wf_linux_init_statistics(wf_linux_io_uring_adapter *adapter) {
    atomic_init(&adapter->stat_submissions, 0);
    atomic_init(&adapter->stat_submission_enters, 0);
    atomic_init(&adapter->stat_completions, 0);
    atomic_init(&adapter->stat_enter_failures, 0);
    atomic_init(&adapter->stat_kernel_waits, 0);
    atomic_init(&adapter->stat_kernel_wakes, 0);
    atomic_init(&adapter->stat_host_wake_writes, 0);
    atomic_init(&adapter->stat_overflow_flushes, 0);
    atomic_init(&adapter->progress_error, 0);
}

int wf_linux_io_uring_init(
    wf_linux_io_uring_adapter *adapter,
    wf_completion_runtime *runtime,
    size_t depth,
    size_t completions
) {
    struct io_uring_params parameters;
    size_t submission_size;
    size_t completion_size;
    int error;

    if (adapter == NULL || runtime == NULL || depth == 0
        || depth > UINT_MAX || completions < depth
        || completions > UINT_MAX) {
        return EINVAL;
    }
    memset(adapter, 0, sizeof(*adapter));
    adapter->ring_descriptor = -1;
    adapter->wait_descriptor = -1;
    adapter->wake_descriptor = -1;
    adapter->submission_mapping = MAP_FAILED;
    adapter->completion_mapping = MAP_FAILED;
    adapter->submission_entries = MAP_FAILED;
    adapter->runtime = runtime;
    adapter->depth = depth;
    atomic_init(&adapter->in_flight, 0);
    wf_linux_init_statistics(adapter);

    memset(&parameters, 0, sizeof(parameters));
    /* The completion queue is sized by the caller rather than left at the
     * kernel's twice-the-depth default, because the depth is a throughput
     * choice about submission and the queue's size is a bound on how many
     * completions can be posted before the overflow path. */
    /* IORING_SETUP_COOP_TASKRUN: a completion the kernel finishes on the
     * submitting thread's behalf (a receive whose poll fired) is posted when
     * that thread next enters the kernel, which every scheduler thread does
     * within a bounded spin, instead of by an inter-processor interrupt that
     * stops whatever the thread was running.  Measured on the TCP echo
     * control test at 64 connections it is the difference between 200 to
     * 225 thousand and 240 to 250 thousand round trips a second, with the
     * same parks and the same enters (`research/investigations/io-model/RESULTS.md`,
     * the batch 0108 section).  A kernel before 5.19 refuses the flag with EINVAL, and
     * the ring is then made without it. */
    parameters.flags = IORING_SETUP_CQSIZE | IORING_SETUP_COOP_TASKRUN;
    parameters.cq_entries = (unsigned)completions;
    adapter->ring_descriptor = (int)syscall(
        __NR_io_uring_setup,
        (unsigned)depth,
        &parameters
    );
    if (adapter->ring_descriptor < 0 && errno == EINVAL) {
        memset(&parameters, 0, sizeof(parameters));
        parameters.flags = IORING_SETUP_CQSIZE;
        parameters.cq_entries = (unsigned)completions;
        adapter->ring_descriptor = (int)syscall(
            __NR_io_uring_setup,
            (unsigned)depth,
            &parameters
        );
    }
    if (adapter->ring_descriptor < 0) {
        error = errno;
        adapter->ring_descriptor = -1;
        return error;
    }
    if ((parameters.features & IORING_FEAT_NODROP) == 0
        || parameters.sq_entries < depth
        || parameters.cq_entries < completions) {
        (void)close(adapter->ring_descriptor);
        adapter->ring_descriptor = -1;
        return ENOTSUP;
    }

    submission_size = parameters.sq_off.array
        + parameters.sq_entries * sizeof(unsigned);
    completion_size = parameters.cq_off.cqes
        + parameters.cq_entries * sizeof(struct io_uring_cqe);
    if ((parameters.features & IORING_FEAT_SINGLE_MMAP) != 0) {
        size_t shared_size = submission_size > completion_size
            ? submission_size
            : completion_size;
        adapter->submission_mapping = mmap(
            NULL,
            shared_size,
            PROT_READ | PROT_WRITE,
            MAP_SHARED | MAP_POPULATE,
            adapter->ring_descriptor,
            IORING_OFF_SQ_RING
        );
        if (adapter->submission_mapping == MAP_FAILED) {
            error = errno;
            (void)close(adapter->ring_descriptor);
            adapter->ring_descriptor = -1;
            return error;
        }
        adapter->submission_mapping_size = shared_size;
        adapter->completion_mapping = adapter->submission_mapping;
        adapter->completion_mapping_size = shared_size;
    } else {
        adapter->submission_mapping = mmap(
            NULL,
            submission_size,
            PROT_READ | PROT_WRITE,
            MAP_SHARED | MAP_POPULATE,
            adapter->ring_descriptor,
            IORING_OFF_SQ_RING
        );
        if (adapter->submission_mapping == MAP_FAILED) {
            error = errno;
            (void)close(adapter->ring_descriptor);
            adapter->ring_descriptor = -1;
            return error;
        }
        adapter->submission_mapping_size = submission_size;
        adapter->completion_mapping = mmap(
            NULL,
            completion_size,
            PROT_READ | PROT_WRITE,
            MAP_SHARED | MAP_POPULATE,
            adapter->ring_descriptor,
            IORING_OFF_CQ_RING
        );
        if (adapter->completion_mapping == MAP_FAILED) {
            error = errno;
            wf_linux_unmap(adapter);
            (void)close(adapter->ring_descriptor);
            adapter->ring_descriptor = -1;
            return error;
        }
        adapter->completion_mapping_size = completion_size;
    }

    adapter->submission_entries_size = parameters.sq_entries
        * sizeof(struct io_uring_sqe);
    adapter->submission_entries = mmap(
        NULL,
        adapter->submission_entries_size,
        PROT_READ | PROT_WRITE,
        MAP_SHARED | MAP_POPULATE,
        adapter->ring_descriptor,
        IORING_OFF_SQES
    );
    if (adapter->submission_entries == MAP_FAILED) {
        error = errno;
        wf_linux_unmap(adapter);
        (void)close(adapter->ring_descriptor);
        adapter->ring_descriptor = -1;
        return error;
    }

#define WF_RING_POINTER(mapping, offset, type)                                \
    ((type *)((unsigned char *)(mapping) + (offset)))
    adapter->submission_head = WF_RING_POINTER(
        adapter->submission_mapping,
        parameters.sq_off.head,
        unsigned
    );
    adapter->submission_tail = WF_RING_POINTER(
        adapter->submission_mapping,
        parameters.sq_off.tail,
        unsigned
    );
    adapter->submission_mask = WF_RING_POINTER(
        adapter->submission_mapping,
        parameters.sq_off.ring_mask,
        unsigned
    );
    adapter->submission_count = WF_RING_POINTER(
        adapter->submission_mapping,
        parameters.sq_off.ring_entries,
        unsigned
    );
    adapter->submission_array = WF_RING_POINTER(
        adapter->submission_mapping,
        parameters.sq_off.array,
        unsigned
    );
    adapter->submission_flags = WF_RING_POINTER(
        adapter->submission_mapping,
        parameters.sq_off.flags,
        unsigned
    );
    adapter->completion_head = WF_RING_POINTER(
        adapter->completion_mapping,
        parameters.cq_off.head,
        unsigned
    );
    adapter->completion_tail = WF_RING_POINTER(
        adapter->completion_mapping,
        parameters.cq_off.tail,
        unsigned
    );
    adapter->completion_mask = WF_RING_POINTER(
        adapter->completion_mapping,
        parameters.cq_off.ring_mask,
        unsigned
    );
    adapter->completion_count = WF_RING_POINTER(
        adapter->completion_mapping,
        parameters.cq_off.ring_entries,
        unsigned
    );
    adapter->completion_overflow = WF_RING_POINTER(
        adapter->completion_mapping,
        parameters.cq_off.overflow,
        unsigned
    );
    adapter->completion_entries = WF_RING_POINTER(
        adapter->completion_mapping,
        parameters.cq_off.cqes,
        struct io_uring_cqe
    );
#undef WF_RING_POINTER

    error = pthread_mutex_init(&adapter->submission_lock, NULL);
    if (error != 0) {
        wf_linux_unmap(adapter);
        (void)close(adapter->ring_descriptor);
        adapter->ring_descriptor = -1;
        return error;
    }
    error = pthread_mutex_init(&adapter->completion_lock, NULL);
    if (error != 0) {
        (void)pthread_mutex_destroy(&adapter->submission_lock);
        wf_linux_unmap(adapter);
        (void)close(adapter->ring_descriptor);
        adapter->ring_descriptor = -1;
        return error;
    }
    error = wf_linux_init_wait_set(adapter);
    if (error != 0) {
        (void)pthread_mutex_destroy(&adapter->completion_lock);
        (void)pthread_mutex_destroy(&adapter->submission_lock);
        wf_linux_unmap(adapter);
        (void)close(adapter->ring_descriptor);
        adapter->ring_descriptor = -1;
        return error;
    }
    adapter->initialized = 1;
    return 0;
}

/* Whether this ring has a form for one record's request kind.
 *
 * Asked by the bridge before the record is handed over, so a kind the ring
 * does not carry goes to the bounded POSIX adapter or the inline executor
 * instead of being refused after the operation was already the ring's
 * (design §7: every submit path ends in a published record). */
int wf_linux_io_uring_carries(const wf_completion_record *record) {
    if (record == NULL) {
        return 0;
    }
    switch (record->request.kind) {
    /* One unpositioned stream read (ordinary native library). The ring carries it as a read at
     * offset -1, which is io_uring's own spelling for "the file's current
     * position", so the request needs no offset and the descriptor's own
     * position advances exactly as it does through `read`. */
    case WF_FILE_READ:
        return record->request.operation.read.descriptor >= 0
            && record->request.operation.read.count != 0
            && record->request.operation.read.count <= UINT32_MAX
            && record->request.operation.read.buffer != NULL;
    case WF_FILE_PREAD:
        return record->request.operation.pread.descriptor >= 0
            && record->request.operation.pread.count != 0
            && record->request.operation.pread.count <= UINT32_MAX
            && record->request.operation.pread.offset >= 0
            && record->request.operation.pread.buffer != NULL;
    case WF_FILE_OPEN_AT:
        /* An open resolves its path against a directory descriptor, and
         * AT_FDCWD is a negative one, so the transfer descriptor check does
         * not apply here.  The bytes the kernel resolves are the submitting
         * frame's own and stay live until the join, so there is no name this
         * adapter cannot hold (design §5). */
        return record->request.operation.open_at.path != NULL
            && record->request.operation.open_at.has_mode <= 1u
            && record->request.operation.open_at.expected_kind
                <= WF_FILE_EXPECT_DIRECTORY;
    case WF_FILE_CLOSE:
        return record->request.operation.close.descriptor >= 0;
    /* The four TCP kinds the ring carries (ordinary native library), which is where a
     * program that waits on a peer first waits in the kernel rather than on a
     * helper thread.  A listen, a bind and a half-close are immediate host
     * calls with nothing to wait for, so they stay on the adapter and this
     * answers no for them
     * (`research/investigations/io-model/NETWORK.md` §5). */
    case WF_FILE_SOCKET_ACCEPT:
        return record->request.operation.accept.descriptor >= 0;
    case WF_FILE_SOCKET_CONNECT:
        return 1;
    case WF_FILE_SOCKET_RECEIVE:
        return record->request.operation.receive.descriptor >= 0
            && record->request.operation.receive.count != 0
            && record->request.operation.receive.count <= UINT32_MAX
            && record->request.operation.receive.buffer != NULL;
    case WF_FILE_SOCKET_SEND:
        return record->request.operation.send.descriptor >= 0
            && record->request.operation.send.count != 0
            && record->request.operation.send.count <= UINT32_MAX
            && record->request.operation.send.buffer != NULL;
    default:
        return 0;
    }
}

/* The descriptor one record's request operates on, or resolves against. */
static int wf_linux_record_descriptor(const wf_completion_record *record) {
    switch (record->request.kind) {
    case WF_FILE_READ:
        return record->request.operation.read.descriptor;
    case WF_FILE_PREAD:
        return record->request.operation.pread.descriptor;
    case WF_FILE_OPEN_AT:
        return record->request.operation.open_at.directory;
    case WF_FILE_SOCKET_ACCEPT:
        return record->request.operation.accept.descriptor;
    case WF_FILE_SOCKET_CONNECT:
        return record->request.operation.endpoint.descriptor;
    case WF_FILE_SOCKET_RECEIVE:
        return record->request.operation.receive.descriptor;
    case WF_FILE_SOCKET_SEND:
        return record->request.operation.send.descriptor;
    case WF_FILE_CLOSE:
    default:
        return record->request.operation.close.descriptor;
    }
}

static int wf_linux_transfer_kind(enum wf_file_operation_kind kind) {
    return kind == WF_FILE_PREAD
        || kind == WF_FILE_READ || kind == WF_FILE_SOCKET_RECEIVE
        || kind == WF_FILE_SOCKET_SEND;
}

/* Whether a re-armed operation waits to read or to write.  A receive waits
 * for the same readiness a read does, and a send for the same a write
 * does. */
static int wf_linux_waits_readable(enum wf_file_operation_kind kind) {
    return kind == WF_FILE_PREAD || kind == WF_FILE_READ
        || kind == WF_FILE_SOCKET_RECEIVE;
}

/* Builds one SQE straight from the record and names the record's own address
 * as the completion's `user_data`.
 *
 * That address is the whole of the identity this adapter keeps: there is no
 * entry pool to index into, no generation to compare, and no range test, so a
 * completion is delivered to the operation that produced it by construction
 * rather than by a check (design §7).  The frame that holds the record joins
 * every outstanding operation before any terminator, so the record is still
 * there when its CQE arrives. */
static void wf_linux_stage_entry_locked(
    wf_linux_io_uring_adapter *adapter,
    wf_completion_record *record
) {
    unsigned tail = wf_linux_load_relaxed(adapter->submission_tail);
    unsigned ring_index = tail & *adapter->submission_mask;
    struct io_uring_sqe *submission =
        &adapter->submission_entries[ring_index];
    memset(submission, 0, sizeof(*submission));
    if (record->ring.waiting_readiness != 0) {
        submission->opcode = IORING_OP_POLL_ADD;
        submission->fd = wf_linux_record_descriptor(record);
        submission->poll_events =
            wf_linux_waits_readable(record->request.kind) ? POLLIN : POLLOUT;
    } else {
        switch (record->request.kind) {
        case WF_FILE_OPEN_AT:
            submission->opcode = IORING_OP_OPENAT;
            submission->fd = record->request.operation.open_at.directory;
            submission->addr =
                (uint64_t)(uintptr_t)record->request.operation.open_at.path;
            submission->open_flags = (uint32_t)(
                record->request.operation.open_at.flags
                | wf_file_open_kind_flags(
                      record->request.operation.open_at.expected_kind
                  )
            );
            submission->len =
                record->request.operation.open_at.has_mode != 0
                    ? (uint32_t)record->request.operation.open_at.mode
                    : 0u;
            break;
        case WF_FILE_CLOSE:
            submission->opcode = IORING_OP_CLOSE;
            submission->fd = record->request.operation.close.descriptor;
            break;
        case WF_FILE_READ:
            /* Offset -1 is io_uring's "use the file's current position", so
             * this is the same opcode as a positioned read with the position
             * left to the descriptor (ordinary native library). */
            submission->opcode = IORING_OP_READ;
            submission->fd = record->request.operation.read.descriptor;
            submission->off = (uint64_t)-1;
            submission->addr =
                (uint64_t)(uintptr_t)record->request.operation.read.buffer;
            submission->len = (uint32_t)record->request.operation.read.count;
            break;
        case WF_FILE_SOCKET_ACCEPT:
            /* The peer record and its length are the request's own storage,
             * so the kernel writes them where the accept join reads them; the
             * length cell is `socklen_t` by the assertion in `file_posix.h`.
             * The connection is closed on exec in the same step that takes
             * it, exactly as `accept4` does it on the adapter. */
            submission->opcode = IORING_OP_ACCEPT;
            submission->fd = record->request.operation.accept.descriptor;
            submission->addr = (uint64_t)(uintptr_t)
                record->request.operation.accept.peer.native.bytes;
            submission->off = (uint64_t)(uintptr_t)
                &record->request.operation.accept.peer_length;
            submission->accept_flags = (uint32_t)SOCK_CLOEXEC;
            break;
        case WF_FILE_SOCKET_CONNECT:
            /* The socket was created and the address record built by the
             * submit below, because the kernel reads that record after this
             * call returns and it therefore has to be the record's. */
            submission->opcode = IORING_OP_CONNECT;
            submission->fd = record->request.operation.endpoint.descriptor;
            submission->addr = (uint64_t)(uintptr_t)
                record->request.operation.endpoint.address.native.bytes;
            submission->off =
                (uint64_t)record->request.operation.endpoint.address_length;
            break;
        case WF_FILE_SOCKET_RECEIVE:
            submission->opcode = IORING_OP_RECV;
            submission->fd = record->request.operation.receive.descriptor;
            submission->addr =
                (uint64_t)(uintptr_t)record->request.operation.receive.buffer;
            submission->len =
                (uint32_t)record->request.operation.receive.count;
            break;
        case WF_FILE_SOCKET_SEND:
            submission->opcode = IORING_OP_SEND;
            submission->fd = record->request.operation.send.descriptor;
            submission->addr =
                (uint64_t)(uintptr_t)record->request.operation.send.buffer;
            submission->len = (uint32_t)record->request.operation.send.count;
            submission->msg_flags = (uint32_t)WF_SOCKET_SEND_FLAGS;
            break;
        case WF_FILE_PREAD:
        default:
            submission->opcode = IORING_OP_READ;
            submission->fd = record->request.operation.pread.descriptor;
            submission->off = (uint64_t)record->request.operation.pread.offset;
            submission->addr =
                (uint64_t)(uintptr_t)record->request.operation.pread.buffer;
            submission->len = (uint32_t)record->request.operation.pread.count;
            break;
        }
    }
    submission->user_data = (uint64_t)(uintptr_t)record;
    adapter->submission_array[ring_index] = ring_index;
    wf_linux_store_release(adapter->submission_tail, tail + 1u);
}

static int wf_linux_kick_locked(wf_linux_io_uring_adapter *adapter) {
    unsigned head = wf_linux_load_acquire(adapter->submission_head);
    unsigned tail = wf_linux_load_acquire(adapter->submission_tail);
    unsigned pending = tail - head;
    int result;
    if (pending == 0) {
        return 0;
    }
    atomic_fetch_add_explicit(
        &adapter->stat_submission_enters,
        1,
        memory_order_relaxed
    );
    do {
        result = wf_linux_enter(adapter->ring_descriptor, pending, 0, 0);
    } while (result == -EINTR);
    if (result < 0) {
        atomic_fetch_add_explicit(
            &adapter->stat_enter_failures,
            1,
            memory_order_relaxed
        );
        return -result;
    }
    return 0;
}

/* Makes room in the submission queue for one more entry, with this thread's
 * own `io_uring_enter` and never with a wait on an event.
 *
 * A deferred doorbell fills the submission queue with entries the kernel has
 * not been told about, so a full queue is first this thread's own backlog.
 * Ringing it is what advances the head, and the kernel consumes the whole
 * staged range synchronously because this design uses no SQPOLL, so a
 * submission waits on a syscall and never on a completion (design §1, §7).
 * A kick that reports success and still leaves no room is a target-runtime
 * defect, not backpressure: it is recorded and the caller fail-stops.  The
 * caller holds the submission lock.  Returns zero when there is room. */
static int wf_linux_make_room_locked(wf_linux_io_uring_adapter *adapter) {
    for (;;) {
        unsigned head = wf_linux_load_acquire(adapter->submission_head);
        unsigned tail = wf_linux_load_relaxed(adapter->submission_tail);
        int error;
        if (tail - head < *adapter->submission_count) {
            return 0;
        }
        error = wf_linux_kick_locked(adapter);
        if (error != 0) {
            return error;
        }
        if (wf_linux_load_acquire(adapter->submission_head) == head) {
            return EBUSY;
        }
    }
}

/* TCP_NODELAY on every socket this ring creates or accepts, right after the
 * host hands it over.  Nagle's coalescing measured an 11.78x-14.98x
 * throughput loss and a p99 tail of 41 ms against 2.4 ms without it
 * (research/investigations/io-model/SCHEDULER-FINDINGS.md, experiment 17).
 * A refusal leaves an ordinary, working socket -- just one that may coalesce
 * small writes -- so it is never folded into the operation's own outcome. */
static void wf_linux_socket_disable_nagle(int descriptor) {
    int one = 1;
    (void)setsockopt(descriptor, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
}

enum wf_linux_io_uring_submit_result wf_linux_io_uring_submit(
    wf_linux_io_uring_adapter *adapter,
    wf_completion_record *record
) {
    int error;

    if (adapter == NULL || adapter->initialized == 0) {
        return WF_LINUX_IO_URING_UNAVAILABLE;
    }
    if (!wf_linux_io_uring_carries(record)) {
        return WF_LINUX_IO_URING_SUBMIT_INVALID;
    }
    if (record->request.kind == WF_FILE_SOCKET_CONNECT) {
        /* A connect needs a socket before the ring has anything to connect,
         * and the host's own address record before the kernel has anything to
         * read.  Both are made here, in the submitting call, for the same
         * reason the completed open's kind check is made on the reaping
         * thread: neither can wait on anything -- `socket` allocates a kernel
         * object and the conversion touches no host at all -- so a second
         * ring round trip would cost more than the operation it wraps.
         *
         * A host that refuses the socket leaves this record for the bounded
         * adapter, which makes the same two host calls and produces the
         * program's outcome from the host's own refusal.  Nothing is
         * published here and nothing is left half-owned. */
        wf_socket_address portable =
            record->request.operation.endpoint.address.portable;
        int descriptor = wf_socket_open(&portable);
        if (descriptor < 0) {
            return WF_LINUX_IO_URING_SUBMIT_INVALID;
        }
        wf_linux_socket_disable_nagle(descriptor);
        /* The portable value is read out of the union before the native
         * record is written back over it, because the two share the arm. */
        record->request.operation.endpoint.address_length =
            wf_socket_native_from_address(
                &portable,
                &record->request.operation.endpoint.address.native
            );
        record->request.operation.endpoint.descriptor = descriptor;
    }

    record->route = WF_COMPLETION_ROUTE_LINUX_IO_URING;
    record->ring.waiting_readiness = 0;
    record->opened_descriptor = -1;
    record->open_outcome = WF_FILE_OPEN_SUCCEEDED;
    record->open_error = 0;

    (void)pthread_mutex_lock(&adapter->submission_lock);
    error = wf_linux_make_room_locked(adapter);
    if (error != 0) {
        (void)pthread_mutex_unlock(&adapter->submission_lock);
        wf_linux_record_progress_error(adapter, error);
        return WF_LINUX_IO_URING_SUBMIT_FAILED;
    }
    atomic_fetch_add_explicit(&adapter->in_flight, 1, memory_order_relaxed);
    /* Every word of the record the reaper will read is written above and by
     * the submitting bridge before this call; this release is what orders
     * them before the reaper's acquire (contract.h, `issued`). */
    atomic_store_explicit(&record->issued, 1u, memory_order_release);
    wf_linux_stage_entry_locked(adapter, record);
    atomic_fetch_add_explicit(
        &adapter->stat_submissions,
        1,
        memory_order_relaxed
    );

    /* The doorbell is deferred.  Staging an SQE is a store to a shared page
     * the kernel already maps, so a submission costs no syscall at all until
     * some thread flushes; `io_uring_enter` is then one call for however many
     * entries have accumulated.  Measured on the eight-wide many-file
     * benchmark, deferring turned 15,360 enters into 2,048 and took 15.6 ms
     * of system CPU off a program whose wall time fell by the same amount
     * (LOOP-PIPELINE.md §9.1).
     *
     * What makes it safe is that every path which can wait flushes first:
     * `wf_linux_io_uring_progress` kicks before it reaps or sleeps, and every
     * join and park in the bridge reaches the kernel through it.  The bridge
     * also flushes before a blocking direct host call, so a submitted open can
     * never sit unkicked behind one. */
    (void)pthread_mutex_unlock(&adapter->submission_lock);
    return WF_LINUX_IO_URING_TARGET_OWNS;
}

static int wf_linux_kick(wf_linux_io_uring_adapter *adapter) {
    int result;
    (void)pthread_mutex_lock(&adapter->submission_lock);
    result = wf_linux_kick_locked(adapter);
    (void)pthread_mutex_unlock(&adapter->submission_lock);
    return result;
}

/* Re-arms one record that a readiness refusal did not finish.
 *
 * It publishes nothing and re-arms the same operation, so several CQEs may
 * name one record while exactly one of them is terminal (design §7).  The
 * doorbell rings here and not at some later pass: a re-attempt left staged
 * behind a completion-only sleep would wait for a CQE the kernel has not been
 * asked to produce. */
static int wf_linux_resubmit_record(
    wf_linux_io_uring_adapter *adapter,
    wf_completion_record *record,
    unsigned waiting_readiness
) {
    int result;
    record->ring.waiting_readiness = waiting_readiness;
    (void)pthread_mutex_lock(&adapter->submission_lock);
    result = wf_linux_make_room_locked(adapter);
    if (result == 0) {
        int kicked;
        wf_linux_stage_entry_locked(adapter, record);
        kicked = wf_linux_kick_locked(adapter);
        if (kicked != 0) {
            result = kicked;
        }
    }
    (void)pthread_mutex_unlock(&adapter->submission_lock);
    return result;
}

/* Decides a completed open's typed outcome.
 *
 * The path resolution — the part of an open that can genuinely wait — is what
 * the ring carries. What remains is reading the mode of the descriptor the
 * kernel already produced, and that is an in-memory inode read which cannot
 * wait on anything, so it is one `fstat` here rather than a second ring
 * operation.
 *
 * That is a measurement, not a preference. The kind check was first written
 * as a linked `IORING_OP_STATX` of the same descriptor, which keeps the
 * reaping thread free of host calls entirely. Two ring round trips per open
 * cost more than the open they wrap: on the two-CPU Linux container the
 * eight-wide many-file program ran 152 ms against 116 ms for the bounded
 * adapter it replaced, and the four-wide one 203 ms against 140 ms. With the
 * check done here instead, the same programs run 119 ms and 141 ms — parity,
 * with the blocking `openat` gone from the scheduler thread. A descriptor the
 * check refuses is disposed of by the same single close the direct path
 * makes, on the error path where nothing is waiting for throughput. */
static void wf_linux_decide_open(
    wf_completion_record *record,
    int32_t completion_result
) {
    struct stat status;
    if (completion_result < 0) {
        record->opened_descriptor = -1;
        record->open_error = -completion_result;
        record->open_outcome = WF_FILE_OPEN_FAILED;
        return;
    }
    record->opened_descriptor = completion_result;
    record->open_error = 0;
    record->open_outcome = WF_FILE_OPEN_SUCCEEDED;
    if (record->request.operation.open_at.expected_kind
        == WF_FILE_EXPECT_ANY) {
        /* WF_IO_NOCACHE, applied to the descriptor the ring produced,
         * exactly where the bounded adapter applies it to the one openat
         * produced, so both backends answer the same policy. */
        wf_file_apply_uncached_reads(record->opened_descriptor);
        return;
    }
    if (fstat(record->opened_descriptor, &status) != 0) {
        record->open_error = errno;
        record->open_outcome = WF_FILE_OPEN_STATUS_FAILED;
    } else {
        record->open_outcome = (unsigned)wf_file_kind_outcome(
            record->request.operation.open_at.expected_kind,
            (unsigned int)status.st_mode
        );
        if (record->open_outcome == WF_FILE_OPEN_SUCCEEDED) {
            wf_file_apply_uncached_reads(record->opened_descriptor);
            return;
        }
    }
    /* One close attempt whose diagnostic is discarded, exactly as the direct
     * path makes it, and never retried. */
    (void)close(record->opened_descriptor);
}

/* Stores one record's terminal result and completes it.
 *
 * Every route out of a ring-owned operation ends here, so an operation's
 * completion is counted in exactly one place, and the store of DONE inside
 * `wf_completion_record_complete` is this adapter's last touch of a record
 * that is a block of the joiner's frame. */
static void wf_linux_complete_record(
    wf_linux_io_uring_adapter *adapter,
    wf_completion_record *record,
    int32_t completion_result
) {
    wf_file_result_head result;
    memset(&result, 0, sizeof(result));
    result.kind = record->request.kind;
    if (record->request.kind == WF_FILE_OPEN_AT) {
        /* The kind decision is the whole answer, and it names the descriptor
         * even where it refused it, exactly as the direct path does. */
        result.open_outcome = (enum wf_file_open_outcome)record->open_outcome;
        result.error_code = record->open_error;
        result.value = record->opened_descriptor;
    } else if (record->request.kind == WF_FILE_SOCKET_CONNECT) {
        /* The ring's connect answers zero, not the descriptor: the socket is
         * the one this adapter created at submit, and a refusal disposes of
         * it with the one close attempt the adapter's own leaf makes. The
         * ordinary linked caller restores the factory's credit on failure. */
        if (completion_result < 0) {
            (void)close(record->request.operation.endpoint.descriptor);
            result.value = -1;
            result.error_code = -completion_result;
        } else {
            result.value = record->request.operation.endpoint.descriptor;
        }
    } else if (record->request.kind == WF_FILE_SOCKET_ACCEPT) {
        if (completion_result < 0) {
            result.value = -1;
            result.error_code = -completion_result;
        } else {
            /* The kernel wrote the peer's own record and its length into the
             * request; the same rewrite the adapter's leaf performs puts it
             * in the form the accept join publishes. */
            wf_linux_socket_disable_nagle(completion_result);
            wf_socket_publish_peer(&record->request);
            result.value = completion_result;
        }
    } else if (completion_result < 0) {
        result.value = -1;
        result.error_code = -completion_result;
    } else {
        result.value = completion_result;
    }
    atomic_fetch_sub_explicit(&adapter->in_flight, 1, memory_order_relaxed);
    atomic_fetch_add_explicit(
        &adapter->stat_completions,
        1,
        memory_order_relaxed
    );
    record->result = result;
    wf_completion_record_complete(record);
}

/* The `user_data` of a cancellation's own completion, which names no record:
 * a record is aligned, so no record lives at this address. */
#define WF_LINUX_CANCEL_TAG ((uint64_t)1u)

static int wf_linux_publish_completion(
    wf_linux_io_uring_adapter *adapter,
    const struct io_uring_cqe *completion,
    int *terminal_published
) {
    wf_completion_record *record =
        (wf_completion_record *)(uintptr_t)completion->user_data;

    *terminal_published = 0;
    if (completion->user_data == WF_LINUX_CANCEL_TAG) {
        /* Whether the kernel found the operation or it had already
         * finished, the record's own completion reports the outcome. */
        return 0;
    }
    if (record == NULL
        || atomic_load_explicit(&record->issued, memory_order_acquire) == 0) {
        return EPROTO;
    }

    if (atomic_load_explicit(&record->deadline, memory_order_acquire)
            == WF_COMPLETION_DEADLINE_FIRED
        && (completion->res == -ECANCELED || completion->res == -EINTR
            || completion->res == -EAGAIN
            || (record->ring.waiting_readiness != 0 && completion->res >= 0))) {
        /* Its deadline passed before it transferred anything [PRE-2]: a
         * refusal, an interruption or a readiness it was re-armed for is not
         * retried, and the operation ends cancelled. */
        *terminal_published = 1;
        wf_linux_complete_record(adapter, record, -ECANCELED);
        return 0;
    }

    if (wf_linux_transfer_kind(record->request.kind)) {
        /* Interruption and readiness refusal are adapter progress on a
         * transfer, exactly as on the bounded POSIX adapter, and never a
         * writer-visible outcome. */
        if (record->ring.waiting_readiness != 0) {
            if (completion->res >= 0) {
                return wf_linux_resubmit_record(adapter, record, 0);
            }
            if (completion->res == -EINTR || completion->res == -EAGAIN) {
                return wf_linux_resubmit_record(adapter, record, 1);
            }
        } else if (completion->res == -EINTR) {
            return wf_linux_resubmit_record(adapter, record, 0);
        } else if (completion->res == -EAGAIN) {
            return wf_linux_resubmit_record(adapter, record, 1);
        }
    } else if (record->request.kind == WF_FILE_OPEN_AT) {
        wf_linux_decide_open(record, completion->res);
    }

    *terminal_published = 1;
    wf_linux_complete_record(adapter, record, completion->res);
    return 0;
}

int wf_linux_io_uring_cancel(
    wf_linux_io_uring_adapter *adapter,
    wf_completion_record *record
) {
    int error;
    unsigned tail;
    unsigned ring_index;
    struct io_uring_sqe *submission;
    if (adapter == NULL || adapter->initialized == 0 || record == NULL) {
        return EINVAL;
    }
    (void)pthread_mutex_lock(&adapter->submission_lock);
    error = wf_linux_make_room_locked(adapter);
    if (error == 0) {
        tail = wf_linux_load_relaxed(adapter->submission_tail);
        ring_index = tail & *adapter->submission_mask;
        submission = &adapter->submission_entries[ring_index];
        memset(submission, 0, sizeof(*submission));
        submission->opcode = IORING_OP_ASYNC_CANCEL;
        submission->fd = -1;
        submission->addr = (uint64_t)(uintptr_t)record;
        submission->user_data = WF_LINUX_CANCEL_TAG;
        adapter->submission_array[ring_index] = ring_index;
        wf_linux_store_release(adapter->submission_tail, tail + 1u);
        error = wf_linux_kick_locked(adapter);
    }
    (void)pthread_mutex_unlock(&adapter->submission_lock);
    if (error != 0) {
        wf_linux_record_progress_error(adapter, error);
    }
    return error;
}

int wf_linux_io_uring_progress(
    wf_linux_io_uring_adapter *adapter,
    size_t budget,
    unsigned wait,
    size_t *published
) {
    size_t total = 0;
    size_t processed = 0;
    int first_error = 0;
    unsigned head;
    unsigned tail;

    if (published != NULL) {
        *published = 0;
    }
    if (adapter == NULL || adapter->initialized == 0 || published == NULL
        || budget == 0) {
        return EINVAL;
    }

    first_error = atomic_load_explicit(
        &adapter->progress_error,
        memory_order_acquire
    );
    if (first_error != 0) {
        return first_error;
    }

    first_error = wf_linux_kick(adapter);
    (void)pthread_mutex_lock(&adapter->completion_lock);
    head = wf_linux_load_relaxed(adapter->completion_head);
    tail = wf_linux_load_acquire(adapter->completion_tail);
    /* Never enter a completion-only sleep while staged SQEs failed to reach
     * the kernel; the scheduler must observe and resolve those rather than
     * waiting for a CQE which cannot yet exist. */
    if (head == tail && wait != 0 && first_error == 0 && total == 0) {
        int entered;
        do {
            entered = wf_linux_enter(
                adapter->ring_descriptor,
                0,
                1,
                IORING_ENTER_GETEVENTS
            );
        } while (entered == -EINTR);
        if (entered < 0 && first_error == 0) {
            first_error = -entered;
            atomic_fetch_add_explicit(
                &adapter->stat_enter_failures,
                1,
                memory_order_relaxed
            );
        }
        tail = wf_linux_load_acquire(adapter->completion_tail);
    }

    for (;;) {
        while (head != tail && processed < budget) {
            unsigned completion_index = head & *adapter->completion_mask;
            struct io_uring_cqe completion =
                adapter->completion_entries[completion_index];
            int terminal_published = 0;
            int error = wf_linux_publish_completion(
                adapter,
                &completion,
                &terminal_published
            );
            head += 1u;
            wf_linux_store_release(adapter->completion_head, head);
            if (first_error == 0 && error != 0) {
                first_error = error;
            }
            total += (size_t)terminal_published;
            processed += 1;
            tail = wf_linux_load_acquire(adapter->completion_tail);
        }
        /* The queue is drained, or the budget is spent.  A queue the kernel
         * overflowed holds its remaining completions on the kernel's own
         * list, and nothing but an `io_uring_enter` moves them into the
         * queue: not a submission, since every submitter may be parked on
         * one of them, and not the ring descriptor's readiness, which stays
         * raised for exactly that reason.  So an empty queue under the
         * overflow flag is entered once, for no minimum, and reaped again.
         * A kick that leaves the flag raised and the queue empty is a
         * kernel that has nothing more to move, and the loop ends. */
        if (processed >= budget || head != tail
            || (wf_linux_load_acquire(adapter->submission_flags)
                & IORING_SQ_CQ_OVERFLOW) == 0) {
            break;
        }
        {
            int entered;
            atomic_fetch_add_explicit(
                &adapter->stat_overflow_flushes,
                1,
                memory_order_relaxed
            );
            do {
                entered = wf_linux_enter(
                    adapter->ring_descriptor,
                    0,
                    0,
                    IORING_ENTER_GETEVENTS
                );
            } while (entered == -EINTR);
            if (entered < 0) {
                if (first_error == 0) {
                    first_error = -entered;
                }
                atomic_fetch_add_explicit(
                    &adapter->stat_enter_failures,
                    1,
                    memory_order_relaxed
                );
                break;
            }
            tail = wf_linux_load_acquire(adapter->completion_tail);
            if (head == tail) {
                break;
            }
        }
    }
    (void)pthread_mutex_unlock(&adapter->completion_lock);

    if (first_error != 0) {
        wf_linux_record_progress_error(adapter, first_error);
    }

    *published = total;
    return first_error;
}

void wf_linux_io_uring_notify(void *context) {
    wf_linux_io_uring_adapter *adapter = context;
    uint64_t one = 1;
    ssize_t written;
    if (adapter == NULL || adapter->initialized == 0
        || adapter->wake_descriptor < 0) {
        return;
    }
    do {
        written = write(adapter->wake_descriptor, &one, sizeof(one));
    } while (written < 0 && errno == EINTR);
    if (written == (ssize_t)sizeof(one)) {
        atomic_fetch_add_explicit(
            &adapter->stat_host_wake_writes,
            1,
            memory_order_relaxed
        );
        return;
    }
    if (written < 0 && errno == EAGAIN) {
        /* A saturated eventfd is already readable, so no wake can be lost. */
        return;
    }
    /* A valid nonblocking eventfd accepts exactly eight bytes or reports
     * EAGAIN while already readable. Anything else is corruption of the
     * compiler-owned wait endpoint; fail-stop here instead of leaving the
     * announced scheduler asleep forever. */
    abort();
}

static int wf_linux_drain_wake_descriptor(
    wf_linux_io_uring_adapter *adapter
) {
    for (;;) {
        uint64_t count;
        ssize_t received;
        do {
            received = read(
                adapter->wake_descriptor,
                &count,
                sizeof(count)
            );
        } while (received < 0 && errno == EINTR);
        if (received == (ssize_t)sizeof(count)) {
            continue;
        }
        if (received < 0 && errno == EAGAIN) {
            return 0;
        }
        return received < 0 ? errno : EIO;
    }
}

int wf_linux_io_uring_park(
    wf_linux_io_uring_adapter *adapter,
    uint64_t observed_epoch,
    uint32_t timeout_milliseconds
) {
    struct epoll_event events[2];
    int timeout;
    int ready;
    int error;
    int index;
    int announced = 0;
    if (adapter == NULL || adapter->initialized == 0
        || adapter->wait_descriptor < 0 || adapter->wake_descriptor < 0) {
        return EINVAL;
    }
    error = wf_linux_io_uring_progress_error(adapter);
    if (error != 0) {
        return error;
    }
    /* Announce sleep under the same lock used by core publishers.  A
     * publisher before this point leaves only an epoch/CQ fact and performs
     * no host wake. A publisher after the second recheck sees the announced
     * sleeper and leaves a level-readable eventfd. */
    wf_completion_wait_lock(&adapter->runtime->wait);
    /* Completions the kernel holds on its overflow list are completions the
     * reaper has not seen, so the overflow flag is a non-empty queue here and
     * below: a sleeper would otherwise be woken at once by the descriptor's
     * readiness and reap nothing, over and over. */
    if (wf_completion_wake_epoch(adapter->runtime) != observed_epoch
        || wf_linux_load_acquire(adapter->completion_head)
            != wf_linux_load_acquire(adapter->completion_tail)
        || (wf_linux_load_acquire(adapter->submission_flags)
            & IORING_SQ_CQ_OVERFLOW) != 0) {
        wf_completion_wait_unlock(&adapter->runtime->wait);
        return 0;
    }
    /* Sequentially consistent, and paired with the sequentially consistent
     * epoch load below: a core publisher raises the epoch and then reads the
     * wake-needed flag without taking the wait's lock. That flag/recheck pair
     * keeps an eventfd wake from being lost. */
    wf_completion_announce_park_locked(adapter->runtime);
    announced = 1;
    if (atomic_load_explicit(
            &adapter->runtime->wake_epoch,
            memory_order_seq_cst
        ) != observed_epoch
        || wf_linux_load_acquire(adapter->completion_head)
            != wf_linux_load_acquire(adapter->completion_tail)
        || (wf_linux_load_acquire(adapter->submission_flags)
            & IORING_SQ_CQ_OVERFLOW) != 0) {
        atomic_fetch_sub_explicit(
            &adapter->runtime->parked_schedulers,
            1,
            memory_order_relaxed
        );
        wf_completion_wait_unlock(&adapter->runtime->wait);
        return 0;
    }
    wf_completion_wait_unlock(&adapter->runtime->wait);
    timeout = timeout_milliseconds == UINT32_MAX
        ? -1
        : timeout_milliseconds > (uint32_t)INT_MAX
            ? INT_MAX
            : (int)timeout_milliseconds;
    atomic_fetch_add_explicit(
        &adapter->stat_kernel_waits,
        1,
        memory_order_relaxed
    );
    do {
        ready = epoll_wait(adapter->wait_descriptor, events, 2, timeout);
    } while (ready < 0 && errno == EINTR
             && wf_completion_wake_epoch(adapter->runtime)
                 == observed_epoch);
    if (ready < 0 && errno == EINTR
        && wf_completion_wake_epoch(adapter->runtime) != observed_epoch) {
        /* A signal raced with a real core announcement. The epoch is the
         * durable wake fact; EINTR itself never becomes a progress failure. */
        error = 0;
    } else if (ready < 0) {
        error = errno;
    } else if (ready == 0) {
        error = ETIMEDOUT;
    } else {
        error = 0;
    }
    if (ready > 0) {
        atomic_fetch_add_explicit(
            &adapter->stat_kernel_wakes,
            1,
            memory_order_relaxed
        );
        for (index = 0; index < ready; ++index) {
            if ((events[index].events & (EPOLLERR | EPOLLHUP)) != 0) {
                error = EIO;
                break;
            }
            if (events[index].data.u32 != WF_LINUX_WAIT_WAKE
                && events[index].data.u32 != WF_LINUX_WAIT_CQ) {
                error = EPROTO;
                break;
            }
        }
    }

    /* Stop advertising this lane before removing the persistent wake record.
     * Both happen while publishers are excluded by the wait's lock, so no eventfd
     * write can land after the drain for a lane that is already active. */
    if (announced != 0) {
        unsigned previous;
        wf_completion_wait_lock(&adapter->runtime->wait);
        previous = atomic_fetch_sub_explicit(
            &adapter->runtime->parked_schedulers,
            1,
            memory_order_relaxed
        );
        if (previous == 0) {
            abort();
        }
        /* The eventfd is one broadcast level fact, not a consumable wake for
         * whichever lane happens to run first. Keep it readable until every
         * scheduler that was announced has left epoll; otherwise a lane
         * waiting for unrelated capacity can steal the only wake from the lane
         * that owns a completed token. */
        if (previous == 1) {
            int drain_error = wf_linux_drain_wake_descriptor(adapter);
            if (error == 0 && drain_error != 0) {
                error = drain_error;
            }
        }
        wf_completion_wait_unlock(&adapter->runtime->wait);
    }
    if (error != 0 && error != ETIMEDOUT) {
        wf_linux_record_progress_error(adapter, error);
        return error;
    }
    if (error == ETIMEDOUT) {
        return error;
    }
    return wf_linux_io_uring_progress_error(adapter);
}

int wf_linux_io_uring_progress_error(
    const wf_linux_io_uring_adapter *adapter
) {
    if (adapter == NULL) {
        return EINVAL;
    }
    return atomic_load_explicit(
        &adapter->progress_error,
        memory_order_acquire
    );
}

size_t wf_linux_io_uring_in_flight(
    const wf_linux_io_uring_adapter *adapter
) {
    if (adapter == NULL) {
        return 0;
    }
    return atomic_load_explicit(&adapter->in_flight, memory_order_acquire);
}

size_t wf_linux_io_uring_capacity(
    const wf_linux_io_uring_adapter *adapter
) {
    if (adapter == NULL || adapter->initialized == 0) {
        return 0;
    }
    return (size_t)*adapter->submission_count;
}

int wf_linux_io_uring_flush(wf_linux_io_uring_adapter *adapter) {
    int error;
    if (adapter == NULL || adapter->initialized == 0) {
        return EINVAL;
    }
    error = wf_linux_kick(adapter);
    if (error != 0) {
        wf_linux_record_progress_error(adapter, error);
    }
    return error;
}

int wf_linux_io_uring_destroy(wf_linux_io_uring_adapter *adapter) {
    int first_error = 0;
    if (adapter == NULL || adapter->initialized == 0) {
        return EINVAL;
    }
    if (wf_linux_io_uring_in_flight(adapter) != 0
        || wf_linux_load_acquire(adapter->submission_head)
            != wf_linux_load_acquire(adapter->submission_tail)
        || wf_linux_load_acquire(adapter->completion_head)
            != wf_linux_load_acquire(adapter->completion_tail)) {
        return EBUSY;
    }
    {
        int error = pthread_mutex_destroy(&adapter->completion_lock);
        if (error != 0) {
            first_error = error;
        }
    }
    {
        int error = pthread_mutex_destroy(&adapter->submission_lock);
        if (first_error == 0 && error != 0) {
            first_error = error;
        }
    }
    if (first_error != 0) {
        return first_error;
    }
    wf_linux_close_wait_set(adapter);
    wf_linux_unmap(adapter);
    if (close(adapter->ring_descriptor) != 0) {
        return errno;
    }
    adapter->ring_descriptor = -1;
    adapter->initialized = 0;
    return 0;
}

wf_linux_io_uring_statistics wf_linux_io_uring_statistics_snapshot(
    const wf_linux_io_uring_adapter *adapter
) {
    wf_linux_io_uring_statistics statistics = {0};
    if (adapter == NULL) {
        return statistics;
    }
    statistics.submissions = atomic_load_explicit(
        &adapter->stat_submissions,
        memory_order_relaxed
    );
    statistics.submission_enters = atomic_load_explicit(
        &adapter->stat_submission_enters,
        memory_order_relaxed
    );
    statistics.completions = atomic_load_explicit(
        &adapter->stat_completions,
        memory_order_relaxed
    );
    statistics.enter_failures = atomic_load_explicit(
        &adapter->stat_enter_failures,
        memory_order_relaxed
    );
    statistics.overflow_flushes = atomic_load_explicit(
        &adapter->stat_overflow_flushes,
        memory_order_relaxed
    );
    statistics.kernel_waits = atomic_load_explicit(
        &adapter->stat_kernel_waits,
        memory_order_relaxed
    );
    statistics.kernel_wakes = atomic_load_explicit(
        &adapter->stat_kernel_wakes,
        memory_order_relaxed
    );
    statistics.host_wake_writes = atomic_load_explicit(
        &adapter->stat_host_wake_writes,
        memory_order_relaxed
    );
    return statistics;
}

#else

typedef int wf_linux_io_uring_translation_unit_is_target_guarded;

#endif
