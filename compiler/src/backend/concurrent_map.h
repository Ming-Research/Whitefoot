/* The runtime's concurrent map: a hash index from 64-bit keys to 64-bit
 * values that many threads read and change at once. A change to a key runs
 * once with that key's entry held exclusively; a read takes no lock and
 * copies the value out. Its design and the measurements that chose it are in
 * research/investigations/concurrent-map/DESIGN.md.
 *
 * Keys lie in [1, 2^62 - 2]. Every operation goes through a user, which one
 * thread holds at a time from wf_cmap_enter to wf_cmap_leave.
 */
#ifndef WF_CONCURRENT_MAP_H
#define WF_CONCURRENT_MAP_H

#include <stdint.h>

typedef struct wf_cmap wf_cmap;
typedef struct wf_cmap_user wf_cmap_user;

/* Users a map can have at once. */
#define WF_CMAP_MAX_USERS 64

/* A map sized for capacity keys, or a small default when capacity is zero. */
wf_cmap *wf_cmap_create(uint64_t capacity);
/* Frees the map; it has no users left. */
void wf_cmap_destroy(wf_cmap *map);
/* A user of map for the calling thread; NULL when map already has
 * WF_CMAP_MAX_USERS. */
wf_cmap_user *wf_cmap_enter(wf_cmap *map);
void wf_cmap_leave(wf_cmap_user *user);
/* 1 and the value when key is present, else 0. */
int wf_cmap_get(wf_cmap_user *user, uint64_t key, uint64_t *value);
/* Inserts or replaces; 1 when key was absent. */
int wf_cmap_insert(wf_cmap_user *user, uint64_t key, uint64_t value);
/* 1 when key was present and is now removed. */
int wf_cmap_remove(wf_cmap_user *user, uint64_t key);
/* Runs edit once on key's value with its entry held exclusively; 1 when key
 * was present, 0 without running edit otherwise. */
int wf_cmap_update(wf_cmap_user *user, uint64_t key, void (*edit)(uint64_t *value, void *env), void *env);

/* A map of entries sized for capacity keys: byte-string keys of any length,
 * each with a slot of slot_size bytes aligned to slot_align, at most 16, that
 * the caller fills. A keyed statement locks one entry; a statement over the
 * whole map holds every entry. */
wf_cmap *wf_cmap_create_entries(uint64_t slot_size, uint64_t slot_align, uint64_t capacity);
/* The user numbered index, below WF_CMAP_MAX_USERS, which one thread holds
 * at a time; a runtime numbers its threads and never leaves. */
wf_cmap_user *wf_cmap_user_at(wf_cmap *map, unsigned index);

/* What a locked entry's unlock needs. */
typedef struct {
    void *cell;
    void *table;
    uint32_t fresh;
    uint32_t upgraded; /* the statement holds the whole map to lock it */
} wf_cmap_entry;

/* Locks key's entry, creating it when absent, and returns its slot's address;
 * entry->fresh is 1 when the entry was created, its slot filled with zeros.
 * With held set, the caller holds the whole map. */
void *wf_cmap_lock_entry(wf_cmap_user *user, const unsigned char *key, uint64_t length, int held,
                         wf_cmap_entry *entry);
/* Unlocks the entry: kept when present, else removed with its slot, which
 * then holds nothing to release. */
void wf_cmap_unlock_entry(wf_cmap_user *user, wf_cmap_entry *entry, int held, int present);
/* For a keyed statement whose block only reads its entry: the slot of key's
 * entry, read beside other such statements and no writer, or a slot of
 * zeros no statement writes when the key is absent; wf_cmap_unread_entry
 * ends the read. */
const void *wf_cmap_read_entry(wf_cmap_user *user, const unsigned char *key, uint64_t length, int held,
                               wf_cmap_entry *entry);
void wf_cmap_unread_entry(wf_cmap_user *user, wf_cmap_entry *entry, int held);
/* Holds every entry of the map, waiting out keyed statements under way;
 * statements over the whole map hold it in the order they asked. */
void wf_cmap_hold(wf_cmap_user *user);
void wf_cmap_unhold(wf_cmap_user *user);

/* One key of a set of entries a statement holds together: its bytes, which
 * stay as they are until the set is released; its hash; its locked cell and
 * slot; whether the entry was created for the set; and whether its slot
 * holds a value, which the caller keeps current and the release reads. */
typedef struct {
    const unsigned char *key;
    uint64_t length;
    uint64_t tag;
    void *cell;
    void *slot;
    uint32_t fresh;
    uint32_t present;
} wf_cmap_held;

/* The keys a statement over a map reaches, collected before it begins, and
 * how it holds them: each key's entry, or the whole map. */
typedef struct {
    wf_cmap_held *entries;
    uint64_t count;
    uint64_t room;
    void *table;
    uint32_t whole;
} wf_cmap_set;

/* Empties a set, keeping its storage. */
void wf_cmap_set_clear(wf_cmap_set *set);
/* Adds a key; a key may be added more than once. */
void wf_cmap_set_add(wf_cmap_set *set, const unsigned char *key, uint64_t length);
/* Holds the entries of the set's keys together, creating the absent ones,
 * for a statement that reaches no other entry of the map: the keys are
 * locked in one order every such statement uses, so two of them never wait
 * for each other in a cycle. When it would wait past its patience, meets
 * two of its keys with one hash, or finds the table full, it gives
 * everything back and holds the whole map instead, setting set->whole, as
 * wf_cmap_hold does. */
void wf_cmap_hold_set(wf_cmap_user *user, wf_cmap_set *set);
/* The held entry of key in a set whose entries are held, or NULL when the
 * key is not one of the set's. */
wf_cmap_held *wf_cmap_set_find(wf_cmap_set *set, const unsigned char *key, uint64_t length);
/* Releases what wf_cmap_hold_set holds: each entry kept when present, else
 * removed with its slot, or the whole map. */
void wf_cmap_release_set(wf_cmap_user *user, wf_cmap_set *set);
/* The number of entries, exact while the map is held or has no users. */
uint64_t wf_cmap_count(wf_cmap *map);
/* With no users left: the slot of an entry not yet drained, whose value the
 * caller releases before calling again, or NULL once every entry has been. */
void *wf_cmap_drain(wf_cmap *map);

#endif
