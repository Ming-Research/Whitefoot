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
 * the caller fills. A keyed statement locks one entry, or reads it beside
 * other readers; a statement over the map holds every entry, or a hold of
 * the entries whose keys it was given. */
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
 * statements over the whole map hold it in the order they asked. While a
 * user holds it, wf_cmap_holds_whole answers nonzero, and that user's statement
 * locks the entries it names as one holding the whole map does. */
void wf_cmap_hold(wf_cmap_user *user);
void wf_cmap_unhold(wf_cmap_user *user);
int wf_cmap_holds_whole(const wf_cmap_user *user);

/* A key set: distinct byte-string keys in increasing lexicographic order of
 * their bytes, a proper prefix first, each with a 64-bit payload. `len` is
 * the number of keys; `store` is the set's memory, NULL while it has none,
 * taken from the includer's host. */
typedef struct wf_key_set {
    uint64_t len;
    void *store;
} wf_key_set;

/* An empty set with room for capacity keys. */
void wf_key_set_new(wf_key_set *set, uint64_t capacity);
/* Adds key with payload when the set lacks it, else replaces its payload. */
void wf_key_set_put(wf_key_set *set, const unsigned char *key, uint64_t length, uint64_t payload);
/* Adds key with payload amount when the set lacks it, else adds amount to
 * its payload modulo 2^64. */
void wf_key_set_add(wf_key_set *set, const unsigned char *key, uint64_t length, uint64_t amount);
/* The payload and the bytes of the key at index, below len; the bytes stay
 * where they are until the set next changes. */
uint64_t wf_key_set_payload(const wf_key_set *set, uint64_t index);
const unsigned char *wf_key_set_key(const wf_key_set *set, uint64_t index, uint64_t *length);
/* Gives the set's memory back, leaving it empty; or gives back the memory
 * of a set whose `store` alone is at hand, NULL for none. */
void wf_key_set_release(wf_key_set *set);
void wf_key_set_free_store(void *store);

/* A hold of several entries of one map, which a statement keeps in its own
 * frame from wf_cmap_hold_begin to wf_cmap_hold_release, so that one
 * statement may hold entries of several maps, and of several header
 * bindings of one map, at once. Each key added has a position, counted from
 * zero in the order of addition; repeated keys share one entry.
 *
 * One added key: its bytes, which stay as they are until the hold is taken;
 * its locked cell and slot, the cell kept only by the first of equal keys,
 * its leader; `rank`, which in the record at index i names the position of
 * the key that is i-th in byte order; whether its entry was created for the
 * hold; and whether it leads its run of equal keys. */
typedef struct {
    const unsigned char *key;
    uint64_t length;
    void *cell;
    void *slot;
    uint64_t rank;
    uint32_t fresh;
    uint32_t leads;
} wf_cmap_held;

/* Keys a hold keeps in itself before it takes memory from the host. */
#define WF_CMAP_HOLD_INLINE 4

typedef struct wf_cmap_holding {
    wf_cmap *map;
    /* The user that took it, NULL until it is taken. */
    wf_cmap_user *user;
    /* The added keys: NULL while they fit in inline_keys, else host memory
     * of room keys, given back at the release. */
    wf_cmap_held *keys;
    uint64_t count;
    uint64_t room;
    /* The table its cells were locked in. */
    void *table;
    /* The map's generation when it was taken (wf_cmap_swap). */
    uint32_t generation;
    /* How the added keys stand: increasing, nondecreasing, or neither. */
    uint8_t order;
    /* It was asked to hold the whole map. */
    uint8_t wants;
    /* It holds the whole map, having taken it itself. */
    uint8_t whole;
    /* The map was held whole before it was taken. */
    uint8_t held;
    wf_cmap_held inline_keys[WF_CMAP_HOLD_INLINE];
} wf_cmap_holding;

/* Begins an empty hold of map's entries. */
void wf_cmap_hold_begin(wf_cmap_holding *hold, wf_cmap *map);
/* Asks the hold, before it is taken, to hold the whole map: the take waits
 * out every statement holding entries of the map and every whole hold
 * before it, as wf_cmap_hold does, keeps new ones out until the release,
 * and locks the added keys' entries under that hold; with no key added it
 * holds the map only, as a statement that counts the map does. */
void wf_cmap_hold_whole(wf_cmap_holding *hold);
/* Adds one key, or every key of a set in the set's order, and answers the
 * position of the key, or of the set's first key, the set's key i being at
 * that position plus i. The set stays as it is until the hold is taken. */
uint64_t wf_cmap_hold_key(wf_cmap_holding *hold, const unsigned char *key, uint64_t length);
uint64_t wf_cmap_hold_keys(wf_cmap_holding *hold, const wf_key_set *set);
/* Holds the entries of the added keys together, creating the absent ones,
 * for a statement that reaches no other entry of the map: their cells are
 * locked in increasing byte order of the keys, without repeats, the order
 * every hold uses, so two holds never wait for each other in a cycle. When
 * it would wait past its patience, meets a cell it holds itself, which two
 * of its keys of one hash make it do, or finds the table full, it gives
 * everything back and holds the whole map instead, as wf_cmap_hold does,
 * setting hold->whole; when user already holds the whole map, it locks the
 * entries under that hold. */
void wf_cmap_hold_take(wf_cmap_user *user, wf_cmap_holding *hold);
/* The slot of the key added at position, below the count added. */
void *wf_cmap_hold_slot(const wf_cmap_holding *hold, uint64_t position);
/* Releases what wf_cmap_hold_take holds: each entry kept when its slot's
 * tag, the tag_width bytes (1, 2, 4 or 8) at tag_offset, differs from
 * none_tag, else removed with its slot; then the whole map when the hold
 * took it. A hold never taken only gives its memory back. Either way the
 * hold is empty after. Answers 1 when the hold may have written the map:
 * it held entries, or the map's entries were swapped since the take, whose
 * held entries then went with the other map. */
int wf_cmap_hold_release(wf_cmap_holding *hold, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag);

/* Exchanges the entries of two maps of one slot layout, with their counts,
 * tables and memory, while each map keeps its identity: its gate, its whole
 * holds' line, its users and their marks, and the includer's members. No
 * other statement may be inside either map: the caller holds a's whole
 * map, or neither map is shared. */
void wf_cmap_swap(wf_cmap *a, wf_cmap *b);

/* The number of entries, exact while the map is held or has no users; a
 * hold's own entries count as they stood when it was taken. */
uint64_t wf_cmap_count(wf_cmap *map);
/* With no users left: the slot of an entry not yet drained, whose value the
 * caller releases before calling again, or NULL once every entry has been. */
void *wf_cmap_drain(wf_cmap *map);

#endif
