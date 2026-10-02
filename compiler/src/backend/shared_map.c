/* Shared maps [SHARE-1] and their atomic statements [SHARE-2, SHARE-3] over
 * the runtime's concurrent map (concurrent_map.c), which this unit compiles
 * in with the completion runtime as its host.
 *
 * A handle is one pointer to a record that counts the map's handles; the
 * address of the map's state is that same pointer. Each driver thread is one
 * user of every map, numbered by its driver, since a statement holding a
 * map's state or an entry never suspends and so ends on the driver it began
 * on. For the same reason a driver thread runs one statement on a map at a
 * time, whose locked entry, or whose set of held entries, waits here for
 * the statement's release.
 */
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif

#include <stdint.h>

#include "completion/bridge.h"

#define WF_CMAP_TAKE(bytes) wf__runtime_take(bytes)
#define WF_CMAP_GIVE(block, bytes) wf__runtime_give((block), (bytes))
#define WF_CMAP_YIELD() wf__runtime_yield()
#define WF_CMAP_EXHAUSTED() wf__runtime_exhausted()
#include "concurrent_map.c"

typedef struct {
    _Atomic uint64_t handles;
    wf_cmap *map;
} wf_shared_map;

/* The entry the statement running on this thread has locked. */
static _Thread_local wf_cmap_entry wf_shared_map_locked;

static wf_cmap_user *wf_shared_map_user(wf_shared_map *shared) {
    return wf_cmap_user_at(shared->map, wf__driver_index());
}

/* A new map whose entries keep slots of `size` bytes aligned to `align`,
 * sized for `capacity` entries, holding one handle. */
void *wf__shared_map_new(uint64_t size, uint64_t align, uint64_t capacity) {
    wf_shared_map *shared = (wf_shared_map *)wf__runtime_take(sizeof(wf_shared_map));
    atomic_store_explicit(&shared->handles, 1u, memory_order_relaxed);
    shared->map = wf_cmap_create_entries(size, align, capacity);
    return shared;
}

void wf__shared_map_share(void *object) {
    wf_shared_map *shared = (wf_shared_map *)object;
    atomic_fetch_add_explicit(&shared->handles, 1u, memory_order_relaxed);
}

/* Nonzero when the handle released was the last. */
int32_t wf__shared_map_release(void *object) {
    wf_shared_map *shared = (wf_shared_map *)object;
    return atomic_fetch_sub_explicit(&shared->handles, 1u, memory_order_acq_rel) == 1u;
}

/* After the last handle: the slot of an entry whose value the caller
 * releases before asking again, or NULL once every entry has been. */
void *wf__shared_map_drain(void *object) {
    return wf_cmap_drain(((wf_shared_map *)object)->map);
}

void wf__shared_map_free(void *object) {
    wf_shared_map *shared = (wf_shared_map *)object;
    wf_cmap_destroy(shared->map);
    wf__runtime_give(shared, sizeof(wf_shared_map));
}

void wf__shared_map_hold(void *object) {
    wf_cmap_hold(wf_shared_map_user((wf_shared_map *)object));
}

void wf__shared_map_unhold(void *object) {
    wf_cmap_unhold(wf_shared_map_user((wf_shared_map *)object));
}

/* Locks the entry under the `length` bytes at `key` and returns its slot,
 * zero-filled, so `None`, when the entry was absent; `held` when the caller
 * holds the map's state. */
void *wf__shared_map_lock(void *object, const unsigned char *key, uint64_t length, int32_t held) {
    return wf_cmap_lock_entry(
        wf_shared_map_user((wf_shared_map *)object),
        key,
        length,
        held,
        &wf_shared_map_locked
    );
}

/* Unlocks this thread's locked entry, kept when its slot holds `Some`. */
void wf__shared_map_unlock(void *object, int32_t held, int32_t present) {
    wf_cmap_unlock_entry(
        wf_shared_map_user((wf_shared_map *)object),
        &wf_shared_map_locked,
        held,
        present
    );
}

/* For a statement that writes nothing through its binder: the slot of the
 * entry under the `length` bytes at `key`, read beside the other statements
 * that only read it, or a slot holding `None`, which no statement writes,
 * when the key is absent. */
const void *wf__shared_map_read(void *object, const unsigned char *key, uint64_t length) {
    return wf_cmap_read_entry(
        wf_shared_map_user((wf_shared_map *)object),
        key,
        length,
        0,
        &wf_shared_map_locked
    );
}

/* Ends this thread's read of an entry. */
void wf__shared_map_unread(void *object) {
    wf_cmap_unread_entry(wf_shared_map_user((wf_shared_map *)object), &wf_shared_map_locked, 0);
}

/* A statement holding a map's state whose keys are computed before its block
 * runs holds those keys' entries and no others. The keys it will reach are
 * collected into this thread's set, which is then held (wf_cmap_hold_set):
 * the statement never suspends, so the set lives on its thread from the
 * first key to the release, and no other map's statement runs inside it. */
static _Thread_local wf_cmap_set wf_shared_map_keyed;
/* The held entry the statement's block is in. */
static _Thread_local wf_cmap_held *wf_shared_map_reached;

/* Begins collecting the keys of a statement on the map. */
void wf__shared_map_keys(void *object) {
    (void)object;
    wf_cmap_set_clear(&wf_shared_map_keyed);
}

/* One key the statement will reach; its bytes stay as they are until the
 * release. */
void wf__shared_map_key(void *object, const unsigned char *key, uint64_t length) {
    (void)object;
    wf_cmap_set_add(&wf_shared_map_keyed, key, length);
}

/* Holds the collected keys' entries, or the whole map when they cannot be
 * held together. */
void wf__shared_map_hold_keys(void *object) {
    wf_cmap_hold_set(wf_shared_map_user((wf_shared_map *)object), &wf_shared_map_keyed);
}

/* The slot of the entry under the `length` bytes at `key`, one of the keys
 * collected, zero-filled, so `None`, when the entry was absent. A key that
 * was not collected is one the compiler's own collection missed, which no
 * program can cause: the statement would reach an entry it does not hold,
 * so the program stops. */
void *wf__shared_map_held(void *object, const unsigned char *key, uint64_t length) {
    if (wf_shared_map_keyed.whole) {
        return wf_cmap_lock_entry(
            wf_shared_map_user((wf_shared_map *)object),
            key,
            length,
            1,
            &wf_shared_map_locked
        );
    }
    wf_cmap_held *held = wf_cmap_set_find(&wf_shared_map_keyed, key, length);
    if (held == NULL) {
        abort();
    }
    wf_shared_map_reached = held;
    return held->slot;
}

/* Ends the block on the entry wf__shared_map_held answered, which stays held
 * until the release and is then kept when its slot holds `Some`. */
void wf__shared_map_leave_held(void *object, int32_t present) {
    if (wf_shared_map_keyed.whole) {
        wf_cmap_unlock_entry(wf_shared_map_user((wf_shared_map *)object), &wf_shared_map_locked, 1, present);
        return;
    }
    wf_shared_map_reached->present = (uint32_t)(present != 0);
}

/* Releases what wf__shared_map_hold_keys holds. */
void wf__shared_map_release_keys(void *object) {
    wf_cmap_release_set(wf_shared_map_user((wf_shared_map *)object), &wf_shared_map_keyed);
}

uint64_t wf__shared_map_count(void *object) {
    return wf_cmap_count(((wf_shared_map *)object)->map);
}
