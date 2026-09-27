#ifndef WHITEFOOT_COMPLETION_CONTRACT_H
#define WHITEFOOT_COMPLETION_CONTRACT_H

/*
 * The completion record, and the wake epoch the engines sleep on.
 *
 * This is an internal compiler/runtime ABI, not a writer-visible API.  It
 * deliberately contains no function pointer which could name Whitefoot code:
 * target adapters fill and publish a record; its submitter waits on its own
 * ordinary stack.
 *
 * One record per operation, and it is a block of the frame that submitted the
 * operation (`research/investigations/io-model/PARK-ON-MISS.md` §5).  There is
 * no slot array, no token, no generation and no claim: an emitted frame
 * reserves the block, hands its address to submit and to join, and the engine
 * that finishes the operation finds the record by that address.  Nothing here
 * can be refused for want of capacity, because there is no pool to exhaust.
 */



#include <stdalign.h>
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>

#if defined(__cplusplus)
extern "C" {
#endif

/* ------------------------------------------------- the typed file request */

/* Whether this native library build has the directory-enumeration
 * facility: one host call that reports a bounded batch of an open directory's
 * entries and advances that descriptor's own position.  Darwin, Linux and
 * Windows all do, through different calls writing different records; a family
 * that has none compiles no enumeration request kind in this private engine.
 * This build capability does not participate in source acceptance. */
#if defined(__APPLE__) || defined(__linux__) || defined(_WIN32)
#define WF_FILE_HAS_DIRECTORY_NEXT 1
#endif

enum wf_file_operation_kind {
    WF_FILE_OPEN_AT = 1,
    WF_FILE_READ = 2,
    WF_FILE_WRITE = 3,
    WF_FILE_PREAD = 4,
    WF_FILE_CLOSE = 7,
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
    /* One bounded batch of directory entries, advancing the descriptor's own
     * enumeration position.  The host call behind it differs by family --
     * `__getdirentries64` on Darwin, `getdents64` on Linux -- and so does the
     * record it writes; the request record does not, because everything that
     * differs is either the call itself or the emitted shim's decoding of the
     * bytes it left behind. */
    WF_FILE_DIRECTORY_NEXT = 8,
#endif
    /* The six TCP request kinds (ordinary native library).  Their values are fixed
     * rather than following the enumerator above it, because the directory
     * kind is compiled only on a family that has the facility and these are
     * compiled on every family. */
    /* Creates one socket of the address's family, binds it, and listens.  The
     * backlog is the target's own maximum and is not a program's resource
     * (`research/investigations/io-model/NETWORK.md` §2). */
    WF_FILE_SOCKET_LISTEN = 9,
    /* Takes one waiting connection from a listener and reports the peer's own
     * address beside the descriptor. */
    WF_FILE_SOCKET_ACCEPT = 10,
    /* Creates one socket of the address's family and connects it. */
    WF_FILE_SOCKET_CONNECT = 11,
    /* One transfer attempt on one direction of one connection.  They are
     * separate kinds from WF_FILE_READ and WF_FILE_WRITE because the ring
     * carries them with different opcodes and the leaf makes different host
     * calls, not because their request shape differs. */
    WF_FILE_SOCKET_RECEIVE = 12,
    WF_FILE_SOCKET_SEND = 13,
    /* One direction's half-close, and the close of the target's object when
     * it is the pair's second release (ordinary native library). */
    WF_FILE_SOCKET_SHUTDOWN = 14,
};

/* Which direction of one connection a half-close releases (ordinary native library). */
enum wf_socket_direction {
    WF_SOCKET_DIRECTION_RECEIVE = 0,
    WF_SOCKET_DIRECTION_SEND = 1
};

/* One internet address in exactly the form an emitted `SocketAddress` value
 * carries (ordinary native library).
 *
 * Sixteen address bytes in two 64-bit words, then the port in the low sixteen
 * bits of a 32-bit word whose bit 16 selects the family.  Byte `i` of the
 * address occupies bits `8 * (i % 8)` of word `i / 8`, so the same rule reads
 * the value on either endianness and an IPv4 address simply leaves bytes 4
 * through 15 zero.  `wf__body_socket_address_v4` and
 * `wf__body_socket_address_v6` (`ordinary_values.c`) build this layout; this
 * is the same layout read and written by the runtime, and the two must stay
 * one fact.
 *
 * The port is the number the program wrote, in host order; the leaf converts
 * it to network order when it builds the native record. */
typedef struct wf_socket_address {
    uint64_t words[2];
    uint32_t port_and_family;
} wf_socket_address;

#define WF_SOCKET_PORT_MASK 0xffffu
#define WF_SOCKET_FAMILY_V6 (1u << 16)

/* Storage for one native address record, sized for the two families this
 * specification admits: 28 bytes holds a `struct sockaddr_in6`, which is the
 * larger of the two, and the union's word gives it the alignment the host's
 * own record needs. */
typedef union wf_socket_native_address {
    unsigned char bytes[28];
    uint64_t alignment[4];
} wf_socket_native_address;

enum wf_file_expected_kind {
    WF_FILE_EXPECT_ANY = 0,
    WF_FILE_EXPECT_REGULAR = 1,
    WF_FILE_EXPECT_DIRECTORY = 2
};

enum wf_file_open_outcome {
    WF_FILE_OPEN_SUCCEEDED = 0,
    WF_FILE_OPEN_FAILED = 1,
    WF_FILE_OPEN_STATUS_FAILED = 2,
    WF_FILE_OPEN_IS_DIRECTORY = 3,
    WF_FILE_OPEN_OTHER_KIND = 4
};

/* One typed request, filled by submit into the submitting frame's record.
 *
 * Path bytes belong to the ordinary library call's local storage. Its native
 * body waits for the private request before returning, so both the record and
 * every borrowed byte remain live until the engine has finished using them.
 * The engine retains pointers and does not copy the path payload. */
typedef struct wf_file_request {
    enum wf_file_operation_kind kind;
    union {
        struct {
            int directory;
            /* The submitting frame's own bytes, live until the join. */
            const char *path;
            int flags;
            unsigned mode;
            unsigned has_mode;
            enum wf_file_expected_kind expected_kind;
            /* Which resource this descriptor will become, on a target whose
             * open needs to know before it opens.  It is the one place a
             * per-target ABI reaches this record: the Windows
             * `wf__completion_file_open_at_submit` carries the extra argument
             * and fills this, every other target fills zero and no leaf reads
             * it (`wf_open` in `ordinary_values.c`). */
            unsigned descriptor_class;
        } open_at;
        struct {
            int descriptor;
            void *buffer;
            size_t count;
        } read;
        struct {
            int descriptor;
            const void *buffer;
            size_t count;
        } write;
        struct {
            int descriptor;
            void *buffer;
            size_t count;
            int64_t offset;
        } pread;
        struct {
            int descriptor;
        } close;
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
        struct {
            int descriptor;
            void *buffer;
            size_t count;
            /* Darwin's facility requires a base-position cell and writes the
             * position of the batch it reported into it.  Linux's takes no
             * such argument and keeps the whole cursor in the descriptor, so
             * on that family this cell is left exactly as the caller gave it.
             * Either way it is scratch storage of the emitted shim's, never a
             * component of the `DirectorySource` value. */
            int64_t *position;
        } directory_next;
#endif
        /* One listen or one connect: both create their own socket from the
         * address's family, so both name an address and no descriptor at
         * submit.  The address arrives in the emitted value's own portable
         * form and the engine that takes the operation converts it, in place,
         * into the native record its host call or its ring entry names --
         * which is why the two forms share one union arm rather than costing
         * the record both.  A ring entry reads that native record after the
         * submitting call returns, so it has to live in the record; a host
         * call copies it and would not, and the shared arm costs nothing
         * either way.  `descriptor` is -1 until the engine has created the
         * socket, and `address_length` is the native record's own length. */
        struct {
            int descriptor;
            unsigned address_length;
            union {
                wf_socket_address portable;
                wf_socket_native_address native;
            } address;
        } endpoint;
        /* One accept.  `peer` is the target's own answer about the connection
         * it handed over: the kernel or the host call writes the native record
         * and its length, and whichever engine completed the operation
         * rewrites it in place as the portable form the accept join
         * publishes.  It is in the request's union rather than in the result
         * head because the head is shared by every kind, and twenty-four more
         * bytes on every operation would put this record past the block an
         * emitted frame reserves. */
        struct {
            int descriptor;
            unsigned peer_length;
            union {
                wf_socket_address portable;
                wf_socket_native_address native;
            } peer;
        } accept;
        struct {
            int descriptor;
            void *buffer;
            size_t count;
        } receive;
        struct {
            int descriptor;
            const void *buffer;
            size_t count;
        } send;
        struct {
            int descriptor;
            enum wf_socket_direction direction;
        } shutdown;
    } operation;
} wf_file_request;

/* What a join reads back.  Four scalars and no bytes: everything an operation
 * transfers has already gone to storage the caller named. */
typedef struct wf_file_result_head {
    enum wf_file_operation_kind kind;
    int64_t value;
    int error_code;
    enum wf_file_open_outcome open_outcome;
} wf_file_result_head;

/* -------------------------------------------------------------- the record */

/* Which engine owns an operation between its submission and its completion. */
enum wf_completion_route {
    WF_COMPLETION_ROUTE_NONE = 0,
    WF_COMPLETION_ROUTE_FILE_ADAPTER = 1,
    WF_COMPLETION_ROUTE_LINUX_IO_URING = 2,
    WF_COMPLETION_ROUTE_INLINE = 3,
    WF_COMPLETION_ROUTE_WINDOWS_IOCP = 4,
    /* No engine yet: the operation's join makes it once its descriptor is
     * ready, because another context shares the thread [WAIT-2]. */
    WF_COMPLETION_ROUTE_READINESS = 5
};

/* The ring's own state inside the record, one platform's at a time.
 *
 * A kernel completion ring keeps something per operation, and the record is
 * where it goes now that there is no entry pool to keep it in (design §7,
 * "What survives, and the one change inside it").  The two rings keep
 * different things, so this is a union rather than a sum of both.
 *
 * The Windows arm is opaque bytes for the same reason the wait below is:
 * `<windows.h>` does not belong in a header both platforms include, and the
 * unit that owns the state -- `windows_iocp.c` -- asserts that its own types
 * fit.  What it holds is the record's own `OVERLAPPED`, first, so that a
 * completion packet's `OVERLAPPED` address recovers the record by subtraction
 * exactly as a CQE's `user_data` names it on Linux; and the handle the
 * operation was issued on. */
#define WF_COMPLETION_RING_WORDS 5u

typedef union wf_completion_ring_state {
    /* The io_uring resubmission state.  A readiness refusal re-arms the same
     * operation as a poll and publishes nothing, so several CQEs may name one
     * record while exactly one is terminal (design §7). */
    unsigned waiting_readiness;
    /* The IOCP arm's `OVERLAPPED` and the handle it was issued on. */
    uint64_t native[WF_COMPLETION_RING_WORDS];
} wf_completion_ring_state;

/* Results become readable after an acquire observation of DONE. The engine
 * must make publication its final record access: join may immediately end
 * the submitting frame's lifetime. No scheduler or stack pointer is stored. */
#define WF_COMPLETION_PENDING 1u
#define WF_COMPLETION_DONE 2u
typedef struct wf_completion_record {
    _Atomic unsigned state;
    /* The typed request, filled by submit. */
    wf_file_request request;
    /* The result the one publication stores, read by the join. */
    wf_file_result_head result;
    /* Whichever ring owns the operation keeps its own state here. */
    wf_completion_ring_state ring;
    /* Which route owns the operation. */
    unsigned route;
    /* Set with release by the ring's submit after every other word of the
     * record is written, and read with acquire by the reaper before it reads
     * any of them.  The kernel carries the record's address from the SQE to
     * the CQE, or from the request to the completion packet, but neither a
     * mapped ring nor a completion port is a C11 synchronization, so this pair
     * is what orders the submitter's writes before the reaper's reads; the
     * thread sanitizer reported exactly that gap without it.  A completion
     * naming a record that was never issued is a protocol failure. */
    _Atomic unsigned issued;
    int opened_descriptor;
    unsigned open_outcome;
    int open_error;
    /* The intrusive link of the file adapter's pending list.  The queue is
     * threaded through the records themselves, so it has no capacity of its
     * own and cannot refuse an operation (design §7). */
    struct wf_completion_record *next;
} wf_completion_record;

/* The ABI constants, and the two assertions that keep them true.  A record
 * that outgrew the reservation is a build failure instead of a kernel write
 * past it. */
/* Preserve the emitted record reservation across the scheduler change.
 * Windows stores OVERLAPPED and its handle in the record; the assertions
 * below check every target's actual layout against this ABI capacity. */
#define WF_COMPLETION_RECORD_BYTES 160u
#define WF_COMPLETION_RECORD_ALIGN 8u

_Static_assert(
    sizeof(wf_completion_record) <= WF_COMPLETION_RECORD_BYTES,
    "the completion record must fit the block an emitted frame reserves"
);
_Static_assert(
    _Alignof(wf_completion_record) <= WF_COMPLETION_RECORD_ALIGN,
    "the completion record must not out-align the reserved block"
);

/* The request union is where a new request kind grows this record, and the
 * six TCP kinds are the first ones whose arms approach the open's.  Stating
 * the bound here means an arm that outgrows it fails at the union that caused
 * it, with the reason, rather than only in the whole-record assertion above.
 * A kind needing more than this puts its extra bytes in the union with the
 * fields it does not use, exactly as the accept's peer record does. */
_Static_assert(
    sizeof(((wf_file_request *)0)->operation)
        == sizeof(((wf_file_request *)0)->operation.open_at),
    "no request arm may be larger than the open's, which is the arm this "
    "record was sized around"
);

static inline void wf_completion_record_init(wf_completion_record *record) {
    atomic_init(&record->state, WF_COMPLETION_PENDING);
}
/* Final access by the publisher; notification may use permanent engine
 * storage afterward, never the record or any other submitting-frame bytes. */
static inline void wf_completion_record_publish(wf_completion_record *record) {
    atomic_store_explicit(&record->state, WF_COMPLETION_DONE, memory_order_release);
}
void wf_completion_record_complete(wf_completion_record *record);

/* ------------------------------------------------------------- the wait */

/* The one host wait set this runtime sleeps on, and the only part of it that
 * differs by platform: a mutex and a condition variable on POSIX, an SRWLOCK
 * and a CONDITION_VARIABLE on Windows.
 *
 * Its storage is opaque here on purpose.  `<pthread.h>` does not belong in a
 * header both platforms include, and neither does `<windows.h>`; the platform
 * unit that implements the six calls below (`wait_host.c`, `wait_windows.c`)
 * asserts that its own types fit this block.  Everything above the block --
 * the epoch, the announcement, the statistics, the park protocol in
 * `runtime.c` -- is one implementation for both.
 *
 * The block is a fixed number of 64-bit words rather than a `max_align_t`
 * array because that is what makes its size and alignment the same fact on
 * both sides of the assertion. */
#define WF_COMPLETION_WAIT_WORDS 24u

typedef struct wf_completion_wait {
    uint64_t storage[WF_COMPLETION_WAIT_WORDS];
} wf_completion_wait;

enum wf_completion_wait_result {
    WF_COMPLETION_WAIT_WOKEN = 0,
    WF_COMPLETION_WAIT_TIMED_OUT = 1,
    WF_COMPLETION_WAIT_FAILED = 2
};

/* Returns zero on success and a platform error code otherwise. */
int wf_completion_wait_init(wf_completion_wait *wait);
int wf_completion_wait_destroy(wf_completion_wait *wait);
void wf_completion_wait_lock(wf_completion_wait *wait);
void wf_completion_wait_unlock(wf_completion_wait *wait);
/* Sleeps with the lock held and returns with it held.  UINT32_MAX asks for no
 * deadline.  A spurious wake is the caller's to tolerate, which every caller
 * does by rechecking its own predicate. */
enum wf_completion_wait_result wf_completion_wait_sleep(
    wf_completion_wait *wait,
    uint32_t timeout_milliseconds
);
/* Wakes one sleeper, or every one when `all` is nonzero.  Called with the lock
 * held. */
void wf_completion_wait_wake(wf_completion_wait *wait, int all);

/* ---------------------------------------------------------- the wake epoch */

/* Compiler-owned host-wait notification.  Every epoch change reaches the
 * callback so a target adapter can join the core's wake source to its native
 * completion wait set.  The callback may only announce a host wait endpoint;
 * it must not run a writer continuation. */
typedef void (*wf_completion_wake_callback)(void *context);

enum wf_completion_park_result {
    WF_COMPLETION_PARK_WOKEN = 0,
    WF_COMPLETION_PARK_EPOCH_CHANGED = 1,
    WF_COMPLETION_PARK_TIMED_OUT = 2,
    WF_COMPLETION_PARK_FAILED = 3
};

typedef struct wf_completion_statistics {
    uint64_t parks;
    uint64_t wake_signals;
    uint64_t compute_notifications;
    uint64_t target_notifications;
} wf_completion_statistics;

/* What is left of the completion core once the record pool is gone: one wake
 * epoch, the sleepers announced against it, and the host endpoint a target
 * adapter joins to it. */
typedef struct wf_completion_runtime {
    wf_completion_wait wait;
    _Atomic uint64_t wake_epoch;
    _Atomic unsigned parked_schedulers;
    /* New announcements set this. Condition-wait notifications clear it under
     * wait's lock; external endpoints retain per-publication notifications. */
    _Atomic unsigned wake_needed;

    _Atomic uint64_t stat_parks;
    _Atomic uint64_t stat_wake_signals;
    _Atomic uint64_t stat_compute_notifications;
    _Atomic uint64_t stat_target_notifications;
    wf_completion_wake_callback wake_callback;
    void *wake_context;
    unsigned initialized;
} wf_completion_runtime;

/* Returns zero on success. */
int wf_completion_runtime_init(wf_completion_runtime *runtime);

/* Announce a new wait while holding runtime->wait. The caller must recheck
 * wake_epoch with SC ordering before sleeping and withdraw its count on exit. */
void wf_completion_announce_park_locked(wf_completion_runtime *runtime);

/* Destroy refuses while any parked scheduler still exists.  It returns zero
 * on success and EBUSY/EINVAL otherwise. */
int wf_completion_runtime_destroy(wf_completion_runtime *runtime);

/* Installs the target's host-wait announcer before any scheduler can park.
 * At most one announcer may be installed. */
int wf_completion_set_wake_callback(
    wf_completion_runtime *runtime,
    wf_completion_wake_callback wake,
    void *context
);

/* One epoch covers both compute publication and target completion.  Scheduler
 * protocol is: progress; snapshot the epoch; recheck all sources; then
 * park_if_unchanged.  The park function announces sleep under the same lock
 * used by publishers, closes the final race, and tolerates spurious host
 * wakes.  UINT32_MAX requests an unbounded wait. */
uint64_t wf_completion_wake_epoch(const wf_completion_runtime *runtime);
void wf_completion_notify_compute(wf_completion_runtime *runtime);
/* Newly runnable bounded target work uses the same epoch and host endpoint. */
void wf_completion_notify_target(wf_completion_runtime *runtime);
enum wf_completion_park_result wf_completion_park_if_unchanged(
    wf_completion_runtime *runtime,
    uint64_t observed_epoch,
    uint32_t timeout_milliseconds
);
unsigned wf_completion_parked_scheduler_count(
    const wf_completion_runtime *runtime
);

wf_completion_statistics wf_completion_statistics_snapshot(
    const wf_completion_runtime *runtime
);

#if defined(__cplusplus)
}
#endif

#endif
