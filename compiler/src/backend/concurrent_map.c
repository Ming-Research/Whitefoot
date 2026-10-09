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
 * Built with WF_CMAP_LOCKED_READ, a read of a word locks its cell like a
 * writer; it is kept for measurement beside the lock-free read.
 *
 * The file that includes this one supplies the host: WF_CMAP_TAKE(bytes) and
 * WF_CMAP_GIVE(block, bytes) for small blocks aligned to 16 bytes, which the
 * runtime takes from its own pool and never from the program's allocator
 * [STOR-8]; WF_CMAP_YIELD() to give up the processor;
 * WF_CMAP_EXHAUSTED() when memory is short; and WF_CMAP_HEAP_CHANGE(delta)
 * for the live requested bytes outside that pool [PRE-2]. Cell arrays of
 * 2 MiB or more and the chunks entries are carved from are mapped from the
 * host here. It may also supply WF_CMAP_HOST_FIELDS, members of its own
 * placed in every map,
 * WF_CMAP_CURRENT_USER(map), the user the calling thread holds, whose
 * spare memory a hold's keys then reuse, and WF_CMAP_SPARE_KEYS(), a
 * `void *` place only the calling thread uses, where the memory of the last
 * key set it freed is kept for its next one.
 */
#if !defined(WF_CMAP_TAKE) || !defined(WF_CMAP_GIVE) || !defined(WF_CMAP_YIELD) || !defined(WF_CMAP_EXHAUSTED)
#error "the includer supplies WF_CMAP_TAKE, WF_CMAP_GIVE, WF_CMAP_YIELD and WF_CMAP_EXHAUSTED"
#endif
#ifndef WF_CMAP_HEAP_CHANGE
#error "the includer supplies WF_CMAP_HEAP_CHANGE for storage outside its pool"
#endif
#ifndef WF_CMAP_CURRENT_USER
#define WF_CMAP_CURRENT_USER(map) ((wf_cmap_user *)NULL)
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
/* And a writer about to help a move out of one of map's tables to its end. */
#ifndef WF_CMAP_FINISHING
#define WF_CMAP_FINISHING(map) ((void)0)
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
 * cells and may wait before it holds the map; and its entries' free memory.
 * Then whether it holds the whole map; the hold whose cells it is locking,
 * which a probe of one of its keys may meet; and the memory of an earlier
 * hold's keys, kept for the next. */
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
 * over the high bits the starting cell is taken from. The product is the
 * key's position [SHARE-1]: odd multiplication is a bijection of 64-bit
 * words, so distinct keys have distinct positions, and a key's starting
 * cell is its position's top bits in a table of every size, which is what
 * lets a scan resume across moves (wf_cmap_scan). */
static inline uint64_t position_of(uint64_t key) { return key * 0x9E3779B97F4A7C15ull; }
static inline uint64_t start_of(const table *t, uint64_t key) { return position_of(key) >> t->shift; }

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
    WF_CMAP_HEAP_CHANGE((int64_t)bytes);
    return c;
}

static void free_cells(cell *c, uint64_t count) {
    size_t bytes = count * sizeof(cell);
    if (bytes < HUGE_BYTES)
        WF_CMAP_GIVE(c, bytes);
    else {
        WF_CMAP_HEAP_CHANGE(-(int64_t)bytes);
        host_unmap(c, bytes);
    }
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
            /* Still a live allocation owned by this map, just as a small
             * spare remains a granted pool block: count until free_cells.
             * Reusing it neither allocates nor releases storage [PRE-2]. */
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
    WF_CMAP_FINISHING(map);
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
 * the live keys and `extra` more in at most three eighths of them, so that
 * at least an eighth more can be claimed before it moves in turn, and at
 * least half t's size. */
static void start_move_for(wf_cmap *map, table *t, uint64_t extra) {
    int idle = 0;
    if (atomic_load_explicit(&t->starting, memory_order_relaxed) != 0 ||
        !atomic_compare_exchange_strong_explicit(&t->starting, &idle, 1, memory_order_relaxed,
                                                 memory_order_relaxed))
        return;
    int64_t used, live;
    totals(map, &used, &live);
    uint64_t capacity = cells_for((live > 0 ? (uint64_t)live : 0) + extra, 3, 8);
    if (capacity < t->capacity / 2)
        capacity = t->capacity / 2 > MIN_CELLS ? t->capacity / 2 : MIN_CELLS;
    table *nt = new_table(map, capacity);
    atomic_store_explicit(&t->next, nt, memory_order_seq_cst);
}

static void start_move(wf_cmap *map, table *t) { start_move_for(map, t, 0); }

/* What a probe answers. SHARED: a hold's probe met a cell the hold has
 * locked itself, which only two of its keys of one hash make it do. */
enum { FOUND, CLAIMED, ABSENT, FULL, REUSED, IMPATIENT, MOVED, SHARED };

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
        WF_CMAP_HEAP_CHANGE((int64_t)bytes);
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
    /* The node request is live; the rest of the chunk is an allocator
     * reserve. Larger nodes are already counted by the host pool [PRE-2]. */
    WF_CMAP_HEAP_CHANGE((int64_t)bytes);
    return n;
}

static void free_node(wf_cmap_user *u, node *n, uint64_t bytes) {
    if (bytes > ENTRY_LARGEST) {
        WF_CMAP_GIVE(n, bytes);
        return;
    }
    WF_CMAP_HEAP_CHANGE(-(int64_t)bytes);
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

/* Whether a key's bytes are a node's. A key of no bytes may have no address. */
static inline int same_key(const node *n, const unsigned char *key, uint64_t length) {
    return n->length == length && (length == 0 || memcmp(n->bytes, key, (size_t)length) == 0);
}

/* Whether hold has locked c. */
static int holds_cell(const wf_cmap_holding *hold, const cell *c) {
    const wf_cmap_held *keys = hold->keys != NULL ? hold->keys : hold->inline_keys;
    for (uint64_t i = 0; i < hold->count; i++)
        if (keys[i].cell == c)
            return 1;
    return 0;
}

/* Locks c, a cell of t, when it holds key, whose hash is tag: 1 and the cell
 * locked when it does, 0 with the cell as it was when it holds another key
 * of that hash, -1 when its key word changed or it waited, counted against
 * u's patience, and must be read again, -2 with the cell as it was when a
 * move out of t has begun, and -3 when it is a cell the hold u is locking
 * holds itself, which it would wait for in vain.
 *
 * The node is read only once the cell is locked and no move has begun: a
 * move that ended before the lock was taken has copied the cell, and a
 * statement in the next table may since have removed the key and freed the
 * node this cell still names. A move that begins after this look waits for
 * the lock, as after keep_cell's. */
static int try_entry(wf_cmap_user *u, table *t, cell *c, uint64_t k, const unsigned char *key,
                     uint64_t length, unsigned *round) {
    if (k & LOCKED) {
        if (u->own != NULL && holds_cell(u->own, c))
            return -3;
        u->waited += wait_for_cell(round);
        return -1;
    }
    WF_CMAP_BEFORE_LOCK(c);
    if (!atomic_compare_exchange_weak_explicit(&c->key, &k, k | LOCKED, memory_order_seq_cst,
                                               memory_order_relaxed)) {
        u->waited += 1;
        return -1;
    }
    if (atomic_load_explicit(&t->next, memory_order_seq_cst) != NULL) {
        unlock(c, k);
        return -2;
    }
    wait_for_readers(c);
    if (same_key(node_at(c), key, length))
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
 * the answer is IMPATIENT; a move out of the table met at a cell of the
 * hash does the same, and the answer is MOVED, and so does a cell of the
 * hash that u's hold has locked itself, and the answer is SHARED.
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
                    r = try_entry(u, t, c, k, key, length, &round);
                    if (r > 0) {
                        atomic_store_explicit(&claimed->key, REMOVED, memory_order_release);
                        *out = c;
                        return FOUND;
                    }
                    if (r == -2) {
                        atomic_store_explicit(&claimed->key, REMOVED, memory_order_release);
                        return MOVED;
                    }
                    if (r == -3) {
                        atomic_store_explicit(&claimed->key, REMOVED, memory_order_release);
                        return SHARED;
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
 * when the whole table holds neither, IMPATIENT, holding no cell, when
 * its waits and retries exhaust u's patience, MOVED, holding no cell,
 * when a move out of t has begun, and SHARED, holding no cell, when it meets
 * a cell u's hold has locked itself. A claimed empty cell given back as
 * removed is counted as used, since it stays taken until the table moves. */
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
                int r = try_entry(u, t, c, k, key, length, &round);
                if (r == -2)
                    return MOVED;
                if (r == -3)
                    return SHARED;
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
            if (r == FOUND || r == IMPATIENT || r == MOVED || r == SHARED)
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
            } else if (r == FULL) {
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

/* The bytes of a map's `None` slot, whole grains, so the block the pool
 * gives is aligned for any slot the map admits. */
static uint64_t none_bytes(uint64_t slot_size) {
    return slot_size == 0 ? ENTRY_GRAIN : (slot_size + ENTRY_GRAIN - 1) / ENTRY_GRAIN * ENTRY_GRAIN;
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
    map->none = take(none_bytes(slot_size));
    memset(map->none, 0, (size_t)none_bytes(slot_size));
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
        if (length != 0)
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
    /* Counted before the unlock: a mover waiting for this cell sums the
     * counts once it has the cell, into the next table's base. */
    count(u, 0, (int64_t)(present != 0) - (int64_t)(entry->fresh == 0));
    if (present) {
        unlock(c, atomic_load_explicit(&c->key, memory_order_relaxed) & ~LOCKED);
    } else {
        node *n = node_at(c);
        free_node(u, n, node_bytes(map, n->length));
        unlock(c, REMOVED);
    }
    /* A claim may cross the threshold, as an insert's may. A move it starts
     * ends before the statement leaves the map, so that a hold of the whole
     * map, which waits the statement out, finds no move under way and may
     * swap the map's entries (wf_cmap_swap). */
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
    if (entry->upgraded)
        wf_cmap_unhold(u);
    else if (!held)
        atomic_store_explicit(&u->active, 0, memory_order_release);
}

/* Finds key, whose hash is tag, in t for a statement that only reads its
 * entry: FOUND with the reader counted on the entry's cell in *out, ABSENT,
 * or IMPATIENT once its waits and retries exhaust u's patience. A reader
 * counts itself and then reads the key word again, and a writer locks the
 * key word and then waits for the count to fall to zero, all sequentially
 * consistent, so either the reader sees the lock and leaves or the writer
 * waits for it. A pending claim of the same hash is passed over: its writer
 * has not begun, and a cell of the key it would find lies further on. MOVED,
 * with no reader counted, when a move out of t had begun once the reader
 * was counted: the cell's node may be freed through the next table, so it
 * is read only when no move had, and a move that begins later waits for the
 * count. */
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
            if (atomic_load_explicit(&t->next, memory_order_seq_cst) != NULL) {
                atomic_fetch_sub_explicit(&c->value, READER_ONE, memory_order_release);
                return MOVED;
            }
            if (same_key(node_at(c), key, length)) {
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
            if (r != MOVED && atomic_load_explicit(&t->next, memory_order_seq_cst) == NULL) {
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
    return r == FOUND ? slot_of(u->map, node_at(c)) : u->map->none;
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
    u->holding = 1;
}

void wf_cmap_unhold(wf_cmap_user *u) {
    u->holding = 0;
    atomic_store_explicit(&u->map->gate, 0, memory_order_seq_cst);
    atomic_fetch_add_explicit(&u->map->hold_serving, 1, memory_order_release);
}

int wf_cmap_holds_whole(const wf_cmap_user *u) { return u->holding; }

/* Key sets: an array of items in the order their keys were first inserted,
 * each naming its key's bytes in an arena of the set's own and carrying the
 * key's tag, and an index of 2 * room slots after the items, each 0 or one
 * more than an item's index, probed from the tag, so that inserting a key
 * finds an earlier insertion of it without comparing keys in order and
 * never moves an item. The order a hold locks entries in is the hold's own
 * (order_hold), not the set's. */
typedef struct {
    uint64_t offset;
    uint64_t length;
    uint64_t tag;
} key_item;

typedef struct {
    uint64_t room;
    /* The items in use, so that a spare's index is cleared slot by slot. */
    uint64_t count;
    uint64_t bytes_used;
    uint64_t bytes_room;
    unsigned char *bytes;
    key_item items[];
} key_store;

/* The most keys a set's first store has room for, whatever its capacity
 * asks: a capacity only sizes the first store, which grows by doubling, so
 * a hint past this asks for this many. */
#define KEY_SET_FIRST_LIMIT (1ull << 16)
/* The least keys a store grows to, and bytes an arena does. */
#define KEY_SET_MIN_ROOM 8ull
#define KEY_SET_MIN_BYTES 64ull
/* The largest store, and arena, a thread keeps as its spare: a set of up to
 * 1,024 keys whose bytes fit in 64 KiB. Of two stores it may keep, it keeps
 * the one with room for more keys, so a small set freed between large ones
 * does not leave every large one taking memory again. */
#define KEY_SET_SPARE_ROOM 1024ull
#define KEY_SET_SPARE_BYTES (64ull * 1024)

/* Byte order, a proper prefix first. A key of no bytes may have no
 * address. */
static int compare_keys(const unsigned char *a, uint64_t a_length, const unsigned char *b, uint64_t b_length) {
    uint64_t shorter = a_length < b_length ? a_length : b_length;
    int order = shorter == 0 ? 0 : memcmp(a, b, (size_t)shorter);
    if (order != 0)
        return order;
    return a_length < b_length ? -1 : a_length > b_length ? 1 : 0;
}

/* The index after a store's items: 2 * room slots of 32 bits. */
static uint32_t *store_index(const key_store *s) { return (uint32_t *)(void *)&s->items[s->room]; }

static size_t store_bytes(uint64_t room) {
    if (room > (SIZE_MAX - sizeof(key_store)) / (sizeof(key_item) + 2 * sizeof(uint32_t)) || room > UINT32_MAX / 2)
        WF_CMAP_EXHAUSTED();
    return sizeof(key_store) + (size_t)room * (sizeof(key_item) + 2 * sizeof(uint32_t));
}

/* A store's room is a power of two, so that its index's mask is room * 2 - 1. */
static uint64_t store_room(uint64_t keys) {
    uint64_t room = KEY_SET_MIN_ROOM;
    while (room < keys)
        room *= 2;
    return room;
}

static key_store *new_store(uint64_t room) {
    key_store *s = take(store_bytes(room));
    s->room = room;
    s->count = 0;
    s->bytes_used = 0;
    s->bytes_room = 0;
    s->bytes = NULL;
    memset(store_index(s), 0, (size_t)room * 2 * sizeof(uint32_t));
    return s;
}

/* A set's first store, with room for `room` keys: the calling thread's
 * spare, with the arena it kept, when it has one that large. A statement
 * naming keys builds a set each time it runs, and the pool the stores come
 * from keeps its free lists behind one lock every driver takes, so `MSET`'s
 * set on four drivers spent more time there than in its statement; with the
 * spare, a thread that has built one such set takes no memory for the next. */
static key_store *first_store(uint64_t room) {
#ifdef WF_CMAP_SPARE_KEYS
    key_store *spare = WF_CMAP_SPARE_KEYS();
    if (spare != NULL && spare->room >= room) {
        WF_CMAP_SPARE_KEYS() = NULL;
        /* Clears only the slots the last set used, so that a small set
         * reusing a large spare does not pay for its whole index. A slot is
         * found by its exact value, past slots already cleared. */
        uint32_t *slots = store_index(spare);
        uint64_t mask = spare->room * 2 - 1;
        for (uint64_t k = 0; k < spare->count; k++)
            for (uint64_t i = spare->items[k].tag & mask;; i = (i + 1) & mask)
                if (slots[i] == (uint32_t)(k + 1)) {
                    slots[i] = 0;
                    break;
                }
        spare->count = 0;
        spare->bytes_used = 0;
        return spare;
    }
#endif
    return new_store(room);
}

/* An item's bytes; a key of no bytes has those of no arena. */
static const unsigned char *item_bytes(const key_store *s, const key_item *item) {
    static const unsigned char no_bytes[1] = {0};
    return item->length == 0 ? no_bytes : s->bytes + item->offset;
}

/* Puts item index at the first free slot of its tag's probe. */
static void index_item(key_store *s, uint64_t index) {
    uint32_t *slots = store_index(s);
    uint64_t mask = s->room * 2 - 1;
    for (uint64_t i = s->items[index].tag & mask;; i = (i + 1) & mask)
        if (slots[i] == 0) {
            slots[i] = (uint32_t)(index + 1);
            return;
        }
}

/* Makes room for one more key and length more bytes. */
static key_store *room_for(wf_key_set *set, uint64_t length) {
    key_store *s = set->store;
    if (s == NULL) {
        s = first_store(KEY_SET_MIN_ROOM);
        set->store = s;
    } else if (set->len == s->room) {
        key_store *grown = new_store(s->room * 2);
        grown->bytes_used = s->bytes_used;
        grown->bytes_room = s->bytes_room;
        grown->bytes = s->bytes;
        memcpy(grown->items, s->items, (size_t)set->len * sizeof(key_item));
        grown->count = set->len;
        for (uint64_t i = 0; i < set->len; i++)
            index_item(grown, i);
        WF_CMAP_GIVE(s, store_bytes(s->room));
        s = grown;
        set->store = s;
    }
    if (length > s->bytes_room - s->bytes_used) {
        if (length > UINT64_MAX - s->bytes_used)
            WF_CMAP_EXHAUSTED();
        uint64_t need = s->bytes_used + length;
        uint64_t room = s->bytes_room > need / 2 ? s->bytes_room * 2 : need;
        if (room < KEY_SET_MIN_BYTES)
            room = KEY_SET_MIN_BYTES;
        unsigned char *bytes = take((size_t)room);
        if (s->bytes_used != 0)
            memcpy(bytes, s->bytes, (size_t)s->bytes_used);
        if (s->bytes != NULL)
            WF_CMAP_GIVE(s->bytes, (size_t)s->bytes_room);
        s->bytes = bytes;
        s->bytes_room = room;
    }
    return s;
}

void wf_cmap_key_set_new(wf_key_set *set, uint64_t capacity) {
    set->len = 0;
    set->store = capacity == 0 ? NULL
                               : first_store(store_room(capacity < KEY_SET_FIRST_LIMIT ? capacity : KEY_SET_FIRST_LIMIT));
}

uint64_t wf_cmap_key_set_insert(wf_key_set *set, const unsigned char *key, uint64_t length) {
    uint64_t tag = tag_of(key, length);
    key_store *s = set->store;
    if (s != NULL) {
        const uint32_t *slots = store_index(s);
        uint64_t mask = s->room * 2 - 1;
        for (uint64_t i = tag & mask; slots[i] != 0; i = (i + 1) & mask) {
            const key_item *item = &s->items[slots[i] - 1];
            if (item->tag == tag && compare_keys(item_bytes(s, item), item->length, key, length) == 0)
                return slots[i] - 1u;
        }
    }
    s = room_for(set, length);
    uint64_t index = set->len;
    if (length != 0)
        memcpy(s->bytes + s->bytes_used, key, (size_t)length);
    s->items[index].offset = s->bytes_used;
    s->items[index].length = length;
    s->items[index].tag = tag;
    s->bytes_used += length;
    set->len = index + 1;
    s->count = index + 1;
    index_item(s, index);
    return index;
}

/* The item at index. An index past the set's keys is one the compiler's own
 * checks admitted, which no program can cause, so the program stops. */
static const key_item *item_at(const wf_key_set *set, uint64_t index) {
    if (index >= set->len)
        abort();
    return &((const key_store *)set->store)->items[index];
}


const unsigned char *wf_cmap_key_set_key(const wf_key_set *set, uint64_t index, uint64_t *length) {
    const key_item *item = item_at(set, index);
    *length = item->length;
    return item_bytes(set->store, item);
}

static void give_store(key_store *s) {
    if (s->bytes != NULL)
        WF_CMAP_GIVE(s->bytes, (size_t)s->bytes_room);
    WF_CMAP_GIVE(s, store_bytes(s->room));
}

/* A freed store no larger than a spare may be becomes the calling thread's
 * spare when it has none or the one it has room for fewer keys, which is
 * given back instead (first_store). */
void wf_cmap_key_set_free_store(void *store) {
    key_store *s = store;
    if (s == NULL)
        return;
#ifdef WF_CMAP_SPARE_KEYS
    if (s->room <= KEY_SET_SPARE_ROOM && s->bytes_room <= KEY_SET_SPARE_BYTES) {
        key_store *spare = WF_CMAP_SPARE_KEYS();
        if (spare == NULL || spare->room < s->room) {
            WF_CMAP_SPARE_KEYS() = s;
            if (spare != NULL)
                give_store(spare);
            return;
        }
    }
#endif
    give_store(s);
}

/* Gives back the calling thread's spare store, if it keeps one. */
void wf_cmap_key_set_drop_spare(void) {
#ifdef WF_CMAP_SPARE_KEYS
    key_store *spare = WF_CMAP_SPARE_KEYS();
    WF_CMAP_SPARE_KEYS() = NULL;
    if (spare != NULL)
        give_store(spare);
#endif
}

void wf_cmap_key_set_release(wf_key_set *set) {
    wf_cmap_key_set_free_store(set->store);
    set->len = 0;
    set->store = NULL;
}

/* Holds of several entries: a statement whose keys are values when it
 * begins holds their entries and no others, so statements on other keys go
 * on beside it. A hold's keys are locked in increasing order of their tags,
 * then of their bytes, a proper prefix first, the same order in every
 * table, so
 * a statement waits for a key's entry only while it holds entries of keys
 * before it: holds never wait for each other in a cycle, and a keyed
 * statement holds one entry and waits for none [WAIT-2]. A probe locks every
 * cell of its key's hash to compare the bytes, so a hold's probe may meet a
 * cell the hold has locked itself, which two of its keys of one hash make it
 * do; it then holds the whole map instead, and a wait caused by another
 * statement's key of the same hash ends with the hold's patience, which
 * holds the whole map too. A hold lives in its statement's frame, so one
 * statement may hold entries of several maps, and of several header
 * bindings, at once. A hold asked to hold the whole map holds it from the
 * start, and its keys' entries under it. */

/* How a hold's added keys stand. */
enum { KEYS_INCREASING, KEYS_NONDECREASING, KEYS_SHUFFLED };

/* The most keys whose memory a user keeps for its next hold, so that a hold
 * of many keys once does not keep its memory for the map's life. */
#define SPARE_KEYS_LIMIT 4096ull

static wf_cmap_held *held_keys(wf_cmap_holding *hold) { return hold->keys != NULL ? hold->keys : hold->inline_keys; }

/* The key that is rank-th in the hold's lock order. */
static inline wf_cmap_held *ranked(wf_cmap_held *keys, uint64_t rank) { return &keys[keys[rank].rank]; }

/* A hold's lock order: by tag, the key's 62-bit hash, then by bytes in
 * lexicographic order, a proper prefix first, so that ordering compares
 * integers and reads key bytes only for keys of one tag. */
static int held_order(const wf_cmap_held *a, const wf_cmap_held *b) {
    if (a->tag != b->tag)
        return a->tag < b->tag ? -1 : 1;
    return compare_keys(a->key, a->length, b->key, b->length);
}

/* Memory for at least *room keys, *room set to what it holds: u's spare
 * when it is large enough, else the pool. Only the thread holding u touches
 * u's spare. */
static wf_cmap_held *take_keys(wf_cmap_user *u, uint64_t *room) {
    if (u != NULL && u->spare_keys != NULL && u->spare_room >= *room) {
        wf_cmap_held *keys = u->spare_keys;
        *room = u->spare_room;
        u->spare_keys = NULL;
        u->spare_room = 0;
        return keys;
    }
    if (*room > SIZE_MAX / sizeof(wf_cmap_held))
        WF_CMAP_EXHAUSTED();
    return take((size_t)*room * sizeof(wf_cmap_held));
}

/* Gives memory of room keys back: as u's spare when it is larger than u's
 * spare and within the limit, else to the pool. */
static void give_keys(wf_cmap_user *u, wf_cmap_held *keys, uint64_t room) {
    if (u != NULL && room > u->spare_room && room <= SPARE_KEYS_LIMIT) {
        if (u->spare_keys != NULL)
            WF_CMAP_GIVE(u->spare_keys, (size_t)u->spare_room * sizeof(wf_cmap_held));
        u->spare_keys = keys;
        u->spare_room = room;
        return;
    }
    WF_CMAP_GIVE(keys, (size_t)room * sizeof(wf_cmap_held));
}

/* Room for more keys after those added, in the host's memory once the
 * hold's own is full. */
static wf_cmap_held *reserve_keys(wf_cmap_holding *hold, uint64_t more) {
    wf_cmap_held *keys = held_keys(hold);
    if (more > UINT64_MAX - hold->count)
        WF_CMAP_EXHAUSTED();
    uint64_t need = hold->count + more;
    if (need <= hold->room)
        return keys;
    uint64_t room = hold->room > need / 2 ? hold->room * 2 : need;
    wf_cmap_user *u = WF_CMAP_CURRENT_USER(hold->map);
    wf_cmap_held *grown = take_keys(u, &room);
    if (hold->count != 0)
        memcpy(grown, keys, (size_t)hold->count * sizeof *keys);
    if (hold->keys != NULL)
        give_keys(u, hold->keys, hold->room);
    hold->keys = grown;
    hold->room = room;
    return grown;
}

void wf_cmap_hold_begin(wf_cmap_holding *hold, wf_cmap *map) {
    hold->map = map;
    hold->user = NULL;
    hold->keys = NULL;
    hold->count = 0;
    hold->room = WF_CMAP_HOLD_INLINE;
    hold->table = NULL;
    hold->generation = 0;
    hold->order = KEYS_INCREASING;
    hold->wants = 0;
    hold->whole = 0;
    hold->held = 0;
    hold->primary = NULL;
    hold->first = 0;
    hold->read = 0;
}

void wf_cmap_hold_whole(wf_cmap_holding *hold) { hold->wants = 1; }

/* Notes where a key added after last stands. */
static void note_order(wf_cmap_holding *hold, const wf_cmap_held *last, const wf_cmap_held *next) {
    if (hold->order == KEYS_SHUFFLED)
        return;
    int order = held_order(last, next);
    if (order > 0)
        hold->order = KEYS_SHUFFLED;
    else if (order == 0)
        hold->order = KEYS_NONDECREASING;
}

static void set_held(wf_cmap_held *e, const unsigned char *key, uint64_t length, uint64_t tag) {
    e->key = key;
    e->length = length;
    e->tag = tag;
    e->cell = NULL;
    e->slot = NULL;
    e->rank = 0;
    e->fresh = 0;
    e->leads = 0;
}

uint64_t wf_cmap_hold_key(wf_cmap_holding *hold, const unsigned char *key, uint64_t length) {
    wf_cmap_held *keys = reserve_keys(hold, 1);
    uint64_t position = hold->count;
    set_held(&keys[position], key, length, tag_of(key, length));
    if (position != 0)
        note_order(hold, &keys[position - 1], &keys[position]);
    hold->count = position + 1;
    return position;
}

void *wf_cmap_held_entry(wf_cmap *map, const unsigned char *key, uint64_t length, int write);

uint64_t wf_cmap_hold_keys(wf_cmap_holding *hold, const wf_key_set *set) {
    uint64_t first = hold->count;
    if (hold->whole && hold->user != NULL) {
        const key_store *s = set->store;
        /* Reserve all positions before selecting slots: repeated keys still
         * occupy the set's own index order in the entries reference. */
        wf_cmap_held *aliases = reserve_keys(hold, set->len);
        for (uint64_t i = 0; i < set->len; i++)
            set_held(&aliases[first + i], NULL, 0, 0);
        hold->count += set->len;
        for (uint64_t i = 0; i < set->len; i++) {
            const key_item *item = &s->items[i];
            void *slot = wf_cmap_held_entry(hold->map, item_bytes(s, item), item->length, 1);
            wf_cmap_held *keys = held_keys(hold);
            keys[first + i].slot = slot;
        }
        return first;
    }
    if (set->len == 0)
        return first;
    wf_cmap_held *keys = reserve_keys(hold, set->len);
    const key_store *s = set->store;
    for (uint64_t i = 0; i < set->len; i++) {
        const key_item *item = &s->items[i];
        set_held(&keys[first + i], item_bytes(s, item), item->length, item->tag);
        if (first + i != 0)
            note_order(hold, &keys[first + i - 1], &keys[first + i]);
    }
    hold->count = first + set->len;
    return first;
}

/* Heap sort of the ranks in the hold's lock order, in place. */
static void sift_ranks(wf_cmap_held *keys, uint64_t root, uint64_t end) {
    for (;;) {
        uint64_t child = 2 * root + 1;
        if (child >= end)
            return;
        if (child + 1 < end && held_order(ranked(keys, child), ranked(keys, child + 1)) < 0)
            child++;
        if (held_order(ranked(keys, root), ranked(keys, child)) >= 0)
            return;
        uint64_t swap = keys[root].rank;
        keys[root].rank = keys[child].rank;
        keys[child].rank = swap;
        root = child;
    }
}

/* The most keys a hold orders by insertion rather than by the heap. */
#define HOLD_INSERTION_SORT 32u

/* Ranks the added keys in the hold's lock order, without memory of its own,
 * and marks the first of each run of equal keys, which locks the run's
 * entry; answers how many lead. Keys added in that order are ranked as they
 * stand. */
static uint64_t order_hold(wf_cmap_holding *hold) {
    wf_cmap_held *keys = held_keys(hold);
    uint64_t added = hold->count;
    for (uint64_t i = 0; i < added; i++) {
        keys[i].rank = i;
        keys[i].cell = NULL;
        keys[i].slot = NULL;
        keys[i].fresh = 0;
        keys[i].leads = 1;
    }
    if (hold->order == KEYS_INCREASING)
        return added;
    if (hold->order == KEYS_SHUFFLED && added <= HOLD_INSERTION_SORT) {
        /* A statement's few keys: insertion into the ranks, comparing tags
         * first, costs less than the heap's sifts. */
        for (uint64_t i = 1; i < added; i++) {
            uint64_t moving = keys[i].rank;
            uint64_t j = i;
            while (j > 0 && held_order(&keys[moving], &keys[keys[j - 1].rank]) < 0) {
                keys[j].rank = keys[j - 1].rank;
                j--;
            }
            keys[j].rank = moving;
        }
    } else if (hold->order == KEYS_SHUFFLED) {
        for (uint64_t i = added / 2; i-- > 0;)
            sift_ranks(keys, i, added);
        for (uint64_t end = added; end-- > 1;) {
            uint64_t swap = keys[0].rank;
            keys[0].rank = keys[end].rank;
            keys[end].rank = swap;
            sift_ranks(keys, 0, end);
        }
    }
    uint64_t leaders = 0;
    for (uint64_t i = 0; i < added; i++) {
        wf_cmap_held *e = ranked(keys, i);
        e->leads = i == 0 || held_order(ranked(keys, i - 1), e) != 0;
        leaders += e->leads;
    }
    return leaders;
}

/* Gives back every cell the hold has locked: a found one as it was, a
 * claimed one as removed, counted as used when `counted` and it was claimed
 * from an empty cell, since it stays taken until the table moves. */
static void give_back(wf_cmap_user *u, wf_cmap_holding *hold, int counted) {
    wf_cmap_held *keys = held_keys(hold);
    for (uint64_t i = 0; i < hold->count; i++) {
        wf_cmap_held *e = &keys[i];
        cell *c = e->cell;
        if (c == NULL)
            continue;
        if (e->fresh == 0) {
            unlock(c, atomic_load_explicit(&c->key, memory_order_relaxed) & ~LOCKED);
        } else {
            /* Counted before the unlock, as an unlocked entry is. */
            if (counted && e->fresh == CLAIMED)
                count(u, 1, 0);
            unlock(c, REMOVED);
        }
        e->cell = NULL;
    }
}

/* Names a node for each entry created for the hold and sets every key's
 * slot, a repeated key's to its leader's. */
static void fill_slots(wf_cmap_user *u, wf_cmap_holding *hold) {
    wf_cmap *map = u->map;
    wf_cmap_held *keys = held_keys(hold);
    void *slot = NULL;
    for (uint64_t i = 0; i < hold->count; i++) {
        wf_cmap_held *e = ranked(keys, i);
        if (e->leads) {
            node *n;
            if (e->fresh == 0) {
                n = node_at(e->cell);
            } else {
                n = new_node(u, node_bytes(map, e->length));
                n->length = e->length;
                if (e->length != 0)
                    memcpy(n->bytes, e->key, (size_t)e->length);
                memset(slot_of(map, n), 0, (size_t)map->slot_size);
                name_node(e->cell, n);
                /* A reused removed cell was counted when it was first
                 * claimed. */
                if (e->fresh == CLAIMED)
                    count(u, 1, 0);
            }
            /* From here the key's bytes are the node's: the caller's may
             * change once the hold is taken, and a move under a whole hold
             * finds the entry again by them (wf_cmap_held_entry). */
            e->key = n->bytes;
            slot = slot_of(map, n);
        }
        e->slot = slot;
    }
}

/* Locks every leading key of an ordered hold in the current table: 1 with
 * all of them locked and every slot set, or 0, holding no cell, once the
 * waits and retries exhaust u's patience, the table has no cell left for
 * one of them, or a probe meets a cell the hold has locked itself. A move
 * met on the way cannot be helped while cells are held, since a mover waits
 * for every locked cell, so the cells go back first and the hold is locked
 * again in the next table. */
static int lock_set(wf_cmap_user *u, wf_cmap_holding *hold) {
    wf_cmap *map = u->map;
    wf_cmap_held *keys = held_keys(hold);
    for (;;) {
        table *t = use_current(u);
        uint64_t i = 0;
        if (atomic_load_explicit(&t->next, memory_order_acquire) == NULL) {
            for (; i < hold->count; i++) {
                wf_cmap_held *e = ranked(keys, i);
                if (!e->leads)
                    continue;
                uint64_t tag = e->tag;
                cell *c;
                int r = acquire_entry(u, t, tag, e->key, e->length, &c);
                if (r == IMPATIENT || r == FULL || r == SHARED) {
                    /* A move sizes the next table for the live keys, which
                     * may be no more room than this one had for the hold's
                     * new keys; under the whole map the hold grows the table
                     * for them (lock_whole). */
                    give_back(u, hold, 1);
                    return 0;
                }
                /* A move met inside the probe, which holds no cell for this
                 * key, or met after it, which gives the cell back. */
                if (r == MOVED || !keep_cell(t, c, r, tag)) {
                    give_back(u, hold, 0);
                    break;
                }
                e->cell = c;
                e->fresh = r == FOUND ? 0 : (uint32_t)r;
            }
            if (i == hold->count) {
                fill_slots(u, hold);
                hold->table = t;
                return 1;
            }
        }
        finish_move(map, t);
        if (impatient(u, 1))
            return 0;
    }
}

/* Locks key's cell in t, whose hash is tag, or claims one, for a hold that
 * holds the whole map: no other statement then holds a cell of the current
 * table, so a locked cell of the key's hash is one of the hold's own,
 * holding another key, passed over without waiting, and no other claim can
 * race, so an absent key claims the first removed cell its probe passed, or
 * else the empty cell that ended it, at once. FULL when the table holds
 * neither, and MOVED, holding no cell, when a move out of t had begun before
 * a found cell's node was read. */
static int acquire_whole(const wf_cmap_holding *hold, table *t, uint64_t tag, const unsigned char *key,
                         uint64_t length, cell **out) {
    for (;;) {
        uint64_t i = start_of(t, tag);
        uint64_t left = t->capacity;
        uint64_t at = 0;
        int spared = 0;
        unsigned round = 0;
        for (;;) {
            cell *c = &t->cells[i];
            uint64_t k = atomic_load_explicit(&c->key, memory_order_acquire);
            uint64_t bare = k & ~(LOCKED | PENDING);
            if (bare == tag) {
                if (k & LOCKED) {
                    /* Under the whole map a cell is locked only by the hold
                     * itself; one that is not the hold's is waited out
                     * rather than compared. */
                    if (!holds_cell(hold, c)) {
                        back_off(&round);
                        continue;
                    }
                } else {
                    if (!atomic_compare_exchange_weak_explicit(&c->key, &k, k | LOCKED, memory_order_seq_cst,
                                                               memory_order_relaxed))
                        continue;
                    if (atomic_load_explicit(&t->next, memory_order_seq_cst) != NULL) {
                        unlock(c, k);
                        return MOVED;
                    }
                    wait_for_readers(c);
                    if (same_key(node_at(c), key, length)) {
                        *out = c;
                        return FOUND;
                    }
                    unlock(c, k);
                }
            } else if (bare == EMPTY) {
                break;
            } else if (k == REMOVED && !spared) {
                at = i;
                spared = 1;
            }
            if (--left == 0) {
                if (!spared)
                    return FULL;
                break;
            }
            i = (i + 1) & t->mask;
        }
        uint64_t target = spared ? at : i;
        uint64_t expected = spared ? REMOVED : EMPTY;
        if (atomic_compare_exchange_strong_explicit(&t->cells[target].key, &expected, tag | LOCKED,
                                                    memory_order_seq_cst, memory_order_relaxed)) {
            *out = &t->cells[target];
            return spared ? REUSED : CLAIMED;
        }
    }
}

/* Locks every leading key of an ordered hold under the whole map, first
 * growing the table when it lacks room for all of them, since a move sizes
 * the next table for the live keys only; a hold under the whole map waits
 * for no other statement, so it needs no patience. */
static void lock_whole(wf_cmap_user *u, wf_cmap_holding *hold, uint64_t leaders) {
    wf_cmap *map = u->map;
    wf_cmap_held *keys = held_keys(hold);
    for (;;) {
        table *t = use_current(u);
        if (atomic_load_explicit(&t->next, memory_order_acquire) == NULL) {
            int64_t used, live;
            totals(map, &used, &live);
            int full = used - t->base + (int64_t)leaders > (int64_t)(t->capacity / 2);
            uint64_t i = 0;
            for (; !full && i < hold->count; i++) {
                wf_cmap_held *e = ranked(keys, i);
                if (!e->leads)
                    continue;
                uint64_t tag = e->tag;
                cell *c;
                int r = acquire_whole(hold, t, tag, e->key, e->length, &c);
                if (r == FULL) {
                    give_back(u, hold, 1);
                    full = 1;
                    break;
                }
                if (r == MOVED || !keep_cell(t, c, r, tag)) {
                    give_back(u, hold, 0);
                    break;
                }
                e->cell = c;
                e->fresh = r == FOUND ? 0 : (uint32_t)r;
            }
            if (!full && i == hold->count) {
                fill_slots(u, hold);
                hold->table = t;
                return;
            }
            if (full) {
                start_move_for(map, t, leaders);
                unsigned round = 0;
                while (atomic_load_explicit(&t->next, memory_order_acquire) == NULL)
                    back_off(&round);
            }
        }
        finish_move(map, t);
    }
}

/* Read a sorted set without inventing absent cells. Missing keys require
 * the whole hold so their absence remains stable beside the other keys. */
static void unread_set(wf_cmap_holding *hold) {
    wf_cmap_held *keys = held_keys(hold);
    for (uint64_t i = 0; i < hold->count; ++i) {
        if (keys[i].leads && keys[i].cell != NULL)
            atomic_fetch_sub_explicit(&((cell *)keys[i].cell)->value, READER_ONE, memory_order_release);
        keys[i].cell = NULL;
    }
}

static int read_set(wf_cmap_user *u, wf_cmap_holding *hold) {
    wf_cmap_held *keys = held_keys(hold);
    for (;;) {
        table *t = use_current(u);
        if (atomic_load_explicit(&t->next, memory_order_acquire) == NULL) {
            uint64_t i = 0;
            wf_cmap_held *leader = NULL;
            for (; i < hold->count; ++i) {
                wf_cmap_held *e = ranked(keys, i);
                if (!e->leads) { e->slot = leader->slot; continue; }
                cell *c = NULL;
                int r = read_in(u, t, e->tag, e->key, e->length, &c);
                if (r == FOUND) { e->cell = c; e->slot = slot_of(u->map, node_at(c)); leader = e; }
                if (r == ABSENT || r == IMPATIENT) { unread_set(hold); return 0; }
                if (r == MOVED) break;
            }
            if (i == hold->count && atomic_load_explicit(&t->next, memory_order_seq_cst) == NULL) {
                hold->table = t;
                return 1;
            }
            unread_set(hold);
        }
        finish_move(u->map, t);
        if (impatient(u, 1)) return 0;
    }
}

static void read_whole(wf_cmap_holding *hold) {
    wf_cmap_held *keys = held_keys(hold);
    for (uint64_t i = 0; i < hold->count; ++i) {
        void *slot = wf_cmap_held_entry(hold->map, keys[i].key, keys[i].length, 0);
        keys[i].cell = NULL;
        keys[i].slot = slot != NULL ? slot : hold->map->none;
    }
}

void wf_cmap_hold_take(wf_cmap_user *u, wf_cmap_holding *hold) {
    uint64_t leaders = order_hold(hold);
    hold->user = u;
    hold->whole = 0;
    hold->held = (uint8_t)(u->holding != 0);
    hold->table = NULL;
    u->own = hold;
    if (hold->read) {
        if (!hold->held && hold->wants) { wf_cmap_hold(u); hold->whole = 1; }
        if (!hold->held && !hold->whole && hold->count != 0) {
            enter_keyed(u);
            u->waited = 0;
            u->patience = WF_CMAP_PATIENCE(u);
            if (!read_set(u, hold)) {
                atomic_store_explicit(&u->active, 0, memory_order_release);
                wf_cmap_hold(u);
                hold->whole = 1;
            }
        }
        if (hold->held || hold->whole) read_whole(hold);
        u->own = NULL;
        if (hold->whole) u->map->whole_hold = hold;
        if (hold->whole || hold->held || hold->count != 0)
            hold->generation = u->map->generation;
        return;
    }
    if (hold->held) {
        if (hold->count != 0)
            lock_whole(u, hold, leaders);
    } else if (hold->wants) {
        wf_cmap_hold(u);
        hold->whole = 1;
        if (hold->count != 0)
            lock_whole(u, hold, leaders);
    } else if (hold->count != 0) {
        enter_keyed(u);
        u->waited = 0;
        u->patience = WF_CMAP_PATIENCE(u);
        if (!lock_set(u, hold)) {
            atomic_store_explicit(&u->active, 0, memory_order_release);
            wf_cmap_hold(u);
            hold->whole = 1;
            lock_whole(u, hold, leaders);
        }
    }
    u->own = NULL;
    if (hold->whole)
        u->map->whole_hold = hold;
    /* Read once the hold keeps every other statement from swapping the map's
     * entries (wf_cmap_swap). */
    if (hold->whole || hold->held || hold->count != 0)
        hold->generation = u->map->generation;
}

/* A position past the keys added is one the compiler's own counting gave,
 * which no program can cause, so the program stops. */
void *wf_cmap_hold_slot(const wf_cmap_holding *hold, uint64_t position) {
    const wf_cmap_held *keys = hold->keys != NULL ? hold->keys : hold->inline_keys;
    if (position >= hold->count)
        abort();
    return keys[position].slot;
}

/* Whether a slot holds `Some`: its tag, the tag_width bytes at tag_offset,
 * differs from none_tag. A width other than 1, 2, 4 or 8 is one no compiled
 * program passes, so the program stops. */
static int slot_present(const void *slot, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag) {
    const unsigned char *at = (const unsigned char *)slot + tag_offset;
    uint64_t tag;
    switch (tag_width) {
    case 1: {
        uint8_t narrow;
        memcpy(&narrow, at, sizeof narrow);
        tag = narrow;
        break;
    }
    case 2: {
        uint16_t narrow;
        memcpy(&narrow, at, sizeof narrow);
        tag = narrow;
        break;
    }
    case 4: {
        uint32_t narrow;
        memcpy(&narrow, at, sizeof narrow);
        tag = narrow;
        break;
    }
    case 8:
        memcpy(&tag, at, sizeof tag);
        break;
    default:
        abort();
    }
    return tag != none_tag;
}

/* Settles a taken hold's entries: each kept when its slot's tag says it holds
 * a value, else removed with its slot, and unlocked, counted as it now
 * stands. Answers whether one was created for the hold. */
static int settle_entries(wf_cmap_user *u, wf_cmap_holding *hold, uint64_t tag_offset, uint32_t tag_width,
                          uint64_t none_tag) {
    wf_cmap *map = u->map;
    wf_cmap_held *keys = held_keys(hold);
    int fresh = 0;
    for (uint64_t i = 0; i < hold->count; i++) {
        wf_cmap_held *e = &keys[i];
        if (!e->leads || e->cell == NULL)
            continue;
        cell *c = e->cell;
        int present = slot_present(e->slot, tag_offset, tag_width, none_tag);
        /* Counted before the unlock: a mover waiting for this cell sums the
         * counts once it has the cell, into the next table's base. */
        count(u, 0, (int64_t)present - (int64_t)(e->fresh == 0));
        if (present) {
            unlock(c, atomic_load_explicit(&c->key, memory_order_relaxed) & ~LOCKED);
        } else {
            node *n = node_at(c);
            free_node(u, n, node_bytes(map, n->length));
            unlock(c, REMOVED);
        }
        fresh |= e->fresh != 0;
        e->cell = NULL;
    }
    return fresh;
}

/* The enclosing ownership or shared outer hold keeps this index stable.
 * Reads touch neither the table nor a hold, even if local writes retained one.
 * Only writes claim cells and change the exclusive whole hold's descriptors. */
void *wf_cmap_held_entry(wf_cmap *map, const unsigned char *key, uint64_t length, int write) {
    if (!write) {
        /* Several readers may share an outer entry containing this table.
         * No user publication, hold update or allocation occurs here. */
        table *t = atomic_load_explicit(&map->current, memory_order_relaxed);
        uint64_t tag = tag_of(key, length), at = start_of(t, tag), left = t->capacity;
        while (left-- != 0) {
            cell *c = &t->cells[at];
            uint64_t k = atomic_load_explicit(&c->key, memory_order_relaxed);
            uint64_t bare = k & ~(LOCKED | PENDING);
            if (bare == EMPTY) break;
            if (bare == tag && same_key(node_at(c), key, length)) return slot_of(map, node_at(c));
            at = (at + 1) & t->mask;
        }
        return NULL;
    }
    wf_cmap_holding *hold = map->whole_hold;
    if (hold == NULL)
        abort(); /* Accepted writes always run under a whole atomic hold. */
    wf_cmap_user *u = hold->user;
    uint64_t tag = tag_of(key, length);
    table *t = use_current(u);
    wf_cmap_held *keys = held_keys(hold);
    for (uint64_t i = 0; i < hold->count; i++) {
        wf_cmap_held *e = &keys[i];
        if (e->cell != NULL && e->tag == tag && same_key(node_at(e->cell), key, length))
            return e->slot;
    }
    cell *c;
    int r;
    for (;;) {
        r = acquire_whole(hold, t, tag, key, length, &c);
        if (r != FULL && r != MOVED) break;
        /* Keep every held node, including None slots, stable while the
         * index moves. Count each physical node before unlocking it; the
         * hold's logical count still subtracts its None slots. */
        keys = held_keys(hold);
        uint64_t saved = hold->count;
        for (uint64_t i = 0; i < saved; i++) {
            wf_cmap_held *e = &keys[i];
            if (e->cell == NULL) continue;
            if (e->fresh != 0) count(u, 0, 1);
            e->fresh = 0;
            unlock(e->cell, atomic_load_explicit(&((cell *)e->cell)->key, memory_order_relaxed) & ~LOCKED);
            e->cell = NULL;
        }
        start_move_for(map, t, 1);
        finish_move(map, t);
        t = use_current(u);
        for (uint64_t i = 0; i < saved; i++) {
            wf_cmap_held *e = &keys[i];
            if (!e->leads) continue;
            cell *previous;
            int found = acquire_whole(hold, t, e->tag, e->key, e->length, &previous);
            if (found != FOUND) abort();
            e->cell = previous;
        }
    }
    node *n;
    if (r == FOUND) n = node_at(c);
    else {
        n = new_node(u, node_bytes(map, length));
        n->length = length;
        if (length != 0) memcpy(n->bytes, key, (size_t)length);
        memset(slot_of(map, n), 0, (size_t)map->slot_size);
        name_node(c, n);
        if (r == CLAIMED) count(u, 1, 0);
    }
    keys = reserve_keys(hold, 1);
    wf_cmap_held *e = &keys[hold->count++];
    set_held(e, n->bytes, length, tag);
    e->cell = c;
    e->slot = slot_of(map, n);
    e->fresh = r == FOUND ? 0 : (uint32_t)r;
    e->leads = 1;
    hold->table = t;
    hold->generation = map->generation;
    return e->slot;
}

int wf_cmap_hold_release(wf_cmap_holding *hold, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag) {
    wf_cmap_user *u = hold->user;
    int wrote = 0;
    if (u != NULL && hold->read) {
        if (!hold->held && !hold->whole) unread_set(hold);
        if (hold->whole) {
            if (hold->map->whole_hold == hold) hold->map->whole_hold = NULL;
            wf_cmap_unhold(u);
        } else if (!hold->held && hold->count != 0) {
            atomic_store_explicit(&u->active, 0, memory_order_release);
        }
        if (hold->keys != NULL) give_keys(u, hold->keys, hold->room);
        wf_cmap_hold_begin(hold, hold->map);
        return 0;
    }
    if (u != NULL) {
        wf_cmap *map = u->map;
        /* Swapped since the take, the hold's entries were settled by the
         * swap and went with the other map; they are not this map's to keep
         * or count. */
        int swapped = (hold->whole || hold->held || hold->count != 0) && hold->generation != map->generation;
        int fresh = 0;
        table *t = hold->table;
        wrote = swapped;
        if (hold->count != 0 && !swapped) {
            wrote = 1;
            fresh = settle_entries(u, hold, tag_offset, tag_width, none_tag);
        }
        /* A claim may cross the threshold, as an insert's may. A move it
         * starts ends before the statement leaves the map, so that a hold of
         * the whole map finds no move under way. */
        if (fresh &&
            (t->capacity <= (1ull << 16) || (atomic_load_explicit(&u->used, memory_order_relaxed) & 31) == 0)) {
            int64_t used, live;
            totals(map, &used, &live);
            if (used - t->base > (int64_t)(t->capacity / 2)) {
                start_move(map, t);
                if (atomic_load_explicit(&t->next, memory_order_acquire) != NULL)
                    finish_move(map, t);
            }
        }
        if (hold->whole) {
            if (map->whole_hold == hold)
                map->whole_hold = NULL;
            wf_cmap_unhold(u);
        } else if (!hold->held && hold->count != 0)
            atomic_store_explicit(&u->active, 0, memory_order_release);
    }
    if (hold->keys != NULL)
        give_keys(u != NULL ? u : WF_CMAP_CURRENT_USER(hold->map), hold->keys, hold->room);
    wf_cmap_hold_begin(hold, hold->map);
    return wrote;
}

/* Swaps: what follows a map's entries, exchanged by wf_cmap_swap, and what
 * stays with the map, its identity: the gate and the tickets of whole holds,
 * the users with their marks, and the host's own members. */

/* Moves every user's counts and entry memory into the map itself, where
 * they go with the entries; called with no statement inside the map, so
 * each user also leaves the table it published. A user's free entries stay
 * in chunks that go with the entries, unused until those chunks are
 * unmapped. */
static void fold_users(wf_cmap *map) {
    int n = atomic_load_explicit(&map->users_seen, memory_order_acquire);
    for (int i = 0; i < n; i++) {
        wf_cmap_user *u = &map->users[i];
        atomic_fetch_add_explicit(&map->folded_used, atomic_load_explicit(&u->used, memory_order_relaxed),
                                  memory_order_relaxed);
        atomic_fetch_add_explicit(&map->folded_live, atomic_load_explicit(&u->live, memory_order_relaxed),
                                  memory_order_relaxed);
        atomic_store_explicit(&u->used, 0, memory_order_relaxed);
        atomic_store_explicit(&u->live, 0, memory_order_relaxed);
        for (chunk *c = u->chunks, *older; c != NULL; c = older) {
            older = c->older;
            c->older = map->chunks;
            map->chunks = c;
        }
        u->chunks = NULL;
        u->cursor = NULL;
        u->room = 0;
        memset(u->free, 0, sizeof u->free);
        atomic_store_explicit(&u->in, NULL, memory_order_relaxed);
    }
}

#define SWAP_FIELD(type, field)                                                                                        \
    do {                                                                                                               \
        type swap_held = a->field;                                                                                     \
        a->field = b->field;                                                                                           \
        b->field = swap_held;                                                                                          \
    } while (0)

/* Settles the entries of the hold that holds map whole, so they are unlocked
 * and counted in map before its entries move: the hold's statement uses none
 * of them after a swap, which writes the table they lie in [REF-2]. */
static void settle_whole_hold(wf_cmap *map, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag) {
    wf_cmap_holding *hold = map->whole_hold;
    if (hold == NULL || hold->user == NULL || hold->count == 0)
        return;
    settle_entries(hold->user, hold, tag_offset, tag_width, none_tag);
    hold->count = 0;
}

/* Two maps of one slot layout exchange their entries, their counts, their
 * tables and the memory their entries live in, while each keeps its
 * identity, so a statement that reached either map before goes on reaching
 * the same one. No other statement may be inside either map: the caller
 * holds a whole, or holds neither map's handle in common with anyone. The
 * entries of a hold of either map taken whole before are settled first,
 * reading each slot's tag as a hold's release does; such a hold learns of
 * the swap from the map's generation. */
void wf_cmap_swap(wf_cmap *a, wf_cmap *b, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag) {
    if (a->slot_size != b->slot_size || a->slot_align != b->slot_align)
        abort();
    settle_whole_hold(a, tag_offset, tag_width, none_tag);
    settle_whole_hold(b, tag_offset, tag_width, none_tag);
    fold_users(a);
    fold_users(b);
    table *current = atomic_load_explicit(&a->current, memory_order_relaxed);
    atomic_store_explicit(&a->current, atomic_load_explicit(&b->current, memory_order_relaxed),
                          memory_order_relaxed);
    atomic_store_explicit(&b->current, current, memory_order_relaxed);
    table *retired = atomic_load_explicit(&a->retired, memory_order_relaxed);
    atomic_store_explicit(&a->retired, atomic_load_explicit(&b->retired, memory_order_relaxed),
                          memory_order_relaxed);
    atomic_store_explicit(&b->retired, retired, memory_order_relaxed);
    int64_t folded = atomic_load_explicit(&a->folded_used, memory_order_relaxed);
    atomic_store_explicit(&a->folded_used, atomic_load_explicit(&b->folded_used, memory_order_relaxed),
                          memory_order_relaxed);
    atomic_store_explicit(&b->folded_used, folded, memory_order_relaxed);
    folded = atomic_load_explicit(&a->folded_live, memory_order_relaxed);
    atomic_store_explicit(&a->folded_live, atomic_load_explicit(&b->folded_live, memory_order_relaxed),
                          memory_order_relaxed);
    atomic_store_explicit(&b->folded_live, folded, memory_order_relaxed);
    SWAP_FIELD(cell *, spare);
    SWAP_FIELD(uint64_t, spare_capacity);
    SWAP_FIELD(chunk *, chunks);
    SWAP_FIELD(uint64_t, drained);
    SWAP_FIELD(void *, pending);
    SWAP_FIELD(uint64_t, pending_bytes);
    a->generation += 1;
    b->generation += 1;
    /* Published before either map is handed to another statement. */
    atomic_thread_fence(memory_order_seq_cst);
}

#undef SWAP_FIELD

/* The count under the hold that holds the map whole, if any: each of its
 * entries counted as it stands now, read by its slot's tag, rather than as
 * it stood at the take, which is all the users' counts know before the
 * release. */
uint64_t wf_cmap_count_held(wf_cmap *map, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag) {
    int64_t used, live;
    totals(map, &used, &live);
    wf_cmap_holding *hold = map->whole_hold;
    if (hold != NULL && hold->user != NULL) {
        wf_cmap_held *keys = held_keys(hold);
        for (uint64_t i = 0; i < hold->count; i++) {
            wf_cmap_held *e = &keys[i];
            if (!e->leads || e->cell == NULL)
                continue;
            live += (int64_t)slot_present(e->slot, tag_offset, tag_width, none_tag) - (int64_t)(e->fresh == 0);
        }
    }
    return live > 0 ? (uint64_t)live : 0;
}

/* A key a scan keeps: its position and its node. */
typedef struct {
    uint64_t position;
    const node *n;
} scanned;

/* The order a scan inserts keys in [SHARE-1]: by position, then by bytes,
 * a proper prefix first. */
static int scanned_before(const scanned *a, const scanned *b) {
    if (a->position != b->position)
        return a->position < b->position;
    uint64_t la = a->n->length, lb = b->n->length;
    uint64_t shorter = la < lb ? la : lb;
    int order = shorter == 0 ? 0 : memcmp(a->n->bytes, b->n->bytes, (size_t)shorter);
    return order != 0 ? order < 0 : la < lb;
}

/* Sorts in place, without memory of its own; a step keeps few keys. */
static void sift_scanned(scanned *keys, uint64_t root, uint64_t count) {
    for (;;) {
        uint64_t child = 2 * root + 1;
        if (child >= count)
            return;
        if (child + 1 < count && scanned_before(&keys[child], &keys[child + 1]))
            child++;
        if (!scanned_before(&keys[root], &keys[child]))
            return;
        scanned held = keys[root];
        keys[root] = keys[child];
        keys[child] = held;
        root = child;
    }
}

static void sort_scanned(scanned *keys, uint64_t count) {
    for (uint64_t i = count / 2; i-- != 0;)
        sift_scanned(keys, i, count);
    for (uint64_t end = count; end > 1; end--) {
        scanned held = keys[0];
        keys[0] = keys[end - 1];
        keys[end - 1] = held;
        sift_scanned(keys, 0, end - 1);
    }
}

/* Keys a scan keeps in its own frame before it takes memory from the host. */
#define SCAN_INLINE 32

uint64_t wf_cmap_scan(wf_cmap *map, uint64_t cursor, uint64_t count, wf_key_set *set, uint64_t tag_offset,
                      uint32_t tag_width, uint64_t none_tag) {
    if (map->slot_size == 0)
        abort();
    table *t = atomic_load_explicit(&map->current, memory_order_acquire);
    /* Every statement that starts a move finishes it before it leaves the
     * map, and the caller holds the map, so no move is under way. */
    if (atomic_load_explicit(&t->next, memory_order_acquire) != NULL)
        abort();
    uint64_t live = wf_cmap_count_held(map, tag_offset, tag_width, none_tag);
    if (live == 0)
        return 0;
    uint64_t first = cursor >> t->shift;
    /* The step's homes are [first, end): a cell is read for each until
     * about `count` keys, or count * 10 * capacity / live cells, have been
     * passed, so sparse tables get a proportionally larger cell budget.
     * When want >= live, the cell budget is the capacity and the homes run
     * to the table's end without the key bound ending the walk.
     * A zero count means ten; rounding cells per key up and capping before
     * multiplying keeps the budget within the capacity without overflow. */
    uint64_t want = count != 0 ? count : 10;
    uint64_t cells = t->capacity / live + (t->capacity % live != 0);
    cells = want >= live || want > t->capacity / 10 / cells ? t->capacity : want * 10 * cells;
    uint64_t end = first, passed = 0;
    while (end < t->capacity) {
        uint64_t k = atomic_load_explicit(&t->cells[end].key, memory_order_relaxed) & KEY_MASK;
        end++;
        if (want < live && k != EMPTY && k != REMOVED && ++passed >= want)
            break;
        if (end - first >= cells)
            break;
    }
    /* The last home's position bound: past every position when the step
     * reaches the table's end. */
    int last = end == t->capacity;
    uint64_t bound = last ? 0 : end << t->shift;
    scanned inline_keys[SCAN_INLINE];
    scanned *keys = inline_keys;
    uint64_t kept = 0, room = SCAN_INLINE;
    /* A key lies at its home or after it, before the first empty cell
     * after it, wrapping at the table's end; removed cells end no run. So
     * the cells from the first home to the first empty cell at or past the
     * last home hold every key of the step's homes, and a full table is
     * read once. */
    for (uint64_t step = 0; step < t->capacity; step++) {
        cell *c = &t->cells[(first + step) & t->mask];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_relaxed) & KEY_MASK;
        if (k == EMPTY) {
            if (first + step + 1 >= end)
                break;
            continue;
        }
        if (k == REMOVED)
            continue;
        uint64_t position = position_of(k);
        if (position < cursor || (!last && position >= bound))
            continue;
        const node *n = node_at(c);
        if (!slot_present(slot_of(map, (node *)n), tag_offset, tag_width, none_tag))
            continue;
        if (kept == room) {
            scanned *grown = take((size_t)(2 * room) * sizeof(scanned));
            memcpy(grown, keys, (size_t)kept * sizeof(scanned));
            if (keys != inline_keys)
                WF_CMAP_GIVE(keys, (size_t)room * sizeof(scanned));
            keys = grown;
            room *= 2;
        }
        keys[kept].position = position;
        keys[kept].n = n;
        kept++;
    }
    sort_scanned(keys, kept);
    for (uint64_t i = 0; i < kept; i++)
        wf_cmap_key_set_insert(set, keys[i].n->bytes, keys[i].n->length);
    if (keys != inline_keys)
        WF_CMAP_GIVE(keys, (size_t)room * sizeof(scanned));
    return bound;
}

void wf_cmap_clear(wf_cmap *map, uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag,
                   void (*release)(void *)) {
    if (map->slot_size == 0 || map->whole_hold == NULL)
        abort();
    wf_cmap *cleared = wf_cmap_create_entries(map->slot_size, map->slot_align, 0);
    wf_cmap_swap(map, cleared, tag_offset, tag_width, none_tag);
    cleared->cleared_release = release;
    cleared->cleared_next = map->cleared;
    map->cleared = cleared;
}

wf_cmap *wf_cmap_take_cleared(wf_cmap *map) {
    wf_cmap *cleared = map->cleared;
    map->cleared = NULL;
    return cleared;
}

void wf_cmap_release_cleared(wf_cmap *cleared) {
    while (cleared != NULL) {
        wf_cmap *next = cleared->cleared_next;
        cleared->cleared_release(cleared);
        cleared = next;
    }
}

uint64_t wf_cmap_count(wf_cmap *map) {
    int64_t used, live;
    totals(map, &used, &live);
    return live > 0 ? (uint64_t)live : 0;
}

void *wf_cmap_drain(wf_cmap *map) {
    if (map->pending != NULL) {
        if (map->pending_bytes > ENTRY_LARGEST)
            WF_CMAP_GIVE(map->pending, map->pending_bytes);
        else
            WF_CMAP_HEAP_CHANGE(-(int64_t)map->pending_bytes);
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
        /* Keep counting the node while the caller releases its value.
         * Small nodes then become unused chunk storage, large ones return
         * to the pool; neither remains live until chunk unmapping. */
        map->pending = n;
        map->pending_bytes = node_bytes(map, n->length);
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
    /* Emitted drops have drained every value. Native callers may destroy
     * slots with no owned payload directly; release their nodes too. */
    if (map->slot_size != 0)
        while (wf_cmap_drain(map) != NULL) {
        }
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
    for (chunk *c = map->chunks; c;) {
        chunk *older = c->older;
        host_unmap(c, ENTRY_CHUNK);
        c = older;
    }
    for (int i = 0; i < WF_CMAP_MAX_USERS; i++) {
        wf_cmap_user *u = &map->users[i];
        for (chunk *c = u->chunks; c;) {
            chunk *older = c->older;
            host_unmap(c, ENTRY_CHUNK);
            c = older;
        }
        if (u->spare_keys != NULL)
            WF_CMAP_GIVE(u->spare_keys, (size_t)u->spare_room * sizeof(wf_cmap_held));
    }
    if (map->none)
        WF_CMAP_GIVE(map->none, none_bytes(map->slot_size));
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
