#ifndef WHITEFOOT_SNAPSHOT_EXPORT_H
#define WHITEFOOT_SNAPSHOT_EXPORT_H

#include <stddef.h>
#include <stdint.h>

/* Immutable compiler-generated layout metadata. No entry is a callback.
 * Offsets are native byte offsets; wire tags are SNAP-2's tags. A reference
 * into a dynamic storage block names its header, not a pointer-valued owner. */
typedef struct wf_snapshot_type wf_snapshot_type;
typedef struct wf_snapshot_field {
    uint64_t offset;
    const wf_snapshot_type *type;
} wf_snapshot_field;
typedef struct wf_snapshot_variant {
    uint64_t count;
    const wf_snapshot_field *fields;
} wf_snapshot_variant;

enum wf_snapshot_shape {
    WF_SNAPSHOT_SCALAR, WF_SNAPSHOT_STRUCT, WF_SNAPSHOT_ENUM,
    WF_SNAPSHOT_ARRAY, WF_SNAPSHOT_SLOTS, WF_SNAPSHOT_RING,
    WF_SNAPSHOT_SEGMENTS, WF_SNAPSHOT_PAGED, WF_SNAPSHOT_BOX,
    WF_SNAPSHOT_HASH, WF_SNAPSHOT_ORDERED
};

struct wf_snapshot_type {
    uint64_t shape;
    uint64_t tag;
    uint64_t width; /* scalar width, or native enum discriminant width */
    uint64_t size;
    uint64_t alignment;
    uint64_t count; /* struct fields or enum variants */
    const wf_snapshot_field *fields;
    const wf_snapshot_variant *variants;
    const wf_snapshot_type *element;
    uint64_t length_offset;
    uint64_t capacity_offset; /* UINT64_MAX: fixed capacity */
    uint64_t head_offset;
    uint64_t data_offset;
    uint64_t fixed_capacity;
    uint64_t page_elements;
    /* Hash: fields[0] cells Box<Slots>, [1] length; element is bucket enum.
     * Ordered: fields[0] root Slots<Box<Node>>, [1] length; element is node.
     * Node: fields[0] leading Slots<Box<Node>>, [1] entries Slots<Entry>.
     * Entry: fields[0] pair struct, [1] after Slots<Box<Node>>. */
};

typedef struct wf_snapshot_owner wf_snapshot_owner;
typedef struct wf_snapshot_read_result {
    uint64_t count;
    uint32_t error; /* 0 Ok, 1 ReadFailed, 2 DeadlinePassed, 3 Cancelled */
} wf_snapshot_read_result;
typedef struct wf_snapshot_outcome {
    uint64_t bytes;
    uint64_t records;
    uint64_t delivered;
    uint64_t discarded;
    uint32_t child; /* 0 complete, 1 export error, 2 exited, 3 signal, 4 protocol */
    uint32_t detail; /* source variant ordinal / exit code / signal */
    uint32_t collection; /* 0 collected, 1 discarded, 2 collection failed */
    uint32_t abort_requested;
} wf_snapshot_outcome;

/* Preparation error: 0 success, 1 reserve unavailable, 2 host refused. */
wf_snapshot_owner *wf_snapshot_prepare(uint64_t scratch, uint64_t output, uint32_t *error);
void wf_snapshot_discard(wf_snapshot_owner *owner);

/* Ordinary atomic-entry hooks, independent of which calls its body reaches.
 * Begin is idempotent and precedes acquisition, including executor retries;
 * end follows release or a failed acquisition before suspending its context.
 * A successful capture queues activation on this executor until end. */
void wf_snapshot_atomic_begin(void);
void wf_snapshot_atomic_end(void);

/* map is the map state pointer, not a Shared owner. Capture returns the same
 * prepared control as its job, NULL on creation failure. No syscall besides
 * fork, allocation, descriptor work or suspension occurs in its parent. */
wf_snapshot_owner *wf_snapshot_capture(
    wf_snapshot_owner *owner, const void *map, const uint64_t *cut,
    const wf_snapshot_type *value, uint64_t slot_tag_offset,
    uint32_t slot_tag_width, uint64_t none_tag, uint64_t value_offset
);

/* Ordinary completion record ABI from bridge.h; start returns 0 immediate,
 * 1 completed submitted record, 2 pending. Finish handles only a submitted
 * record. has_deadline distinguishes the epoch from no deadline; cancellation
 * is an ordinary CancelState.
 * The caller retains its window, job and watch until finish. */
int wf_snapshot_read_start(
    wf_snapshot_owner *owner, void *out, uint64_t count, uint64_t deadline,
    uint32_t has_deadline, void *cancel, void *record, wf_snapshot_read_result *result
);
void wf_snapshot_read_finish(
    wf_snapshot_owner *owner, const void *out, void *record,
    wf_snapshot_read_result *result
);
/* Trusted helper leaf stores the outcome; source finish consumes it exactly
 * once after the helper's completion record has been joined. */
void wf_snapshot_run_finish(wf_snapshot_owner *owner, uint32_t abort_requested);
void wf_snapshot_take_outcome(wf_snapshot_owner *owner, wf_snapshot_outcome *result);

/* Private read-only captured traversal implemented beside keyed_table.c's
 * included map representation. No public map ABI or user callbacks. */
int wf_snapshot_map_next(
    const void *map, uint64_t *cursor, uint64_t tag_offset,
    uint32_t tag_width, uint64_t none_tag,
    const unsigned char **key, uint64_t *length, const void **slot
);

#endif
