#ifndef WHITEFOOT_COMPLETION_BRIDGE_H
#define WHITEFOOT_COMPLETION_BRIDGE_H

#include <stdint.h>

#if defined(__cplusplus)
extern "C" {
#endif

/* Every submit fills the record the caller supplies and answers nothing: the
 * runtime either accepted the operation or executed it itself and published
 * its completion into the record, and either way the operation is the
 * runtime's and will be joined
 * (`research/investigations/io-model/PARK-ON-MISS.md` §7, "Every submit path
 * ends in a published record" -- "never with a 0 the caller must interpret").
 * The ordinary linked body always joins the supplied record; engine selection
 * is private to this implementation and returns no alternate-call verdict.
 * A `NULL` record, or an argument this ABI cannot mean, is a contract violation
 * and terminates;
 * an argument the host itself would refuse is an ordinary failed outcome and
 * is published as one, which is why a `pread` offset above `INT64_MAX`
 * completes with `EINVAL` rather than terminating. */
void wf__completion_file_read_submit(
    int descriptor,
    void *buffer,
    uint64_t count,
    void *record
);

void wf__completion_file_pread_submit(
    int descriptor,
    void *buffer,
    uint64_t count,
    uint64_t file_offset,
    void *record
);

void wf__completion_file_write_submit(
    int descriptor,
    const void *buffer,
    uint64_t count,
    void *record
);

void wf__completion_file_open_at_submit(
    int directory,
    const char *path,
    int flags,
    unsigned mode,
    unsigned has_mode,
    unsigned expected_kind,
#if defined(_WIN32)
    unsigned descriptor_class,
#endif
    void *record
);

void wf__completion_file_close_submit(
    int descriptor,
    void *record
);

/* One write at the end of a file opened for appending, which waits on the
 * host's storage and never on another party, so it is a kind of its own
 * rather than an unpositioned stream write. */
void wf__completion_file_append_submit(
    int descriptor,
    const void *buffer,
    uint64_t count,
    void *record
);

/* Hands every byte written to the file before it to the host's durability
 * mechanism [PRE-2]. */
void wf__completion_file_sync_submit(
    int descriptor,
    void *record
);

/* A record no host operation completes: the driver whose context waits on
 * it completes it once the monotonic clock has reached `deadline`
 * (`sleep_until` [PRE-2]). */
void wf__completion_sleep_submit(
    uint64_t deadline,
    void *record
);

/* Bounds the wait for the next record this thread submits by a reading of
 * the monotonic clock, zero for none.  A body calls it just before the
 * submit, which takes it into the record, so the routing can keep an
 * operation with a deadline off the thread that must end it; once the clock
 * reaches the deadline the driver ends the wait by cancelling the operation
 * through its route, and the operation completes with its own outcome or
 * with a cancellation, which `wf__completion_deadline_passed` then
 * reports. */
void wf__completion_next_deadline(uint64_t deadline);

/* Whether the record's deadline ended its operation: the driver cancelled
 * it and it transferred nothing. */
int wf__completion_deadline_passed(const void *record);

/* The monotonic clock the deadlines are readings of. */
uint64_t wf__completion_monotonic_ns(void);

/* The platform's directory-enumeration facility through the same native
 * progress normalization as file operations --
 * `__getdirentries64` on Darwin, `getdents64` on Linux.  EINTR and readiness
 * refusal never cross this ABI as writer-visible errors.  `position` is the
 * base-position cell Darwin's facility requires; Linux keeps the whole cursor
 * in the descriptor and leaves the cell untouched.  The native record the
 * batch holds differs by platform and is decoded by the ordinary linked
 * library, not here. */
void wf__completion_directory_next_submit(
    int descriptor,
    void *buffer,
    uint64_t count,
    int64_t *position,
    void *record
);

/* The six TCP submits (ordinary native library).
 *
 * A listen and a connect name one address and no descriptor, because each
 * creates its own socket from the address's family; the address arrives as
 * the three scalars an emitted `SocketAddress` value is (`contract.h`,
 * `wf_socket_address`), so nothing here reads a layout the emitted module
 * holds a pointer into.  A receive and a send are one descriptor, one buffer
 * and one count, exactly as a stream read and a write are.  A half-close
 * names the direction it releases; the runtime keeps the pair's own count and
 * closes the target's object on the second of the two. */
void wf__completion_socket_listen_submit(
    uint64_t address_low,
    uint64_t address_high,
    uint32_t port_and_family,
    void *record
);

void wf__completion_socket_accept_submit(
    int listener,
    void *record
);

void wf__completion_socket_connect_submit(
    uint64_t address_low,
    uint64_t address_high,
    uint32_t port_and_family,
    void *record
);

void wf__completion_socket_receive_submit(
    int descriptor,
    void *buffer,
    uint64_t count,
    void *record
);

void wf__completion_socket_send_submit(
    int descriptor,
    const void *buffer,
    uint64_t count,
    void *record
);

void wf__completion_socket_shutdown_submit(
    int descriptor,
    unsigned direction,
    void *record
);

/* A shutdown's joined value is zero for the first half and one for the
 * descriptor-close attempt on its last half. Its error code independently
 * reports the close outcome. The ordinary linked close uses that private
 * result to return one descriptor credit to its explicit factory argument. */

void wf__completion_file_open_join(
    const void *record,
    int64_t *value,
    int *error_code,
    unsigned *open_outcome
);

/* The accept's own join: the accepted descriptor, the host's refusal, and the
 * three scalars of the peer's address.  Every other TCP kind is joined
 * through `wf__completion_file_join`, because none of them publishes anything
 * beyond the two the file join already carries. */
void wf__completion_socket_accept_join(
    const void *record,
    int64_t *value,
    int *error_code,
    uint64_t *peer_low,
    uint64_t *peer_high,
    uint32_t *peer_tag
);

/* Every join runs the scheduler core's one rule over the record (design §2):
 * read it if it is DONE, park this stack and continue on another if a stack is
 * free or READY, and otherwise wait in place with nothing running above the
 * join, making target progress until the record is DONE or there is nothing
 * left to do but sleep (§2's fourth line, I/O arm; §6 step 4).  A thread that
 * is not on a pool stack has no stack to park and takes that last arm
 * directly. */
void wf__completion_file_join(
    const void *record,
    int64_t *value,
    int *error_code
);

uint64_t wf__completion_file_submissions(void);
uint64_t wf__completion_file_fallback_submissions(void);
uint64_t wf__completion_file_helper_executions(void);
uint64_t wf__completion_target_helper_count(void);
uint64_t wf__completion_target_helper_executions(void);
/* Records completed: one per submission, whichever engine finished it. */
uint64_t wf__completion_publications(void);
/* Operations the engine executed inside the submitting call itself, because
 * the operation had no kernel completion form, no host work at all, or the
 * adapter measured that submitting it could only add a queue crossing to a
 * host call this thread was about to make (design §7.1, primitive 7).  It is
 * a throughput fact and never an outcome: the record is published either way. */
uint64_t wf__completion_inline_executions(void);
/* Operations the platform's kernel completion ring carried: io_uring on
 * Linux, the completion port on Windows, and none on a target with neither.
 * One name, because a link has one ring. */
uint64_t wf__completion_native_ring_submissions(void);
/* Calls that carried staged submissions to the kernel, where the ring defers
 * its doorbell.  `io_uring` does, so this is far below the submission count and
 * the distance between them is what deferring bought; a ring that carries each
 * request inside the call that issues it answers zero. */
uint64_t wf__completion_native_ring_submission_enters(void);

/* Observational counters for the shared host wait. Announcements can be
 * cancelled by an epoch change before sleeping; signals count host wake
 * requests, not threads awakened. Reads do not initialize the wait runtime
 * and are individually atomic, not a simultaneous snapshot. */
uint64_t wf__completion_wait_announcements(void);
uint64_t wf__completion_wait_signals(void);

/* Whether a submitted record has not completed yet: what a waiting host
 * operation's start answers, so the frame that called it waits only for an
 * operation that is still pending. */
int wf__completion_pending(const void *record);

/* ------------------------------------------------------ waiting contexts */

/* Contexts [WAIT-2], each a chain of resumable frames
 * (design/compiler/waiting-contexts.md).  The emitted code of a
 * waiting function allocates its frame with `wf__context_frame_allocate` and
 * releases it with `wf__context_frame_release`, last in, first out, from the
 * context that runs it. */
void *wf__context_frame_allocate(uint64_t bytes);
void wf__context_frame_release(void *frame);

/* The block a context keeps for its one pending host operation: a completion
 * record first, then whatever the operation's linked body keeps until its
 * finish reads the record.  The linked bodies check that their layout fits. */
#if defined(_WIN32)
#define WF_CONTEXT_OPERATION_BYTES 704u
#elif defined(__APPLE__)
#define WF_CONTEXT_OPERATION_BYTES 1216u
#else
#define WF_CONTEXT_OPERATION_BYTES 448u
#endif
#define WF_CONTEXT_OPERATION_ALIGN 16u
void *wf__context_operation(void);

/* Called by a frame whose host operation `operation` did not complete in its
 * start: returns zero when the record is complete after all, and otherwise
 * parks the running context on it, to resume `frame` once it completes, and
 * returns nonzero, after which the frame suspends. A record complete after
 * all counts as a wait that did not suspend, and may yield the context
 * instead [WAIT-2]. */
int wf__context_wait(void *operation, void *frame);

/* [WAIT-2] a host operation its start answered: answers 1 when the running
 * context has yielded to another ready context and its frame suspends, and 0
 * when it continues. */
int wf__context_pass(void *frame);

/* [WAIT-3] a context start: `wf__context_prepare` makes the context and
 * returns an argument block of `bytes` from its arena, and
 * `wf__context_launch` calls `start` on that block, in the new context, to
 * make its outermost frame, and makes it ready.  `group` is the two words
 * the starting activation keeps: its unfinished contexts and its waiter. */
void *wf__context_prepare(uint64_t bytes);
void wf__context_launch(uint64_t *group, void *arguments, void *(*start)(void *arguments));
/* Returns zero when every context the group's activation started has
 * finished, and otherwise records the running context as the group's waiter,
 * to resume `frame`, and returns nonzero, after which the frame suspends. */
int wf__context_join_wait(uint64_t *group, void *frame);

/* Shared objects [SHARE-1]: a header the runtime keeps, then the object's
 * state at WF_SHARED_STATE_OFFSET, which the emitted code stores, reads and
 * drops.  `wf__shared_new` returns an object with one handle and room for
 * `state_bytes` of state; `wf__shared_share` adds a handle; and
 * `wf__shared_release` removes one, returning nonzero when it was the last,
 * after which the emitted code drops the state and calls `wf__shared_free`. */
#define WF_SHARED_STATE_OFFSET 64u
void *wf__shared_new(uint64_t state_bytes);
void wf__shared_share(void *object);
int wf__shared_release(void *object);
void wf__shared_free(void *object);

/* An atomic statement [SHARE-2, SHARE-3]: `wf__shared_acquire` returns zero
 * when the running context now holds the object, for writing when `write` is
 * nonzero and for reading otherwise, and nonzero when it waits, after which
 * the frame suspends and resumes holding it.  `wf__shared_unlock` ends the
 * hold.  `wf__shared_watch`, called holding the object after a guard read
 * false, ends the hold and waits until a statement that writes the object
 * ends; it returns nonzero, the frame suspends, and on resuming the statement
 * acquires the object again and re-reads its guard. */
int wf__shared_acquire(void *object, uint32_t write, void *frame);
void wf__shared_unlock(void *object, uint32_t write);
int wf__shared_watch(void *object, uint32_t write, void *frame);
/* The acquire of a statement inside the block of a statement holding a
 * map's state or an entry, which keeps its driver and never suspends: it
 * returns once the running context holds the object. */
void wf__shared_take(void *object, uint32_t write);

/* What the runtime's concurrent maps take from this runtime: the number of
 * the driver running the caller, below WF_CMAP_MAX_USERS, which numbers a
 * map's users; blocks from the context pool, never from the program's
 * allocator [STOR-8]; a yield of the processor; and the end a frame no
 * memory can hold brings. */
unsigned wf__driver_index(void);
void *wf__runtime_take(uint64_t bytes);
void wf__runtime_give(void *block, uint64_t bytes);
void wf__runtime_yield(void);
_Noreturn void wf__runtime_exhausted(void);

/* The root context runs the entry: `wf__context_root_begin` makes it the
 * running context, the launcher calls the entry's ramp, and
 * `wf__context_root_run` drives every context until that frame finishes,
 * then releases it. */
void wf__context_root_begin(void);
void wf__context_root_run(void *frame);

#if defined(__cplusplus)
}
#endif

#endif
