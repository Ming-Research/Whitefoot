/* The runtime's concurrent map (concurrent_map.h). Its design and the
 * measurements behind it are in research/investigations/concurrent-map/
 * DESIGN.md, "The index".
 *
 * Open addressing with linear probing over 16-byte cells, a key word and a
 * value, so that most operations touch one cache line. The key word's top
 * bit locks the cell, and in a map of entries the next marks a claim not yet
 * settled; zero is an empty cell and KEY_MASK a removed one, so keys lie in
 * [1, 2^62 - 2].
 *
 * A writer locks its key's cell by compare-and-swap, runs once and stores
 * the key back: changes to a key are exclusive. A reader takes no lock: it
 * waits while the cell is locked, since a claimed cell has no value yet, and
 * then reads the value. A cell never holds another key and its value is
 * written by one store, so the value read is one the key held during the
 * read; an entry larger than a word will need a version. A removed key
 * leaves its cell marked removed until the table is moved, so a probe never
 * stops early, and a probe that has seen every cell stops there.
 *
 * A table is moved, to a larger one or to one of its own size that drops
 * removed cells, once half its cells are used: one writer makes the next
 * table, and writers that meet the move copy blocks of cells into it and
 * then retry there. A writer checks for a move after it locks its cell and
 * gives the cell back unchanged when one has begun, and a mover publishes
 * the move before it reads a cell and waits while the cell is locked, all
 * sequentially consistent: either the mover sees the writer's lock, or the
 * writer sees the move. So a mover only reads the cells, and readers go on
 * reading cells no writer changes again. Each user publishes the table it works in, as
 * growt's handles do, and a moved table is freed once no user is in it; the
 * cells of the last one freed are kept for the next move to that size, so a
 * map whose size holds steady moves between warm tables.
 *
 * Built with WF_CMAP_LOCKED_READ, a read locks its cell like a writer, the
 * read an atomic statement gets when every statement holds its entry
 * exclusively; it is kept for measurement beside the lock-free read.
 *
 * The file that includes this one supplies the host: WF_CMAP_TAKE(bytes) and
 * WF_CMAP_GIVE(block, bytes) for small blocks aligned to 16 bytes, which the
 * runtime takes from its own pool and never from the program's allocator
 * [STOR-8]; WF_CMAP_YIELD() to give up the processor; and
 * WF_CMAP_EXHAUSTED() when memory is short. Cell arrays of 2 MiB or more and
 * the chunks entries are carved from are mapped from the host here.
 */
#if !defined(WF_CMAP_TAKE) || !defined(WF_CMAP_GIVE) || !defined(WF_CMAP_YIELD) || !defined(WF_CMAP_EXHAUSTED)
#error "the includer supplies WF_CMAP_TAKE, WF_CMAP_GIVE, WF_CMAP_YIELD and WF_CMAP_EXHAUSTED"
#endif

#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#include <windows.h>
#else
#include <sys/mman.h>
#endif

#include "concurrent_map.h"

/* The map's test drives interleavings through these points: a writer that
 * is about to claim an empty or removed cell for an entry's key, and one
 * about to lock a cell of its key's hash. */
#ifndef WF_CMAP_BEFORE_CLAIM
#define WF_CMAP_BEFORE_CLAIM(t, index) ((void)0)
#endif
#ifndef WF_CMAP_BEFORE_LOCK
#define WF_CMAP_BEFORE_LOCK(c) ((void)0)
#endif
/* And a reader that has found its entry, before it looks for a move. */
#ifndef WF_CMAP_READ_FOUND
#define WF_CMAP_READ_FOUND(t) ((void)0)
#endif
/* And a statement over the whole map that has taken its place in line, and
 * one that has closed the gate to keyed statements. */
#ifndef WF_CMAP_HOLD_QUEUED
#define WF_CMAP_HOLD_QUEUED(u) ((void)0)
#endif
#ifndef WF_CMAP_HOLD_CLOSED
#define WF_CMAP_HOLD_CLOSED(u) ((void)0)
#endif

/* The pauses a keyed statement waits for cells, with each retry it makes
 * counted as one, over every probe and table it tries, past which it holds
 * the whole map instead (wf_cmap_lock_entry): 0.77 ms on the measuring
 * host, far past a statement's usual wait, so that the hold is a bound and
 * not a path the map takes under ordinary contention. A waiting keyed
 * statement does not park, so it counts pauses where a parked object
 * statement counts its vain wakes. The map's test sets it per user. */
#define PATIENCE (1ull << 16)
#ifndef WF_CMAP_PATIENCE
#define WF_CMAP_PATIENCE(u) PATIENCE
#endif

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

/* One thread's use of the map: the table it is in; the cells it claimed and
 * the keys it added less those it removed, summed only when a claim may cross
 * the threshold; whether it is inside a keyed statement, which a statement
 * over the whole map waits out; how long its keyed statement has waited for
 * cells and may wait before it holds the map; and its entries' free memory. */
struct wf_cmap_user {
    _Alignas(64) _Atomic(table *) in;
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
    /* Set while one statement holds the whole map, how many keyed
     * statements found it set and wait to begin, and the tickets that order
     * statements over the whole map: the next one to take and the one whose
     * turn it is. */
    _Alignas(64) _Atomic int gate;
    _Atomic int waiting;
    _Atomic uint64_t hold_next;
    _Atomic uint64_t hold_serving;
    /* Where wf_cmap_drain has reached, and a large node it handed out last,
     * given back to the pool on the next call. */
    uint64_t drained;
    void *pending;
    uint64_t pending_bytes;
    void *raw;
    wf_cmap_user users[WF_CMAP_MAX_USERS];
};

static inline void pause_once(void) {
#if defined(__x86_64__) || defined(__i386__)
    __builtin_ia32_pause();
#elif defined(__aarch64__)
    __asm__ volatile("yield");
#endif
}

/* Waits a little longer each round, then yields the processor. */
static inline void back_off(unsigned *round) {
    unsigned spins = *round < 6 ? 1u << *round : 64u;
    for (unsigned i = 0; i < spins; i++)
        pause_once();
    if (++*round > 64) {
        WF_CMAP_YIELD();
        *round = 6;
    }
}

/* A writer that finds its key's cell locked waits 16 pauses, then twice as
 * long each time up to 1024, about 0.2 to 12 microseconds on the measuring
 * host and as long as a sleep and wake by the system: the holder then makes
 * many changes in a row rather than handing the cell's line to a waiter on
 * each. Answers the pauses it waited. A writer of a word's key can be
 * overtaken without bound; a keyed statement that has waited its patience
 * holds the whole map instead (wf_cmap_lock_entry). */
static inline unsigned wait_for_cell(unsigned *round) {
    unsigned shift = *round < 6 ? *round + 4 : 10;
    for (unsigned i = 0; i < (1u << shift); i++)
        pause_once();
    if (*round < 6)
        ++*round;
    return 1u << shift;
}

/* One multiplication by the 64-bit golden ratio: cheap, and it spreads keys
 * over the high bits the starting cell is taken from. */
static inline uint64_t start_of(const table *t, uint64_t key) { return (key * 0x9E3779B97F4A7C15ull) >> t->shift; }

static void bad_key(void) { abort(); }

static inline void check_key(uint64_t key) {
    if (__builtin_expect(key == EMPTY || key >= REMOVED, 0))
        bad_key();
}

/* Host memory, zeroed, in a run of whole pages: aligned to and advised into
 * huge pages when it is 2 MiB or more, since a random probe in a large table
 * otherwise pays a page walk on most accesses. */
static void *host_map(size_t bytes) {
#if defined(_WIN32)
    return VirtualAlloc(NULL, bytes, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
#else
    if (bytes < HUGE_BYTES) {
        void *p = mmap(NULL, bytes, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        return p == MAP_FAILED ? NULL : p;
    }
    char *raw = mmap(NULL, bytes + HUGE_BYTES, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (raw == MAP_FAILED)
        return NULL;
    char *start = (char *)(((uintptr_t)raw + HUGE_BYTES - 1) & ~(uintptr_t)(HUGE_BYTES - 1));
    if (start > raw)
        munmap(raw, (size_t)(start - raw));
    munmap(start + bytes, (size_t)(raw + HUGE_BYTES - start));
#if defined(MADV_HUGEPAGE) && !defined(WF_CMAP_NO_HUGE)
    madvise(start, bytes, MADV_HUGEPAGE);
#endif
    return start;
#endif
}

static void host_unmap(void *p, size_t bytes) {
#if defined(_WIN32)
    (void)bytes;
    VirtualFree(p, 0, MEM_RELEASE);
#else
    munmap(p, bytes);
#endif
}

static void *take(size_t bytes) {
    void *p = WF_CMAP_TAKE(bytes);
    if (p == NULL)
        WF_CMAP_EXHAUSTED();
    return p;
}

/* Cell arrays of 2 MiB or more come from the host, so that their pages
 * arrive zeroed when first touched, by whichever mover touches them, instead
 * of being cleared up front by one thread; smaller ones from the pool. */
static cell *new_cells(uint64_t count) {
    size_t bytes = count * sizeof(cell);
    if (bytes < HUGE_BYTES) {
        cell *c = take(bytes);
        memset(c, 0, bytes);
        return c;
    }
    cell *c = host_map(bytes);
    if (c == NULL)
        WF_CMAP_EXHAUSTED();
    return c;
}

static void free_cells(cell *c, uint64_t count) {
    size_t bytes = count * sizeof(cell);
    if (bytes < HUGE_BYTES)
        WF_CMAP_GIVE(c, bytes);
    else
        host_unmap(c, bytes);
}

static void lock_map(wf_cmap *map) {
    unsigned round = 0;
    while (atomic_exchange_explicit(&map->lock, 1, memory_order_acquire))
        back_off(&round);
}

static void unlock_map(wf_cmap *map) { atomic_store_explicit(&map->lock, 0, memory_order_release); }

/* A table of at least capacity cells, a power of two, on the map's spare
 * cells when they are that size. */
static table *new_table(wf_cmap *map, uint64_t capacity) {
    table *t = take(sizeof *t);
    memset(t, 0, sizeof *t);
    unsigned bits = 0;
    while ((1ull << bits) < capacity)
        bits++;
    t->capacity = 1ull << bits;
    t->mask = t->capacity - 1;
    t->shift = 64 - bits;
    cell *spare = NULL;
    if (map != NULL) {
        lock_map(map);
        if (map->spare != NULL && map->spare_capacity == t->capacity) {
            spare = map->spare;
            map->spare = NULL;
        }
        unlock_map(map);
    }
    if (spare != NULL) {
        memset(spare, 0, t->capacity * sizeof(cell));
        t->cells = spare;
    } else {
        t->cells = new_cells(t->capacity);
    }
    return t;
}

static void free_table(table *t) {
    free_cells(t->cells, t->capacity);
    WF_CMAP_GIVE(t, sizeof *t);
}

static void count(wf_cmap_user *u, int64_t used, int64_t live) {
    atomic_store_explicit(&u->used, atomic_load_explicit(&u->used, memory_order_relaxed) + used,
                          memory_order_relaxed);
    atomic_store_explicit(&u->live, atomic_load_explicit(&u->live, memory_order_relaxed) + live,
                          memory_order_relaxed);
}

static void totals(wf_cmap *map, int64_t *used, int64_t *live) {
    int64_t u = atomic_load_explicit(&map->folded_used, memory_order_relaxed);
    int64_t l = atomic_load_explicit(&map->folded_live, memory_order_relaxed);
    int n = atomic_load_explicit(&map->users_seen, memory_order_acquire);
    for (int i = 0; i < n; i++) {
        u += atomic_load_explicit(&map->users[i].used, memory_order_relaxed);
        l += atomic_load_explicit(&map->users[i].live, memory_order_relaxed);
    }
    *used = u;
    *live = l;
}

/* Frees the moved tables no user is in, keeping the cells of the newest.
 * A table is retired after the map's current table has changed, and a user
 * publishes its table before it checks that the table is still current, all
 * sequentially consistent: either this scan sees the user in the table, or
 * the user sees that the table is no longer current. */
static void reclaim(wf_cmap *map) {
    if (atomic_load_explicit(&map->lock, memory_order_relaxed) ||
        atomic_exchange_explicit(&map->lock, 1, memory_order_acquire))
        return;
    int n = atomic_load_explicit(&map->users_seen, memory_order_seq_cst);
    table *kept = NULL, **tail = &kept;
    int spared = 0;
    for (table *r = atomic_load_explicit(&map->retired, memory_order_relaxed), *older; r != NULL; r = older) {
        older = r->older;
        int in_use = 0;
        for (int i = 0; i < n && !in_use; i++)
            in_use = atomic_load_explicit(&map->users[i].in, memory_order_seq_cst) == r;
        if (in_use) {
            *tail = r;
            tail = &r->older;
        } else if (spared) {
            free_table(r);
        } else {
            if (map->spare != NULL)
                free_cells(map->spare, map->spare_capacity);
            map->spare = r->cells;
            map->spare_capacity = r->capacity;
            WF_CMAP_GIVE(r, sizeof *r);
            spared = 1;
        }
    }
    *tail = NULL;
    atomic_store_explicit(&map->retired, kept, memory_order_relaxed);
    unlock_map(map);
}

/* The current table, published as the one user u is in. */
static table *switch_table(wf_cmap_user *u) {
    wf_cmap *map = u->map;
    table *t = atomic_load_explicit(&map->current, memory_order_acquire);
    for (;;) {
        atomic_store_explicit(&u->in, t, memory_order_seq_cst);
        table *now = atomic_load_explicit(&map->current, memory_order_seq_cst);
        if (now == t)
            break;
        t = now;
    }
    if (atomic_load_explicit(&map->retired, memory_order_relaxed) != NULL)
        reclaim(map);
    return t;
}

static inline table *use_current(wf_cmap_user *u) {
    table *t = atomic_load_explicit(&u->map->current, memory_order_acquire);
    if (__builtin_expect(atomic_load_explicit(&u->in, memory_order_relaxed) == t, 1))
        return t;
    return switch_table(u);
}

/* Places a key moved from an older table. Only movers write a table that is
 * not yet current and no two move the same key, but two may reach one empty
 * cell, so a mover claims the cell before it writes the value. */
static void place(table *t, uint64_t key, uint64_t value) {
    for (uint64_t i = start_of(t, key);; i = (i + 1) & t->mask) {
        uint64_t expected = EMPTY;
        if (atomic_compare_exchange_strong_explicit(&t->cells[i].key, &expected, key | LOCKED,
                                                    memory_order_relaxed, memory_order_relaxed)) {
            atomic_store_explicit(&t->cells[i].value, value, memory_order_relaxed);
            atomic_store_explicit(&t->cells[i].key, key, memory_order_relaxed);
            return;
        }
    }
}

/* Copies one block of t's keys into t's successor, waiting out the writers
 * that hold a cell; a writer that locks a cell after this read sees the
 * move and leaves the cell as it was. */
static void move_block(wf_cmap *map, table *t, table *nt, uint64_t block) {
    uint64_t end = (block + 1) * BLOCK < t->capacity ? (block + 1) * BLOCK : t->capacity;
    for (uint64_t i = block * BLOCK; i < end; i++) {
        cell *c = &t->cells[i];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_seq_cst);
        unsigned round = 0;
        while (k & LOCKED) {
            back_off(&round);
            k = atomic_load_explicit(&c->key, memory_order_seq_cst);
        }
        if (k == EMPTY || k == REMOVED)
            continue;
        uint64_t value = atomic_load_explicit(&c->value, memory_order_seq_cst);
        if (map->slot_size != 0) {
            /* A reader that counted itself before this read finds the move
             * and leaves; one under way reads until it ends. */
            round = 0;
            while (value >> READER_SHIFT) {
                back_off(&round);
                value = atomic_load_explicit(&c->value, memory_order_seq_cst);
            }
        }
        place(nt, k, value);
    }
}

/* Moves blocks of t until none is left to claim; the mover of the last makes
 * the successor current and retires t. */
static void help(wf_cmap *map, table *t) {
    table *nt = atomic_load_explicit(&t->next, memory_order_acquire);
    uint64_t blocks = (t->capacity + BLOCK - 1) / BLOCK;
    for (;;) {
        uint64_t block = atomic_fetch_add_explicit(&t->claimed, 1, memory_order_relaxed);
        if (block >= blocks)
            return;
        move_block(map, t, nt, block);
        if (atomic_fetch_add_explicit(&t->moved, 1, memory_order_acq_rel) + 1 == blocks) {
            /* Writers wait for the move, so the counts are settled: the new
             * table's used cells are its live keys. */
            int64_t used, live;
            totals(map, &used, &live);
            nt->base = used - live;
            atomic_store_explicit(&map->current, nt, memory_order_seq_cst);
            lock_map(map);
            t->older = atomic_load_explicit(&map->retired, memory_order_relaxed);
            atomic_store_explicit(&map->retired, t, memory_order_relaxed);
            unlock_map(map);
        }
    }
}

/* Helps the move out of t and waits until its successor is current. */
static void finish_move(wf_cmap *map, table *t) {
    help(map, t);
    unsigned round = 0;
    while (atomic_load_explicit(&map->current, memory_order_acquire) == t)
        back_off(&round);
}

/* Cells for keys: the fewest, a power of two, that hold them in at most
 * num/den of the cells. */
static uint64_t cells_for(uint64_t keys, uint64_t num, uint64_t den) {
    uint64_t capacity = MIN_CELLS;
    while (capacity * num < keys * den)
        capacity <<= 1;
    return capacity;
}

/* Makes t's successor, unless another writer is making it: cells that hold
 * the live keys in at most three eighths of them, so that at least an eighth
 * more can be claimed before it moves in turn, and at least half t's size. */
static void start_move(wf_cmap *map, table *t) {
    int idle = 0;
    if (atomic_load_explicit(&t->starting, memory_order_relaxed) != 0 ||
        !atomic_compare_exchange_strong_explicit(&t->starting, &idle, 1, memory_order_relaxed,
                                                 memory_order_relaxed))
        return;
    int64_t used, live;
    totals(map, &used, &live);
    uint64_t capacity = cells_for(live > 0 ? (uint64_t)live : 0, 3, 8);
    if (capacity < t->capacity / 2)
        capacity = t->capacity / 2 > MIN_CELLS ? t->capacity / 2 : MIN_CELLS;
    table *nt = new_table(map, capacity);
    atomic_store_explicit(&t->next, nt, memory_order_seq_cst);
}

enum { FOUND, CLAIMED, ABSENT, FULL, REUSED, IMPATIENT };

/* Locks key's cell in t, or with claim set locks an empty cell for it; FULL
 * when a claim found no empty cell in the whole table. */
static int acquire(table *t, uint64_t key, int claim, cell **out) {
    uint64_t i = start_of(t, key);
    uint64_t left = t->capacity;
    unsigned round = 0;
    for (;;) {
        cell *c = &t->cells[i];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_acquire);
        uint64_t bare = k & ~LOCKED;
        if (bare == key) {
            if (k & LOCKED) {
                wait_for_cell(&round);
                continue;
            }
            if (atomic_compare_exchange_weak_explicit(&c->key, &k, k | LOCKED, memory_order_seq_cst,
                                                      memory_order_relaxed)) {
                *out = c;
                return FOUND;
            }
            continue;
        }
        if (bare == EMPTY) {
            if (!claim)
                return ABSENT;
            if (atomic_compare_exchange_weak_explicit(&c->key, &k, key | LOCKED, memory_order_seq_cst,
                                                      memory_order_relaxed)) {
                *out = c;
                return CLAIMED;
            }
            continue;
        }
        if (--left == 0)
            return claim ? FULL : ABSENT;
        i = (i + 1) & t->mask;
    }
}

/* After acquire answered r for c: 1 when no move out of t has begun, so the
 * cell is the writer's; otherwise 0, with the cell given back unchanged, a
 * claimed one as removed, since another writer may have probed past it to a
 * later cell. */
static int keep_cell(table *t, cell *c, int r, uint64_t key) {
    if (atomic_load_explicit(&t->next, memory_order_seq_cst) == NULL)
        return 1;
    atomic_store_explicit(&c->key, r == FOUND ? key : REMOVED, memory_order_release);
    return 0;
}

/* Locks key's cell in the current table, helping any move it meets. */
static int lock_key(wf_cmap_user *u, uint64_t key, int claim, cell **out, table **in) {
    for (;;) {
        table *t = use_current(u);
        if (atomic_load_explicit(&t->next, memory_order_acquire) == NULL) {
            int r = acquire(t, key, claim, out);
            if (r == ABSENT) {
                *in = t;
                return r;
            }
            if (r == FOUND || r == CLAIMED) {
                if (keep_cell(t, *out, r, key)) {
                    *in = t;
                    return r;
                }
            } else {
                start_move(u->map, t);
                unsigned round = 0;
                while (atomic_load_explicit(&t->next, memory_order_acquire) == NULL)
                    back_off(&round);
            }
        }
        finish_move(u->map, t);
    }
}

static inline void unlock(cell *c, uint64_t key) { atomic_store_explicit(&c->key, key, memory_order_release); }

/* Entries: a map created by wf_cmap_create_entries keeps byte-string keys in
 * nodes, each the key's length and bytes and then a slot of the map's slot
 * size, its address stored in the value word of the key's cell. A cell's key
 * word holds the key's hash in place of the key, and a probe compares a key's
 * bytes only once it has locked a cell of the same hash, so a node is freed
 * under its cell's lock and no probe reads a freed node. Every operation on
 * an entry locks its cell, reading or writing. A key found absent claims the
 * first removed cell its probe passed, or else the empty cell that ended it,
 * and settles the claim against other claims of its hash (settle_claim). */

typedef struct node {
    uint64_t length;
    unsigned char bytes[];
} node;

static uint64_t slot_offset(const wf_cmap *map, uint64_t length) {
    uint64_t at = sizeof(node) + length;
    return (at + map->slot_align - 1) / map->slot_align * map->slot_align;
}

static uint64_t node_bytes(const wf_cmap *map, uint64_t length) {
    uint64_t bytes = slot_offset(map, length) + map->slot_size;
    return (bytes + ENTRY_GRAIN - 1) / ENTRY_GRAIN * ENTRY_GRAIN;
}

static void *slot_of(const wf_cmap *map, node *n) { return (char *)n + slot_offset(map, n->length); }

/* The node a cell of entries names, its readers' count left out. */
static inline node *node_at(cell *c) {
    return (node *)(uintptr_t)(atomic_load_explicit(&c->value, memory_order_relaxed) & ADDRESS_MASK);
}

/* Names n in a locked cell, keeping the count of readers, which one that
 * counted itself before seeing the lock takes back. */
static inline void name_node(cell *c, node *n) {
    atomic_fetch_and_explicit(&c->value, ~ADDRESS_MASK, memory_order_relaxed);
    atomic_fetch_or_explicit(&c->value, (uint64_t)(uintptr_t)n, memory_order_relaxed);
}

/* Waits until no reader of a cell its caller has locked is under way: none
 * begins once the lock is set, and one under way runs a block that waits for
 * nothing. */
static void wait_for_readers(cell *c) {
    unsigned round = 0;
    while (atomic_load_explicit(&c->value, memory_order_seq_cst) >> READER_SHIFT)
        back_off(&round);
}

static node *new_node(wf_cmap_user *u, uint64_t bytes) {
    if (bytes > ENTRY_LARGEST)
        return take(bytes);
    unsigned k = (unsigned)(bytes / ENTRY_GRAIN) - 1;
    free_entry *f = u->free[k];
    if (f != NULL) {
        u->free[k] = f->next;
        return (node *)(void *)f;
    }
    if (u->room < bytes) {
        chunk *c = host_map(ENTRY_CHUNK);
        if (c == NULL)
            WF_CMAP_EXHAUSTED();
        c->older = u->chunks;
        u->chunks = c;
        u->cursor = (char *)c + ENTRY_GRAIN;
        u->room = ENTRY_CHUNK - ENTRY_GRAIN;
    }
    node *n = (node *)(void *)u->cursor;
    u->cursor += bytes;
    u->room -= bytes;
    if ((uint64_t)(uintptr_t)n >> READER_SHIFT)
        abort();
    return n;
}

static void free_node(wf_cmap_user *u, node *n, uint64_t bytes) {
    if (bytes > ENTRY_LARGEST) {
        WF_CMAP_GIVE(n, bytes);
        return;
    }
    unsigned k = (unsigned)(bytes / ENTRY_GRAIN) - 1;
    free_entry *f = (free_entry *)(void *)n;
    f->next = u->free[k];
    u->free[k] = f;
}

/* A key's hash: eight bytes at a time, each mixed in by a multiplication,
 * then a finalizing mix, seeded with the length. */
static uint64_t hash_bytes(const unsigned char *p, uint64_t n) {
    uint64_t h = 0x243F6A8885A308D3ull ^ (n * 0x9E3779B97F4A7C15ull);
    while (n >= 8) {
        uint64_t w;
        memcpy(&w, p, 8);
        h = (h ^ w) * 0xBF58476D1CE4E5B9ull;
        h ^= h >> 31;
        p += 8;
        n -= 8;
    }
    if (n != 0) {
        uint64_t w = 0;
        memcpy(&w, p, (size_t)n);
        h = (h ^ w) * 0x94D049BB133111EBull;
        h ^= h >> 29;
    }
    h ^= h >> 32;
    h *= 0xD6E8FEB86659FD93ull;
    h ^= h >> 32;
    return h;
}

/* The hash a key's cell holds: within the key range, so never empty or
 * removed. A test narrows it with WF_CMAP_TAG_MASK so that keys share
 * hashes and only their bytes tell them apart. */
#ifndef WF_CMAP_TAG_MASK
#define WF_CMAP_TAG_MASK KEY_MASK
#endif
static uint64_t tag_of(const unsigned char *key, uint64_t length) {
    uint64_t tag = hash_bytes(key, length) & KEY_MASK & WF_CMAP_TAG_MASK;
    if (tag == EMPTY)
        return 1;
    if (tag == REMOVED)
        return REMOVED - 1;
    return tag;
}

/* Counts pauses waited, or a retry as one pause, against u's patience: 1
 * once the statement has waited past it. */
static inline int impatient(wf_cmap_user *u, uint64_t pauses) {
    u->waited += pauses;
    return u->waited > u->patience;
}

/* Locks c when it holds key, whose hash is tag: 1 and the cell locked when
 * it does, 0 with the cell as it was when it holds another key of that hash,
 * and -1 when its key word changed or it waited, counted against u's
 * patience, and must be read again. */
static int try_entry(wf_cmap_user *u, cell *c, uint64_t k, const unsigned char *key, uint64_t length,
                     unsigned *round) {
    if (k & LOCKED) {
        u->waited += wait_for_cell(round);
        return -1;
    }
    WF_CMAP_BEFORE_LOCK(c);
    if (!atomic_compare_exchange_weak_explicit(&c->key, &k, k | LOCKED, memory_order_seq_cst,
                                               memory_order_relaxed)) {
        u->waited += 1;
        return -1;
    }
    wait_for_readers(c);
    node *n = node_at(c);
    if (n->length == length && memcmp(n->bytes, key, (size_t)length) == 0)
        return 1;
    unlock(c, k);
    *round = 0;
    return 0;
}

/* What settle_claim answers besides FOUND. */
enum { SETTLED = 16, YIELDED };

/* After claiming the cell at index at for key, whose hash is tag, starting
 * at start and marked pending: reads every other cell from start to the next
 * empty cell for a cell of the same hash. A settled one is locked, waiting
 * for its holder, and when it holds key the claimed cell goes back as removed
 * and the answer is FOUND with that cell locked. A pending one, another claim
 * not yet settled, is waited out when it lies after the claimed cell and wins
 * when it lies before it: the claimed cell goes back as removed and the answer
 * is YIELDED. Otherwise the claim settles and the answer is SETTLED. A wait
 * that exhausts u's patience gives the claimed cell back as removed too, and
 * the answer is IMPATIENT.
 *
 * Two writers of one key that each claim a cell both mark it and then read
 * the other's cell, all sequentially consistent, and every cell before a
 * claimed one stays non-empty, so at least one of them sees the other's
 * claim: the one behind wins if both see the other pending, the one that
 * settled first otherwise, and a key never holds two cells. A writer waits
 * on a pending claim only after its own, and a settled cell's holder runs a
 * statement that takes no further cell, so writers never wait in a cycle. */
static int settle_claim(wf_cmap_user *u, table *t, uint64_t start, uint64_t at, uint64_t tag,
                        const unsigned char *key, uint64_t length, cell **out) {
    cell *claimed = &t->cells[at];
    uint64_t own = (at - start) & t->mask;
    uint64_t i = start;
    uint64_t left = t->capacity;
    unsigned round = 0;
    while (left > 0) {
        cell *c = &t->cells[i];
        if (c != claimed) {
            uint64_t k = atomic_load_explicit(&c->key, memory_order_seq_cst);
            uint64_t bare = k & ~(LOCKED | PENDING);
            if (bare == EMPTY)
                break;
            if (bare == tag) {
                int r = -1;
                if (k & PENDING) {
                    if (((i - start) & t->mask) < own) {
                        atomic_store_explicit(&claimed->key, REMOVED, memory_order_release);
                        return YIELDED;
                    }
                    u->waited += wait_for_cell(&round);
                } else {
                    r = try_entry(u, c, k, key, length, &round);
                    if (r > 0) {
                        atomic_store_explicit(&claimed->key, REMOVED, memory_order_release);
                        *out = c;
                        return FOUND;
                    }
                }
                if (r < 0) {
                    if (impatient(u, 0)) {
                        atomic_store_explicit(&claimed->key, REMOVED, memory_order_release);
                        return IMPATIENT;
                    }
                    continue;
                }
            }
        }
        left--;
        i = (i + 1) & t->mask;
    }
    atomic_store_explicit(&claimed->key, tag | LOCKED, memory_order_release);
    *out = claimed;
    return SETTLED;
}

/* Locks the cell of key, whose hash is tag, in t, or claims one for it: the
 * first removed cell the probe passed, so that a key that is missed again and
 * again keeps reusing one cell instead of leaving a removed cell each time in
 * front of the next probe, or else the empty cell that ended the probe; FULL
 * when the whole table holds neither, and IMPATIENT, holding no cell, when
 * its waits and retries exhaust u's patience. A claimed empty cell given back
 * as removed is counted as used, since it stays taken until the table
 * moves. */
static int acquire_entry(wf_cmap_user *u, table *t, uint64_t tag, const unsigned char *key, uint64_t length,
                         cell **out) {
    uint64_t start = start_of(t, tag);
    for (;;) {
        uint64_t i = start;
        uint64_t left = t->capacity;
        unsigned round = 0;
        uint64_t at = 0;
        int spared = 0;
        int claimed = 0;
        int from_empty = 0;
        while (!claimed) {
            cell *c = &t->cells[i];
            uint64_t k = atomic_load_explicit(&c->key, memory_order_acquire);
            uint64_t bare = k & ~(LOCKED | PENDING);
            if (bare == tag) {
                int r = try_entry(u, c, k, key, length, &round);
                if (r < 0) {
                    if (impatient(u, 0))
                        return IMPATIENT;
                    continue;
                }
                if (r > 0) {
                    *out = c;
                    return FOUND;
                }
            } else if (bare == EMPTY && !spared) {
                WF_CMAP_BEFORE_CLAIM(t, i);
                if (!atomic_compare_exchange_weak_explicit(&c->key, &k, tag | LOCKED | PENDING,
                                                           memory_order_seq_cst, memory_order_relaxed)) {
                    if (impatient(u, 1))
                        return IMPATIENT;
                    continue;
                }
                at = i;
                claimed = 1;
                from_empty = 1;
                break;
            } else if (k == REMOVED && !spared) {
                at = i;
                spared = 1;
            }
            if (bare != EMPTY && --left > 0) {
                i = (i + 1) & t->mask;
                continue;
            }
            if (!spared)
                return FULL;
            WF_CMAP_BEFORE_CLAIM(t, at);
            uint64_t removed = REMOVED;
            if (!atomic_compare_exchange_strong_explicit(&t->cells[at].key, &removed, tag | LOCKED | PENDING,
                                                         memory_order_seq_cst, memory_order_relaxed))
                break;
            claimed = 1;
        }
        if (claimed) {
            int r = settle_claim(u, t, start, at, tag, key, length, out);
            if (r == SETTLED)
                return from_empty ? CLAIMED : REUSED;
            if (from_empty)
                count(u, 1, 0);
            if (r == FOUND || r == IMPATIENT)
                return r;
        }
        /* A lost claim or one that gave way: the probe starts again. */
        if (impatient(u, 1))
            return IMPATIENT;
    }
}

/* Locks key's cell in the current table, or claims one, helping any move it
 * meets, as lock_key does for a word's key; IMPATIENT, holding no cell, when
 * u's patience runs out. */
static int lock_entry(wf_cmap_user *u, uint64_t tag, const unsigned char *key, uint64_t length, cell **out,
                      table **in) {
    for (;;) {
        table *t = use_current(u);
        if (atomic_load_explicit(&t->next, memory_order_acquire) == NULL) {
            int r = acquire_entry(u, t, tag, key, length, out);
            if (r == IMPATIENT)
                return r;
            if (r == FOUND || r == CLAIMED || r == REUSED) {
                if (keep_cell(t, *out, r, tag)) {
                    *in = t;
                    return r;
                }
            } else {
                start_move(u->map, t);
                unsigned round = 0;
                while (atomic_load_explicit(&t->next, memory_order_acquire) == NULL)
                    back_off(&round);
            }
        }
        finish_move(u->map, t);
        if (impatient(u, 1))
            return IMPATIENT;
    }
}

/* Marks u inside a keyed statement, once no statement holds the whole map.
 * The mark and the hold's gate are each written and then the other read,
 * all sequentially consistent: either the hold sees the mark and waits for
 * the statement, or the statement sees the gate and waits for the hold. A
 * statement that waits is counted, and the next hold waits until every
 * counted statement has begun, so that holds one after another do not keep
 * keyed statements out. */
static void enter_keyed(wf_cmap_user *u) {
    wf_cmap *map = u->map;
    atomic_store_explicit(&u->active, 1, memory_order_seq_cst);
    if (atomic_load_explicit(&map->gate, memory_order_seq_cst) == 0)
        return;
    atomic_store_explicit(&u->active, 0, memory_order_release);
    atomic_fetch_add_explicit(&map->waiting, 1, memory_order_seq_cst);
    for (;;) {
        unsigned round = 0;
        while (atomic_load_explicit(&map->gate, memory_order_acquire) != 0)
            back_off(&round);
        atomic_store_explicit(&u->active, 1, memory_order_seq_cst);
        if (atomic_load_explicit(&map->gate, memory_order_seq_cst) == 0)
            break;
        atomic_store_explicit(&u->active, 0, memory_order_release);
    }
    atomic_fetch_sub_explicit(&map->waiting, 1, memory_order_release);
}

wf_cmap *wf_cmap_create_entries(uint64_t slot_size, uint64_t slot_align, uint64_t capacity) {
    /* The target refuses a program whose map's slot needs more alignment
     * before emission, so this stops only a caller that breaks the
     * contract. */
    if (slot_align == 0 || slot_align > ENTRY_GRAIN || (slot_align & (slot_align - 1)) != 0)
        abort();
    wf_cmap *map = wf_cmap_create(capacity);
    map->slot_size = slot_size;
    map->slot_align = slot_align;
    return map;
}

wf_cmap_user *wf_cmap_user_at(wf_cmap *map, unsigned index) {
    wf_cmap_user *u = &map->users[index];
    if (atomic_load_explicit(&u->busy, memory_order_relaxed) == 0) {
        atomic_store(&u->busy, 1);
        int n = atomic_load(&map->users_seen);
        while (n < (int)index + 1 && !atomic_compare_exchange_weak(&map->users_seen, &n, (int)index + 1)) {
        }
    }
    return u;
}

/* A keyed statement waits for its entry while other statements hold it. Once
 * it has waited and retried past its patience, it gives back what it
 * claimed, leaves the statements under way and holds the whole map, as a
 * statement over the map does, and then locks its entry, which nothing else
 * holds by then. Its hold waits for the holds before it in line, one for
 * each other user at most, and, after it closes the gate, for the keyed
 * statements already under way, one for each other user at most; keyed
 * statements that begin between two earlier holds are bounded by those
 * holds' own steps. So the statement takes effect [WAIT-2]. A held
 * statement waits for nothing, since its map's holder excludes every other
 * statement. */
void *wf_cmap_lock_entry(wf_cmap_user *u, const unsigned char *key, uint64_t length, int held,
                         wf_cmap_entry *entry) {
    if (!held)
        enter_keyed(u);
    wf_cmap *map = u->map;
    uint64_t tag = tag_of(key, length);
    cell *c;
    table *t;
    u->waited = 0;
    u->patience = held ? UINT64_MAX : WF_CMAP_PATIENCE(u);
    int r = lock_entry(u, tag, key, length, &c, &t);
    entry->upgraded = r == IMPATIENT;
    if (r == IMPATIENT) {
        atomic_store_explicit(&u->active, 0, memory_order_release);
        wf_cmap_hold(u);
        u->patience = UINT64_MAX;
        r = lock_entry(u, tag, key, length, &c, &t);
    }
    node *n;
    if (r == FOUND) {
        n = node_at(c);
    } else {
        n = new_node(u, node_bytes(map, length));
        n->length = length;
        memcpy(n->bytes, key, (size_t)length);
        memset(slot_of(map, n), 0, (size_t)map->slot_size);
        name_node(c, n);
        /* A reused removed cell was counted when it was first claimed. */
        if (r == CLAIMED)
            count(u, 1, 0);
    }
    entry->cell = c;
    entry->table = t;
    entry->fresh = r != FOUND;
    return slot_of(map, n);
}

void wf_cmap_unlock_entry(wf_cmap_user *u, wf_cmap_entry *entry, int held, int present) {
    wf_cmap *map = u->map;
    cell *c = entry->cell;
    table *t = entry->table;
    if (present) {
        unlock(c, atomic_load_explicit(&c->key, memory_order_relaxed) & ~LOCKED);
    } else {
        node *n = node_at(c);
        free_node(u, n, node_bytes(map, n->length));
        unlock(c, REMOVED);
    }
    count(u, 0, (int64_t)(present != 0) - (int64_t)(entry->fresh == 0));
    if (entry->upgraded)
        wf_cmap_unhold(u);
    else if (!held)
        atomic_store_explicit(&u->active, 0, memory_order_release);
    /* A claim may cross the threshold, as an insert's may. */
    if (entry->fresh &&
        (t->capacity <= (1ull << 16) || (atomic_load_explicit(&u->used, memory_order_relaxed) & 31) == 0)) {
        int64_t used, live;
        totals(map, &used, &live);
        if (used - t->base > (int64_t)(t->capacity / 2)) {
            start_move(map, t);
            if (atomic_load_explicit(&t->next, memory_order_acquire) != NULL)
                finish_move(map, t);
        }
    }
}

/* Finds key, whose hash is tag, in t for a statement that only reads its
 * entry: FOUND with the reader counted on the entry's cell in *out, ABSENT,
 * or IMPATIENT once its waits and retries exhaust u's patience. A reader
 * counts itself and then reads the key word again, and a writer locks the
 * key word and then waits for the count to fall to zero, all sequentially
 * consistent, so either the reader sees the lock and leaves or the writer
 * waits for it. A pending claim of the same hash is passed over: its writer
 * has not begun, and a cell of the key it would find lies further on. */
static int read_in(wf_cmap_user *u, table *t, uint64_t tag, const unsigned char *key, uint64_t length,
                   cell **out) {
    uint64_t i = start_of(t, tag);
    uint64_t left = t->capacity;
    unsigned round = 0;
    while (left > 0) {
        cell *c = &t->cells[i];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_seq_cst);
        uint64_t bare = k & ~(LOCKED | PENDING);
        if (bare == EMPTY)
            return ABSENT;
        if (bare == tag && (k & PENDING) == 0) {
            if (k & LOCKED) {
                u->waited += wait_for_cell(&round);
                if (impatient(u, 0))
                    return IMPATIENT;
                continue;
            }
            atomic_fetch_add_explicit(&c->value, READER_ONE, memory_order_seq_cst);
            if (atomic_load_explicit(&c->key, memory_order_seq_cst) != k) {
                atomic_fetch_sub_explicit(&c->value, READER_ONE, memory_order_release);
                if (impatient(u, 1))
                    return IMPATIENT;
                continue;
            }
            node *n = node_at(c);
            if (n->length == length && memcmp(n->bytes, key, (size_t)length) == 0) {
                *out = c;
                return FOUND;
            }
            atomic_fetch_sub_explicit(&c->value, READER_ONE, memory_order_release);
            round = 0;
        }
        left--;
        i = (i + 1) & t->mask;
    }
    return ABSENT;
}

/* Reads key's entry in the current table, helping any move it meets: an
 * answer stands only if no move began before it was given, since a writer
 * may change the key in the next table once a move has begun. */
static int read_entry(wf_cmap_user *u, uint64_t tag, const unsigned char *key, uint64_t length, cell **out,
                      table **in) {
    for (;;) {
        table *t = use_current(u);
        if (atomic_load_explicit(&t->next, memory_order_acquire) == NULL) {
            int r = read_in(u, t, tag, key, length, out);
            if (r == IMPATIENT)
                return r;
            if (r == FOUND)
                WF_CMAP_READ_FOUND(t);
            if (atomic_load_explicit(&t->next, memory_order_seq_cst) == NULL) {
                *in = t;
                return r;
            }
            if (r == FOUND)
                atomic_fetch_sub_explicit(&(*out)->value, READER_ONE, memory_order_release);
        }
        finish_move(u->map, t);
        if (impatient(u, 1))
            return IMPATIENT;
    }
}

const void *wf_cmap_read_entry(wf_cmap_user *u, const unsigned char *key, uint64_t length, int held,
                               wf_cmap_entry *entry) {
    if (!held)
        enter_keyed(u);
    uint64_t tag = tag_of(key, length);
    cell *c = NULL;
    table *t;
    u->waited = 0;
    u->patience = held ? UINT64_MAX : WF_CMAP_PATIENCE(u);
    int r = read_entry(u, tag, key, length, &c, &t);
    entry->upgraded = r == IMPATIENT;
    if (r == IMPATIENT) {
        atomic_store_explicit(&u->active, 0, memory_order_release);
        wf_cmap_hold(u);
        u->patience = UINT64_MAX;
        r = read_entry(u, tag, key, length, &c, &t);
    }
    entry->cell = r == FOUND ? c : NULL;
    entry->table = t;
    entry->fresh = 0;
    return r == FOUND ? slot_of(u->map, node_at(c)) : NULL;
}

void wf_cmap_unread_entry(wf_cmap_user *u, wf_cmap_entry *entry, int held) {
    if (entry->cell != NULL)
        atomic_fetch_sub_explicit(&((cell *)entry->cell)->value, READER_ONE, memory_order_release);
    if (entry->upgraded)
        wf_cmap_unhold(u);
    else if (!held)
        atomic_store_explicit(&u->active, 0, memory_order_release);
}

/* Statements over the whole map take turns by ticket, so that one that
 * holds the map again and again does not keep another out [WAIT-2]. On its
 * turn a statement waits for the keyed statements a hold before it kept
 * waiting, then closes the gate, once the hold before it has opened it, and
 * waits out the keyed statements under way. */
void wf_cmap_hold(wf_cmap_user *u) {
    wf_cmap *map = u->map;
    uint64_t ticket = atomic_fetch_add_explicit(&map->hold_next, 1, memory_order_relaxed);
    WF_CMAP_HOLD_QUEUED(u);
    unsigned round = 0;
    while (atomic_load_explicit(&map->hold_serving, memory_order_acquire) != ticket)
        back_off(&round);
    round = 0;
    while (atomic_load_explicit(&map->waiting, memory_order_acquire) != 0)
        back_off(&round);
    for (int open = 0; !atomic_compare_exchange_weak_explicit(&map->gate, &open, 1, memory_order_seq_cst,
                                                              memory_order_relaxed);
         open = 0)
        back_off(&round);
    WF_CMAP_HOLD_CLOSED(u);
    int n = atomic_load_explicit(&map->users_seen, memory_order_seq_cst);
    for (int i = 0; i < n; i++) {
        round = 0;
        while (atomic_load_explicit(&map->users[i].active, memory_order_seq_cst) != 0)
            back_off(&round);
    }
}

void wf_cmap_unhold(wf_cmap_user *u) {
    atomic_store_explicit(&u->map->gate, 0, memory_order_seq_cst);
    atomic_fetch_add_explicit(&u->map->hold_serving, 1, memory_order_release);
}

uint64_t wf_cmap_count(wf_cmap *map) {
    int64_t used, live;
    totals(map, &used, &live);
    return live > 0 ? (uint64_t)live : 0;
}

void *wf_cmap_drain(wf_cmap *map) {
    if (map->pending != NULL) {
        WF_CMAP_GIVE(map->pending, map->pending_bytes);
        map->pending = NULL;
    }
    table *t = atomic_load_explicit(&map->current, memory_order_acquire);
    while (map->drained < t->capacity) {
        cell *c = &t->cells[map->drained++];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_relaxed);
        if (k == EMPTY || k == REMOVED)
            continue;
        node *n = node_at(c);
        atomic_store_explicit(&c->key, REMOVED, memory_order_relaxed);
        /* A large node goes back to the pool once its value is released;
         * the rest leave with their chunks when the map is destroyed. */
        uint64_t bytes = node_bytes(map, n->length);
        if (bytes > ENTRY_LARGEST) {
            map->pending = n;
            map->pending_bytes = bytes;
        }
        return slot_of(map, n);
    }
    return NULL;
}

wf_cmap *wf_cmap_create(uint64_t capacity) {
    char *raw = take(sizeof(wf_cmap) + 64);
    wf_cmap *map = (wf_cmap *)(((uintptr_t)raw + 63) & ~(uintptr_t)63);
    memset(map, 0, sizeof *map);
    map->raw = raw;
    for (int i = 0; i < WF_CMAP_MAX_USERS; i++)
        map->users[i].map = map;
    /* Half full when it holds capacity keys, as dense as a table gets
     * before it moves, since reads cost less in a smaller table. The capacity
     * only sizes the first table, which a map outgrows by moving, so a hint
     * past CAPACITY_LIMIT keys asks for that many: a larger one would have
     * the cell count's bytes overflow. */
    if (capacity > CAPACITY_LIMIT)
        capacity = CAPACITY_LIMIT;
    table *t = new_table(NULL, capacity ? cells_for(capacity, 1, 2) : DEFAULT_CELLS);
    atomic_store(&map->current, t);
    return map;
}

void wf_cmap_destroy(wf_cmap *map) {
    table *t = atomic_load(&map->current);
    table *pending = atomic_load(&t->next);
    if (pending)
        free_table(pending);
    free_table(t);
    for (table *r = atomic_load(&map->retired); r;) {
        table *older = r->older;
        free_table(r);
        r = older;
    }
    if (map->spare)
        free_cells(map->spare, map->spare_capacity);
    for (int i = 0; i < WF_CMAP_MAX_USERS; i++)
        for (chunk *c = map->users[i].chunks; c;) {
            chunk *older = c->older;
            host_unmap(c, ENTRY_CHUNK);
            c = older;
        }
    WF_CMAP_GIVE(map->raw, sizeof(wf_cmap) + 64);
}

wf_cmap_user *wf_cmap_enter(wf_cmap *map) {
    for (int i = 0; i < WF_CMAP_MAX_USERS; i++) {
        int idle = 0;
        if (atomic_compare_exchange_strong(&map->users[i].busy, &idle, 1)) {
            int n = atomic_load(&map->users_seen);
            while (n < i + 1 && !atomic_compare_exchange_weak(&map->users_seen, &n, i + 1)) {
            }
            return &map->users[i];
        }
    }
    return NULL;
}

void wf_cmap_leave(wf_cmap_user *u) {
    wf_cmap *map = u->map;
    atomic_fetch_add_explicit(&map->folded_used, atomic_load_explicit(&u->used, memory_order_relaxed),
                              memory_order_relaxed);
    atomic_fetch_add_explicit(&map->folded_live, atomic_load_explicit(&u->live, memory_order_relaxed),
                              memory_order_relaxed);
    atomic_store_explicit(&u->used, 0, memory_order_relaxed);
    atomic_store_explicit(&u->live, 0, memory_order_relaxed);
    atomic_store_explicit(&u->in, NULL, memory_order_release);
    atomic_store(&u->busy, 0);
}

int wf_cmap_get(wf_cmap_user *u, uint64_t key, uint64_t *value) {
#ifdef WF_CMAP_LOCKED_READ
    cell *c;
    table *t;
    if (lock_key(u, key, 0, &c, &t) == ABSENT)
        return 0;
    *value = atomic_load_explicit(&c->value, memory_order_relaxed);
    unlock(c, key);
    return 1;
#else
    table *t = use_current(u);
    uint64_t i = start_of(t, key);
    uint64_t left = t->capacity;
    for (;;) {
        cell *c = &t->cells[i];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_acquire);
        uint64_t bare = k & ~LOCKED;
        if (__builtin_expect(bare == key, 1)) {
            if (__builtin_expect(k & LOCKED, 0)) {
                pause_once();
                continue;
            }
            *value = atomic_load_explicit(&c->value, memory_order_relaxed);
            return 1;
        }
        if (bare == EMPTY || --left == 0)
            return 0;
        i = (i + 1) & t->mask;
    }
#endif
}

int wf_cmap_insert(wf_cmap_user *u, uint64_t key, uint64_t value) {
    check_key(key);
    cell *c;
    table *t;
    int r = lock_key(u, key, 1, &c, &t);
    atomic_store_explicit(&c->value, value, memory_order_relaxed);
    unlock(c, key);
    if (r == FOUND)
        return 0;
    count(u, 1, 1);
    /* A claim may cross the threshold: half the cells used. Small tables
     * check every claim, so that racing claims seldom fill one. */
    if (t->capacity <= (1ull << 16) || (atomic_load_explicit(&u->used, memory_order_relaxed) & 31) == 0) {
        int64_t used, live;
        totals(u->map, &used, &live);
        if (used - t->base > (int64_t)(t->capacity / 2)) {
            start_move(u->map, t);
            if (atomic_load_explicit(&t->next, memory_order_acquire) != NULL)
                finish_move(u->map, t);
        }
    }
    return 1;
}

int wf_cmap_remove(wf_cmap_user *u, uint64_t key) {
    cell *c;
    table *t;
    if (lock_key(u, key, 0, &c, &t) == ABSENT)
        return 0;
    unlock(c, REMOVED);
    count(u, 0, -1);
    return 1;
}

int wf_cmap_update(wf_cmap_user *u, uint64_t key, void (*edit)(uint64_t *value, void *env), void *env) {
    cell *c;
    table *t;
    if (lock_key(u, key, 0, &c, &t) == ABSENT)
        return 0;
    uint64_t value = atomic_load_explicit(&c->value, memory_order_relaxed);
    edit(&value, env);
    atomic_store_explicit(&c->value, value, memory_order_relaxed);
    unlock(c, key);
    return 1;
}
