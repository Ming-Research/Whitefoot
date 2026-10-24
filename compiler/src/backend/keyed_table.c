/* Maps and key sets [SHARE-1], the entries and whole maps an atomic
 * statement holds as lock units of its object's state [SHARE-3], and the
 * guards that read them, over the runtime's concurrent map
 * (concurrent_map.c), which this unit compiles in with the completion
 * runtime as its host (completion/bridge.h declares this unit's functions).
 *
 * A map is one pointer, wherever the value that holds it lives; the value
 * owns the map and releases it with itself. Each physical executor is one user of
 * every table, retaining its identity when its logical driver is reassigned:
 * a statement holding entries never suspends and ends on its original thread. Everything a
 * statement holds lives in what its compiled code reserves in its frame: a
 * wf_table_entry for one key, a hold for several keys or the whole table,
 * and a watch for a guard. So one statement may hold several maps of its
 * state at once, and this unit keeps nothing per
 * thread for a statement; it keeps each thread's spare key set memory, which
 * no statement holds.
 */
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif

#include <stdint.h>

#include "completion/bridge.h"

#ifndef WF_CMAP_TAKE
#define WF_CMAP_TAKE(bytes) wf__runtime_take(bytes)
#define WF_CMAP_GIVE(block, bytes) wf__runtime_give((block), (bytes))
#define WF_CMAP_YIELD() wf__runtime_yield()
#define WF_CMAP_EXHAUSTED() wf__runtime_exhausted()
#define WF_CMAP_GRANTED(bytes) wf__runtime_granted(bytes)
#endif
#ifndef WF_CMAP_HEAP_CHANGE
#define WF_CMAP_HEAP_CHANGE(delta) wf__heap_change(delta)
#endif
/* A table keeps the watches of the guards that read it. */
#define WF_CMAP_HOST_FIELDS wf_watch_list watch;
/* The calling thread's user of a map, whose spare memory a hold's keys
 * reuse. */
#define WF_CMAP_CURRENT_USER(map) wf_cmap_user_at((map), wf__driver_index())
/* The calling thread's spare key set memory. A thread of its own, not a
 * driver's number: the key set's functions are pure, so a compute worker may
 * build a set too, and every thread that is no driver counts as driver 0. */
static _Thread_local void *wf_key_set_spare;
#define WF_CMAP_SPARE_KEYS() (wf_key_set_spare)
#include "concurrent_map.c"


/* What a statement on one key keeps in its frame: the entry's unlock, the
 * table's user, whether the statement only reads the entry, and whether the
 * user held the whole table when it took the entry. */
typedef struct wf_table_entry {
    wf_cmap_entry inner;
    wf_cmap_user *user;
    uint32_t read;
    uint32_t held;
} wf_table_entry;

_Static_assert(sizeof(wf_table_entry) == WF_TABLE_ENTRY_SIZE, "WF_TABLE_ENTRY_SIZE is an entry's size");
_Static_assert(_Alignof(wf_table_entry) == WF_TABLE_ENTRY_ALIGN, "WF_TABLE_ENTRY_ALIGN is an entry's alignment");
_Static_assert(sizeof(wf_cmap_holding) == WF_TABLE_HOLD_SIZE, "WF_TABLE_HOLD_SIZE is a hold's size");
_Static_assert(_Alignof(wf_cmap_holding) == WF_TABLE_HOLD_ALIGN, "WF_TABLE_HOLD_ALIGN is a hold's alignment");
_Static_assert(sizeof(wf_key_set) == 16 && _Alignof(wf_key_set) == 8, "a key set is its count and its memory");

/* A selection descriptor uses the hold's frame layout without becoming a
 * table hold. Its mode is private to this unit and it owns no table records. */
enum { WF_TABLE_READ_SELECTION = 2 };

/* A statement that may have written the table ends: the table's watches are
 * woken when it has any, which costs a statement that finds none one load of
 * a word only a guard's registration and wake write. */
static inline void table_written(wf_cmap *map) {
    if (__builtin_expect(__atomic_load_n(&map->watch.count, __ATOMIC_RELAXED) != 0u, 0))
        wf__watch_written(&map->watch);
}

void wf__key_set_new(wf_key_set *out, uint64_t capacity) { wf_cmap_key_set_new(out, capacity); }

uint64_t wf__key_set_bytes(const wf_key_set *set) { return wf_cmap_key_set_bytes(set); }

uint64_t wf__key_set_insert(wf_key_set *set, const unsigned char *key, uint64_t length) {
    return wf_cmap_key_set_insert(set, key, length);
}

void wf__key_set_free(void *store) { wf_cmap_key_set_free_store(store); }

/* [SHARE-1] the key at index, below the set's count: copies as many of its
 * first bytes as out has room for and answers its length. */
uint64_t wf__key_set_read_key(const wf_key_set *set, uint64_t index, unsigned char *out, uint64_t room) {
    uint64_t length;
    const unsigned char *key = wf_cmap_key_set_key(set, index, &length);
    uint64_t copied = length < room ? length : room;
    if (copied != 0)
        memcpy(out, key, (size_t)copied);
    return length;
}

void *wf__map_new(uint64_t slot_size, uint64_t slot_align, uint64_t capacity) {
    return wf_cmap_create_entries(slot_size, slot_align, capacity);
}

/* The whole hold a write to a map runs under. A map no statement holds
 * whole is written under a hold of its own, taken here at its first such
 * write: the map is then one only its owner reaches, or one inside a shared
 * state that the object's own lock covers, and either way one context at a
 * time writes it (compiler/waiting-contexts/state-locks). */
static wf_cmap_holding *table_whole(wf_cmap *map) {
    if (map->whole_hold == NULL) {
        map->local_hold = take(sizeof(wf_cmap_holding));
        wf_cmap_hold_begin(map->local_hold, map);
        wf_cmap_hold_whole(map->local_hold);
        wf_cmap_hold_take(WF_CMAP_CURRENT_USER(map), map->local_hold);
    }
    return map->whole_hold;
}

/* Gives up the map's own write hold, if it keeps one, counting each entry it
 * wrote by the slot tag the arguments locate. A clear under that hold has
 * released what it took out already (wf__keyed_table_clear). */
static void table_settle(wf_cmap *map, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag) {
    wf_cmap_holding *hold = map->local_hold;
    if (hold == NULL)
        return;
    map->local_hold = NULL;
    wf_cmap_hold_release(hold, tag_offset, tag_width, none_tag);
    WF_CMAP_GIVE(hold, sizeof *hold);
}

void wf__keyed_table_settle(void *table, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag) {
    table_settle((wf_cmap *)table, tag_offset, tag_width, none_tag);
}

void *wf__table_held_entry(void *table, const unsigned char *key, uint64_t length, uint32_t write) {
    wf_cmap *map = table;
    if (write != 0)
        table_whole(map);
    return wf_cmap_held_entry(map, key, length, write != 0);
}

void wf__table_held_entries(void *table, const wf_key_set *set, uint64_t *entries, void *read_record) {
    wf_cmap *map = table;
    if (read_record != NULL) {
        /* A private frame descriptor, never registered with the map or
         * released as a hold. Each selected slot probes the stable index. */
        wf_cmap_holding *selection = read_record;
        wf_cmap_hold_begin(selection, map);
        selection->wants = WF_TABLE_READ_SELECTION;
        selection->table = (void *)set;
        entries[0] = (uint64_t)(uintptr_t)selection;
        entries[1] = 0;
        entries[2] = set->len;
        return;
    }
    wf_cmap_holding *hold = table_whole(map);
    uint64_t first = wf_cmap_hold_keys(hold, set);
    entries[0] = (uint64_t)(uintptr_t)hold;
    entries[1] = first;
    entries[2] = set->len;
}

uint64_t wf__keyed_table_count(void *table, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag) {
    return wf_cmap_count_held((wf_cmap *)table, tag_offset, tag_width, none_tag);
}

/* A drained map's counts are not read again, so its own write hold is
 * given up counting by the one tag byte every slot begins with. */
uint64_t *wf__keyed_table_drain(void *table) {
    table_settle((wf_cmap *)table, 0, 1, 0);
    return (uint64_t *)wf_cmap_drain((wf_cmap *)table);
}

uint64_t wf__keyed_table_scan_within(void *table, uint64_t cursor, uint64_t count, uint64_t limit,
                                    wf_key_set *set, uint32_t *needs, uint64_t tag_offset,
                                    uint32_t tag_width, uint64_t none_tag) {
    return wf_cmap_scan_within((wf_cmap *)table, cursor, count, limit, set, needs, tag_offset, tag_width, none_tag);
}

uint64_t wf__keyed_table_scan(void *table, uint64_t cursor, uint64_t count, wf_key_set *set, uint64_t tag_offset,
                              uint32_t tag_width, uint64_t none_tag) {
    return wf_cmap_scan((wf_cmap *)table, cursor, count, set, tag_offset, tag_width, none_tag);
}

/* The cleared entries go to a map of their own, which release, the table's
 * drop helper, drains and frees [SHARE-1]: once the statement's hold is given
 * up (wf__table_hold_release), or at once under the map's own hold, which
 * only shared_new or the map's release gives up, so that the values a clear
 * took out are released before the program goes on from it. */
void wf__keyed_table_clear(void *table, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag,
                           void (*release)(void *)) {
    wf_cmap *map = (wf_cmap *)table;
    table_whole(map);
    wf_cmap_clear(map, tag_offset, tag_width, none_tag, release);
    if (map->whole_hold == map->local_hold)
        wf_cmap_release_cleared(wf_cmap_take_cleared(map));
}

uint64_t wf__keyed_table_release_reserve(void *table) { return wf_cmap_release_reserve((wf_cmap *)table); }

/* No statement reaches a table that is freed, so no guard's watch is
 * registered on it; one still registered would be left on a freed list. */
void wf__keyed_table_free(void *table) {
    wf_cmap *map = (wf_cmap *)table;
    table_settle(map, 0, 1, 0);
    if (__atomic_load_n(&map->watch.count, __ATOMIC_RELAXED) != 0u)
        abort();
    wf_cmap_destroy(map);
}

/* The table's watches stay with it: a statement that holds a whole and
 * swaps writes it, which its hold's release reports (wf_cmap_hold_release). */
void wf__keyed_table_swap(void *a, void *b, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag) {
    wf_cmap_swap((wf_cmap *)a, (wf_cmap *)b, tag_offset, tag_width, none_tag);
}

void *wf__table_lock_entry(void *table, const unsigned char *key, uint64_t length, uint32_t read,
                           wf_table_entry *entry) {
    wf_cmap *map = (wf_cmap *)table;
    wf_cmap_user *u = wf_cmap_user_at(map, wf__driver_index());
    int held = wf_cmap_holds_whole(u);
    entry->user = u;
    entry->read = (read & 1u) != 0u;
    entry->held = (uint32_t)held;
    /* Flags: shared read, stable absence, non-inserting exclusive access. */
    if (entry->read)
        return access_existing_entry(u, key, length, held, &entry->inner, 0, (read & 2u) != 0);
    if ((read & 4u) != 0)
        return wf_cmap_lock_present_entry(u, key, length, held, (read & 2u) != 0, &entry->inner);
    return wf_cmap_lock_entry(u, key, length, held, &entry->inner);
}

/* A read writes nothing, whatever `present` says. */
void wf__table_unlock_entry(wf_table_entry *entry, uint32_t present) {
    wf_cmap_user *u = entry->user;
    if (entry->read) {
        wf_cmap_unread_entry(u, &entry->inner, (int)entry->held);
        return;
    }
    int had_cell = entry->inner.cell != NULL;
    wf_cmap_unlock_entry(u, &entry->inner, (int)entry->held, present != 0u);
    if (had_cell)
        table_written(u->map);
}

void wf__table_hold_begin(void *hold, void *table) { wf_cmap_hold_begin((wf_cmap_holding *)hold, (wf_cmap *)table); }

void wf__table_hold_read(void *hold) { ((wf_cmap_holding *)hold)->read = 1; }

void wf__table_hold_whole(void *hold) { wf_cmap_hold_whole((wf_cmap_holding *)hold); }

uint64_t wf__table_hold_key(void *hold, const unsigned char *key, uint64_t length) {
    return wf_cmap_hold_key((wf_cmap_holding *)hold, key, length);
}

uint64_t wf__table_hold_keys(void *hold, const wf_key_set *set) {
    return wf_cmap_hold_keys((wf_cmap_holding *)hold, set);
}

/* The user is the taking driver's, since a statement may suspend between
 * building its hold and taking it, but not while it holds a table. */
void wf__table_hold_take(void *hold) {
    wf_cmap_holding *h = (wf_cmap_holding *)hold;
    wf_cmap_hold_take(wf_cmap_user_at(h->map, wf__driver_index()), h);
}

void *wf__table_hold_slot(void *hold, uint64_t position) {
    const wf_cmap_holding *selection = hold;
    if (selection->wants == WF_TABLE_READ_SELECTION) {
        uint64_t length;
        const unsigned char *key = wf_cmap_key_set_key(selection->table, position, &length);
        void *slot = wf_cmap_held_entry(selection->map, key, length, 0);
        return slot != NULL ? slot : selection->map->none;
    }
    return wf_cmap_hold_slot(selection, position);
}

void wf__table_hold_release(void *hold, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag) {
    wf_cmap_holding *h = (wf_cmap_holding *)hold;
    wf_cmap *map = h->map;
    /* Taken while the hold still keeps every other clear out. */
    wf_cmap *cleared = h->user != NULL && h->whole && !h->read && map->whole_hold == h
                           ? wf_cmap_take_cleared(map) : NULL;
    if (wf_cmap_hold_release(h, tag_offset, tag_width, none_tag))
        table_written(map);
    wf_cmap_release_cleared(cleared);
}

void wf__watch_table(void *watch, void *table) { wf__watch_unit(watch, &((wf_cmap *)table)->watch); }
