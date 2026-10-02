/* Tests the runtime's concurrent map (concurrent_map.c):
 *
 * - one thread's random operations against a plain reference, on a map
 *   created for one key so that it grows many times;
 * - threads whose updates must all reach the sum of the values;
 * - threads inserting and removing, whose counts must match what is left;
 * - recorded histories of threads on few keys, and on many keys while the
 *   table grows, checked key by key for linearizability, which is local to
 *   each object (Herlihy and Wing), by Wing and Gong's search with Lowe's
 *   memoization as Porcupine implements it; the checker is first shown to
 *   refuse a history that is not linearizable and to accept one that is;
 * - interleavings too rare for threads to meet, driven step by step through
 *   the map's own functions, which is why the test includes its source;
 * - a map of entries: one thread's random operations on byte-string keys of
 *   many lengths against a plain reference, through moves; threads counting
 *   on shared keys while another holds the whole map and finds every
 *   entry's sum equal to a total each statement adds to with its entry
 *   locked; threads removing and keeping a few keys, spread or all starting
 *   at one cell so that their claims race in one run, never holding one
 *   key in two statements or two cells; and a drain that hands out every
 *   present entry once;
 * - a keyed statement that waits out its patience and holds the whole map
 *   instead: from inside a claim it gives back, after a lost claim of an
 *   empty or a removed cell, a move or a lost lock it must retry, after a
 *   bounded number of statements on a key others keep locking, and with
 *   every statement doing so while the map moves, holds count and claims
 *   race; and statements over the whole map that hold it in the order they
 *   asked, each after the keyed statements the hold before it kept
 *   waiting.
 *
 * Prints the first failure and exits 1, or exits 0.
 */
#define _GNU_SOURCE
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include <sched.h>

/* The host the runtime supplies the map, here from the C library. */
#define WF_CMAP_TAKE(bytes) aligned_alloc(16, ((size_t)(bytes) + 15) / 16 * 16)
#define WF_CMAP_GIVE(block, bytes) free(block)
#define WF_CMAP_YIELD() sched_yield()
#define WF_CMAP_EXHAUSTED() abort()
struct table;
struct cell;
static void before_claim(struct table *t, unsigned long long index);
static void before_lock(struct cell *c);
static uint64_t counted_key(uint64_t k, unsigned char *bytes);
#define WF_CMAP_BEFORE_CLAIM(t, index) before_claim((t), (index))
#define WF_CMAP_BEFORE_LOCK(c) before_lock(c)
static void read_found(struct table *t);
#define WF_CMAP_READ_FOUND(t) read_found(t)
struct wf_cmap_user;
static uint64_t patience_of(struct wf_cmap_user *u);
static void hold_seen(struct wf_cmap_user *u, int closed);
#define WF_CMAP_PATIENCE(u) patience_of(u)
#define WF_CMAP_HOLD_QUEUED(u) hold_seen((u), 0)
#define WF_CMAP_HOLD_CLOSED(u) hold_seen((u), 1)

/* The tests a build runs: locked reads change only wf_cmap_get, which only
 * the tests of word keys call, and narrowed hashes change only entries'
 * hashes, which only the tests of entries use, so each such build runs the
 * tests its change reaches, the default build runs both, and it alone runs
 * the tests of turns and bounds on one key, which neither change reaches. */
#ifdef WF_CMAP_TAG_MASK
#define WORD_TESTS 0
#else
#define WORD_TESTS 1
#endif
#ifdef WF_CMAP_LOCKED_READ
#define ENTRY_TESTS 0
#else
#define ENTRY_TESTS 1
#endif

#include "concurrent_map.c"

#define THREADS 4

/* Each user's patience, the map's own but in the tests that set it. */
static uint64_t patience[WF_CMAP_MAX_USERS];

static uint64_t patience_of(wf_cmap_user *u) { return patience[u - u->map->users]; }

static void set_patience(uint64_t first, uint64_t rest) {
    patience[0] = first;
    for (unsigned i = 1; i < WF_CMAP_MAX_USERS; i++)
        patience[i] = rest;
}

/* A clock the tests of holds count statements on, and what it read when
 * each user's statement over the whole map took its place in line and when
 * it closed the gate. */
static _Atomic uint64_t *hold_clock;
static uint64_t queued_at[WF_CMAP_MAX_USERS], closed_at[WF_CMAP_MAX_USERS];
static _Atomic int closed_seen[WF_CMAP_MAX_USERS];

static void hold_seen(wf_cmap_user *u, int closed) {
    _Atomic uint64_t *clock = hold_clock;
    if (clock != NULL)
        (closed ? closed_at : queued_at)[u - u->map->users] = atomic_load(clock);
    if (closed)
        atomic_store(&closed_seen[u - u->map->users], 1);
}

static uint64_t mix64(uint64_t z) {
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ull;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBull;
    return z ^ (z >> 31);
}

static void fail(const char *what, unsigned long long a, unsigned long long b) {
    printf("concurrent-map-test: %s (%llu, %llu)\n", what, a, b);
    exit(1);
}

static uint64_t next(uint64_t *state) {
    *state += 0x9E3779B97F4A7C15ull;
    return mix64(*state);
}

/* Distinct keys in the map's range [1, 2^62 - 2]: the finalizer's steps
 * taken modulo 2^62 are a bijection, and the two values it could give that
 * fall outside the range are checked for. */
static uint64_t key_of(uint64_t index) {
    const uint64_t mask = (1ull << 62) - 1;
    uint64_t x = index & mask;
    x ^= x >> 31;
    x = (x * 0xBF58476D1CE4E5B9ull) & mask;
    x ^= x >> 29;
    x = (x * 0x94D049BB133111EBull) & mask;
    x ^= x >> 32;
    if (x + 1 >= mask)
        fail("a test key falls outside the map's range", index, x);
    return x + 1;
}


static void add_one(uint64_t *value, void *env) {
    (void)env;
    *value += 1;
}

static void sequential(void) {
    enum { KEYS = 4096, OPS = 1000000 };
    static uint8_t present[KEYS];
    static uint64_t value[KEYS];
    wf_cmap *map = wf_cmap_create(1);
    wf_cmap_user *user = wf_cmap_enter(map);
    uint64_t state = 7;
    for (unsigned i = 0; i < OPS; i++) {
        uint64_t r = next(&state);
        unsigned k = (unsigned)((r >> 8) % KEYS);
        uint64_t key = key_of(k), got = 0;
        int result;
        switch (r & 3) {
        case 0:
            result = wf_cmap_get(user, key, &got);
            if (result != present[k] || (result && got != value[k]))
                fail("a get disagrees with the reference", k, got);
            break;
        case 1:
            result = wf_cmap_insert(user, key, r);
            if (result != !present[k])
                fail("an insert disagrees with the reference", k, (unsigned long long)result);
            present[k] = 1;
            value[k] = r;
            break;
        case 2:
            result = wf_cmap_remove(user, key);
            if (result != present[k])
                fail("a remove disagrees with the reference", k, (unsigned long long)result);
            present[k] = 0;
            break;
        default:
            result = wf_cmap_update(user, key, add_one, NULL);
            if (result != present[k])
                fail("an update disagrees with the reference", k, (unsigned long long)result);
            value[k] += (uint64_t)present[k];
            break;
        }
    }
    for (unsigned k = 0; k < KEYS; k++) {
        uint64_t got = 0;
        int result = wf_cmap_get(user, key_of(k), &got);
        if (result != present[k] || (result && got != value[k]))
            fail("a key's final state disagrees with the reference", k, got);
    }
    wf_cmap_leave(user);
    wf_cmap_destroy(map);
}

enum { SHARED_KEYS = 1 << 14 };

typedef struct {
    wf_cmap *map;
    unsigned thread;
    uint64_t updates, inserted, removed;
    int churn;
} worker_t;

static void *work(void *arg) {
    worker_t *w = arg;
    uint64_t state = mix64(0xC0FFEEull + w->thread);
    wf_cmap_user *user = wf_cmap_enter(w->map);
    for (unsigned i = 0; i < 200000; i++) {
        uint64_t r = next(&state);
        if (!w->churn) {
            w->updates += (uint64_t)wf_cmap_update(user, key_of((r >> 8) % SHARED_KEYS), add_one, NULL);
        } else {
            uint64_t key = key_of((r >> 8) % (2 * SHARED_KEYS));
            if (r & 1)
                w->inserted += (uint64_t)wf_cmap_insert(user, key, r);
            else
                w->removed += (uint64_t)wf_cmap_remove(user, key);
        }
    }
    wf_cmap_leave(user);
    return NULL;
}

/* Runs THREADS workers on a map holding the keys below SHARED_KEYS, with
 * value equal to their index, and checks what is left. */
static void concurrent(int churn) {
    wf_cmap *map = wf_cmap_create(churn ? 1 : SHARED_KEYS);
    wf_cmap_user *user = wf_cmap_enter(map);
    for (uint64_t i = 0; i < SHARED_KEYS; i++)
        wf_cmap_insert(user, key_of(i), i);
    pthread_t t[THREADS];
    worker_t w[THREADS];
    for (unsigned i = 0; i < THREADS; i++) {
        w[i] = (worker_t){map, i, 0, 0, 0, churn};
        pthread_create(&t[i], NULL, work, &w[i]);
    }
    uint64_t updates = 0, inserted = 0, removed = 0;
    for (unsigned i = 0; i < THREADS; i++) {
        pthread_join(t[i], NULL);
        updates += w[i].updates;
        inserted += w[i].inserted;
        removed += w[i].removed;
    }
    uint64_t sum = 0, live = 0;
    for (uint64_t i = 0; i < 2 * SHARED_KEYS; i++) {
        uint64_t v;
        if (wf_cmap_get(user, key_of(i), &v)) {
            live++;
            sum += v;
        }
    }
    if (!churn && (live != SHARED_KEYS || sum != (uint64_t)SHARED_KEYS * (SHARED_KEYS - 1) / 2 + updates))
        fail("an update was lost", live, sum);
    if (churn && live != SHARED_KEYS + inserted - removed)
        fail("the live count after churn is wrong", live, SHARED_KEYS + inserted - removed);
    wf_cmap_leave(user);
    wf_cmap_destroy(map);
}

/* Linearizability. */

enum { GET, INSERT, REMOVE, UPDATE };

typedef struct {
    uint64_t call, ret, arg, out;
    int kind, key, result;
    unsigned thread;
} op_t;

typedef struct {
    wf_cmap *map;
    op_t *ops;
    unsigned count, thread, keys;
    _Atomic int *go;
} history_t;

/* What stamps an operation's call and return: one counter, sequentially
 * consistent, so that a return stamped before a call means the first
 * operation happens before the second, the order linearizability asks the
 * map to respect. A clock read is not ordered with the operation's own
 * memory accesses, and two processors' clocks need not agree to within an
 * operation's length, so clocks could order two operations that overlap. */
static _Atomic uint64_t history_clock;

static void *record(void *arg) {
    history_t *h = arg;
    uint64_t state = mix64(0x57AE55ull ^ h->thread);
    wf_cmap_user *user = wf_cmap_enter(h->map);
    while (!atomic_load(h->go)) {
    }
    for (unsigned i = 0; i < h->count; i++) {
        uint64_t r = next(&state);
        op_t *o = &h->ops[i];
        o->key = (int)((r >> 8) % h->keys);
        o->kind = (int)(r & 3);
        /* Distinct values far apart, so that updates never make one value
         * look like another. */
        o->arg = ((uint64_t)(h->thread + 1) << 40) | ((uint64_t)i << 12);
        uint64_t key = key_of((uint64_t)o->key), v = 0;
        o->thread = h->thread;
        o->call = atomic_fetch_add(&history_clock, 1);
        switch (o->kind) {
        case GET:
            o->result = wf_cmap_get(user, key, &v);
            o->out = v;
            break;
        case INSERT:
            o->result = wf_cmap_insert(user, key, o->arg);
            break;
        case REMOVE:
            o->result = wf_cmap_remove(user, key);
            break;
        default:
            o->result = wf_cmap_update(user, key, add_one, NULL);
            break;
        }
        o->ret = atomic_fetch_add(&history_clock, 1);
    }
    wf_cmap_leave(user);
    return NULL;
}

typedef struct {
    int present;
    uint64_t value;
} reg_t;

/* Applies o to the sequential map s; 0 when o's result is not the one the
 * sequential map gives. */
static int apply(reg_t *s, const op_t *o) {
    switch (o->kind) {
    case GET:
        return o->result == s->present && (!o->result || o->out == s->value);
    case INSERT:
        if (o->result != !s->present)
            return 0;
        s->present = 1;
        s->value = o->arg;
        return 1;
    case REMOVE:
        if (o->result != s->present)
            return 0;
        s->present = 0;
        return 1;
    default:
        if (o->result != s->present)
            return 0;
        s->value += (uint64_t)s->present;
        return 1;
    }
}

typedef struct entry {
    struct entry *prev, *next, *match;
    uint64_t time;
    int id, is_call;
} entry_t;

typedef struct {
    uint64_t *bits;
    reg_t state;
} seen_t;

static int order_entries(const void *a, const void *b) {
    const entry_t *x = *(entry_t *const *)a, *y = *(entry_t *const *)b;
    if (x->time != y->time)
        return x->time < y->time ? -1 : 1;
    return y->is_call - x->is_call;
}

static uint64_t hash_seen(const uint64_t *bits, unsigned words, reg_t s) {
    uint64_t h = (uint64_t)s.present * 31 + s.value;
    for (unsigned i = 0; i < words; i++)
        h = mix64(h ^ bits[i]);
    return h;
}

/* 1 when some order of ops consistent with their real-time order explains
 * every result. */
static int linearizable(op_t **ops, unsigned n) {
    if (n == 0)
        return 1;
    entry_t *entries = calloc(2 * (size_t)n, sizeof *entries);
    entry_t **order = malloc(2 * (size_t)n * sizeof *order);
    for (unsigned i = 0; i < n; i++) {
        entries[2 * i] = (entry_t){.time = ops[i]->call, .id = (int)i, .is_call = 1};
        entries[2 * i + 1] = (entry_t){.time = ops[i]->ret, .id = (int)i, .is_call = 0};
        entries[2 * i].match = &entries[2 * i + 1];
        order[2 * i] = &entries[2 * i];
        order[2 * i + 1] = &entries[2 * i + 1];
    }
    qsort(order, 2 * (size_t)n, sizeof *order, order_entries);
    entry_t head = {0};
    entry_t *last = &head;
    for (unsigned i = 0; i < 2 * n; i++) {
        last->next = order[i];
        order[i]->prev = last;
        last = order[i];
    }
    unsigned words = (n + 63) / 64;
    uint64_t *bits = calloc(words, sizeof *bits);
    size_t cap = 1u << 16, used = 0;
    seen_t *cache = calloc(cap, sizeof *cache);
    typedef struct {
        entry_t *call;
        reg_t state;
    } frame_t;
    frame_t *stack = malloc((size_t)n * sizeof *stack);
    unsigned depth = 0;
    reg_t state = {0, 0};
    entry_t *e = head.next;
    int ok = 1;
    while (head.next != NULL) {
        if (e != NULL && e->is_call) {
            reg_t after = state;
            int fresh = 0;
            if (apply(&after, ops[e->id])) {
                bits[e->id / 64] |= 1ull << (e->id % 64);
                size_t slot = hash_seen(bits, words, after) & (cap - 1);
                fresh = 1;
                while (cache[slot].bits != NULL) {
                    if (cache[slot].state.present == after.present && cache[slot].state.value == after.value &&
                        memcmp(cache[slot].bits, bits, words * sizeof *bits) == 0) {
                        fresh = 0;
                        break;
                    }
                    slot = (slot + 1) & (cap - 1);
                }
                if (fresh) {
                    cache[slot].bits = malloc(words * sizeof *bits);
                    memcpy(cache[slot].bits, bits, words * sizeof *bits);
                    cache[slot].state = after;
                    if (++used * 2 > cap) {
                        size_t old_cap = cap;
                        seen_t *old = cache;
                        cap *= 2;
                        cache = calloc(cap, sizeof *cache);
                        for (size_t k = 0; k < old_cap; k++) {
                            if (old[k].bits == NULL)
                                continue;
                            size_t s = hash_seen(old[k].bits, words, old[k].state) & (cap - 1);
                            while (cache[s].bits != NULL)
                                s = (s + 1) & (cap - 1);
                            cache[s] = old[k];
                        }
                        free(old);
                    }
                } else {
                    bits[e->id / 64] &= ~(1ull << (e->id % 64));
                }
            }
            if (fresh) {
                stack[depth++] = (frame_t){e, state};
                state = after;
                e->prev->next = e->next;
                if (e->next)
                    e->next->prev = e->prev;
                entry_t *r = e->match;
                r->prev->next = r->next;
                if (r->next)
                    r->next->prev = r->prev;
                e = head.next;
            } else {
                e = e->next;
            }
        } else {
            if (depth == 0) {
                ok = 0;
                break;
            }
            frame_t f = stack[--depth];
            entry_t *c = f.call, *r = c->match;
            state = f.state;
            bits[c->id / 64] &= ~(1ull << (c->id % 64));
            r->prev->next = r;
            if (r->next)
                r->next->prev = r;
            c->prev->next = c;
            if (c->next)
                c->next->prev = c;
            e = c->next;
        }
    }
    for (size_t k = 0; k < cap; k++)
        free(cache[k].bits);
    free(cache);
    free(stack);
    free(bits);
    free(order);
    free(entries);
    return ok;
}

/* The checker refuses a get that returns a value no insert stored before
 * it, and accepts a get that overlaps the insert and finds nothing. */
static void checker_self_test(void) {
    op_t insert = {.call = 0, .ret = 10, .arg = 5, .kind = INSERT, .result = 1};
    op_t wrong = {.call = 20, .ret = 30, .out = 7, .kind = GET, .result = 1};
    op_t *refused[] = {&insert, &wrong};
    if (linearizable(refused, 2))
        fail("the checker accepted a get of a value never stored", 0, 0);
    op_t long_insert = {.call = 0, .ret = 30, .arg = 5, .kind = INSERT, .result = 1};
    op_t miss = {.call = 10, .ret = 20, .kind = GET, .result = 0};
    op_t *accepted[] = {&long_insert, &miss};
    if (!linearizable(accepted, 2))
        fail("the checker refused a get that overlaps the insert", 0, 0);
}

static void histories(unsigned rounds) {
    enum { OPS = 4000, FEW = 4, MANY = 256 };
    for (unsigned round = 0; round < rounds; round++) {
        /* Few keys on a map sized for them, where operations contend, and
         * many on a map created for one, where they cross table moves. */
        unsigned keys = round % 2 ? FEW : MANY;
        wf_cmap *map = wf_cmap_create(round % 2 ? FEW : 1);
        _Atomic int go = 0;
        pthread_t t[THREADS];
        history_t h[THREADS];
        for (unsigned i = 0; i < THREADS; i++) {
            h[i] = (history_t){map, calloc(OPS, sizeof(op_t)), OPS, i + round * THREADS, keys, &go};
            pthread_create(&t[i], NULL, record, &h[i]);
        }
        atomic_store(&go, 1);
        for (unsigned i = 0; i < THREADS; i++)
            pthread_join(t[i], NULL);
        op_t **per_key = malloc((size_t)THREADS * OPS * sizeof *per_key);
        for (unsigned k = 0; k < keys; k++) {
            unsigned n = 0;
            for (unsigned i = 0; i < THREADS; i++)
                for (unsigned j = 0; j < OPS; j++)
                    if (h[i].ops[j].key == (int)k)
                        per_key[n++] = &h[i].ops[j];
            if (!linearizable(per_key, n)) {
                /* The history, so that a failure can be read. */
                for (unsigned i = 0; i < n; i++)
                    printf("thread %u kind %d arg %llx result %d out %llx call %llu ret %llu\n", per_key[i]->thread,
                           per_key[i]->kind, (unsigned long long)per_key[i]->arg, per_key[i]->result,
                           (unsigned long long)per_key[i]->out, (unsigned long long)per_key[i]->call,
                           (unsigned long long)per_key[i]->ret);
                fail("a key's history is not linearizable (round, key)", round, k);
            }
        }
        free(per_key);
        for (unsigned i = 0; i < THREADS; i++)
            free(h[i].ops);
        wf_cmap_destroy(map);
    }
}

/* Two writers claim cells for two keys that start at the same cell, so the
 * second claims the cell after the first's. The second finishes before a
 * move begins and the first sees the move: the first's cell must stay in the
 * probe, or a read of the second key stops short of it in the table that is
 * still current. */
static void claim_given_back(void) {
    wf_cmap *map = wf_cmap_create(1);
    wf_cmap_user *first = wf_cmap_enter(map), *second = wf_cmap_enter(map);
    table *t = atomic_load(&map->current);
    uint64_t a = key_of(0), b = 0;
    for (uint64_t i = 1; b == 0; i++)
        if (start_of(t, key_of(i)) == start_of(t, a))
            b = key_of(i);
    cell *ca = NULL, *cb = NULL;
    if (acquire(t, a, 1, &ca) != CLAIMED || acquire(t, b, 1, &cb) != CLAIMED || cb == ca)
        fail("the two claims did not take two cells", 0, 0);
    if (!keep_cell(t, cb, CLAIMED, b))
        fail("a claim before any move was given back", 0, 0);
    atomic_store(&cb->value, 7);
    unlock(cb, b);
    count(second, 1, 1);
    start_move(map, t);
    if (keep_cell(t, ca, CLAIMED, a))
        fail("a claim kept its cell after a move began", 0, 0);
    uint64_t v = 0;
    if (!wf_cmap_get(first, b, &v) || v != 7)
        fail("a key claimed past a cell given back was lost", v, 0);
    /* An insert helps the move to its end; t may be freed after it, and with
     * locked reads the get above already moved it. */
    if (wf_cmap_insert(second, b, 7) != 0)
        fail("the move lost a key", 0, 0);
    if (!wf_cmap_get(first, b, &v) || v != 7 || wf_cmap_get(first, a, &v))
        fail("the move did not carry the keys as they were", v, 0);
    wf_cmap_leave(first);
    wf_cmap_leave(second);
    wf_cmap_destroy(map);
}

/* Entries. */

/* At rest, a map's used cells less those counted before its table became
 * current are exactly the cells of that table that are not empty: every
 * claim of an empty cell is counted once, whether it is kept or given back. */
static void check_cells(wf_cmap *map, const char *what) {
    table *t = atomic_load(&map->current);
    int64_t used, live, taken = 0;
    totals(map, &used, &live);
    for (uint64_t i = 0; i < t->capacity; i++)
        taken += atomic_load(&t->cells[i].key) != EMPTY;
    if (used - t->base != taken)
        fail(what, (uint64_t)(used - t->base), (uint64_t)taken);
}

/* A step another writer takes once, when the writer under test is about to
 * claim the cell at index; the claim tests below set it. */
static void (*at_claim)(struct table *t, unsigned long long index);

static void before_claim(struct table *t, unsigned long long index) {
    void (*step)(struct table *, unsigned long long) = at_claim;
    at_claim = NULL;
    if (step != NULL)
        step(t, index);
}

/* The same, when the writer under test is about to lock a cell of its key's
 * hash. */
static void (*at_lock)(struct cell *c);

static void before_lock(struct cell *c) {
    void (*step)(struct cell *) = at_lock;
    at_lock = NULL;
    if (step != NULL)
        step(c);
}

/* The other writer and the keys the claim tests use. */
static wf_cmap_user *other_user;
static const unsigned char *claim_key, *gone_key;
static uint64_t claim_length, gone_length;

/* As a writer of claim_key that read the cell after index empty before the
 * writer under test did: claims that cell and stores 7 there. */
static void claim_after(struct table *t, unsigned long long index) {
    wf_cmap *map = other_user->map;
    cell *c = &t->cells[(index + 1) & t->mask];
    uint64_t tag = tag_of(claim_key, claim_length), empty = EMPTY;
    if (!atomic_compare_exchange_strong(&c->key, &empty, tag | LOCKED))
        fail("the cell after the removed one was not empty", index, 0);
    node *n = new_node(other_user, node_bytes(map, claim_length));
    n->length = claim_length;
    memcpy(n->bytes, claim_key, (size_t)claim_length);
    memset(slot_of(map, n), 0, (size_t)map->slot_size);
    ((uint64_t *)slot_of(map, n))[0] = 7;
    atomic_store(&c->value, (uint64_t)(uintptr_t)n);
    count(other_user, 1, 1);
    unlock(c, tag);
}

/* As other writers that ran after the writer under test passed gone_key's
 * cell: removes gone_key, and then stores 7 under claim_key, which reuses
 * that cell, behind the one the writer under test is about to claim. */
static void insert_behind(struct table *t, unsigned long long index) {
    (void)t;
    (void)index;
    wf_cmap_entry entry;
    wf_cmap_lock_entry(other_user, gone_key, gone_length, 0, &entry);
    wf_cmap_unlock_entry(other_user, &entry, 0, 0);
    uint64_t *slot = wf_cmap_lock_entry(other_user, claim_key, claim_length, 0, &entry);
    if (!entry.fresh)
        fail("the other writer found the key before inserting it", 0, 0);
    slot[0] = 7;
    wf_cmap_unlock_entry(other_user, &entry, 0, 1);
}

/* Bytes of the first counted key after skip, other than k, that starts
 * where k does in t. */
static uint64_t key_beside(table *t, const unsigned char *k, uint64_t k_length, uint64_t *skip,
                           unsigned char *bytes) {
    for (;;) {
        uint64_t length = counted_key(++*skip, bytes);
        if (start_of(t, tag_of(bytes, length)) == start_of(t, tag_of(k, k_length)))
            return length;
    }
}

/* Key k claims a removed cell while a second writer of k claims the empty
 * cell after it, which the first read as empty before the second claimed it:
 * the first must find the second's cell and give its own back, or k holds
 * two cells. */
static void claim_ahead(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    table *t = atomic_load(&map->current);
    unsigned char k[16], other[16];
    uint64_t k_length = counted_key(0, k), skip = 0;
    uint64_t other_length = key_beside(t, k, k_length, &skip, other);
    wf_cmap_entry entry;
    wf_cmap_lock_entry(first, other, other_length, 0, &entry);
    wf_cmap_unlock_entry(first, &entry, 0, 0);
    other_user = wf_cmap_user_at(map, 1);
    claim_key = k;
    claim_length = k_length;
    at_claim = claim_after;
    uint64_t *slot = wf_cmap_lock_entry(first, k, k_length, 0, &entry);
    if (at_claim != NULL)
        fail("the key did not claim the removed cell", 0, 0);
    if (entry.fresh || slot[0] != 7)
        fail("a key claimed a removed cell beside its own claimed cell", entry.fresh, slot[0]);
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    if (wf_cmap_count(map) != 1)
        fail("the key counted twice", wf_cmap_count(map), 1);
    wf_cmap_destroy(map);
}

/* Key k is about to claim a cell, a removed one when spare is set and the
 * empty one after the removed cells otherwise, when other writers remove
 * the key whose cell its probe passed before it and then insert k there:
 * the first must find k behind its own claim and give its claim back, or k
 * holds two cells. */
static void claim_behind(int spare) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    table *t = atomic_load(&map->current);
    unsigned char k[16], gone[16], removed[16];
    uint64_t k_length = counted_key(0, k), skip = 0;
    uint64_t gone_bytes = key_beside(t, k, k_length, &skip, gone);
    wf_cmap_entry entry;
    wf_cmap_lock_entry(first, gone, gone_bytes, 0, &entry);
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    if (spare) {
        uint64_t removed_length = key_beside(t, k, k_length, &skip, removed);
        wf_cmap_lock_entry(first, removed, removed_length, 0, &entry);
        wf_cmap_unlock_entry(first, &entry, 0, 0);
    }
    other_user = wf_cmap_user_at(map, 1);
    claim_key = k;
    claim_length = k_length;
    gone_key = gone;
    gone_length = gone_bytes;
    at_claim = insert_behind;
    uint64_t *slot = wf_cmap_lock_entry(first, k, k_length, 0, &entry);
    if (at_claim != NULL)
        fail("the key never came to claim a cell", (uint64_t)spare, 0);
    if (entry.fresh || slot[0] != 7)
        fail("a key claimed a cell after its own cell behind (spare, fresh)", (uint64_t)spare, entry.fresh);
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    if (wf_cmap_count(map) != 1)
        fail("the key counted twice (spare)", (uint64_t)spare, wf_cmap_count(map));
    /* Two cells taken either way: the passed key's and the removed key's, or
     * the passed key's and the empty one claimed and given back. */
    int64_t used, live;
    totals(map, &used, &live);
    if (used != 2)
        fail("taken cells miscounted (spare, used)", (uint64_t)spare, (uint64_t)used);
    uint64_t drained = 0;
    for (uint64_t *left; (left = wf_cmap_drain(map)) != NULL;)
        drained++;
    if (drained != 1)
        fail("the key holds two cells (spare, cells)", (uint64_t)spare, drained);
    wf_cmap_destroy(map);
}

/* settle_claim on cells set by hand to two pending claims of one hash: the
 * claim ahead gives way to the one behind, and the one behind waits until
 * the claim ahead is given back and then settles. */
static void *give_back_later(void *arg) {
    cell *c = arg;
    struct timespec pause = {0, 2000000};
    nanosleep(&pause, NULL);
    atomic_store(&c->key, REMOVED);
    return NULL;
}

static void settle_pending(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *u = wf_cmap_user_at(map, 0);
    u->patience = UINT64_MAX;
    table *t = atomic_load(&map->current);
    unsigned char k[16];
    uint64_t length = counted_key(0, k), tag = tag_of(k, length);
    uint64_t at = start_of(t, tag), next = (at + 1) & t->mask;
    cell *behind = &t->cells[at], *ahead = &t->cells[next], *out = NULL;
    atomic_store(&behind->key, tag | LOCKED | PENDING);
    atomic_store(&ahead->key, tag | LOCKED | PENDING);
    if (settle_claim(u, t, at, next, tag, k, length, &out) != YIELDED || atomic_load(&ahead->key) != REMOVED)
        fail("a claim ahead of a pending claim of its hash did not give way", 0, 0);
    atomic_store(&ahead->key, tag | LOCKED | PENDING);
    pthread_t thread;
    pthread_create(&thread, NULL, give_back_later, ahead);
    int r = settle_claim(u, t, at, at, tag, k, length, &out);
    uint64_t seen = atomic_load(&ahead->key);
    pthread_join(thread, NULL);
    if (r != SETTLED || out != behind || seen != REMOVED || atomic_load(&behind->key) != (tag | LOCKED))
        fail("a claim behind a pending claim did not wait it out and settle", (uint64_t)r, seen);
    wf_cmap_destroy(map);
}

/* As another writer of claim_key that claimed the cell of the key the
 * writer under test passed, removed meanwhile, and gives its claim back a
 * little later: marks that cell pending for claim_key. */
static pthread_t pending_thread;

static void claim_pending_behind(struct table *t, unsigned long long index) {
    (void)index;
    uint64_t tag = tag_of(claim_key, claim_length);
    cell *c = &t->cells[start_of(t, tag)];
    atomic_store(&c->key, tag | LOCKED | PENDING);
    pthread_create(&pending_thread, NULL, give_back_later, c);
}

/* Key k claims the empty cell after a live key's cell that another writer of
 * k has meanwhile claimed, pending: k gives its empty cell back, counted as
 * taken, and claims again once the other claim is given back. */
static void claim_yields(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    table *t = atomic_load(&map->current);
    unsigned char k[16], gone[16];
    uint64_t k_length = counted_key(0, k), skip = 0;
    uint64_t gone_bytes = key_beside(t, k, k_length, &skip, gone);
    wf_cmap_entry entry;
    wf_cmap_lock_entry(first, gone, gone_bytes, 0, &entry);
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    claim_key = k;
    claim_length = k_length;
    at_claim = claim_pending_behind;
    uint64_t *slot = wf_cmap_lock_entry(first, k, k_length, 0, &entry);
    pthread_join(pending_thread, NULL);
    if (at_claim != NULL || !entry.fresh)
        fail("the key did not claim again after giving way", at_claim == NULL, entry.fresh);
    slot[0] = 1;
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    check_cells(map, "an empty cell claimed and given back went uncounted (counted, taken)");
    wf_cmap_destroy(map);
}

/* As another writer of claim_key that reused the cell of the key the writer
 * under test passed, removed meanwhile, and holds it a little longer: k's
 * cell, holding 7, locked there until a thread lets it go. */
static pthread_t holding_thread;

static void *let_go_later(void *arg) {
    cell *c = arg;
    struct timespec pause = {0, 2000000};
    nanosleep(&pause, NULL);
    atomic_store(&c->key, atomic_load(&c->key) & ~LOCKED);
    return NULL;
}

static void hold_behind(struct table *t, unsigned long long index) {
    (void)index;
    wf_cmap *map = other_user->map;
    uint64_t tag = tag_of(claim_key, claim_length);
    cell *c = &t->cells[start_of(t, tag)];
    node *n = new_node(other_user, node_bytes(map, claim_length));
    n->length = claim_length;
    memcpy(n->bytes, claim_key, (size_t)claim_length);
    memset(slot_of(map, n), 0, (size_t)map->slot_size);
    ((uint64_t *)slot_of(map, n))[0] = 7;
    atomic_store(&c->value, (uint64_t)(uintptr_t)n);
    atomic_store(&c->key, tag | LOCKED);
    pthread_create(&holding_thread, NULL, let_go_later, c);
}

/* Key k, with no patience, claims the empty cell after a live key's cell
 * that another writer of k has meanwhile reused and holds: k gives its claim
 * back, counted as taken, holds the whole map, and then finds the other
 * writer's cell once it is let go. */
static void claim_impatient(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    table *t = atomic_load(&map->current);
    unsigned char k[16], gone[16];
    uint64_t k_length = counted_key(0, k), skip = 0;
    uint64_t gone_bytes = key_beside(t, k, k_length, &skip, gone);
    wf_cmap_entry entry;
    wf_cmap_lock_entry(first, gone, gone_bytes, 0, &entry);
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    other_user = wf_cmap_user_at(map, 1);
    claim_key = k;
    claim_length = k_length;
    at_claim = hold_behind;
    set_patience(0, PATIENCE);
    uint64_t *slot = wf_cmap_lock_entry(first, k, k_length, 0, &entry);
    pthread_join(holding_thread, NULL);
    set_patience(PATIENCE, PATIENCE);
    cell *claimed = &t->cells[(start_of(t, tag_of(k, k_length)) + 1) & t->mask];
    if (at_claim != NULL || !entry.upgraded || entry.fresh || slot[0] != 7)
        fail("an impatient claim did not hold the map and find its key (upgraded, fresh)", entry.upgraded,
             entry.fresh);
    if (atomic_load(&claimed->key) != REMOVED)
        fail("an impatient claim kept its cell", atomic_load(&claimed->key), 0);
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    if (atomic_load(&map->gate) != 0 || atomic_load(&map->hold_serving) != atomic_load(&map->hold_next))
        fail("an impatient statement left the map held (gate, turns behind)", (uint64_t)atomic_load(&map->gate),
             atomic_load(&map->hold_next) - atomic_load(&map->hold_serving));
    check_cells(map, "an impatient claim's cell went uncounted (counted, taken)");
    wf_cmap_destroy(map);
}

/* As another writer that begins a move just after a reader has found its
 * entry and before the reader looks for one. */
static wf_cmap *read_found_map;

static void read_found(struct table *t) {
    wf_cmap *map = read_found_map;
    read_found_map = NULL;
    if (map != NULL)
        start_move(map, t);
}

/* A read that found its entry in a table a move has begun leaves it and
 * reads the entry in the next table, where writers then work. */
static void reads_follow_moves(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *u = wf_cmap_user_at(map, 0);
    unsigned char k[16];
    uint64_t k_length = counted_key(3, k);
    wf_cmap_entry entry;
    uint64_t *slot = wf_cmap_lock_entry(u, k, k_length, 0, &entry);
    slot[0] = 41;
    wf_cmap_unlock_entry(u, &entry, 0, 1);
    table *first = atomic_load(&map->current);
    read_found_map = map;
    const uint64_t *read = wf_cmap_read_entry(u, k, k_length, 0, &entry);
    if (read_found_map != NULL || read == NULL || read[0] != 41)
        fail("a read across a move lost its entry (found, value)", read != NULL, read != NULL ? read[0] : 0);
    if (entry.table == first || entry.table != atomic_load(&map->current))
        fail("a read kept its entry in a table a move had begun (moved, current)", first != atomic_load(&map->current),
             entry.table == atomic_load(&map->current));
    wf_cmap_unread_entry(u, &entry, 0);
    wf_cmap_destroy(map);
}

/* A move waits for the reads under way in the table it moves: while one user
 * reads an entry, another's move does not finish, and it finishes once the
 * read ends. */
static wf_cmap *move_map;
static _Atomic int move_done;

static void *move_now(void *arg) {
    (void)arg;
    wf_cmap_user *u = wf_cmap_user_at(move_map, 1);
    table *t = use_current(u);
    start_move(move_map, t);
    finish_move(move_map, t);
    atomic_store(&move_done, 1);
    return NULL;
}

static void reads_block_moves(void) {
    move_map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *u = wf_cmap_user_at(move_map, 0);
    unsigned char k[16];
    uint64_t k_length = counted_key(5, k);
    wf_cmap_entry entry;
    uint64_t *slot = wf_cmap_lock_entry(u, k, k_length, 0, &entry);
    slot[0] = 7;
    wf_cmap_unlock_entry(u, &entry, 0, 1);
    table *first = atomic_load(&move_map->current);
    const uint64_t *read = wf_cmap_read_entry(u, k, k_length, 0, &entry);
    atomic_store(&move_done, 0);
    pthread_t mover;
    pthread_create(&mover, NULL, move_now, NULL);
    struct timespec pause = {0, 50000000};
    nanosleep(&pause, NULL);
    if (atomic_load(&move_done) || atomic_load(&move_map->current) != first)
        fail("a move finished while a read of its table was under way (done, value)", atomic_load(&move_done), read[0]);
    wf_cmap_unread_entry(u, &entry, 0);
    pthread_join(mover, NULL);
    if (atomic_load(&move_map->current) == first)
        fail("a move did not finish once the read ended", 0, 0);
    read = wf_cmap_read_entry(u, k, k_length, 0, &entry);
    if (read == NULL || read[0] != 7)
        fail("a moved entry was lost (found, value)", read != NULL, read != NULL ? read[0] : 0);
    wf_cmap_unread_entry(u, &entry, 0);
    wf_cmap_destroy(move_map);
}

/* The retries a keyed statement makes without waiting for a held cell. */
enum { LOST_EMPTY_CLAIM, CLAIM_MOVED, LOST_REMOVED_CLAIM, LOST_LOCK };

/* As another writer that, just before the writer under test claims the
 * empty cell at index, either makes that cell removed, so the claim's
 * compare-and-swap loses, or begins a move, so the claim is given back. */
static void lose_claim(struct table *t, unsigned long long index) {
    atomic_store(&t->cells[index].key, REMOVED);
}

static void move_under_claim(struct table *t, unsigned long long index) {
    (void)index;
    start_move(other_user->map, t);
}

/* As the writer of gone_key that, just before the writer under test claims
 * the removed cell at index, gone_key's own, reuses it for gone_key. */
static void reuse_removed(struct table *t, unsigned long long index) {
    wf_cmap *map = other_user->map;
    node *n = new_node(other_user, node_bytes(map, gone_length));
    n->length = gone_length;
    memcpy(n->bytes, gone_key, (size_t)gone_length);
    memset(slot_of(map, n), 0, (size_t)map->slot_size);
    atomic_store(&t->cells[index].value, (uint64_t)(uintptr_t)n);
    atomic_store(&t->cells[index].key, tag_of(gone_key, gone_length));
}

/* As another writer that removes the key in c just before the writer under
 * test locks it, so the lock's compare-and-swap loses. */
static void remove_under_lock(struct cell *c) { atomic_store(&c->key, REMOVED); }

/* A statement with no patience that never waits for a held cell still runs
 * out of it when it must try again: after a lost claim of an empty or a
 * removed cell, after a claim given back to a move, and after a lost lock of
 * its key's cell. Each retry makes it hold the whole map. */
static void retries_count(int how) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    table *t = atomic_load(&map->current);
    other_user = wf_cmap_user_at(map, 1);
    unsigned char k[16], gone[16];
    uint64_t k_length = counted_key(0, k), skip = 0;
    wf_cmap_entry entry;
    uint64_t *slot;
    if (how == LOST_REMOVED_CLAIM) {
        uint64_t gone_bytes = key_beside(t, k, k_length, &skip, gone);
        wf_cmap_lock_entry(first, gone, gone_bytes, 0, &entry);
        wf_cmap_unlock_entry(first, &entry, 0, 0);
        gone_key = gone;
        gone_length = gone_bytes;
    } else if (how == LOST_LOCK) {
        slot = wf_cmap_lock_entry(first, k, k_length, 0, &entry);
        slot[0] = 5;
        wf_cmap_unlock_entry(first, &entry, 0, 1);
    }
    at_claim = how == LOST_EMPTY_CLAIM ? lose_claim
               : how == CLAIM_MOVED    ? move_under_claim
               : how == LOST_REMOVED_CLAIM ? reuse_removed
                                           : NULL;
    at_lock = how == LOST_LOCK ? remove_under_lock : NULL;
    set_patience(0, PATIENCE);
    slot = wf_cmap_lock_entry(first, k, k_length, 0, &entry);
    set_patience(PATIENCE, PATIENCE);
    if (at_claim != NULL || at_lock != NULL || !entry.upgraded || !entry.fresh)
        fail("a retry did not count against patience (retry, upgraded)", (uint64_t)how, entry.upgraded);
    slot[0] = 1;
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    slot = wf_cmap_lock_entry(first, k, k_length, 0, &entry);
    if (entry.fresh || slot[0] != 1)
        fail("the key was lost after the retry (retry, fresh)", (uint64_t)how, entry.fresh);
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    wf_cmap_destroy(map);
}

enum { ENTRY_KEYS = 3000, ENTRY_KEY_BYTES = 600 };

/* Key k's bytes: k's own eight bytes, so that keys differ, then up to 55
 * bytes drawn from k, or for every 97th key about 500, so that its node is
 * larger than the chunks' largest grain. */
static uint64_t entry_key(uint64_t k, unsigned char *bytes) {
    uint64_t length = 8 + mix64(k * 7 + 1) % 56;
    if (k % 97 == 0)
        length = 500 + k % 100;
    memcpy(bytes, &k, 8);
    for (uint64_t i = 8; i < length; i++)
        bytes[i] = (unsigned char)(mix64(k ^ (i << 32)) >> 56);
    return length;
}

static void entries_sequential(void) {
    enum { OPS = 400000 };
    static uint8_t present[ENTRY_KEYS];
    static uint64_t value[ENTRY_KEYS];
    unsigned char bytes[ENTRY_KEY_BYTES];
    wf_cmap *map = wf_cmap_create_entries(16, 8, 0);
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    uint64_t state = 11, live = 0;
    for (unsigned i = 0; i < OPS; i++) {
        uint64_t r = next(&state);
        unsigned k = (unsigned)((r >> 8) % ENTRY_KEYS);
        uint64_t length = entry_key(k, bytes);
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, bytes, length, 0, &entry);
        if (entry.fresh != !present[k])
            fail("an entry's freshness disagrees with the reference", k, entry.fresh);
        if (entry.fresh && (slot[0] != 0 || slot[1] != 0))
            fail("a fresh entry's slot is not zero", k, slot[0]);
        if (present[k] && slot[0] != value[k])
            fail("an entry's value disagrees with the reference", k, slot[0]);
        int keep = (r & 3) != 0;
        if (keep) {
            slot[0] = r;
            value[k] = r;
        }
        live += (uint64_t)keep - (uint64_t)present[k];
        present[k] = (uint8_t)keep;
        wf_cmap_unlock_entry(user, &entry, 0, keep);
    }
    if (wf_cmap_count(map) != live)
        fail("the count of entries disagrees with the reference", wf_cmap_count(map), live);
    check_cells(map, "claims miscounted the cells they took (counted, taken)");
    uint64_t drained = 0;
    for (uint64_t *slot; (slot = wf_cmap_drain(map)) != NULL;)
        drained++;
    if (drained != live)
        fail("the drain did not hand out every entry once", drained, live);
    wf_cmap_destroy(map);
}

enum { COUNTED_KEYS = 64, COUNTING = 100000 };
/* Statements each thread of a counting or churning test runs: a quarter as
 * many where every statement that waits holds the map, which still holds it
 * thousands of times. */
static uint64_t statements;

typedef struct {
    wf_cmap *map;
    unsigned index;
    _Atomic uint64_t *total;
    _Atomic int *stop;
    uint64_t holds;
} counter_t;

static uint64_t counted_key(uint64_t k, unsigned char *bytes) {
    memcpy(bytes, "key:", 4);
    for (int i = 0; i < 8; i++)
        bytes[4 + i] = (unsigned char)('0' + (k >> (3 * i)) % 8);
    return 12;
}

static void *count_entries(void *arg) {
    counter_t *c = arg;
    wf_cmap_user *user = wf_cmap_user_at(c->map, c->index);
    unsigned char bytes[16];
    uint64_t state = mix64(c->index + 99);
    for (uint64_t i = 0; i < statements; i++) {
        uint64_t k = next(&state) % COUNTED_KEYS;
        uint64_t length = counted_key(k, bytes);
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, bytes, length, 0, &entry);
        if (entry.fresh)
            slot[0] = 0;
        /* The total moves first, the entry a little later, both inside the
         * statement. */
        atomic_fetch_add(c->total, 1);
        for (volatile int spin = 0; spin < 20; spin++) {
        }
        slot[0] += 1;
        wf_cmap_unlock_entry(user, &entry, 0, 1);
    }
    return NULL;
}

static void *hold_entries(void *arg) {
    counter_t *c = arg;
    wf_cmap_user *user = wf_cmap_user_at(c->map, c->index);
    unsigned char bytes[16];
    while (!atomic_load(c->stop)) {
        wf_cmap_hold(user);
        uint64_t sum = 0, total = atomic_load(c->total);
        for (uint64_t k = 0; k < COUNTED_KEYS; k++) {
            uint64_t length = counted_key(k, bytes);
            wf_cmap_entry entry;
            uint64_t *slot = wf_cmap_lock_entry(user, bytes, length, 1, &entry);
            if (!entry.fresh)
                sum += slot[0];
            wf_cmap_unlock_entry(user, &entry, 1, !entry.fresh);
        }
        if (sum != total || wf_cmap_count(c->map) > COUNTED_KEYS)
            fail("a hold saw a keyed statement half done (sum, total)", sum, total);
        wf_cmap_unhold(user);
        c->holds++;
    }
    return NULL;
}

/* With capacity 1 and no patience, the map moves while the counted keys
 * arrive, and every statement that waits holds the whole map. */
static void entries_held(uint64_t capacity, uint64_t patient) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, capacity);
    set_patience(patient, patient);
    statements = patient == 0 ? COUNTING / 4 : COUNTING;
    _Atomic uint64_t total = 0;
    _Atomic int stop = 0;
    pthread_t t[THREADS + 1];
    counter_t c[THREADS + 1];
    for (unsigned i = 0; i <= THREADS; i++) {
        c[i] = (counter_t){map, i, &total, &stop, 0};
        pthread_create(&t[i], NULL, i == THREADS ? hold_entries : count_entries, &c[i]);
    }
    for (unsigned i = 0; i < THREADS; i++)
        pthread_join(t[i], NULL);
    atomic_store(&stop, 1);
    pthread_join(t[THREADS], NULL);
    if (c[THREADS].holds == 0)
        fail("the holder never held the map", 0, 0);
    uint64_t sum = 0;
    for (uint64_t *slot; (slot = wf_cmap_drain(map)) != NULL;)
        sum += slot[0];
    if (sum != THREADS * statements)
        fail("a keyed statement's count was lost (sum, patience)", sum, patient);
    set_patience(PATIENCE, PATIENCE);
    wf_cmap_destroy(map);
}

/* Statements that find their key absent and leave it absent take no cell
 * for good: the same key missed many times keeps the table it started in. */
static void entries_misses(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    table *first = atomic_load(&map->current);
    unsigned char bytes[16];
    for (uint64_t round = 0; round < 20000; round++) {
        uint64_t length = counted_key(round % 3, bytes);
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, bytes, length, 0, &entry);
        if (slot[0] != 0)
            fail("an absent key's slot was not empty", round, slot[0]);
        wf_cmap_unlock_entry(user, &entry, 0, 0);
    }
    int64_t used, live;
    totals(map, &used, &live);
    if (atomic_load(&map->current) != first || used > 3 || live != 0)
        fail("misses of three keys took cells for good (used, live)", (uint64_t)used, (uint64_t)live);
    wf_cmap_destroy(map);
}

/* Statements on a few keys that remove them as often as they keep them,
 * from several threads: no two statements hold one key at once, and no
 * increment is lost. Crowded, the keys all start at one cell, so that their
 * claims of removed and empty cells race in one run of cells. */
enum { CHURN_KEYS = 6, CHURN_OPS = 200000 };
static unsigned char churn_bytes[CHURN_KEYS][16];
static uint64_t churn_lengths[CHURN_KEYS];
static _Atomic int churn_holding[CHURN_KEYS];
static _Atomic uint64_t churn_removed;

static void *churn_entries(void *arg) {
    counter_t *c = arg;
    wf_cmap_user *user = wf_cmap_user_at(c->map, c->index);
    uint64_t state = 0x9e3779b97f4a7c15ull * (c->index + 1);
    for (uint64_t i = 0; i < statements; i++) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        uint64_t k = state % CHURN_KEYS;
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, churn_bytes[k], churn_lengths[k], 0, &entry);
        if (atomic_exchange(&churn_holding[k], 1))
            fail("two statements held one key at once", k, i);
        uint64_t next = slot[0] + 1;
        atomic_store(&churn_holding[k], 0);
        if ((state >> 20) % 2 == 0) {
            atomic_fetch_add(&churn_removed, next);
            wf_cmap_unlock_entry(user, &entry, 0, 0);
        } else {
            slot[0] = next;
            wf_cmap_unlock_entry(user, &entry, 0, 1);
        }
    }
    return NULL;
}

static void entries_churn(int crowded, uint64_t patient) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    set_patience(patient, patient);
    statements = patient == 0 ? CHURN_OPS / 4 : CHURN_OPS;
    table *t = atomic_load(&map->current);
    uint64_t skip = 0;
    churn_lengths[0] = counted_key(0, churn_bytes[0]);
    for (uint64_t k = 1; k < CHURN_KEYS; k++)
        churn_lengths[k] = crowded ? key_beside(t, churn_bytes[0], churn_lengths[0], &skip, churn_bytes[k])
                                   : counted_key(k, churn_bytes[k]);
    atomic_store(&churn_removed, 0);
    pthread_t threads[THREADS];
    counter_t c[THREADS];
    for (unsigned i = 0; i < THREADS; i++) {
        c[i] = (counter_t){map, i, NULL, NULL, 0};
        pthread_create(&threads[i], NULL, churn_entries, &c[i]);
    }
    for (unsigned i = 0; i < THREADS; i++)
        pthread_join(threads[i], NULL);
    if (wf_cmap_count(map) > CHURN_KEYS)
        fail("churned keys counted more than once (crowded, count)", (uint64_t)crowded, wf_cmap_count(map));
    check_cells(map, "churned claims miscounted the cells they took (counted, taken)");
    uint64_t sum = atomic_load(&churn_removed), cells = 0;
    for (uint64_t *slot; (slot = wf_cmap_drain(map)) != NULL; cells++)
        sum += slot[0];
    if (cells > CHURN_KEYS)
        fail("a churned key holds two cells (crowded, cells)", (uint64_t)crowded, cells);
    if (sum != THREADS * statements)
        fail("a churned key's increment was lost (sum, patience)", sum, patient);
    set_patience(PATIENCE, PATIENCE);
    wf_cmap_destroy(map);
}

/* Statements on one key whose blocks only read it beside statements that
 * write it: two threads write all eight words of the slot to one new value
 * and count it, two read the slot and check that its words agree and never
 * fall, and every statement marks itself inside, so that a reader and a
 * writer inside at once fail the test. With moves, the writers also insert
 * keys of their own, so the map moves under the readers, and readers also
 * read keys that are absent. */
enum { READ_WORDS = 8, READ_OPS = 100000 };
static unsigned char read_key[16];
static uint64_t read_length;
static _Atomic int read_writing, read_readers, read_most, read_done;
static _Atomic uint64_t read_writes;

static void *write_shared(void *arg) {
    counter_t *c = arg;
    wf_cmap_user *user = wf_cmap_user_at(c->map, c->index);
    unsigned char bytes[16];
    for (uint64_t i = 0; i < statements; i++) {
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, read_key, read_length, 0, &entry);
        if (atomic_exchange(&read_writing, 1) || atomic_load(&read_readers) != 0)
            fail("a writer of a key ran beside its readers (writer, readers)", 1, atomic_load(&read_readers));
        uint64_t next = slot[0] + 1;
        for (int w = 0; w < READ_WORDS; w++)
            slot[w] = next;
        atomic_store(&read_writing, 0);
        atomic_fetch_add(&read_writes, 1);
        wf_cmap_unlock_entry(user, &entry, 0, 1);
        if (c->total != NULL && i % 8 == 0) {
            uint64_t length = counted_key(1000 + c->index * statements + i, bytes);
            uint64_t *other = wf_cmap_lock_entry(user, bytes, length, 0, &entry);
            other[0] = i;
            wf_cmap_unlock_entry(user, &entry, 0, 1);
        }
    }
    return NULL;
}

static void *read_shared(void *arg) {
    counter_t *c = arg;
    wf_cmap_user *user = wf_cmap_user_at(c->map, c->index);
    unsigned char absent[16];
    uint64_t absent_length = counted_key(999, absent), last = 0;
    while (!atomic_load(&read_done)) {
        wf_cmap_entry entry;
        const uint64_t *slot = wf_cmap_read_entry(user, read_key, read_length, 0, &entry);
        int inside = atomic_fetch_add(&read_readers, 1) + 1;
        if (atomic_load(&read_writing))
            fail("a reader of a key ran beside its writer (readers, writing)", (uint64_t)inside, 1);
        int most = atomic_load(&read_most);
        while (inside > most && !atomic_compare_exchange_weak(&read_most, &most, inside)) {
        }
        uint64_t first = slot[0];
        for (volatile int spin = 0; spin < 50; spin++) {
        }
        for (int w = 1; w < READ_WORDS; w++)
            if (slot[w] != first)
                fail("a reader saw a torn value (first, word)", first, slot[w]);
        if (first < last)
            fail("a reader saw a key's value fall (last, now)", last, first);
        last = first;
        atomic_fetch_sub(&read_readers, 1);
        wf_cmap_unread_entry(user, &entry, 0);
        const uint64_t *none = wf_cmap_read_entry(user, absent, absent_length, 0, &entry);
        if (none == NULL || entry.cell != NULL)
            fail("a reader found a key never written (slot, cell)", none != NULL, entry.cell != NULL);
        for (int w = 0; w < READ_WORDS; w++)
            if (none[w] != 0)
                fail("an absent key's slot did not read None (word, value)", (uint64_t)w, none[w]);
        wf_cmap_unread_entry(user, &entry, 0);
        c->holds++;
    }
    return NULL;
}

static void entries_shared_reads(int moves) {
    wf_cmap *map = wf_cmap_create_entries(READ_WORDS * 8, 8, moves ? 1 : 0);
    read_length = counted_key(7, read_key);
    statements = READ_OPS;
    atomic_store(&read_writes, 0);
    atomic_store(&read_most, 0);
    atomic_store(&read_done, 0);
    wf_cmap_entry entry;
    uint64_t *slot = wf_cmap_lock_entry(wf_cmap_user_at(map, 0), read_key, read_length, 0, &entry);
    for (int w = 0; w < READ_WORDS; w++)
        slot[w] = 0;
    wf_cmap_unlock_entry(wf_cmap_user_at(map, 0), &entry, 0, 1);
    _Atomic uint64_t marker = 0;
    pthread_t t[THREADS];
    counter_t c[THREADS];
    for (unsigned i = 0; i < THREADS; i++) {
        c[i] = (counter_t){map, i, moves ? &marker : NULL, NULL, 0};
        pthread_create(&t[i], NULL, i < 2 ? write_shared : read_shared, &c[i]);
    }
    for (unsigned i = 0; i < 2; i++)
        pthread_join(t[i], NULL);
    atomic_store(&read_done, 1);
    for (unsigned i = 2; i < THREADS; i++)
        pthread_join(t[i], NULL);
    slot = wf_cmap_lock_entry(wf_cmap_user_at(map, 0), read_key, read_length, 0, &entry);
    if (slot[0] != atomic_load(&read_writes))
        fail("a write to a read key was lost (value, writes)", slot[0], atomic_load(&read_writes));
    wf_cmap_unlock_entry(wf_cmap_user_at(map, 0), &entry, 0, 1);
    if (c[2].holds + c[3].holds == 0)
        fail("no reader read the key", moves, 0);
    if (getenv("CMAP_SHARED_VERBOSE"))
        printf("shared reads (moves %d): %llu reads, at most %d readers at once\n", moves,
               (unsigned long long)(c[2].holds + c[3].holds), atomic_load(&read_most));
    wf_cmap_destroy(map);
}

/* Statements on one key from every thread at once, where only the first
 * thread's run out of patience: each of those that holds the map is
 * overtaken, once it has closed the gate, by at most the one statement of
 * each other thread already under way. Another user holds the key until the
 * first thread's first statement has closed its gate, so at least that one
 * holds the map whatever the host runs in parallel. */
enum { HOT_OPS = 2000 };
static _Atomic uint64_t hot_clock;
static _Atomic int hot_done;
static unsigned char hot_key[16];
static uint64_t hot_length, hot_upgraded, hot_worst;

static void *hot_statements(void *arg) {
    counter_t *c = arg;
    wf_cmap_user *user = wf_cmap_user_at(c->map, c->index);
    uint64_t upgraded = 0, worst = 0;
    for (uint64_t i = 0; c->index == 0 ? i < HOT_OPS : !atomic_load(&hot_done); i++) {
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, hot_key, hot_length, 0, &entry);
        uint64_t now = atomic_fetch_add(&hot_clock, 1);
        if (entry.upgraded) {
            upgraded++;
            if (now - closed_at[c->index] > worst)
                worst = now - closed_at[c->index];
        }
        for (volatile int spin = 0; spin < 50; spin++) {
        }
        slot[0] += 1;
        wf_cmap_unlock_entry(user, &entry, 0, 1);
    }
    if (c->index == 0) {
        hot_upgraded = upgraded;
        hot_worst = worst;
        atomic_store(&hot_done, 1);
    }
    return NULL;
}

static void entries_bounded(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    hot_length = counted_key(1, hot_key);
    atomic_store(&hot_clock, 0);
    atomic_store(&hot_done, 0);
    atomic_store(&closed_seen[0], 0);
    set_patience(0, UINT64_MAX);
    hold_clock = &hot_clock;
    wf_cmap_user *first = wf_cmap_user_at(map, THREADS);
    wf_cmap_entry held;
    wf_cmap_lock_entry(first, hot_key, hot_length, 0, &held);
    pthread_t threads[THREADS];
    counter_t c[THREADS];
    for (unsigned i = 0; i < THREADS; i++) {
        c[i] = (counter_t){map, i, NULL, NULL, 0};
        pthread_create(&threads[i], NULL, hot_statements, &c[i]);
    }
    struct timespec pause = {0, 1000000};
    for (unsigned waited = 0; !atomic_load(&closed_seen[0]); waited++) {
        if (waited == 10000)
            fail("a statement out of patience never held the map", 0, 0);
        nanosleep(&pause, NULL);
    }
    wf_cmap_unlock_entry(first, &held, 0, 1);
    for (unsigned i = 0; i < THREADS; i++)
        pthread_join(threads[i], NULL);
    hold_clock = NULL;
    set_patience(PATIENCE, PATIENCE);
    if (hot_upgraded == 0)
        fail("a statement out of patience never held the map", 0, 0);
    if (hot_worst > THREADS - 1)
        fail("a statement holding the map was overtaken past the bound (overtaken, bound)", hot_worst,
             THREADS - 1);
    uint64_t *slot = wf_cmap_drain(map);
    if (slot == NULL || slot[0] != atomic_load(&hot_clock))
        fail("a statement on the one key was lost (count, statements)", slot ? slot[0] : 0, atomic_load(&hot_clock));
    wf_cmap_destroy(map);
}

/* A statement over the whole map whose turn has come waits for the keyed
 * statements the hold before it kept waiting to begin: with one counted and
 * none beginning, it does not close the gate until the count drops. */
static _Atomic int held_once;

static void *hold_once(void *arg) {
    wf_cmap_user *user = arg;
    wf_cmap_hold(user);
    atomic_store(&held_once, 1);
    wf_cmap_unhold(user);
    return NULL;
}

static void holds_wait_for_counted(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    atomic_store(&held_once, 0);
    atomic_fetch_add(&map->waiting, 1);
    pthread_t thread;
    pthread_create(&thread, NULL, hold_once, wf_cmap_user_at(map, 0));
    struct timespec pause = {0, 5000000};
    nanosleep(&pause, NULL);
    if (atomic_load(&held_once) || atomic_load(&map->gate))
        fail("a hold closed the gate before a counted keyed statement began (held, gate)",
             (uint64_t)atomic_load(&held_once), (uint64_t)atomic_load(&map->gate));
    atomic_fetch_sub(&map->waiting, 1);
    pthread_join(thread, NULL);
    if (!atomic_load(&held_once) || atomic_load(&map->gate))
        fail("a hold did not follow the counted statement (held, gate)", (uint64_t)atomic_load(&held_once),
             (uint64_t)atomic_load(&map->gate));
    wf_cmap_destroy(map);
}

/* Two threads holding the whole map again and again: once the second has
 * taken its place in line, the first holds the map at most once before the
 * second does. */
enum { TURN_HOLDS = 2000 };
static _Atomic uint64_t first_holds;
static _Atomic int turn_done;
static uint64_t turn_worst;

static void *hold_again(void *arg) {
    counter_t *c = arg;
    wf_cmap_user *user = wf_cmap_user_at(c->map, c->index);
    uint64_t worst = 0;
    for (uint64_t i = 0; c->index == 1 ? i < TURN_HOLDS : !atomic_load(&turn_done); i++) {
        wf_cmap_hold(user);
        if (c->index == 0)
            atomic_fetch_add(&first_holds, 1);
        else if (atomic_load(&first_holds) - queued_at[1] > worst)
            worst = atomic_load(&first_holds) - queued_at[1];
        wf_cmap_unhold(user);
    }
    if (c->index == 1) {
        turn_worst = worst;
        atomic_store(&turn_done, 1);
    }
    return NULL;
}

static void holds_in_turn(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    atomic_store(&first_holds, 0);
    atomic_store(&turn_done, 0);
    hold_clock = &first_holds;
    pthread_t threads[2];
    counter_t c[2];
    for (unsigned i = 0; i < 2; i++) {
        c[i] = (counter_t){map, i, NULL, NULL, 0};
        pthread_create(&threads[i], NULL, hold_again, &c[i]);
    }
    for (unsigned i = 0; i < 2; i++)
        pthread_join(threads[i], NULL);
    hold_clock = NULL;
    if (turn_worst > 1)
        fail("a statement over the map was overtaken by later ones (holds, bound)", turn_worst, 1);
    wf_cmap_destroy(map);
}

/* A capacity past what any table can hold sizes the first table at the
 * limit, and the map works. */
/* Sets of entries held together (wf_cmap_hold_set). */

enum { SET_KEYS = 8 };

/* One thread's sets of up to eight keys, some of them repeated, against a
 * plain reference: every key of a set is found, fresh exactly when the
 * reference lacks it, with the reference's value; a key outside the set is
 * not found; and what a release keeps and removes is what the next set
 * finds. The map starts with one cell, so the sets cross many moves. */
static void sets_sequential(void) {
    enum { OPS = 100000 };
    static uint8_t present[ENTRY_KEYS];
    static uint64_t value[ENTRY_KEYS];
    static unsigned char bytes[SET_KEYS + 1][ENTRY_KEY_BYTES];
    wf_cmap *map = wf_cmap_create_entries(16, 8, 1);
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    wf_cmap_set set = {0};
    uint64_t state = 23, live = 0, whole = 0;
    for (unsigned i = 0; i < OPS; i++) {
        uint64_t r = next(&state);
        unsigned count = 1 + (unsigned)(r % SET_KEYS);
        unsigned keys[SET_KEYS];
        uint64_t lengths[SET_KEYS + 1];
        wf_cmap_set_clear(&set);
        for (unsigned j = 0; j < count; j++) {
            /* Every fourth key repeats the one before it. */
            keys[j] = j > 0 && (next(&state) & 3) == 0 ? keys[j - 1] : (unsigned)(next(&state) % ENTRY_KEYS);
            lengths[j] = entry_key(keys[j], bytes[j]);
            wf_cmap_set_add(&set, bytes[j], lengths[j]);
        }
        wf_cmap_hold_set(user, &set);
        if (set.whole) {
            /* Two of its keys share a hash: the statement holds the map. */
            whole++;
            wf_cmap_release_set(user, &set);
            continue;
        }
        unsigned outside = (unsigned)(next(&state) % ENTRY_KEYS);
        int inside = 0;
        for (unsigned j = 0; j < count; j++)
            inside |= keys[j] == outside;
        lengths[SET_KEYS] = entry_key(outside, bytes[SET_KEYS]);
        if (!inside && wf_cmap_set_find(&set, bytes[SET_KEYS], lengths[SET_KEYS]) != NULL)
            fail("a set found a key it does not hold", outside, i);
        for (unsigned j = 0; j < count; j++) {
            unsigned k = keys[j];
            wf_cmap_held *held = wf_cmap_set_find(&set, bytes[j], lengths[j]);
            if (held == NULL)
                fail("a set did not find one of its keys", k, i);
            uint64_t *slot = held->slot;
            if (held->present != present[k])
                fail("a held entry's presence disagrees with the reference", k, held->present);
            if (present[k] && slot[0] != value[k])
                fail("a held entry's value disagrees with the reference", k, slot[0]);
            if (!present[k] && held->fresh && (slot[0] != 0 || slot[1] != 0))
                fail("a fresh held entry's slot is not zero", k, slot[0]);
            uint64_t change = next(&state);
            int keep = (change & 3) != 0;
            /* A slot left empty is zero again, as a fresh one is, since the
             * set may name the key once more. */
            slot[0] = keep ? change : 0;
            value[k] = change;
            live += (uint64_t)keep - (uint64_t)present[k];
            present[k] = (uint8_t)keep;
            held->present = (uint32_t)keep;
        }
        wf_cmap_release_set(user, &set);
    }
#ifndef WF_CMAP_TAG_MASK
    if (whole != 0)
        fail("a set of keys with different hashes held the whole map", whole, 0);
#endif
    if (wf_cmap_count(map) != live)
        fail("the count of entries disagrees with the reference after sets", wf_cmap_count(map), live);
    check_cells(map, "sets miscounted the cells they took (counted, taken)");
    unsigned char probe[ENTRY_KEY_BYTES];
    for (unsigned k = 0; k < ENTRY_KEYS; k++) {
        wf_cmap_entry entry;
        uint64_t length = entry_key(k, probe);
        uint64_t *slot = wf_cmap_lock_entry(user, probe, length, 0, &entry);
        if (entry.fresh == present[k] || (present[k] && slot[0] != value[k]))
            fail("a keyed statement disagrees with what the sets left", k, entry.fresh);
        wf_cmap_unlock_entry(user, &entry, 0, present[k]);
    }
    wf_cmap_destroy(map);
}

/* Two keys absent from map whose hashes differ, the first before the second
 * in a set's order. */
static void ordered_pair(unsigned char first[16], unsigned char second[16]) {
    uint64_t k = 0;
    counted_key(k++, first);
    do
        counted_key(k++, second);
    while (tag_of(first, 12) == tag_of(second, 12));
    if (tag_of(first, 12) > tag_of(second, 12)) {
        unsigned char swap[16];
        memcpy(swap, first, 16);
        memcpy(first, second, 16);
        memcpy(second, swap, 16);
    }
}

/* As another writer that, just before the set under test claims its first
 * cell, moves the whole table, so the claim lands in a table that is no
 * longer current. */
static void move_all(struct table *t, unsigned long long index) {
    (void)index;
    start_move(other_user->map, t);
    help(other_user->map, t);
}

/* A set that claims a cell in a table a move has left gives the cell back
 * and holds its keys in the next table: kept where it was claimed, its
 * entries would stay in a table no statement reads again. */
static void sets_follow_moves(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    other_user = wf_cmap_user_at(map, 1);
    unsigned char a[16], b[16];
    ordered_pair(a, b);
    wf_cmap_set set = {0};
    wf_cmap_set_add(&set, a, 12);
    wf_cmap_set_add(&set, b, 12);
    at_claim = move_all;
    wf_cmap_hold_set(first, &set);
    if (at_claim != NULL || set.whole)
        fail("the set met no move, or held the whole map for one (whole)", set.whole, 0);
    for (unsigned i = 0; i < 2; i++) {
        wf_cmap_held *held = wf_cmap_set_find(&set, i == 0 ? a : b, 12);
        *(uint64_t *)held->slot = 5 + i;
        held->present = 1;
    }
    wf_cmap_release_set(first, &set);
    for (unsigned i = 0; i < 2; i++) {
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(first, i == 0 ? a : b, 12, 0, &entry);
        if (entry.fresh || slot[0] != 5 + i)
            fail("a set's entry was left in a table that had moved (key, fresh)", i, entry.fresh);
        wf_cmap_unlock_entry(first, &entry, 0, 1);
    }
    wf_cmap_destroy(map);
}

/* A set with no patience that has claimed a cell for its first key and then
 * loses the lock of its second gives the claim back, counted as a cell
 * taken, and holds the whole map. */
static void sets_give_back_counted(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    other_user = wf_cmap_user_at(map, 1);
    unsigned char a[16], b[16];
    ordered_pair(a, b);
    wf_cmap_entry entry;
    uint64_t *slot = wf_cmap_lock_entry(first, b, 12, 0, &entry);
    slot[0] = 9;
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    wf_cmap_set set = {0};
    wf_cmap_set_add(&set, b, 12);
    wf_cmap_set_add(&set, a, 12);
    at_lock = remove_under_lock;
    set_patience(0, PATIENCE);
    wf_cmap_hold_set(first, &set);
    set_patience(PATIENCE, PATIENCE);
    if (at_lock != NULL || !set.whole)
        fail("a set that lost a lock did not hold the whole map (whole)", set.whole, 0);
    wf_cmap_release_set(first, &set);
    check_cells(map, "a set's given-back claim was not counted (counted, taken)");
    wf_cmap_destroy(map);
}

/* Threads holding sets while others hold single entries and one holds the
 * whole map. A set's statement moves an amount from its first key to its
 * others, one write at a time, so the keys' sum is unchanged only once the
 * statement has ended; a keyed statement adds one to the total and then to
 * its key; a statement holding every key as one set, and one holding the
 * whole map, must each find the sum equal to the total. */
enum { ACCOUNTS = 48, TRANSFERS = 40000 };
static _Atomic uint64_t set_wholes, set_holds;

typedef struct {
    wf_cmap *map;
    unsigned index;
    _Atomic uint64_t *total;
    _Atomic int *stop;
    uint64_t checks;
} mover_t;

static void *move_between(void *arg) {
    mover_t *m = arg;
    wf_cmap_user *user = wf_cmap_user_at(m->map, m->index);
    unsigned char bytes[SET_KEYS][16];
    wf_cmap_set set = {0};
    uint64_t state = mix64(m->index + 7);
    for (uint64_t i = 0; i < statements; i++) {
        unsigned count = 2 + (unsigned)(next(&state) % (SET_KEYS - 1));
        wf_cmap_set_clear(&set);
        uint64_t lengths[SET_KEYS];
        for (unsigned j = 0; j < count; j++) {
            lengths[j] = counted_key(next(&state) % ACCOUNTS, bytes[j]);
            wf_cmap_set_add(&set, bytes[j], lengths[j]);
        }
        wf_cmap_hold_set(user, &set);
        atomic_fetch_add(&set_holds, 1);
        if (set.whole) {
            atomic_fetch_add(&set_wholes, 1);
            /* Under the whole map: the same statement through held entries. */
            for (unsigned j = 0; j < count; j++) {
                wf_cmap_entry entry;
                uint64_t *slot = wf_cmap_lock_entry(user, bytes[j], lengths[j], 1, &entry);
                slot[0] += j == 0 ? (uint64_t)(count - 1) * 3 : (uint64_t)0 - 3;
                wf_cmap_unlock_entry(user, &entry, 1, slot[0] != 0);
            }
        } else {
            for (unsigned j = 0; j < count; j++) {
                wf_cmap_held *held = wf_cmap_set_find(&set, bytes[j], lengths[j]);
                if (held == NULL)
                    fail("a set did not find a key it added", j, i);
                uint64_t *slot = held->slot;
                slot[0] += j == 0 ? (uint64_t)(count - 1) * 3 : (uint64_t)0 - 3;
                held->present = slot[0] != 0;
                for (volatile int spin = 0; spin < 20; spin++) {
                }
            }
        }
        wf_cmap_release_set(user, &set);
    }
    return NULL;
}

static void *count_account(void *arg) {
    mover_t *m = arg;
    wf_cmap_user *user = wf_cmap_user_at(m->map, m->index);
    unsigned char bytes[16];
    uint64_t state = mix64(m->index + 31);
    for (uint64_t i = 0; i < statements; i++) {
        uint64_t length = counted_key(next(&state) % ACCOUNTS, bytes);
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, bytes, length, 0, &entry);
        atomic_fetch_add(m->total, 1);
        slot[0] += 1;
        wf_cmap_unlock_entry(user, &entry, 0, slot[0] != 0);
    }
    return NULL;
}

/* Holds every account, as one set or as the whole map by turns. */
static void *audit_accounts(void *arg) {
    mover_t *m = arg;
    wf_cmap_user *user = wf_cmap_user_at(m->map, m->index);
    static unsigned char bytes[ACCOUNTS][16];
    wf_cmap_set set = {0};
    while (!atomic_load(m->stop)) {
        uint64_t sum = 0, total;
        if (m->checks % 2 == 0) {
            wf_cmap_set_clear(&set);
            for (unsigned k = 0; k < ACCOUNTS; k++)
                wf_cmap_set_add(&set, bytes[k], counted_key(k, bytes[k]));
            wf_cmap_hold_set(user, &set);
            total = atomic_load(m->total);
            if (set.whole) {
                for (unsigned k = 0; k < ACCOUNTS; k++) {
                    wf_cmap_entry entry;
                    uint64_t *slot = wf_cmap_lock_entry(user, bytes[k], 12, 1, &entry);
                    sum += slot[0];
                    wf_cmap_unlock_entry(user, &entry, 1, !entry.fresh);
                }
            } else {
                for (unsigned k = 0; k < ACCOUNTS; k++) {
                    wf_cmap_held *held = wf_cmap_set_find(&set, bytes[k], 12);
                    if (held == NULL)
                        fail("the audit's set lacks an account", k, 0);
                    sum += *(uint64_t *)held->slot;
                }
            }
            wf_cmap_release_set(user, &set);
        } else {
            wf_cmap_hold(user);
            total = atomic_load(m->total);
            for (unsigned k = 0; k < ACCOUNTS; k++) {
                wf_cmap_entry entry;
                uint64_t *slot = wf_cmap_lock_entry(user, bytes[k], counted_key(k, bytes[k]), 1, &entry);
                sum += slot[0];
                wf_cmap_unlock_entry(user, &entry, 1, !entry.fresh);
            }
            wf_cmap_unhold(user);
        }
        if (sum != total)
            fail("an audit saw a statement half done (sum, total)", sum, total);
        m->checks++;
    }
    return NULL;
}

/* With capacity 1 the map moves while sets are held and locked, and with no
 * patience every set that waits holds the whole map instead. */
static void sets_move_amounts(uint64_t capacity, uint64_t patient) {
    enum { MOVERS = 3, COUNTERS = 2 };
    wf_cmap *map = wf_cmap_create_entries(8, 8, capacity);
    set_patience(patient, patient);
    statements = patient == 0 ? TRANSFERS / 4 : TRANSFERS;
    atomic_store(&set_wholes, 0);
    atomic_store(&set_holds, 0);
    _Atomic uint64_t total = 0;
    _Atomic int stop = 0;
    pthread_t t[MOVERS + COUNTERS + 1];
    mover_t m[MOVERS + COUNTERS + 1];
    for (unsigned i = 0; i <= MOVERS + COUNTERS; i++) {
        m[i] = (mover_t){map, i, &total, &stop, 0};
        pthread_create(&t[i], NULL, i < MOVERS ? move_between : i < MOVERS + COUNTERS ? count_account : audit_accounts,
                       &m[i]);
    }
    for (unsigned i = 0; i < MOVERS + COUNTERS; i++)
        pthread_join(t[i], NULL);
    atomic_store(&stop, 1);
    pthread_join(t[MOVERS + COUNTERS], NULL);
    if (m[MOVERS + COUNTERS].checks < 2)
        fail("the audit never held the accounts both ways", m[MOVERS + COUNTERS].checks, 0);
#ifndef WF_CMAP_TAG_MASK
    /* Keys with different hashes and ordinary patience: a set holds the map
     * only after a wait no cycle causes, so nearly every set holds its
     * entries. */
    if (patient == PATIENCE && atomic_load(&set_wholes) * 20 > atomic_load(&set_holds))
        fail("sets held the whole map more than once in twenty (whole, sets)", atomic_load(&set_wholes),
             atomic_load(&set_holds));
#endif
    check_cells(map, "sets and keyed statements miscounted the cells they took (counted, taken)");
    uint64_t sum = 0;
    for (uint64_t *slot; (slot = wf_cmap_drain(map)) != NULL;)
        sum += slot[0];
    if (sum != COUNTERS * statements)
        fail("a statement's change was lost under sets (sum, patience)", sum, patient);
    set_patience(PATIENCE, PATIENCE);
    wf_cmap_destroy(map);
}

/* Two threads whose sets name the same two keys in opposite orders, with
 * patience that never runs out: each locks them in the map's one order, so
 * neither waits for the other while it holds a key the other waits for. Sets
 * locked in the order they were added would stop here until the alarm. */
static unsigned char pair_bytes[2][16];

static void *hold_pair(void *arg) {
    mover_t *m = arg;
    wf_cmap_user *user = wf_cmap_user_at(m->map, m->index);
    wf_cmap_set set = {0};
    for (uint64_t i = 0; i < statements; i++) {
        wf_cmap_set_clear(&set);
        wf_cmap_set_add(&set, pair_bytes[m->index], 12);
        wf_cmap_set_add(&set, pair_bytes[1 - m->index], 12);
        wf_cmap_hold_set(user, &set);
        if (set.whole)
            fail("a pair of keys with patience to spare held the whole map", m->index, i);
        for (unsigned j = 0; j < 2; j++) {
            wf_cmap_held *held = wf_cmap_set_find(&set, pair_bytes[j], 12);
            *(uint64_t *)held->slot += 1;
            held->present = 1;
        }
        wf_cmap_release_set(user, &set);
    }
    return NULL;
}

static void sets_in_one_order(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    set_patience(UINT64_MAX, UINT64_MAX);
    statements = 200000;
    /* Two keys whose hashes differ under every build's hash. */
    uint64_t second = 1;
    counted_key(0, pair_bytes[0]);
    do
        counted_key(second++, pair_bytes[1]);
    while (tag_of(pair_bytes[0], 12) == tag_of(pair_bytes[1], 12));
    pthread_t t[2];
    mover_t m[2];
    for (unsigned i = 0; i < 2; i++) {
        m[i] = (mover_t){map, i, NULL, NULL, 0};
        pthread_create(&t[i], NULL, hold_pair, &m[i]);
    }
    for (unsigned i = 0; i < 2; i++)
        pthread_join(t[i], NULL);
    uint64_t sum = 0;
    for (uint64_t *slot; (slot = wf_cmap_drain(map)) != NULL;)
        sum += slot[0];
    if (sum != 4 * statements)
        fail("a pair's change was lost (sum, expected)", sum, 4 * statements);
    set_patience(PATIENCE, PATIENCE);
    wf_cmap_destroy(map);
}

static void entries_huge_capacity(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 1ull << 61);
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    unsigned char bytes[16];
    uint64_t length = counted_key(1, bytes);
    wf_cmap_entry entry;
    uint64_t *slot = wf_cmap_lock_entry(user, bytes, length, 0, &entry);
    slot[0] = 5;
    wf_cmap_unlock_entry(user, &entry, 0, 1);
    slot = wf_cmap_lock_entry(user, bytes, length, 0, &entry);
    if (entry.fresh || slot[0] != 5)
        fail("a map sized past the limit lost its key", entry.fresh, slot[0]);
    wf_cmap_unlock_entry(user, &entry, 0, 1);
    if (atomic_load(&map->current)->capacity > 2 * CAPACITY_LIMIT)
        fail("a capacity past the limit sized a larger table", atomic_load(&map->current)->capacity, 0);
    wf_cmap_destroy(map);
}

int main(void) {
    /* Writers that wait on each other in a cycle fail the test here rather
     * than at the gate's limit. */
    alarm(120);
    set_patience(PATIENCE, PATIENCE);
    if (ENTRY_TESTS) {
        entries_huge_capacity();
        entries_sequential();
        claim_ahead();
        claim_behind(1);
        claim_behind(0);
        settle_pending();
        claim_yields();
        claim_impatient();
        retries_count(LOST_EMPTY_CLAIM);
        retries_count(CLAIM_MOVED);
        retries_count(LOST_REMOVED_CLAIM);
        retries_count(LOST_LOCK);
        entries_misses();
        entries_churn(0, PATIENCE);
        entries_churn(1, PATIENCE);
        entries_churn(1, 0);
        entries_held(0, PATIENCE);
        entries_held(1, 0);
        reads_follow_moves();
        reads_block_moves();
        entries_shared_reads(0);
        entries_shared_reads(1);
        sets_sequential();
        sets_follow_moves();
        sets_give_back_counted();
        sets_in_one_order();
        sets_move_amounts(0, PATIENCE);
        sets_move_amounts(1, PATIENCE);
        sets_move_amounts(1, 0);
    }
    if (ENTRY_TESTS && WORD_TESTS) {
        entries_bounded();
        holds_wait_for_counted();
        holds_in_turn();
    }
    if (WORD_TESTS) {
        checker_self_test();
        claim_given_back();
        sequential();
        concurrent(0);
        concurrent(1);
        histories(20);
    }
    printf("concurrent-map-test: all checks passed\n");
    return 0;
}
