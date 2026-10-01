/* The runtime's concurrent map (concurrent_map.h). Its design and the
 * measurements behind it are in research/investigations/concurrent-map/
 * DESIGN.md, "The index".
 *
 * Open addressing with linear probing over 16-byte cells, a key word and a
 * value, so that most operations touch one cache line. The key word's top
 * bit locks the cell, and the next is kept for marking waiters once they
 * park; zero is an empty cell and KEY_MASK a removed one, so keys lie in
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
 */
#define _GNU_SOURCE
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>

#include "concurrent_map.h"

#define LOCKED (1ull << 63)
#define KEY_MASK ((1ull << 62) - 1)
#define EMPTY 0ull
#define REMOVED KEY_MASK
/* Cells a map created without a capacity starts with, 64 KiB. */
#define DEFAULT_CELLS 4096ull
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

/* One thread's use of the map: the table it is in, and the cells it claimed
 * and keys it added less those it removed, summed only when a claim may
 * cross the threshold. */
struct wf_cmap_user {
    _Alignas(64) _Atomic(table *) in;
    _Atomic int64_t used;
    _Atomic int64_t live;
    wf_cmap *map;
    _Atomic int busy;
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
        sched_yield();
        *round = 6;
    }
}

/* A writer that finds its key's cell locked waits 16 pauses, then twice as
 * long each time up to 1024, about 0.2 to 12 microseconds on the measuring
 * host and as long as a sleep and wake by the system: the holder then makes
 * many changes in a row rather than handing the cell's line to a waiter on
 * each. Waiting writers can be overtaken without bound until they park
 * (DESIGN.md, "The index"). */
static inline void wait_for_cell(unsigned *round) {
    unsigned shift = *round < 6 ? *round + 4 : 10;
    for (unsigned i = 0; i < (1u << shift); i++)
        pause_once();
    if (*round < 6)
        ++*round;
}

/* One multiplication by the 64-bit golden ratio: cheap, and it spreads keys
 * over the high bits the starting cell is taken from. */
static inline uint64_t start_of(const table *t, uint64_t key) { return (key * 0x9E3779B97F4A7C15ull) >> t->shift; }

static void bad_key(void) { abort(); }

static inline void check_key(uint64_t key) {
    if (__builtin_expect(key == EMPTY || key >= REMOVED, 0))
        bad_key();
}

/* Cell arrays of 2 MiB or more are mapped, so that their pages arrive zeroed
 * when first touched, by whichever mover touches them, instead of being
 * cleared up front by one thread. They are aligned to and advised into huge
 * pages, since a random probe in a large table otherwise pays a page walk on
 * most accesses. */
static cell *new_cells(uint64_t count) {
    size_t bytes = count * sizeof(cell);
    if (bytes < HUGE_BYTES) {
        cell *c = aligned_alloc(64, bytes);
        if (c != NULL)
            memset(c, 0, bytes);
        return c;
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
    return (cell *)start;
}

static void free_cells(cell *c, uint64_t count) {
    size_t bytes = count * sizeof(cell);
    if (bytes < HUGE_BYTES)
        free(c);
    else
        munmap(c, bytes);
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
    table *t = calloc(1, sizeof *t);
    if (t == NULL)
        return NULL;
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
    if (t->cells == NULL) {
        free(t);
        return NULL;
    }
    return t;
}

static void free_table(table *t) {
    free_cells(t->cells, t->capacity);
    free(t);
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
            free(r);
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
static void move_block(table *t, table *nt, uint64_t block) {
    uint64_t end = (block + 1) * BLOCK < t->capacity ? (block + 1) * BLOCK : t->capacity;
    for (uint64_t i = block * BLOCK; i < end; i++) {
        cell *c = &t->cells[i];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_seq_cst);
        unsigned round = 0;
        while (k & LOCKED) {
            back_off(&round);
            k = atomic_load_explicit(&c->key, memory_order_seq_cst);
        }
        if (k != EMPTY && k != REMOVED)
            place(nt, k, atomic_load_explicit(&c->value, memory_order_relaxed));
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
        move_block(t, nt, block);
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
    if (nt == NULL)
        abort();
    atomic_store_explicit(&t->next, nt, memory_order_seq_cst);
}

enum { FOUND, CLAIMED, ABSENT, FULL };

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

wf_cmap *wf_cmap_create(uint64_t capacity) {
    wf_cmap *map = aligned_alloc(64, sizeof(wf_cmap));
    if (map == NULL)
        return NULL;
    memset(map, 0, sizeof *map);
    for (int i = 0; i < WF_CMAP_MAX_USERS; i++)
        map->users[i].map = map;
    /* Half full when it holds capacity keys, as dense as a table gets
     * before it moves, since reads cost less in a smaller table. */
    table *t = new_table(NULL, capacity ? cells_for(capacity, 1, 2) : DEFAULT_CELLS);
    if (t == NULL) {
        free(map);
        return NULL;
    }
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
    free(map);
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
