/* The runtime's concurrent map (concurrent_map.h). Its design and the
 * measurements behind it are in research/investigations/concurrent-map/
 * DESIGN.md, "The index: a first design".
 *
 * A bucket is one 64-byte line: a ticket lock whose taken count doubles as
 * the bucket's version, three key and value slots, and a pointer to an
 * overflow bucket whose low bit marks a bucket moved to the next table.
 * Writers hold the home bucket's ticket; readers take none and check that
 * no ticket was taken while they read. Keys are never zero; zero marks an
 * empty slot.
 *
 * Two read variants are built only for measurement, beside the copy-out
 * read: WF_CMAP_LOCKED_READ, where a read holds its bucket's ticket like a
 * writer, as every atomic statement holds its object today, and
 * WF_CMAP_SHARED_READ, where readers of a bucket hold it together under a
 * reader-writer ticket lock. Which read the language uses for statements
 * that only read is an open question of the investigation's stage (b).
 */
#define _GNU_SOURCE
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>

#include "concurrent_map.h"

#define SLOTS 3
/* The table doubles once it holds this many keys per bucket: with three
 * slots a bucket then overflows rarely enough that a lookup's one branch is
 * almost always predicted. */
#define KEYS_PER_BUCKET 1.5
/* A map created without a capacity starts with 2^10 buckets, 64 KiB; one
 * created with a capacity starts with at least 2^2. */
#define DEFAULT_BITS 10
#define MIN_BITS 2
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

struct wf_cmap {
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

/* One multiplication by the 64-bit golden ratio: cheap, and it spreads keys
 * over the high bits the bucket index is taken from. */
static inline uint64_t hash_of(uint64_t key) { return key * 0x9E3779B97F4A7C15ull; }

static inline bucket *home(table *t, uint64_t h) { return &t->buckets[h >> t->shift]; }

static inline bucket *untag(uintptr_t link) { return (bucket *)(link & ~MOVED); }

/* Bucket arrays of 2 MiB or more are aligned to and advised into huge
 * pages: a random lookup in a large table otherwise pays a page walk on
 * most accesses, which costs a third of single-thread throughput at 2^20
 * keys on the measuring host. */
static bucket *new_buckets(uint64_t count) {
    size_t bytes = count * sizeof(bucket);
    size_t align = bytes >= (2u << 20) ? (2u << 20) : 64;
    bytes = (bytes + align - 1) / align * align;
    bucket *b = aligned_alloc(align, bytes);
    if (b == NULL)
        return NULL;
#ifdef MADV_HUGEPAGE
    if (align > 64)
        madvise(b, bytes, MADV_HUGEPAGE);
#endif
    memset(b, 0, bytes);
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

#ifdef WF_CMAP_SHARED_READ
/* Built with WF_CMAP_SHARED_READ, the state is a reader-writer ticket lock
 * (Mellor-Crummey and Scott): taken counts writers in its high half and
 * readers in its low half, served counts those that have left. A writer
 * waits for every writer and reader that came before it; a reader waits
 * only for the writers before it, so readers of one bucket hold it together
 * and arrival order is kept between readers and writers. Halves count
 * modulo 2^16 and a reader changes its half without carrying into the
 * writers'. */
static inline uint32_t lock(bucket *b) {
    uint32_t t = atomic_fetch_add_explicit(&b->taken, 1u << 16, memory_order_relaxed);
    for (;;) {
        uint32_t served = atomic_load_explicit(&b->served, memory_order_acquire);
        if ((served >> 16) == (t >> 16) && (served & 0xFFFFu) == (t & 0xFFFFu))
            return t;
        pause_once();
    }
}

static inline void unlock(bucket *b, uint32_t ticket) {
    (void)ticket;
    atomic_fetch_add_explicit(&b->served, 1u << 16, memory_order_release);
}

static inline uint32_t low_increment(uint32_t word) { return (word & 0xFFFF0000u) | ((word + 1) & 0xFFFFu); }

static inline void lock_shared(bucket *b) {
    uint32_t old = atomic_load_explicit(&b->taken, memory_order_relaxed);
    while (!atomic_compare_exchange_weak_explicit(&b->taken, &old, low_increment(old), memory_order_relaxed,
                                                  memory_order_relaxed)) {
    }
    while ((atomic_load_explicit(&b->served, memory_order_acquire) >> 16) != (old >> 16))
        pause_once();
}

static inline void unlock_shared(bucket *b) {
    uint32_t old = atomic_load_explicit(&b->served, memory_order_relaxed);
    while (!atomic_compare_exchange_weak_explicit(&b->served, &old, low_increment(old), memory_order_release,
                                                  memory_order_relaxed)) {
    }
}
#else
/* Takes the bucket's ticket and waits to be served. */
static inline uint32_t lock(bucket *b) {
    uint32_t ticket = atomic_fetch_add_explicit(&b->taken, 1, memory_order_relaxed);
    unsigned rounds = 0;
    for (;;) {
        uint32_t served = atomic_load_explicit(&b->served, memory_order_acquire);
        if (served == ticket)
            break;
        /* The next in line checks at once; the others wait in proportion to
         * the holders ahead of them, so the line is not passed among
         * waiters that cannot take it yet. */
        uint32_t ahead = ticket - served;
        for (uint32_t k = 0; k < (ahead - 1) * 32; k++)
            pause_once();
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
#endif

static void count_keys(wf_cmap *map, int64_t delta) {
    counter *c = my_count;
    if (c)
        atomic_store_explicit(&c->keys, atomic_load_explicit(&c->keys, memory_order_relaxed) + delta,
                              memory_order_relaxed);
    else
        atomic_fetch_add_explicit(&map->folded, delta, memory_order_relaxed);
}

static int64_t keys_now(wf_cmap *map) {
    int64_t sum = atomic_load_explicit(&map->folded, memory_order_relaxed);
    int used = atomic_load_explicit(&map->counters_used, memory_order_acquire);
    for (int i = 0; i < used; i++)
        sum += atomic_load_explicit(&map->counts[i].keys, memory_order_relaxed);
    return sum;
}

/* The index of the slot of c holding key, or SLOTS when none does, chosen
 * by conditional moves rather than branches: which slot holds a key is
 * random, so a branch on it would mispredict and discard the following
 * operations' loads. */
static inline int match(bucket *c, uint64_t key) {
    uint64_t k0 = atomic_load_explicit(&c->key[0], memory_order_relaxed);
    uint64_t k1 = atomic_load_explicit(&c->key[1], memory_order_relaxed);
    uint64_t k2 = atomic_load_explicit(&c->key[2], memory_order_relaxed);
    int i = k2 == key ? 2 : SLOTS;
    i = k1 == key ? 1 : i;
    return k0 == key ? 0 : i;
}

/* Appends a pair to a chain known not to hold its key; the caller holds the
 * chain's home ticket or owns the chain outright. Returns 1 when it had to
 * allocate an overflow bucket. */
static int append(bucket *b, uint64_t key, uint64_t value) {
    for (;;) {
        int i = match(b, 0);
        if (i < SLOTS) {
            atomic_store_explicit(&b->value[i], value, memory_order_relaxed);
            atomic_store_explicit(&b->key[i], key, memory_order_relaxed);
            return 0;
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
static void help(wf_cmap *map, table *t) {
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
static void grow(wf_cmap *map, table *t) {
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
static table *writer_table(wf_cmap *map) {
    table *t = atomic_load_explicit(&map->current, memory_order_acquire);
    if (atomic_load_explicit(&t->next, memory_order_acquire) != NULL) {
        help(map, t);
        t = atomic_load_explicit(&map->current, memory_order_acquire);
    }
    return t;
}

/* Locks the home bucket of h, following moved buckets to later tables. */
static bucket *lock_home(wf_cmap *map, uint64_t h, uint32_t *ticket) {
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

wf_cmap *wf_cmap_create(uint64_t capacity) {
    unsigned bits = capacity ? MIN_BITS : DEFAULT_BITS;
    while (bits < 40 && (double)(1ull << bits) * KEYS_PER_BUCKET < (double)capacity)
        bits++;
    wf_cmap *map = aligned_alloc(64, sizeof(wf_cmap));
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

void wf_cmap_leave(wf_cmap *map) {
    counter *c = my_count;
    if (c == NULL)
        return;
    atomic_fetch_add_explicit(&map->folded, atomic_load_explicit(&c->keys, memory_order_relaxed),
                              memory_order_relaxed);
    atomic_store_explicit(&c->keys, 0, memory_order_relaxed);
    atomic_store(&c->used, 0);
    my_count = NULL;
}

static _Atomic uint64_t *slot_of(bucket *b, uint64_t key, _Atomic uint64_t **value);
static bucket *lock_home(wf_cmap *map, uint64_t h, uint32_t *ticket);
static __attribute__((noinline)) int get_slow(wf_cmap *map, uint64_t h, uint64_t key, uint64_t *value);

int wf_cmap_get(wf_cmap *map, uint64_t key, uint64_t *value) {
#ifdef WF_CMAP_LOCKED_READ
    uint32_t ticket;
    bucket *held = lock_home(map, hash_of(key), &ticket);
    _Atomic uint64_t *v;
    int present = slot_of(held, key, &v) != NULL;
    if (present)
        *value = atomic_load_explicit(v, memory_order_relaxed);
    unlock(held, ticket);
    return present;
#elif defined(WF_CMAP_SHARED_READ)
    uint64_t hashed = hash_of(key);
    table *tab = atomic_load_explicit(&map->current, memory_order_acquire);
    bucket *held;
    for (;;) {
        held = home(tab, hashed);
        lock_shared(held);
        if (!(atomic_load_explicit(&held->next, memory_order_relaxed) & MOVED))
            break;
        unlock_shared(held);
        tab = atomic_load_explicit(&tab->next, memory_order_acquire);
    }
    _Atomic uint64_t *v;
    int present = slot_of(held, key, &v) != NULL;
    if (present)
        *value = atomic_load_explicit(v, memory_order_relaxed);
    unlock_shared(held);
    return present;
#endif
    uint64_t h = hash_of(key);
    table *t = atomic_load_explicit(&map->current, memory_order_acquire);
    bucket *b = home(t, h);
    uint32_t served = atomic_load_explicit(&b->served, memory_order_acquire);
    uint32_t taken = atomic_load_explicit(&b->taken, memory_order_acquire);
    uintptr_t link = atomic_load_explicit(&b->next, memory_order_acquire);
    uint64_t m0 = -(uint64_t)(atomic_load_explicit(&b->key[0], memory_order_relaxed) == key);
    uint64_t m1 = -(uint64_t)(atomic_load_explicit(&b->key[1], memory_order_relaxed) == key);
    uint64_t m2 = -(uint64_t)(atomic_load_explicit(&b->key[2], memory_order_relaxed) == key);
    uint64_t seen = (atomic_load_explicit(&b->value[0], memory_order_relaxed) & m0) |
                    (atomic_load_explicit(&b->value[1], memory_order_relaxed) & m1) |
                    (atomic_load_explicit(&b->value[2], memory_order_relaxed) & m2);
    uint64_t hit = (m0 | m1 | m2) & 1;
    /* One rarely taken branch: a writer holds the bucket, the bucket has
     * moved, or the key is not in the home slots and an overflow bucket
     * exists. */
    if (__builtin_expect((taken != served) | ((link & MOVED) != 0) | ((hit == 0) & (link != 0)), 0))
        return get_slow(map, h, key, value);
    atomic_thread_fence(memory_order_acquire);
    if (__builtin_expect(atomic_load_explicit(&b->taken, memory_order_relaxed) != taken, 0))
        return get_slow(map, h, key, value);
    *value = seen;
    return (int)hit;
}

/* The copy-out read's general path: waits out writers, follows moves and
 * overflow buckets. */
static int get_slow(wf_cmap *map, uint64_t h, uint64_t key, uint64_t *value) {
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
        int i = match(c, key);
        if (i < SLOTS) {
            *value = &c->value[i];
            return &c->key[i];
        }
    }
    return NULL;
}

int wf_cmap_insert(wf_cmap *map, uint64_t key, uint64_t value) {
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
        if ((double)keys_now(map) > (double)t->count * KEYS_PER_BUCKET)
            grow(map, t);
    }
    return 1;
}

int wf_cmap_remove(wf_cmap *map, uint64_t key) {
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

int wf_cmap_update(wf_cmap *map, uint64_t key, void (*edit)(uint64_t *value, void *env), void *env) {
    uint32_t ticket;
    bucket *b = lock_home(map, hash_of(key), &ticket);
    _Atomic uint64_t *v;
    _Atomic uint64_t *k = slot_of(b, key, &v);
    if (k) {
        uint64_t value = atomic_load_explicit(v, memory_order_relaxed);
        edit(&value, env);
        atomic_store_explicit(v, value, memory_order_relaxed);
    }
    unlock(b, ticket);
    return k != NULL;
}
