/* Serves the concurrent-map investigation: its first candidate index,
 * research/investigations/concurrent-map/DESIGN.md, "The index: a first
 * design". A prototype for measurement; the runtime takes it only once the
 * criteria choose it.
 *
 * A bucket is one 64-byte line: a ticket lock whose taken count doubles as
 * the bucket's version, three key and value slots, and a pointer to an
 * overflow bucket whose low bit marks a bucket moved to the next table.
 * Writers hold the home bucket's ticket; readers take none and check that
 * no ticket was taken while they read. Keys are never zero; zero marks an
 * empty slot.
 */
#define _GNU_SOURCE
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "cmap.h"

#define SLOTS 3
#define MIN_BITS 4
#define CHUNK 64
#define MAX_THREADS 256
#define MOVED ((uintptr_t)1)

typedef struct bucket {
    _Atomic uint32_t served;
    _Atomic uint32_t taken;
    _Atomic uint64_t key[SLOTS];
    _Atomic uint64_t value[SLOTS];
    _Atomic uintptr_t next; /* overflow bucket, tagged MOVED on a home bucket */
} bucket;

_Static_assert(sizeof(bucket) == 64, "a bucket is one cache line");

typedef struct table {
    bucket *buckets;
    uint64_t count;
    unsigned shift; /* 64 minus the index bits */
    _Atomic(struct table *) next;
    _Atomic uint64_t claimed;
    _Atomic uint64_t moved;
    struct table *older; /* retired tables, freed with the map */
} table;

typedef struct {
    _Alignas(64) _Atomic int64_t keys;
    _Atomic int used;
} counter;

struct cm_map {
    _Alignas(64) _Atomic(table *) current;
    _Alignas(64) _Atomic int64_t folded;
    _Atomic(table *) retired;
    _Atomic int counters_used; /* counters at or above this index were never used */
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

static inline uint64_t hash_of(uint64_t key) { return key * CM_GOLDEN; }

static inline bucket *home(table *t, uint64_t h) { return &t->buckets[h >> t->shift]; }

static inline bucket *untag(uintptr_t link) { return (bucket *)(link & ~MOVED); }

static bucket *new_buckets(uint64_t count) {
    bucket *b = aligned_alloc(64, count * sizeof(bucket));
    if (b)
        memset(b, 0, count * sizeof(bucket));
    return b;
}

static table *new_table(unsigned bits) {
    table *t = calloc(1, sizeof *t);
    if (t == NULL)
        return NULL;
    t->count = 1ull << bits;
    t->shift = 64 - bits;
    t->buckets = new_buckets(t->count);
    if (t->buckets == NULL) {
        free(t);
        return NULL;
    }
    return t;
}

/* Takes the bucket's ticket and waits to be served. */
static inline uint32_t lock(bucket *b) {
    uint32_t ticket = atomic_fetch_add_explicit(&b->taken, 1, memory_order_relaxed);
    unsigned rounds = 0;
    for (;;) {
        uint32_t served = atomic_load_explicit(&b->served, memory_order_acquire);
        if (served == ticket)
            break;
        uint32_t ahead = ticket - served;
        for (uint32_t k = 0; k < ahead * 16; k++)
            pause_once();
        if (++rounds > 4096) {
            sched_yield();
            rounds = 0;
        }
    }
    /* The ticket is ordered before every store under it, so a reader that
     * sees one of those stores also sees the ticket and starts over. */
    atomic_thread_fence(memory_order_release);
    return ticket;
}

static inline void unlock(bucket *b, uint32_t ticket) {
    atomic_store_explicit(&b->served, ticket + 1, memory_order_release);
}

static void count_keys(cm_map *map, int64_t delta) {
    counter *c = my_count;
    if (c)
        atomic_store_explicit(&c->keys, atomic_load_explicit(&c->keys, memory_order_relaxed) + delta,
                              memory_order_relaxed);
    else
        atomic_fetch_add_explicit(&map->folded, delta, memory_order_relaxed);
}

static int64_t keys_now(cm_map *map) {
    int64_t sum = atomic_load_explicit(&map->folded, memory_order_relaxed);
    int used = atomic_load_explicit(&map->counters_used, memory_order_acquire);
    for (int i = 0; i < used; i++)
        sum += atomic_load_explicit(&map->counts[i].keys, memory_order_relaxed);
    return sum;
}

/* Appends a pair to a chain known not to hold its key; the caller holds the
 * chain's home ticket or owns the chain outright. Returns 1 when it had to
 * allocate an overflow bucket. */
static int append(bucket *b, uint64_t key, uint64_t value) {
    for (;;) {
        for (int i = 0; i < SLOTS; i++) {
            if (atomic_load_explicit(&b->key[i], memory_order_relaxed) == 0) {
                atomic_store_explicit(&b->value[i], value, memory_order_relaxed);
                atomic_store_explicit(&b->key[i], key, memory_order_relaxed);
                return 0;
            }
        }
        bucket *next = untag(atomic_load_explicit(&b->next, memory_order_relaxed));
        if (next == NULL) {
            bucket *fresh = new_buckets(1);
            if (fresh == NULL)
                abort();
            atomic_store_explicit(&fresh->value[0], value, memory_order_relaxed);
            atomic_store_explicit(&fresh->key[0], key, memory_order_relaxed);
            uintptr_t tag = atomic_load_explicit(&b->next, memory_order_relaxed) & MOVED;
            atomic_store_explicit(&b->next, (uintptr_t)fresh | tag, memory_order_release);
            return 1;
        }
        b = next;
    }
}

/* Moves one home bucket of t into the two buckets of t's successor it
 * splits into, and marks it moved. */
static void move_bucket(table *t, table *nt, uint64_t index) {
    bucket *b = &t->buckets[index];
    uint32_t ticket = lock(b);
    for (bucket *c = b; c; c = untag(atomic_load_explicit(&c->next, memory_order_relaxed))) {
        for (int i = 0; i < SLOTS; i++) {
            uint64_t key = atomic_load_explicit(&c->key[i], memory_order_relaxed);
            if (key)
                append(home(nt, hash_of(key)), key, atomic_load_explicit(&c->value[i], memory_order_relaxed));
        }
    }
    uintptr_t link = atomic_load_explicit(&b->next, memory_order_relaxed);
    atomic_store_explicit(&b->next, link | MOVED, memory_order_release);
    unlock(b, ticket);
}

/* Moves runs of buckets of t until none is left to claim; the mover of the
 * last run makes the successor current. */
static void help(cm_map *map, table *t) {
    table *nt = atomic_load_explicit(&t->next, memory_order_acquire);
    for (;;) {
        uint64_t start = atomic_fetch_add_explicit(&t->claimed, CHUNK, memory_order_relaxed);
        if (start >= t->count)
            return;
        uint64_t end = start + CHUNK < t->count ? start + CHUNK : t->count;
        for (uint64_t i = start; i < end; i++)
            move_bucket(t, nt, i);
        uint64_t done = atomic_fetch_add_explicit(&t->moved, end - start, memory_order_acq_rel) + (end - start);
        if (done == t->count) {
            atomic_store_explicit(&map->current, nt, memory_order_release);
            /* Retire t: it stays allocated until the map is destroyed, since
             * readers may still be inside it. */
            table *old = atomic_load_explicit(&map->retired, memory_order_relaxed);
            do {
                t->older = old;
            } while (!atomic_compare_exchange_weak_explicit(&map->retired, &old, t, memory_order_acq_rel,
                                                            memory_order_relaxed));
            return;
        }
    }
}

/* Starts doubling t if no move has started, then helps whichever move is
 * under way. */
static void grow(cm_map *map, table *t) {
    if (atomic_load_explicit(&t->next, memory_order_acquire) == NULL) {
        table *nt = new_table(64 - t->shift + 1);
        if (nt == NULL)
            abort();
        table *expected = NULL;
        if (!atomic_compare_exchange_strong_explicit(&t->next, &expected, nt, memory_order_acq_rel,
                                                     memory_order_acquire)) {
            free(nt->buckets);
            free(nt);
        }
    }
    help(map, t);
}

/* The table a writer starts in: the current one, after helping any move
 * from it. */
static table *writer_table(cm_map *map) {
    table *t = atomic_load_explicit(&map->current, memory_order_acquire);
    if (atomic_load_explicit(&t->next, memory_order_acquire) != NULL) {
        help(map, t);
        t = atomic_load_explicit(&map->current, memory_order_acquire);
    }
    return t;
}

/* Locks the home bucket of h, following moved buckets to later tables. */
static bucket *lock_home(cm_map *map, uint64_t h, uint32_t *ticket) {
    table *t = writer_table(map);
    for (;;) {
        bucket *b = home(t, h);
        *ticket = lock(b);
        if (!(atomic_load_explicit(&b->next, memory_order_relaxed) & MOVED))
            return b;
        unlock(b, *ticket);
        t = atomic_load_explicit(&t->next, memory_order_acquire);
    }
}

const char *CM(name)(void) { return "wf-index"; }
int CM(flags)(void) { return 0; }

cm_map *CM(create)(uint64_t capacity) {
    unsigned bits = MIN_BITS;
    while (bits < 40 && (double)(1ull << bits) * 2.25 < (double)capacity)
        bits++;
    cm_map *map = aligned_alloc(64, sizeof(cm_map));
    if (map == NULL)
        return NULL;
    memset(map, 0, sizeof *map);
    table *t = new_table(bits);
    if (t == NULL) {
        free(map);
        return NULL;
    }
    atomic_store(&map->current, t);
    return map;
}

static void free_table(table *t) {
    for (uint64_t i = 0; i < t->count; i++) {
        bucket *c = untag(atomic_load_explicit(&t->buckets[i].next, memory_order_relaxed));
        while (c) {
            bucket *next = untag(atomic_load_explicit(&c->next, memory_order_relaxed));
            free(c);
            c = next;
        }
    }
    free(t->buckets);
    free(t);
}

void CM(destroy)(cm_map *map) {
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

void CM(enter)(cm_map *map) {
    for (int i = 0; i < MAX_THREADS; i++) {
        int free_slot = 0;
        if (atomic_compare_exchange_strong(&map->counts[i].used, &free_slot, 1)) {
            my_count = &map->counts[i];
            int used = atomic_load(&map->counters_used);
            while (used < i + 1 && !atomic_compare_exchange_weak(&map->counters_used, &used, i + 1)) {
            }
            return;
        }
    }
    my_count = NULL;
}

void CM(leave)(cm_map *map) {
    counter *c = my_count;
    if (c == NULL)
        return;
    atomic_fetch_add_explicit(&map->folded, atomic_load_explicit(&c->keys, memory_order_relaxed),
                              memory_order_relaxed);
    atomic_store_explicit(&c->keys, 0, memory_order_relaxed);
    atomic_store(&c->used, 0);
    my_count = NULL;
}

int CM(get)(cm_map *map, uint64_t key, uint64_t *value) {
    uint64_t h = hash_of(key);
    table *t = atomic_load_explicit(&map->current, memory_order_acquire);
    bucket *b = home(t, h);
    for (;;) {
        uint32_t served = atomic_load_explicit(&b->served, memory_order_acquire);
        uint32_t taken = atomic_load_explicit(&b->taken, memory_order_acquire);
        if (taken != served) {
            pause_once();
            continue;
        }
        uintptr_t link = atomic_load_explicit(&b->next, memory_order_acquire);
        if (link & MOVED) {
            t = atomic_load_explicit(&t->next, memory_order_acquire);
            b = home(t, h);
            continue;
        }
        int found = 0;
        uint64_t seen = 0;
        for (bucket *c = b; c && !found;) {
            for (int i = 0; i < SLOTS; i++) {
                if (atomic_load_explicit(&c->key[i], memory_order_relaxed) == key) {
                    seen = atomic_load_explicit(&c->value[i], memory_order_relaxed);
                    found = 1;
                    break;
                }
            }
            c = untag(atomic_load_explicit(&c->next, memory_order_relaxed));
        }
        atomic_thread_fence(memory_order_acquire);
        if (atomic_load_explicit(&b->taken, memory_order_relaxed) != taken)
            continue;
        if (found)
            *value = seen;
        return found;
    }
}

/* The slot of key in a locked chain, or NULL. */
static _Atomic uint64_t *slot_of(bucket *b, uint64_t key, _Atomic uint64_t **value) {
    for (bucket *c = b; c; c = untag(atomic_load_explicit(&c->next, memory_order_relaxed))) {
        for (int i = 0; i < SLOTS; i++) {
            if (atomic_load_explicit(&c->key[i], memory_order_relaxed) == key) {
                *value = &c->value[i];
                return &c->key[i];
            }
        }
    }
    return NULL;
}

int CM(insert)(cm_map *map, uint64_t key, uint64_t value) {
    uint64_t h = hash_of(key);
    uint32_t ticket;
    bucket *b = lock_home(map, h, &ticket);
    _Atomic uint64_t *v;
    if (slot_of(b, key, &v)) {
        atomic_store_explicit(v, value, memory_order_relaxed);
        unlock(b, ticket);
        return 0;
    }
    int overflowed = append(b, key, value);
    unlock(b, ticket);
    count_keys(map, 1);
    if (overflowed) {
        table *t = atomic_load_explicit(&map->current, memory_order_acquire);
        if ((double)keys_now(map) > (double)t->count * 2.25)
            grow(map, t);
    }
    return 1;
}

int CM(remove)(cm_map *map, uint64_t key) {
    uint32_t ticket;
    bucket *b = lock_home(map, hash_of(key), &ticket);
    _Atomic uint64_t *v;
    _Atomic uint64_t *k = slot_of(b, key, &v);
    if (k)
        atomic_store_explicit(k, 0, memory_order_relaxed);
    unlock(b, ticket);
    if (k)
        count_keys(map, -1);
    return k != NULL;
}

int CM(update)(cm_map *map, uint64_t key) {
    uint32_t ticket;
    bucket *b = lock_home(map, hash_of(key), &ticket);
    _Atomic uint64_t *v;
    _Atomic uint64_t *k = slot_of(b, key, &v);
    if (k)
        atomic_store_explicit(v, atomic_load_explicit(v, memory_order_relaxed) + 1, memory_order_relaxed);
    unlock(b, ticket);
    return k != NULL;
}
