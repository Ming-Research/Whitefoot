#ifndef WF_CONCURRENT_MAP_LAYOUT_H
#define WF_CONCURRENT_MAP_LAYOUT_H

/* Private representation shared by the ordinary map and restricted snapshot
 * walker. Contains data layouts only; the public map interface is unchanged.
 * The includer supplies the same WF_CMAP_HOST_FIELDS used for its map. */
#include <stddef.h>
#include <stdatomic.h>
#include <stdint.h>
#include "concurrent_map.h"

#define LOCKED (1ull << 63)
#define PENDING (1ull << 62)
#define KEY_MASK ((1ull << 62) - 1)
#define EMPTY 0ull
#define REMOVED KEY_MASK
/* A map of entries keeps, in the top bits of a cell's value word above the
 * node's address, how many keyed statements that only read the entry are
 * under way (wf_cmap_read_entry). */
#define READER_SHIFT 48
#define READER_ONE (1ull << READER_SHIFT)
#define ADDRESS_MASK (READER_ONE - 1)
/* Cells a map created without a capacity starts with, 64 KiB. */
#define DEFAULT_CELLS 4096ull
/* The most keys a map's first table is sized for: 2^27 cells of 16 bytes. */
#define CAPACITY_LIMIT (1ull << 26)
#define MIN_CELLS 16ull
/* Cells one helper moves at a time. */
#define BLOCK 4096ull
#define HUGE_BYTES ((size_t)2 << 20)

typedef struct cell {
    _Atomic uint64_t key;
    _Atomic uint64_t value;
} cell;

_Static_assert(sizeof(cell) == 16, "four cells share a cache line");

typedef struct table {
    cell *cells;
    uint64_t capacity;
    uint64_t mask;
    unsigned shift; /* 64 minus the index bits */
    _Atomic(struct table *) next;
    _Atomic uint64_t claimed; /* blocks handed to movers */
    _Atomic uint64_t moved;   /* blocks moved */
    _Atomic int starting;     /* set by the one writer that makes next */
    int64_t base;             /* claims counted before this table was current */
    struct table *older;      /* moved tables not yet freed, under the map's lock */
} table;

/* Entries are carved in multiples of ENTRY_GRAIN bytes from chunks of
 * ENTRY_CHUNK each user maps from the host, and reused through the user's
 * free lists; an entry larger than ENTRY_LARGEST comes from the pool. */
#define ENTRY_GRAIN 16u
#define ENTRY_CLASSES 32u
#define ENTRY_LARGEST (ENTRY_GRAIN * ENTRY_CLASSES)
#define ENTRY_CHUNK ((size_t)1 << 20)

typedef struct free_entry {
    struct free_entry *next;
} free_entry;

typedef struct chunk {
    struct chunk *older;
} chunk;

/* One thread's use of the map: its published table address, an integer
 * token that may outlive the allocation and is never dereferenced; the
 * cells it claimed and the keys it added less those it removed, summed only
 * when a claim may cross the threshold; whether it is inside a keyed
 * statement, which a statement over the whole map waits out; how long its
 * keyed statement has waited for cells and may wait before it holds the
 * map; and its entries' free memory.
 * Then whether it holds the whole map; the hold whose cells it is locking,
 * which a probe of one of its keys may meet; and the memory of an earlier
 * hold's keys, kept for the next. */
struct wf_cmap_user {
    _Alignas(64) _Atomic uintptr_t in;
    _Atomic int64_t used;
    _Atomic int64_t live;
    _Atomic int active;
    wf_cmap *map;
    uint64_t waited;
    uint64_t patience;
    _Atomic int busy;
    free_entry *free[ENTRY_CLASSES];
    char *cursor;
    size_t room;
    chunk *chunks;
    int holding;
    const struct wf_cmap_holding *own;
    wf_cmap_held *spare_keys;
    uint64_t spare_room;
};

struct wf_cmap {
    _Alignas(64) _Atomic(table *) current;
    _Alignas(64) _Atomic int64_t folded_used;
    _Atomic int64_t folded_live;
    _Atomic int users_seen;
    _Atomic int lock;              /* guards the moved tables and the spare cells */
    _Atomic(table *) retired;      /* moved tables, newest first */
    cell *spare;                   /* cells of the last table freed, or NULL */
    uint64_t spare_capacity;
    /* A map of entries: the size and alignment of an entry's slot, zero for
     * a map of words. */
    uint64_t slot_size;
    uint64_t slot_align;
    /* A slot of zeros, `None`, that a statement reading an absent key reads
     * and none writes. */
    void *none;
#ifdef WF_CMAP_HOST_FIELDS
    WF_CMAP_HOST_FIELDS
#endif
    /* Set while one statement holds the whole map, how many keyed
     * statements found it set and wait to begin, and the tickets that order
     * statements over the whole map: the next one to take and the one whose
     * turn it is. */
    _Alignas(64) _Atomic int gate;
    _Atomic int waiting;
    _Atomic uint64_t hold_next;
    _Atomic uint64_t hold_serving;
    /* Where wf_cmap_drain has reached, and the node it handed out last,
     * released on the next call, after the caller releases its value. */
    uint64_t drained;
    void *pending;
    uint64_t pending_bytes;
    /* Chunks of entries the users carved before the map's entries were last
     * swapped (wf_cmap_swap), which go with those entries, and how many
     * swaps the map has had. */
    chunk *chunks;
    uint32_t generation;
    /* The hold that holds the map whole, from its take to its release, whose
     * entries a swap settles before it exchanges the map's (wf_cmap_swap). */
    wf_cmap_holding *whole_hold;
    /* Maps holding the entries a clear took out under the whole hold
     * (wf_cmap_clear), released after it: the list's head on the cleared
     * map, and on each listed map the next one and its release. */
    struct wf_cmap *cleared;
    struct wf_cmap *cleared_next;
    void (*cleared_release)(void *);
    void *raw;
    wf_cmap_user users[WF_CMAP_MAX_USERS];
};

typedef struct node {
    uint64_t length;
    unsigned char bytes[];
} node;

#endif

