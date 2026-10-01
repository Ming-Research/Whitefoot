/* The runtime's concurrent map (concurrent_map.h). Its design and the
 * measurements behind it are in research/investigations/concurrent-map/
 * DESIGN.md, "The index".
 *
 * Open addressing with linear probing over 16-byte cells, a key word and a
 * value, so that most operations touch one cache line. The key word's top
 * bit locks the cell and the next marks it moved to the next table; zero is
 * an empty cell and KEY_MASK a removed one, so keys lie in [1, 2^62 - 2].
 *
 * A writer locks its key's cell by compare-and-swap, runs once and stores
 * the key back: changes to a key are exclusive. A reader takes no lock: it
 * reads the key word, the value and the key word again, and starts over when
 * the cell was locked or changed between. A removed key leaves its cell
 * marked removed until the table is moved, so a probe never stops early.
 *
 * A table is moved, to a larger one or to one of its own size that drops
 * removed cells, once half its cells are used: writers mark blocks of cells
 * moved and copy their keys, readers go on reading the frozen cells, and
 * writers that meet the move help it and then retry in the new table. Moved
 * tables stay allocated until the map is destroyed, since a reader may still
 * be inside one.
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
#define MOVED (1ull << 62)
#define KEY_MASK ((1ull << 62) - 1)
#define EMPTY 0ull
#define REMOVED KEY_MASK
/* Cells a map created without a capacity starts with, 64 KiB. */
#define DEFAULT_CELLS 4096ull
#define MIN_CELLS 16ull
/* Cells one helper moves at a time. */
#define BLOCK 4096ull
#define MAX_THREADS 256

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
    int64_t base;             /* claims counted before this table was current */
    struct table *older;      /* moved tables, freed with the map */
} table;

/* What one thread changed: cells it claimed and keys it added less those it
 * removed; summed only when a claim may cross the threshold. */
typedef struct {
    _Alignas(64) _Atomic int64_t used;
    _Atomic int64_t live;
    _Atomic int busy;
} counter;

struct wf_cmap {
    _Alignas(64) _Atomic(table *) current;
    _Alignas(64) _Atomic int64_t folded_used;
    _Atomic int64_t folded_live;
    _Atomic(table *) retired;
    _Atomic int counters_used;
    counter counts[MAX_THREADS];
};

static _Thread_local counter *my_count;

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

/* One multiplication by the 64-bit golden ratio: cheap, and it spreads keys
 * over the high bits the starting cell is taken from. */
static inline uint64_t start_of(const table *t, uint64_t key) { return (key * 0x9E3779B97F4A7C15ull) >> t->shift; }

static void bad_key(void) { abort(); }

static inline void check_key(uint64_t key) {
    if (__builtin_expect(key == EMPTY || key >= REMOVED, 0))
        bad_key();
}

#define HUGE_BYTES ((size_t)2 << 20)

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
#ifdef MADV_HUGEPAGE
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

static table *new_table(uint64_t capacity) {
    table *t = calloc(1, sizeof *t);
    if (t == NULL)
        return NULL;
    unsigned bits = 0;
    while ((1ull << bits) < capacity)
        bits++;
    t->capacity = 1ull << bits;
    t->mask = t->capacity - 1;
    t->shift = 64 - bits;
    t->cells = new_cells(t->capacity);
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

static void count(wf_cmap *map, int64_t used, int64_t live) {
    counter *c = my_count;
    if (c) {
        atomic_store_explicit(&c->used, atomic_load_explicit(&c->used, memory_order_relaxed) + used,
                              memory_order_relaxed);
        atomic_store_explicit(&c->live, atomic_load_explicit(&c->live, memory_order_relaxed) + live,
                              memory_order_relaxed);
    } else {
        atomic_fetch_add_explicit(&map->folded_used, used, memory_order_relaxed);
        atomic_fetch_add_explicit(&map->folded_live, live, memory_order_relaxed);
    }
}

static void totals(wf_cmap *map, int64_t *used, int64_t *live) {
    int64_t u = atomic_load_explicit(&map->folded_used, memory_order_relaxed);
    int64_t l = atomic_load_explicit(&map->folded_live, memory_order_relaxed);
    int n = atomic_load_explicit(&map->counters_used, memory_order_acquire);
    for (int i = 0; i < n; i++) {
        u += atomic_load_explicit(&map->counts[i].used, memory_order_relaxed);
        l += atomic_load_explicit(&map->counts[i].live, memory_order_relaxed);
    }
    *used = u;
    *live = l;
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

/* Freezes one block of t's cells, waiting out the writers that hold them,
 * and copies their keys into t's successor. */
static void move_block(table *t, table *nt, uint64_t block) {
    uint64_t end = (block + 1) * BLOCK < t->capacity ? (block + 1) * BLOCK : t->capacity;
    for (uint64_t i = block * BLOCK; i < end; i++) {
        cell *c = &t->cells[i];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_acquire);
        unsigned round = 0;
        for (;;) {
            if (k & LOCKED) {
                back_off(&round);
                k = atomic_load_explicit(&c->key, memory_order_acquire);
                continue;
            }
            if (atomic_compare_exchange_weak_explicit(&c->key, &k, k | MOVED, memory_order_acq_rel,
                                                      memory_order_acquire))
                break;
        }
        if (k != EMPTY && k != REMOVED)
            place(nt, k, atomic_load_explicit(&c->value, memory_order_relaxed));
    }
}

/* Moves blocks of t until none is left to claim; the mover of the last makes
 * the successor current. */
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
            atomic_store_explicit(&map->current, nt, memory_order_release);
            table *old = atomic_load_explicit(&map->retired, memory_order_relaxed);
            do {
                t->older = old;
            } while (!atomic_compare_exchange_weak_explicit(&map->retired, &old, t, memory_order_acq_rel,
                                                            memory_order_relaxed));
        }
    }
}

/* Helps the move out of t and waits until its successor is current. */
static table *finish_move(wf_cmap *map, table *t) {
    help(map, t);
    unsigned round = 0;
    table *now;
    while ((now = atomic_load_explicit(&map->current, memory_order_acquire)) == t)
        back_off(&round);
    return now;
}

/* Starts moving t, if no move has started, to a table that holds the live
 * keys at a quarter of its cells. */
static void start_move(wf_cmap *map, table *t) {
    if (atomic_load_explicit(&t->next, memory_order_acquire) != NULL)
        return;
    int64_t used, live;
    totals(map, &used, &live);
    uint64_t capacity = MIN_CELLS;
    while (capacity < 4 * (uint64_t)(live > 0 ? live : 0))
        capacity <<= 1;
    if (capacity < t->capacity / 2)
        capacity = t->capacity / 2 > MIN_CELLS ? t->capacity / 2 : MIN_CELLS;
    table *nt = new_table(capacity);
    if (nt == NULL)
        abort();
    table *expected = NULL;
    if (!atomic_compare_exchange_strong_explicit(&t->next, &expected, nt, memory_order_acq_rel,
                                                 memory_order_acquire))
        free_table(nt);
}

/* The table a writer starts in: the current one, once no move out of it is
 * under way. */
static table *writer_table(wf_cmap *map) {
    table *t = atomic_load_explicit(&map->current, memory_order_acquire);
    while (atomic_load_explicit(&t->next, memory_order_acquire) != NULL)
        t = finish_move(map, t);
    return t;
}

enum { FOUND, CLAIMED, ABSENT, RETRY };

/* Locks key's cell in t, or with claim set locks an empty cell for it;
 * RETRY when the probe met a moved cell. */
static int acquire(table *t, uint64_t key, int claim, cell **out) {
    uint64_t i = start_of(t, key);
    unsigned round = 0;
    for (;;) {
        cell *c = &t->cells[i];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_acquire);
        if (k & MOVED)
            return RETRY;
        uint64_t bare = k & ~LOCKED;
        if (bare == key) {
            if (k & LOCKED) {
                back_off(&round);
                continue;
            }
            if (atomic_compare_exchange_weak_explicit(&c->key, &k, k | LOCKED, memory_order_acquire,
                                                      memory_order_relaxed)) {
                *out = c;
                return FOUND;
            }
            continue;
        }
        if (bare == EMPTY) {
            if (!claim)
                return ABSENT;
            if (atomic_compare_exchange_weak_explicit(&c->key, &k, key | LOCKED, memory_order_acquire,
                                                      memory_order_relaxed)) {
                *out = c;
                return CLAIMED;
            }
            continue;
        }
        i = (i + 1) & t->mask;
    }
}

/* Locks key's cell in the current table, helping any move it meets. */
static int lock_key(wf_cmap *map, uint64_t key, int claim, cell **out, table **in) {
    table *t = writer_table(map);
    for (;;) {
        int r = acquire(t, key, claim, out);
        if (r != RETRY) {
            *in = t;
            return r;
        }
        t = finish_move(map, t);
    }
}

static inline void unlock(cell *c, uint64_t key) { atomic_store_explicit(&c->key, key, memory_order_release); }

wf_cmap *wf_cmap_create(uint64_t capacity) {
    wf_cmap *map = aligned_alloc(64, sizeof(wf_cmap));
    if (map == NULL)
        return NULL;
    memset(map, 0, sizeof *map);
    uint64_t cells = capacity ? 2 * capacity : DEFAULT_CELLS;
    table *t = new_table(cells > MIN_CELLS ? cells : MIN_CELLS);
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
    free(map);
}

void wf_cmap_enter(wf_cmap *map) {
    for (int i = 0; i < MAX_THREADS; i++) {
        int idle = 0;
        if (atomic_compare_exchange_strong(&map->counts[i].busy, &idle, 1)) {
            my_count = &map->counts[i];
            int n = atomic_load(&map->counters_used);
            while (n < i + 1 && !atomic_compare_exchange_weak(&map->counters_used, &n, i + 1)) {
            }
            return;
        }
    }
    my_count = NULL;
}

void wf_cmap_leave(wf_cmap *map) {
    counter *c = my_count;
    if (c == NULL)
        return;
    atomic_fetch_add_explicit(&map->folded_used, atomic_load_explicit(&c->used, memory_order_relaxed),
                              memory_order_relaxed);
    atomic_fetch_add_explicit(&map->folded_live, atomic_load_explicit(&c->live, memory_order_relaxed),
                              memory_order_relaxed);
    atomic_store_explicit(&c->used, 0, memory_order_relaxed);
    atomic_store_explicit(&c->live, 0, memory_order_relaxed);
    atomic_store(&c->busy, 0);
    my_count = NULL;
}

int wf_cmap_get(wf_cmap *map, uint64_t key, uint64_t *value) {
#ifdef WF_CMAP_LOCKED_READ
    cell *c;
    table *t;
    if (lock_key(map, key, 0, &c, &t) == ABSENT)
        return 0;
    *value = atomic_load_explicit(&c->value, memory_order_relaxed);
    unlock(c, key);
    return 1;
#else
    table *t = atomic_load_explicit(&map->current, memory_order_acquire);
    uint64_t i = start_of(t, key);
    for (;;) {
        cell *c = &t->cells[i];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_acquire);
        uint64_t bare = k & ~(LOCKED | MOVED);
        if (__builtin_expect(bare == key, 1)) {
            if (__builtin_expect(k & LOCKED, 0)) {
                pause_once();
                continue;
            }
            uint64_t v = atomic_load_explicit(&c->value, memory_order_relaxed);
            atomic_thread_fence(memory_order_acquire);
            if (__builtin_expect(atomic_load_explicit(&c->key, memory_order_relaxed) != k, 0))
                continue;
            *value = v;
            return 1;
        }
        if (bare == EMPTY)
            return 0;
        i = (i + 1) & t->mask;
    }
#endif
}

int wf_cmap_insert(wf_cmap *map, uint64_t key, uint64_t value) {
    check_key(key);
    cell *c;
    table *t;
    int r = lock_key(map, key, 1, &c, &t);
    atomic_store_explicit(&c->value, value, memory_order_relaxed);
    unlock(c, key);
    if (r == FOUND)
        return 0;
    count(map, 1, 1);
    /* A claim may cross the threshold: half the cells used. Small tables
     * check every claim, so that racing claims never fill one. */
    counter *mine = my_count;
    int64_t local = mine ? atomic_load_explicit(&mine->used, memory_order_relaxed) : 0;
    if (t->capacity <= (1ull << 16) || (local & 31) == 0) {
        int64_t used, live;
        totals(map, &used, &live);
        if (used - t->base > (int64_t)(t->capacity / 2)) {
            start_move(map, t);
            finish_move(map, t);
        }
    }
    return 1;
}

int wf_cmap_remove(wf_cmap *map, uint64_t key) {
    cell *c;
    table *t;
    if (lock_key(map, key, 0, &c, &t) == ABSENT)
        return 0;
    unlock(c, REMOVED);
    count(map, 0, -1);
    return 1;
}

int wf_cmap_update(wf_cmap *map, uint64_t key, void (*edit)(uint64_t *value, void *env), void *env) {
    cell *c;
    table *t;
    if (lock_key(map, key, 0, &c, &t) == ABSENT)
        return 0;
    uint64_t value = atomic_load_explicit(&c->value, memory_order_relaxed);
    edit(&value, env);
    atomic_store_explicit(&c->value, value, memory_order_relaxed);
    unlock(c, key);
    return 1;
}
