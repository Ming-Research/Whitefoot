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
 *   waiting;
 * - key sets, in the order their keys are first inserted, and holds of
 *   several entries kept in a statement's frame: positions with repeats,
 *   the order they lock in, the release at each tag width, holds of two
 *   maps at once, holds of the whole map, a hold waiting out a move under
 *   way, swaps of two maps' entries, clears whose entries are released
 *   after the hold, scans in steps across moves that write nothing of the
 *   map, a hold that keeps finding its keys after their bytes change, and
 *   the keyed tables' functions
 *   (keyed_table.c), which the test compiles in with the map, standing in
 *   for the completion runtime they take a driver's number and guards'
 *   watches from.
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

/* The host the runtime supplies the map, here from the C library, counting
 * the blocks it has handed out and not had back. */
static _Atomic int64_t blocks_out;
static _Atomic uint64_t allocations;
static _Atomic int64_t mapped_bytes_out;
static void *test_take(size_t bytes);
static void test_give(void *block);
#define WF_CMAP_TAKE(bytes, origin) ((void)(origin), test_take((size_t)(bytes)))
#define WF_CMAP_GIVE(block, bytes, origin) ((void)(origin), test_give(block))
#define WF_CMAP_YIELD() sched_yield()
#define WF_CMAP_EXHAUSTED() abort()
#define WF_CMAP_HEAP_CHANGE(origin, delta) ((void)(origin), atomic_fetch_add_explicit(&mapped_bytes_out, (delta), memory_order_relaxed))
#define WF_CMAP_ORIGIN() 0u
#define WF_CMAP_RETAIN(origin) ((void)(origin))
#define WF_CMAP_RELEASE(origin) ((void)(origin))
struct wf_cmap;
static void finishing(struct wf_cmap *map);
#define WF_CMAP_FINISHING(map) finishing(map)
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
/* Whether timing ratios between threads hold in this build. ThreadSanitizer
 * slows every thread by an unequal factor, so waits outlast their patience
 * far more often than on real hardware; its build checks for races, and the
 * ordinary builds keep the ratios. */
#if defined(__SANITIZE_THREAD__)
#define TIMED_RATIOS 0
#elif defined(__has_feature)
#if __has_feature(thread_sanitizer)
#define TIMED_RATIOS 0
#endif
#endif
#ifndef TIMED_RATIOS
#define TIMED_RATIOS 1
#endif
/* Whether this build narrows entries' hashes, so that most keys share one;
 * the map's source defines the mask itself when the build does not. */
#define SHARED_HASHES (!WORD_TESTS)
#ifdef WF_CMAP_LOCKED_READ
#define ENTRY_TESTS 0
#else
#define ENTRY_TESTS 1
#endif

#include "keyed_table.c"

static void *test_take(size_t bytes) {
    atomic_fetch_add(&allocations, 1);
    atomic_fetch_add(&blocks_out, 1);
    return aligned_alloc(16, (bytes + 15) / 16 * 16);
}

static void test_give(void *block) {
    atomic_fetch_sub(&blocks_out, 1);
    free(block);
}

/* What keyed_table.c takes from the completion runtime, here: each test
 * thread names the driver it stands for, and a guard's watch is counted on
 * its table, whose wakes are counted. */
static _Thread_local unsigned test_driver;
static _Atomic uint64_t written_calls;

/* The standalone map harness does not run ordinary object holds. */
void *wf__shared_new(uint64_t bytes) { return test_take(WF_SHARED_STATE_OFFSET + bytes); }
void wf__shared_take(void *object, uint32_t write) { (void)object; (void)write; abort(); }
void wf__shared_unlock(void *object, uint32_t write) { (void)object; (void)write; abort(); }

unsigned wf__driver_index(void) { return test_driver; }

void wf__watch_written(wf_watch_list *list) {
    (void)list;
    atomic_fetch_add(&written_calls, 1);
}

void wf__watch_unit(void *watch, wf_watch_list *list) {
    (void)watch;
    list->count += 1;
}

/* A step the test of moves under way takes once, when the thread standing
 * for driver 1 is about to help a move of pause_map to its end: it waits
 * there until it is let go. */
static wf_cmap *pause_map;
static _Atomic int pause_armed, pause_reached, pause_released;

static void finishing(struct wf_cmap *map) {
    if (map != pause_map || test_driver != 1 || !atomic_exchange(&pause_armed, 0))
        return;
    atomic_store(&pause_reached, 1);
    while (!atomic_load(&pause_released))
        sched_yield();
}

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
 * claim the cell at index; the claim tests below set it. Threads of the
 * other tests pass here too, finding no step, so the step is taken by one
 * exchange after a plain look. */
typedef void (*claim_step)(struct table *t, unsigned long long index);
static _Atomic(claim_step) at_claim;

static void before_claim(struct table *t, unsigned long long index) {
    claim_step step = atomic_load_explicit(&at_claim, memory_order_relaxed);
    if (step != NULL && (step = atomic_exchange(&at_claim, NULL)) != NULL)
        step(t, index);
}

/* The same, when the writer under test is about to lock a cell of its key's
 * hash. */
typedef void (*lock_step)(struct cell *c);
static _Atomic(lock_step) at_lock;

static void before_lock(struct cell *c) {
    lock_step step = atomic_load_explicit(&at_lock, memory_order_relaxed);
    if (step != NULL && (step = atomic_exchange(&at_lock, NULL)) != NULL)
        step(c);
}

/* The other writer and the keys the claim tests use. */
static wf_cmap_user *other_user;
static const unsigned char *claim_key, *gone_key;
static uint64_t claim_length, gone_length;

/* A simulated competing removal must release its node and live count,
 * just as a real writer does, before another claim replaces the cell. */
static void remove_between_steps(wf_cmap_user *u, cell *c) {
    node *n = node_at(c);
    free_node(u, n, node_bytes(u->map, n->length));
    count(u, 0, -1);
    atomic_store(&c->key, REMOVED);
}

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
    /* This synthetic claim has no node: give it back before destruction. */
    atomic_store(&behind->key, REMOVED);
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
    remove_between_steps(other_user, c);
    atomic_store(&c->key, tag | LOCKED | PENDING);
    pthread_create(&pending_thread, NULL, give_back_later, c);
}

/* Key k claims the empty cell after a live key's cell that another writer of
 * k has meanwhile claimed, pending: k gives its empty cell back, counted as
 * taken, and claims again once the other claim is given back. */
static void claim_yields(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    other_user = wf_cmap_user_at(map, 1);
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
    remove_between_steps(other_user, c);
    node *n = new_node(other_user, node_bytes(map, claim_length));
    n->length = claim_length;
    memcpy(n->bytes, claim_key, (size_t)claim_length);
    memset(slot_of(map, n), 0, (size_t)map->slot_size);
    ((uint64_t *)slot_of(map, n))[0] = 7;
    atomic_store(&c->value, (uint64_t)(uintptr_t)n);
    count(other_user, 0, 1);
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
static _Atomic(wf_cmap *) read_found_map;

static void read_found(struct table *t) {
    wf_cmap *map = atomic_load_explicit(&read_found_map, memory_order_relaxed);
    if (map != NULL && (map = atomic_exchange(&read_found_map, NULL)) != NULL)
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
    count(other_user, 1, 0);
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
    count(other_user, 0, 1);
    atomic_store(&t->cells[index].key, tag_of(gone_key, gone_length));
}

/* As another writer that removes the key in c just before the writer under
 * test locks it, so the lock's compare-and-swap loses. */
static void remove_under_lock(struct cell *c) { remove_between_steps(other_user, c); }

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

/* Holds of several entries (wf_cmap_hold_take). The slots of these tests
 * keep a value in their first word, which is also their tag: zero is None. */
#define VALUE_TAG 0, 8, 0

enum { SET_KEYS = 8 };

/* One thread's holds of up to eight keys, some of them repeated, against a
 * plain reference: each key's position is the order it was added in,
 * repeated keys share one slot and different keys do not, the keys are
 * ranked in the hold's lock order, an entry is fresh exactly when the reference lacks
 * it and holds the reference's value otherwise, and what a release keeps
 * and removes is what the next hold finds. The map starts with one cell, so
 * the holds cross many moves. */
static void holds_sequential(void) {
    enum { OPS = 15000 };
    static uint8_t present[ENTRY_KEYS];
    static uint64_t value[ENTRY_KEYS];
    static unsigned char bytes[SET_KEYS][ENTRY_KEY_BYTES];
    wf_cmap *map = wf_cmap_create_entries(16, 8, 1);
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    wf_cmap_holding hold;
    uint64_t state = 23, live = 0, whole = 0;
    for (unsigned i = 0; i < OPS; i++) {
        uint64_t r = next(&state);
        unsigned count = 1 + (unsigned)(r % SET_KEYS);
        unsigned keys[SET_KEYS];
        wf_cmap_hold_begin(&hold, map);
        for (unsigned j = 0; j < count; j++) {
            /* Every fourth key repeats the one before it. */
            keys[j] = j > 0 && (next(&state) & 3) == 0 ? keys[j - 1] : (unsigned)(next(&state) % ENTRY_KEYS);
            uint64_t length = entry_key(keys[j], bytes[j]);
            if (wf_cmap_hold_key(&hold, bytes[j], length) != j)
                fail("a key's position is not the order it was added in", j, i);
        }
        user->waited = 0;
        wf_cmap_hold_take(user, &hold);
        /* A hold that meets a cell it holds itself, which two of its keys of
         * one hash make it do, holds the map at once: waiting for that cell,
         * it would wait until its patience ended. */
        if (hold.whole && user->waited != 0)
            fail("a hold waited for a cell it holds itself (whole, waited)", hold.whole, user->waited);
        whole += hold.whole;
        wf_cmap_held *held = held_keys(&hold);
        for (unsigned j = 1; j < count; j++)
            if (held_order(ranked(held, j - 1), ranked(held, j)) > 0)
                fail("a hold's keys are not ranked in its lock order", j, i);
        for (unsigned j = 0; j < count; j++) {
            unsigned k = keys[j];
            uint64_t *slot = wf_cmap_hold_slot(&hold, j);
            for (unsigned m = 0; m < j; m++)
                if ((keys[m] == k) != (wf_cmap_hold_slot(&hold, m) == slot))
                    fail("repeated keys do not share one slot, or different keys do (key, other)", k, keys[m]);
            if (present[k] ? slot[0] != value[k] : (slot[0] != 0 || slot[1] != 0))
                fail("a held entry disagrees with the reference (key, value)", k, slot[0]);
            if (held[j].leads && (held[j].fresh != 0) != !present[k])
                fail("a held entry's freshness disagrees with the reference", k, held[j].fresh);
        }
        for (unsigned j = 0; j < count; j++) {
            unsigned k = keys[j];
            int first = 1;
            for (unsigned m = 0; m < j; m++)
                first &= keys[m] != k;
            if (!first)
                continue;
            uint64_t change = next(&state) | 1;
            int keep = (change & 6) != 0;
            uint64_t *slot = wf_cmap_hold_slot(&hold, j);
            /* A slot left empty is zero again, as a fresh one is. */
            slot[0] = keep ? change : 0;
            value[k] = change;
            live += (uint64_t)keep - (uint64_t)present[k];
            present[k] = (uint8_t)keep;
        }
        wf_cmap_hold_release(&hold, VALUE_TAG);
    }
    if (!SHARED_HASHES && whole != 0)
        fail("a hold of keys with different hashes held the whole map", whole, 0);
    /* With hashes narrowed, most holds of several keys share one. */
    if (SHARED_HASHES && whole == 0)
        fail("no hold of keys sharing a hash held the whole map", whole, 0);
    if (wf_cmap_count(map) != live)
        fail("the count of entries disagrees with the reference after holds", wf_cmap_count(map), live);
    check_cells(map, "holds miscounted the cells they took (counted, taken)");
    unsigned char probe[ENTRY_KEY_BYTES];
    for (unsigned k = 0; k < ENTRY_KEYS; k++) {
        wf_cmap_entry entry;
        uint64_t length = entry_key(k, probe);
        uint64_t *slot = wf_cmap_lock_entry(user, probe, length, 0, &entry);
        if (entry.fresh == present[k] || (present[k] && slot[0] != value[k]))
            fail("a keyed statement disagrees with what the holds left", k, entry.fresh);
        wf_cmap_unlock_entry(user, &entry, 0, present[k]);
    }
    wf_cmap_destroy(map);
}

/* Two keys absent from map whose hashes differ, the first before the second
 * in a hold's lock order. */
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

/* As another writer that, just before the hold under test claims its first
 * cell, moves the whole table, so the claim lands in a table that is no
 * longer current. */
static void move_all(struct table *t, unsigned long long index) {
    (void)index;
    start_move(other_user->map, t);
    help(other_user->map, t);
}

/* A hold that claims a cell in a table a move has left gives the cell back
 * and holds its keys in the next table: kept where it was claimed, its
 * entries would stay in a table no statement reads again. */
static void holds_follow_moves(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    other_user = wf_cmap_user_at(map, 1);
    unsigned char a[16], b[16];
    ordered_pair(a, b);
    wf_cmap_holding hold;
    wf_cmap_hold_begin(&hold, map);
    wf_cmap_hold_key(&hold, a, 12);
    wf_cmap_hold_key(&hold, b, 12);
    at_claim = move_all;
    wf_cmap_hold_take(first, &hold);
    if (at_claim != NULL || hold.whole)
        fail("the hold met no move, or held the whole map for one (whole)", hold.whole, 0);
    for (unsigned i = 0; i < 2; i++)
        *(uint64_t *)wf_cmap_hold_slot(&hold, i) = 5 + i;
    wf_cmap_hold_release(&hold, VALUE_TAG);
    for (unsigned i = 0; i < 2; i++) {
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(first, i == 0 ? a : b, 12, 0, &entry);
        if (entry.fresh || slot[0] != 5 + i)
            fail("a hold's entry was left in a table that had moved (key, fresh)", i, entry.fresh);
        wf_cmap_unlock_entry(first, &entry, 0, 1);
    }
    wf_cmap_destroy(map);
}

/* As another writer that, just before the statement under test locks a cell
 * of its key's hash, moves the whole table, so the lock is taken in a table
 * that is no longer current. */
static void move_at_lock(struct cell *c) {
    (void)c;
    wf_cmap *map = other_user->map;
    table *t = atomic_load(&map->current);
    start_move(map, t);
    help(map, t);
}

/* A statement that locks its key's cell in a table a move has left reads
 * nothing through the cell, whose node a statement in the next table may
 * have freed, gives the cell back as it was and locks the key in the next
 * table: a keyed statement, and a hold, which holds no cell for that key
 * while it does. */
static void locks_follow_moves(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    other_user = wf_cmap_user_at(map, 1);
    unsigned char a[16], b[16];
    ordered_pair(a, b);
    wf_cmap_entry entry;
    for (unsigned i = 0; i < 2; i++) {
        uint64_t *slot = wf_cmap_lock_entry(first, i == 0 ? a : b, 12, 0, &entry);
        slot[0] = 3 + i;
        wf_cmap_unlock_entry(first, &entry, 0, 1);
    }
    at_lock = move_at_lock;
    uint64_t *slot = wf_cmap_lock_entry(first, a, 12, 0, &entry);
    if (at_lock != NULL || entry.fresh || entry.upgraded || slot[0] != 3)
        fail("a keyed statement lost its key to a move at its lock (fresh, value)", entry.fresh, slot[0]);
    slot[0] = 7;
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    wf_cmap_holding hold;
    wf_cmap_hold_begin(&hold, map);
    wf_cmap_hold_key(&hold, a, 12);
    wf_cmap_hold_key(&hold, b, 12);
    at_lock = move_at_lock;
    wf_cmap_hold_take(first, &hold);
    if (at_lock != NULL || hold.whole)
        fail("a hold met no move at its lock, or held the whole map for one (whole)", hold.whole, 0);
    for (unsigned i = 0; i < 2; i++) {
        wf_cmap_held *held = &held_keys(&hold)[i];
        if (held->fresh || *(uint64_t *)wf_cmap_hold_slot(&hold, i) != (i == 0 ? 7u : 4u))
            fail("a hold lost a key to a move at its lock (key, fresh)", i, held->fresh);
    }
    wf_cmap_hold_release(&hold, VALUE_TAG);
    check_cells(map, "a move at a lock miscounted the cells taken (counted, taken)");
    wf_cmap_destroy(map);
}

/* The key a writer in the next table removes, and its length. */
static const unsigned char *freed_key;
static uint64_t freed_length;

/* As move_at_lock, and then as a statement in the next table that removes
 * freed_key, so that its node is freed while the old table's cell still
 * names it. */
static void move_and_remove_at_lock(struct cell *c) {
    move_at_lock(c);
    wf_cmap_entry entry;
    wf_cmap_lock_entry(other_user, freed_key, freed_length, 0, &entry);
    wf_cmap_unlock_entry(other_user, &entry, 0, 0);
}

/* A statement that locks its key's cell in a table a move has left, after a
 * statement in the next table has removed the key and freed its node, reads
 * nothing of that node: the key is long, so its node went back to the host,
 * and a build that checks memory sees a read of it. The statement finds the
 * key absent in the next table. */
static void locks_read_no_freed_node(void) {
    enum { LONG = 600 };
    static unsigned char key[LONG];
    for (unsigned i = 0; i < LONG; i++)
        key[i] = (unsigned char)(i * 7 + 1);
    for (int with_hold = 0; with_hold < 2; with_hold++) {
        wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
        wf_cmap_user *first = wf_cmap_user_at(map, 0);
        other_user = wf_cmap_user_at(map, 1);
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(first, key, LONG, 0, &entry);
        slot[0] = 9;
        wf_cmap_unlock_entry(first, &entry, 0, 1);
        freed_key = key;
        freed_length = LONG;
        at_lock = move_and_remove_at_lock;
        if (with_hold) {
            wf_cmap_holding hold;
            wf_cmap_hold_begin(&hold, map);
            wf_cmap_hold_key(&hold, key, LONG);
            wf_cmap_hold_take(first, &hold);
            if (at_lock != NULL || hold.whole || !held_keys(&hold)[0].fresh)
                fail("a hold found a key removed in the next table (whole, hold)", hold.whole, 1);
            wf_cmap_hold_release(&hold, VALUE_TAG);
        } else {
            wf_cmap_lock_entry(first, key, LONG, 0, &entry);
            if (at_lock != NULL || !entry.fresh)
                fail("a statement found a key removed in the next table (fresh, hold)", entry.fresh, 0);
            wf_cmap_unlock_entry(first, &entry, 0, 0);
        }
        if (wf_cmap_count(map) != 0)
            fail("a removed key was counted after a move at a lock (count, hold)", wf_cmap_count(map),
                 (uint64_t)with_hold);
        wf_cmap_destroy(map);
    }
}

/* A hold with no patience that has claimed a cell for its first key and then
 * loses the lock of its second gives the claim back, counted as a cell
 * taken, and holds the whole map. Its keys come in the reverse of byte
 * order, which is the order it locks them in. */
static void holds_give_back_counted(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *first = wf_cmap_user_at(map, 0);
    other_user = wf_cmap_user_at(map, 1);
    unsigned char a[16], b[16];
    ordered_pair(a, b);
    wf_cmap_entry entry;
    uint64_t *slot = wf_cmap_lock_entry(first, b, 12, 0, &entry);
    slot[0] = 9;
    wf_cmap_unlock_entry(first, &entry, 0, 1);
    wf_cmap_holding hold;
    wf_cmap_hold_begin(&hold, map);
    wf_cmap_hold_key(&hold, b, 12);
    wf_cmap_hold_key(&hold, a, 12);
    at_lock = remove_under_lock;
    set_patience(0, PATIENCE);
    wf_cmap_hold_take(first, &hold);
    set_patience(PATIENCE, PATIENCE);
    if (at_lock != NULL || !hold.whole)
        fail("a hold that lost a lock did not hold the whole map (whole)", hold.whole, 0);
    wf_cmap_hold_release(&hold, VALUE_TAG);
    check_cells(map, "a hold's given-back claim was not counted (counted, taken)");
    wf_cmap_destroy(map);
}

/* Threads holding several entries while others hold single entries and one
 * holds every account. A hold's statement moves an amount from its first
 * key to its others, one write at a time, so the keys' sum is unchanged only
 * once the statement has ended; a keyed statement adds one to the total and
 * then to its key; a statement holding every key, as one hold, as one hold
 * of the whole map, and as the whole map with each key locked under it,
 * must find the sum equal to the total, and the map held whole counts its
 * entries exactly. */
enum { ACCOUNTS = 48, TRANSFERS = 4000 };
static _Atomic uint64_t set_wholes, set_holds;
/* The audit's checks, which the movers go on transferring past their quota
 * to wait for, so that the audit holds the accounts every way while
 * transfers run even where its thread takes turns with five others on three
 * CPUs, as a hosted macOS runner's did. */
static _Atomic uint64_t audits;

typedef struct {
    wf_cmap *map;
    unsigned index;
    _Atomic uint64_t *total;
    _Atomic int *stop;
    uint64_t checks;
} mover_t;

static void *move_between(void *arg) {
    mover_t *m = arg;
    test_driver = m->index;
    wf_cmap_user *user = wf_cmap_user_at(m->map, m->index);
    unsigned char bytes[SET_KEYS][16];
    wf_cmap_holding hold;
    uint64_t state = mix64(m->index + 7);
    for (uint64_t i = 0; i < statements || atomic_load(&audits) < 3; i++) {
        unsigned count = 2 + (unsigned)(next(&state) % (SET_KEYS - 1));
        wf_cmap_hold_begin(&hold, m->map);
        for (unsigned j = 0; j < count; j++)
            wf_cmap_hold_key(&hold, bytes[j], counted_key(next(&state) % ACCOUNTS, bytes[j]));
        wf_cmap_hold_take(user, &hold);
        atomic_fetch_add(&set_holds, 1);
        if (hold.whole)
            atomic_fetch_add(&set_wholes, 1);
        for (unsigned j = 0; j < count; j++) {
            uint64_t *slot = wf_cmap_hold_slot(&hold, j);
            slot[0] += j == 0 ? (uint64_t)(count - 1) * 3 : (uint64_t)0 - 3;
            for (volatile int spin = 0; spin < 20; spin++) {
            }
        }
        wf_cmap_hold_release(&hold, VALUE_TAG);
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

static void *audit_accounts(void *arg) {
    mover_t *m = arg;
    test_driver = m->index;
    wf_cmap_user *user = wf_cmap_user_at(m->map, m->index);
    static unsigned char bytes[ACCOUNTS][16];
    wf_cmap_holding hold;
    while (!atomic_load(m->stop)) {
        uint64_t sum = 0, total, way = m->checks % 3;
        if (way < 2) {
            wf_cmap_hold_begin(&hold, m->map);
            if (way == 1)
                wf_cmap_hold_whole(&hold);
            for (unsigned k = 0; k < ACCOUNTS; k++)
                wf_cmap_hold_key(&hold, bytes[k], counted_key(k, bytes[k]));
            wf_cmap_hold_take(user, &hold);
            total = atomic_load(m->total);
            uint64_t present = 0;
            for (unsigned k = 0; k < ACCOUNTS; k++) {
                uint64_t value = *(uint64_t *)wf_cmap_hold_slot(&hold, k);
                sum += value;
                present += value != 0;
            }
            /* An account is kept exactly when it holds a value, and every
             * account is held here as it stood. */
            if (way == 1 && wf_cmap_count(m->map) != present)
                fail("a map held whole miscounted its entries (count, present)", wf_cmap_count(m->map), present);
            wf_cmap_hold_release(&hold, VALUE_TAG);
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
        atomic_fetch_add(&audits, 1);
    }
    return NULL;
}

/* With capacity 1 the map moves while holds are taken and locked, and with
 * no patience every hold that waits holds the whole map instead. */
static void holds_move_amounts(uint64_t capacity, uint64_t patient) {
    enum { MOVERS = 3, COUNTERS = 2 };
    wf_cmap *map = wf_cmap_create_entries(8, 8, capacity);
    set_patience(patient, patient);
    statements = patient == 0 ? TRANSFERS / 4 : TRANSFERS;
    atomic_store(&set_wholes, 0);
    atomic_store(&set_holds, 0);
    atomic_store(&audits, 0);
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
    if (m[MOVERS + COUNTERS].checks < 3)
        fail("the audit never held the accounts every way", m[MOVERS + COUNTERS].checks, 3);
    /* Keys with different hashes and ordinary patience: a hold holds the
     * map only after a wait no cycle causes or when the table is full, so
     * nearly every hold holds its entries. */
    if (TIMED_RATIOS && !SHARED_HASHES && patient == PATIENCE && atomic_load(&set_wholes) * 20 > atomic_load(&set_holds))
        fail("holds held the whole map more than once in twenty (whole, holds)", atomic_load(&set_wholes),
             atomic_load(&set_holds));
    check_cells(map, "holds and keyed statements miscounted the cells they took (counted, taken)");
    uint64_t sum = 0;
    for (uint64_t *slot; (slot = wf_cmap_drain(map)) != NULL;)
        sum += slot[0];
    if (sum != COUNTERS * statements)
        fail("a statement's change was lost under holds (sum, patience)", sum, patient);
    set_patience(PATIENCE, PATIENCE);
    wf_cmap_destroy(map);
}

/* Two threads whose holds name the same two keys in opposite orders, with
 * patience that never runs out: each locks them in the hold's order, so neither
 * waits for the other while it holds a key the other waits for, and each
 * key's slot is at the position it was added at. Holds locking their keys
 * in the order they were added would stop here until the alarm. */
static unsigned char pair_bytes[2][16];

static void *hold_pair(void *arg) {
    mover_t *m = arg;
    test_driver = m->index;
    wf_cmap_user *user = wf_cmap_user_at(m->map, m->index);
    wf_cmap_holding hold;
    for (uint64_t i = 0; i < statements; i++) {
        wf_cmap_hold_begin(&hold, m->map);
        wf_cmap_hold_key(&hold, pair_bytes[m->index], 12);
        wf_cmap_hold_key(&hold, pair_bytes[1 - m->index], 12);
        wf_cmap_hold_take(user, &hold);
        if (hold.whole)
            fail("a pair of keys with patience to spare held the whole map", m->index, i);
        /* The first position counts in the low half and the second in the
         * high, so that a slot answered for the wrong position shows. */
        *(uint64_t *)wf_cmap_hold_slot(&hold, 0) += 1;
        *(uint64_t *)wf_cmap_hold_slot(&hold, 1) += 1ull << 32;
        wf_cmap_hold_release(&hold, VALUE_TAG);
    }
    return NULL;
}

static void holds_in_one_order(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 0);
    set_patience(UINT64_MAX, UINT64_MAX);
    statements = 20000;
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
    /* Each key stands first in one thread's holds and second in the
     * other's. */
    for (unsigned k = 0; k < 2; k++) {
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(wf_cmap_user_at(map, 0), pair_bytes[k], 12, 0, &entry);
        if (slot[0] != (statements | statements << 32))
            fail("a pair's slot was reached at the wrong position or lost a change (key, value)", k, slot[0]);
        wf_cmap_unlock_entry(wf_cmap_user_at(map, 0), &entry, 0, 1);
    }
    set_patience(PATIENCE, PATIENCE);
    wf_cmap_destroy(map);
}

/* Key sets: distinct keys in the order each was first inserted, a key
 * inserted again answering the index of its first insertion; a set grows
 * past its capacity, a capacity past the first store's limit asks for the
 * limit, and a release, of the set or of its memory alone, gives back every
 * block the set took. */
static int key_is(const wf_key_set *set, uint64_t index, const char *bytes, uint64_t length) {
    uint64_t got;
    const unsigned char *key = wf_cmap_key_set_key(set, index, &got);
    return got == length && (length == 0 || memcmp(key, bytes, (size_t)length) == 0);
}

static void key_sets(void) {
    static const struct {
        const char *bytes;
        uint64_t length;
    } words[] = {{"b", 1}, {"ab", 2}, {"", 0}, {"abc", 3}, {"a", 1}, {"\xff", 1}, {"a\0", 2}, {"abd", 3}, {"B", 1}};
    enum { WORDS = sizeof words / sizeof words[0] };
    int64_t before = atomic_load(&blocks_out);
    wf_key_set set;
    wf_cmap_key_set_new(&set, 0);
    if (set.len != 0 || set.store != NULL)
        fail("a set of no capacity took memory (len, store)", set.len, set.store != NULL);
    for (unsigned i = 0; i < WORDS; i++)
        if (wf_cmap_key_set_insert(&set, (const unsigned char *)words[i].bytes, words[i].length) != i)
            fail("a new key's index is not the set's length (word, index)", i, i);
    if (set.len != WORDS)
        fail("a set lost or repeated a key (len, words)", set.len, WORDS);
    for (unsigned i = 0; i < WORDS; i++)
        if (!key_is(&set, i, words[i].bytes, words[i].length))
            fail("a set's keys are not in insertion order (index, word)", i, i);
    /* "ab" was inserted second, "a" fifth; "zz" is new, once. */
    uint64_t ab = wf_cmap_key_set_insert(&set, (const unsigned char *)"ab", 2);
    uint64_t zz = wf_cmap_key_set_insert(&set, (const unsigned char *)"zz", 2);
    uint64_t zz_again = wf_cmap_key_set_insert(&set, (const unsigned char *)"zz", 2);
    uint64_t a = wf_cmap_key_set_insert(&set, (const unsigned char *)"a", 1);
    uint64_t prefix = wf_cmap_key_set_insert(&set, (const unsigned char *)"a\0", 2);
    if (set.len != WORDS + 1 || ab != 1 || zz != WORDS || zz_again != WORDS || a != 4 || prefix != 6 ||
        !key_is(&set, WORDS, "zz", 2) || !key_is(&set, 0, "b", 1))
        fail("a repeated key did not answer its first index (len, index of ab)", set.len, ab);
    wf_cmap_key_set_release(&set);
    /* A released set's store and arena stay as the thread's spare, two
     * blocks, which the next set reuses whole and a drop gives back. */
    int64_t kept = atomic_load(&blocks_out) - before;
    if (set.len != 0 || set.store != NULL || kept > 2)
        fail("a released set kept more than its spare (len, blocks)", set.len, (uint64_t)kept);
    wf_cmap_key_set_new(&set, 4);
    wf_cmap_key_set_insert(&set, (const unsigned char *)"spare", 5);
    wf_cmap_key_set_insert(&set, (const unsigned char *)"kept", 4);
    if (atomic_load(&blocks_out) - before != kept || set.len != 2 || !key_is(&set, 0, "spare", 5) ||
        !key_is(&set, 1, "kept", 4) || wf_cmap_key_set_insert(&set, (const unsigned char *)"spare", 5) != 0)
        fail("a set built after a released one took memory or lost keys (blocks, len)",
             (uint64_t)(atomic_load(&blocks_out) - before), set.len);
    wf_cmap_key_set_release(&set);
    wf_cmap_key_set_drop_spare();
    if (atomic_load(&blocks_out) != before)
        fail("a dropped spare kept memory (blocks)", (uint64_t)(atomic_load(&blocks_out) - before), 0);
    /* A spare too small for a set is passed over, and the larger store that
     * set leaves replaces it, so the next set of that size takes nothing. */
    wf_cmap_key_set_new(&set, 1);
    wf_cmap_key_set_insert(&set, (const unsigned char *)"a", 1);
    wf_cmap_key_set_release(&set);
    wf_cmap_key_set_new(&set, 10);
    for (unsigned i = 0; i < 10; i++) {
        unsigned char key[2] = {'k', (unsigned char)('0' + i)};
        wf_cmap_key_set_insert(&set, key, 2);
    }
    wf_cmap_key_set_release(&set);
    int64_t larger = atomic_load(&blocks_out);
    wf_cmap_key_set_new(&set, 10);
    for (unsigned i = 0; i < 10; i++) {
        unsigned char key[2] = {'k', (unsigned char)('0' + i)};
        wf_cmap_key_set_insert(&set, key, 2);
    }
    if (atomic_load(&blocks_out) != larger || set.len != 10)
        fail("a set after a larger one was freed took memory (blocks, len)",
             (uint64_t)(atomic_load(&blocks_out) - larger), set.len);
    wf_cmap_key_set_release(&set);
    wf_cmap_key_set_drop_spare();
    /* A small set built in a large spare finds none of the spare's earlier
     * keys: their index slots were cleared, not left to answer old indices. */
    wf_cmap_key_set_new(&set, 100);
    for (unsigned i = 0; i < 100; i++) {
        unsigned char key[3] = {'s', (unsigned char)('0' + i / 10), (unsigned char)('0' + i % 10)};
        wf_cmap_key_set_insert(&set, key, 3);
    }
    wf_cmap_key_set_release(&set);
    wf_cmap_key_set_new(&set, 2);
    uint64_t reused_first = wf_cmap_key_set_insert(&set, (const unsigned char *)"s57", 3);
    uint64_t reused_second = wf_cmap_key_set_insert(&set, (const unsigned char *)"s05", 3);
    uint64_t reused_again = wf_cmap_key_set_insert(&set, (const unsigned char *)"s57", 3);
    if (reused_first != 0 || reused_second != 1 || reused_again != 0 || set.len != 2)
        fail("a set in a reused spare found an earlier set's key (first, second)", reused_first, reused_second);
    wf_cmap_key_set_release(&set);
    wf_cmap_key_set_drop_spare();
    /* A set whose bytes pass the spare's bound is given back whole. */
    static unsigned char long_key[40000];
    memset(long_key, 'x', sizeof long_key);
    wf_cmap_key_set_new(&set, 2);
    long_key[0] = 'a';
    wf_cmap_key_set_insert(&set, long_key, sizeof long_key);
    long_key[0] = 'b';
    wf_cmap_key_set_insert(&set, long_key, sizeof long_key);
    wf_cmap_key_set_release(&set);
    if (atomic_load(&blocks_out) != before)
        fail("a set past the spare's bytes was kept (blocks)", (uint64_t)(atomic_load(&blocks_out) - before), 0);
    /* Many keys of many lengths, inserted in no order, some of them again,
     * against the index each first got. */
    enum { MANY = 1000 };
    static uint64_t first_index[MANY];
    static unsigned char bytes[ENTRY_KEY_BYTES];
    for (unsigned k = 0; k < MANY; k++)
        first_index[k] = UINT64_MAX;
    wf_cmap_key_set_new(&set, 3);
    uint64_t state = 41, distinct = 0;
    for (unsigned i = 0; i < 2 * MANY; i++) {
        unsigned k = (unsigned)(next(&state) % MANY);
        uint64_t length = entry_key(k, bytes);
        uint64_t index = wf_cmap_key_set_insert(&set, bytes, length);
        if (first_index[k] == UINT64_MAX) {
            if (index != distinct)
                fail("a grown set gave a new key another index (index, distinct)", index, distinct);
            first_index[k] = index;
            distinct += 1;
        } else if (index != first_index[k]) {
            fail("a grown set moved a key (index, first)", index, first_index[k]);
        }
    }
    if (set.len != distinct)
        fail("a grown set lost or repeated keys (len, distinct)", set.len, distinct);
    for (unsigned k = 0; k < MANY; k++)
        if (first_index[k] != UINT64_MAX) {
            uint64_t length = entry_key(k, bytes);
            if (!key_is(&set, first_index[k], (const char *)bytes, length))
                fail("a grown set's key is not at its index (key, index)", k, first_index[k]);
        }
    /* Released through its memory alone, as compiled code does, with the
     * spare it may leave. */
    wf_cmap_key_set_free_store(set.store);
    wf_cmap_key_set_free_store(NULL);
    wf_cmap_key_set_drop_spare();
    if (atomic_load(&blocks_out) != before)
        fail("a set released through its memory kept some (blocks)", (uint64_t)(atomic_load(&blocks_out) - before), 0);
    wf_cmap_key_set_new(&set, 1ull << 60);
    if (((key_store *)set.store)->room != KEY_SET_FIRST_LIMIT)
        fail("a capacity past the limit sized another store (room, limit)", ((key_store *)set.store)->room,
             KEY_SET_FIRST_LIMIT);
    wf_cmap_key_set_insert(&set, (const unsigned char *)"k", 1);
    wf_cmap_key_set_release(&set);
    if (atomic_load(&blocks_out) != before)
        fail("a set sized at the limit kept memory (blocks)", (uint64_t)(atomic_load(&blocks_out) - before), 0);
}

/* Writes the width bytes of a tag at offset of a slot. */
static void put_tag(unsigned char *slot, uint64_t offset, uint32_t width, uint64_t tag) {
    uint8_t one = (uint8_t)tag;
    uint16_t two = (uint16_t)tag;
    uint32_t four = (uint32_t)tag;
    const void *bytes = width == 1 ? (const void *)&one : width == 2 ? (const void *)&two
                      : width == 4 ? (const void *)&four : (const void *)&tag;
    memcpy(slot + offset, bytes, width);
}

/* The entry of key in map read as a keyed statement that only reads does:
 * 1 and its slot's first byte when it is present, else 0. */
static int read_present(wf_cmap_user *user, const unsigned char *key, uint64_t length, unsigned char *first) {
    wf_cmap_entry entry;
    const unsigned char *slot = wf_cmap_read_entry(user, key, length, 0, &entry);
    int present = entry.cell != NULL;
    *first = slot[0];
    wf_cmap_unread_entry(user, &entry, 0);
    return present;
}

/* A hold of one key set alone takes its entries in the hold's lock order,
 * not in the set's insertion order: eight keys inserted one way are ranked
 * by held_order, and each position still answers its own key's slot. */
static void holds_sort_a_set(void) {
    wf_cmap *map = wf_cmap_create_entries(16, 8, 0);
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    wf_key_set set;
    wf_cmap_key_set_new(&set, 8);
    unsigned char names[8][2];
    for (unsigned i = 0; i < 8; i++) {
        names[i][0] = 'q';
        names[i][1] = (unsigned char)('7' - i);
        wf_cmap_key_set_insert(&set, names[i], 2);
    }
    wf_cmap_holding hold;
    wf_cmap_hold_begin(&hold, map);
    wf_cmap_hold_keys(&hold, &set);
    wf_cmap_hold_take(user, &hold);
    wf_cmap_held *keys = held_keys(&hold);
    for (uint64_t i = 1; i < hold.count; i++)
        if (held_order(ranked(keys, i - 1), ranked(keys, i)) >= 0)
            fail("a set's hold is not ranked in lock order (rank, count)", i, hold.count);
    for (uint64_t i = 0; i < hold.count; i++)
        if (keys[i].length != 2 || memcmp(keys[i].key, names[i], 2) != 0)
            fail("a set's hold moved a key from its position (position, count)", i, hold.count);
    wf_cmap_hold_release(&hold, 0, 1, 0);
    wf_cmap_key_set_release(&set);
    wf_cmap_key_set_drop_spare();
    wf_cmap_destroy(map);
}

/* A hold's positions answer the slot of the key added at each, single keys
 * and a key set's keys alike, repeated keys sharing an entry, and the
 * release keeps exactly the entries whose tag, read at its width, differs
 * from the None tag, whatever the slot's other bytes hold. A hold of more
 * keys than its own room takes memory that its user keeps for the next. */
static void holds_positions(void) {
    for (uint32_t width = 1; width <= 8; width *= 2) {
        uint64_t offset = width == 1 ? 5 : width == 8 ? 8 : 4, none = 3;
        wf_cmap *map = wf_cmap_create_entries(16, 8, 0);
        wf_cmap_user *user = wf_cmap_user_at(map, 0);
        wf_key_set set;
        wf_cmap_key_set_new(&set, 4);
        wf_cmap_key_set_insert(&set, (const unsigned char *)"k6", 2);
        wf_cmap_key_set_insert(&set, (const unsigned char *)"k2", 2);
        wf_cmap_key_set_insert(&set, (const unsigned char *)"k4", 2);
        wf_cmap_holding hold;
        wf_cmap_hold_begin(&hold, map);
        uint64_t five = wf_cmap_hold_key(&hold, (const unsigned char *)"k5", 2);
        uint64_t keys = wf_cmap_hold_keys(&hold, &set);
        uint64_t two = wf_cmap_hold_key(&hold, (const unsigned char *)"k2", 2);
        uint64_t one = wf_cmap_hold_key(&hold, (const unsigned char *)"k1", 2);
        uint64_t again = wf_cmap_hold_key(&hold, (const unsigned char *)"k5", 2);
        if (five != 0 || keys != 1 || two != 4 || one != 5 || again != 6 || hold.count != 7)
            fail("positions are not the order the keys were added in (set, last)", keys, again);
        if (hold.keys == NULL)
            fail("seven keys stayed in a hold's own room (count, room)", hold.count, WF_CMAP_HOLD_INLINE);
        wf_cmap_hold_take(user, &hold);
        /* The hold locks in its own order whatever order the set's keys
         * were inserted in: its ranks never decrease in held_order. */
        for (uint64_t i = 1; i < hold.count; i++)
            if (held_order(ranked(held_keys(&hold), i - 1), ranked(held_keys(&hold), i)) > 0)
                fail("a hold's ranks are not in its lock order (width, rank)", width, i);
        /* The set's keys keep its insertion order: k6, k2, k4. */
        unsigned char *k5 = wf_cmap_hold_slot(&hold, 0), *k6 = wf_cmap_hold_slot(&hold, 1);
        unsigned char *k2 = wf_cmap_hold_slot(&hold, 2), *k4 = wf_cmap_hold_slot(&hold, 3);
        unsigned char *k1 = wf_cmap_hold_slot(&hold, 5);
        if (wf_cmap_hold_slot(&hold, 4) != k2 || wf_cmap_hold_slot(&hold, 6) != k5)
            fail("a repeated key did not share its entry (width, position)", width, 4);
        unsigned char *distinct[] = {k1, k2, k4, k5, k6};
        for (unsigned i = 0; i < 5; i++)
            for (unsigned j = 0; j < i; j++)
                if (distinct[i] == distinct[j])
                    fail("two keys share a slot (width, key)", width, i);
        memset(k1, 0, 16);
        put_tag(k1, offset, width, none);
        memset(k4, 0xee, 16);
        put_tag(k4, offset, width, none);
        memset(k2, 0, 16);
        put_tag(k2, offset, width, none + 1);
        k2[0] = 22;
        memset(k5, 0, 16);
        put_tag(k5, offset, width, 0);
        k5[0] = 55;
        /* Equal to None in its low byte, and different at the width. */
        memset(k6, 0, 16);
        put_tag(k6, offset, width, width == 1 ? none + 2 : none | 1ull << (8 * width - 8));
        k6[0] = 66;
        wf_cmap_hold_release(&hold, offset, width, none);
        static const char *const names[] = {"k1", "k2", "k4", "k5", "k6"};
        static const unsigned char kept[] = {0, 22, 0, 55, 66};
        for (unsigned i = 0; i < 5; i++) {
            unsigned char first;
            if (read_present(user, (const unsigned char *)names[i], 2, &first) != (kept[i] != 0) ||
                (kept[i] != 0 && first != kept[i]))
                fail("a release kept or removed an entry against its tag (width, key)", width, i);
        }
        if (wf_cmap_count(map) != 3)
            fail("a release counted its entries wrong (width, count)", width, wf_cmap_count(map));
        if (user->spare_keys == NULL)
            fail("a released hold's memory was not kept for the next (width, room)", width, user->spare_room);
        wf_cmap_hold_begin(&hold, map);
        for (unsigned i = 0; i < 5; i++)
            wf_cmap_hold_key(&hold, (const unsigned char *)"k1", 2);
        if (user->spare_keys != NULL || hold.keys == NULL)
            fail("a hold past its own room did not take its user's spare memory (width, count)", width, hold.count);
        wf_cmap_hold_take(user, &hold);
        put_tag(wf_cmap_hold_slot(&hold, 3), offset, width, none);
        wf_cmap_hold_release(&hold, offset, width, none);
        unsigned char first;
        if (read_present(user, (const unsigned char *)"k1", 2, &first))
            fail("a hold of one key added five times kept it against its tag (width)", width, 0);
        wf_cmap_key_set_release(&set);
        wf_cmap_destroy(map);
    }
}

/* Statements holding entries of two maps at once, the first map's before
 * the second's: movers move amounts between accounts of the two, keyed
 * statements count on either, and an audit holds every account of both, by
 * keys or each map whole. The sum over both maps equals the count of keyed
 * statements in every audit and at the end. Patience never runs out where
 * hashes are whole, so holds taken out of order would wait for each other
 * until the alarm. Movers and counters run their counts and then on until
 * the audit has held both maps both ways, by keys and whole, so every
 * observation the audit owes is made while statements still run, whatever
 * the scheduler gives the audit's thread. */
enum { PAIR_ACCOUNTS = 16, PAIR_MOVES = 3000, PAIR_COUNTS = 6000, PAIR_AUDITS = 2 };

typedef struct {
    wf_cmap *maps[2];
    unsigned index;
    _Atomic uint64_t *total;
    _Atomic int *stop;
    _Atomic uint64_t *audits;
    uint64_t checks;
} across_t;

static void *move_across(void *arg) {
    across_t *p = arg;
    test_driver = p->index;
    wf_cmap_user *users[2] = {wf_cmap_user_at(p->maps[0], p->index), wf_cmap_user_at(p->maps[1], p->index)};
    unsigned char bytes[2][3][16];
    wf_cmap_holding holds[2];
    uint64_t state = mix64(p->index + 77);
    for (uint64_t i = 0; i < PAIR_MOVES || atomic_load(p->audits) < PAIR_AUDITS; i++) {
        unsigned counts[2] = {1 + (unsigned)(next(&state) % 3), 1 + (unsigned)(next(&state) % 3)};
        int forward = (int)(next(&state) & 1);
        for (unsigned t = 0; t < 2; t++) {
            wf_cmap_hold_begin(&holds[t], p->maps[t]);
            for (unsigned j = 0; j < counts[t]; j++)
                wf_cmap_hold_key(&holds[t], bytes[t][j], counted_key(next(&state) % PAIR_ACCOUNTS, bytes[t][j]));
        }
        wf_cmap_hold_take(users[0], &holds[0]);
        wf_cmap_hold_take(users[1], &holds[1]);
        /* Each of the giving map's keys gives one unit per key of the
         * other, which takes one per key of the giver. */
        for (unsigned t = 0; t < 2; t++)
            for (unsigned j = 0; j < counts[t]; j++) {
                uint64_t *slot = wf_cmap_hold_slot(&holds[t], j);
                uint64_t units = counts[1 - t];
                *slot += (t == 0) == forward ? (uint64_t)0 - units : units;
            }
        wf_cmap_hold_release(&holds[1], VALUE_TAG);
        wf_cmap_hold_release(&holds[0], VALUE_TAG);
    }
    return NULL;
}

static void *count_across(void *arg) {
    across_t *p = arg;
    test_driver = p->index;
    unsigned char bytes[16];
    uint64_t state = mix64(p->index + 13);
    for (uint64_t i = 0; i < PAIR_COUNTS || atomic_load(p->audits) < PAIR_AUDITS; i++) {
        wf_cmap *map = p->maps[next(&state) & 1];
        wf_cmap_user *user = wf_cmap_user_at(map, p->index);
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, bytes, counted_key(next(&state) % PAIR_ACCOUNTS, bytes), 0, &entry);
        atomic_fetch_add(p->total, 1);
        slot[0] += 1;
        wf_cmap_unlock_entry(user, &entry, 0, slot[0] != 0);
    }
    return NULL;
}

static void *audit_across(void *arg) {
    across_t *p = arg;
    test_driver = p->index;
    static unsigned char bytes[PAIR_ACCOUNTS][16];
    wf_cmap_holding holds[2];
    while (!atomic_load(p->stop)) {
        uint64_t sum = 0;
        for (unsigned t = 0; t < 2; t++) {
            wf_cmap_hold_begin(&holds[t], p->maps[t]);
            if (p->checks % 2 == 1)
                wf_cmap_hold_whole(&holds[t]);
            for (unsigned k = 0; k < PAIR_ACCOUNTS; k++)
                wf_cmap_hold_key(&holds[t], bytes[k], counted_key(k, bytes[k]));
        }
        wf_cmap_hold_take(wf_cmap_user_at(p->maps[0], p->index), &holds[0]);
        wf_cmap_hold_take(wf_cmap_user_at(p->maps[1], p->index), &holds[1]);
        uint64_t total = atomic_load(p->total);
        for (unsigned t = 0; t < 2; t++)
            for (unsigned k = 0; k < PAIR_ACCOUNTS; k++)
                sum += *(uint64_t *)wf_cmap_hold_slot(&holds[t], k);
        wf_cmap_hold_release(&holds[1], VALUE_TAG);
        wf_cmap_hold_release(&holds[0], VALUE_TAG);
        if (sum != total)
            fail("an audit of two maps saw a statement half done (sum, total)", sum, total);
        p->checks++;
        atomic_store(p->audits, p->checks);
    }
    return NULL;
}

static void holds_across_maps(void) {
    enum { MOVERS = 2, COUNTERS = 2 };
    wf_cmap *maps[2] = {wf_cmap_create_entries(8, 8, 0), wf_cmap_create_entries(8, 8, 0)};
    set_patience(SHARED_HASHES ? PATIENCE : UINT64_MAX, SHARED_HASHES ? PATIENCE : UINT64_MAX);
    _Atomic uint64_t total = 0;
    _Atomic int stop = 0;
    _Atomic uint64_t audits = 0;
    pthread_t t[MOVERS + COUNTERS + 1];
    across_t p[MOVERS + COUNTERS + 1];
    for (unsigned i = 0; i <= MOVERS + COUNTERS; i++) {
        p[i] = (across_t){{maps[0], maps[1]}, i, &total, &stop, &audits, 0};
        pthread_create(&t[i], NULL, i < MOVERS ? move_across : i < MOVERS + COUNTERS ? count_across : audit_across, &p[i]);
    }
    for (unsigned i = 0; i < MOVERS + COUNTERS; i++)
        pthread_join(t[i], NULL);
    atomic_store(&stop, 1);
    pthread_join(t[MOVERS + COUNTERS], NULL);
    if (p[MOVERS + COUNTERS].checks < PAIR_AUDITS)
        fail("the audit never held both maps both ways", p[MOVERS + COUNTERS].checks, PAIR_AUDITS);
    uint64_t sum = 0;
    for (unsigned m = 0; m < 2; m++) {
        for (uint64_t *slot; (slot = wf_cmap_drain(maps[m])) != NULL;)
            sum += slot[0];
        wf_cmap_destroy(maps[m]);
    }
    if (atomic_load(&total) < COUNTERS * PAIR_COUNTS)
        fail("the counters ran fewer statements than their counts (counted, expected)", atomic_load(&total),
             COUNTERS * PAIR_COUNTS);
    if (sum != atomic_load(&total))
        fail("a change was lost across two maps (sum, counted)", sum, atomic_load(&total));
    set_patience(PATIENCE, PATIENCE);
}

/* A hold of the whole map waits out a statement holding an entry, keeps new
 * ones out until its release, and still reaches its added keys' slots,
 * creating the absent ones; the map held whole counts its entries exactly,
 * and the release keeps and removes the hold's entries by their tags. With
 * no key added it only holds the map, as a statement that counts does. */
static wf_cmap *whole_map;
static unsigned char whole_keys[12][16];
static _Atomic int entry_holding, entry_release, whole_taken, beside_started, beside_done;

static void *hold_one_entry(void *arg) {
    (void)arg;
    test_driver = 1;
    wf_cmap_user *user = wf_cmap_user_at(whole_map, 1);
    wf_cmap_entry entry;
    wf_cmap_lock_entry(user, whole_keys[1], 12, 0, &entry);
    atomic_store(&entry_holding, 1);
    while (!atomic_load(&entry_release))
        sched_yield();
    wf_cmap_unlock_entry(user, &entry, 0, 1);
    return NULL;
}

static void *take_whole(void *arg) {
    test_driver = 2;
    wf_cmap_hold_take(wf_cmap_user_at(whole_map, 2), arg);
    atomic_store(&whole_taken, 1);
    return NULL;
}

static void *lock_beside(void *arg) {
    (void)arg;
    test_driver = 3;
    wf_cmap_user *user = wf_cmap_user_at(whole_map, 3);
    wf_cmap_entry entry;
    atomic_store(&beside_started, 1);
    uint64_t *slot = wf_cmap_lock_entry(user, whole_keys[5], 12, 0, &entry);
    slot[0] += 1;
    wf_cmap_unlock_entry(user, &entry, 0, 1);
    atomic_store(&beside_done, 1);
    return NULL;
}

/* Waits up to a second for flag, and fails with what when it stays clear. */
static void wait_for(_Atomic int *flag, const char *what) {
    struct timespec pause = {0, 100000};
    for (unsigned waited = 0; !atomic_load(flag); waited++) {
        if (waited == 10000)
            fail(what, 0, 0);
        nanosleep(&pause, NULL);
    }
}

static void holds_whole(void) {
    struct timespec settle = {0, 2000000};
    whole_map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_user *user = wf_cmap_user_at(whole_map, 0);
    for (unsigned k = 0; k < 12; k++)
        counted_key(k, whole_keys[k]);
    for (unsigned k = 0; k < 10; k++) {
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, whole_keys[k], 12, 0, &entry);
        slot[0] = k + 1;
        wf_cmap_unlock_entry(user, &entry, 0, 1);
    }
    atomic_store(&entry_holding, 0);
    atomic_store(&entry_release, 0);
    atomic_store(&whole_taken, 0);
    atomic_store(&beside_started, 0);
    atomic_store(&beside_done, 0);
    atomic_store(&closed_seen[2], 0);
    pthread_t holder, taker, beside;
    pthread_create(&holder, NULL, hold_one_entry, NULL);
    wait_for(&entry_holding, "the statement on an entry never locked it");
    wf_cmap_holding hold;
    wf_cmap_hold_begin(&hold, whole_map);
    wf_cmap_hold_whole(&hold);
    wf_cmap_hold_key(&hold, whole_keys[1], 12);
    wf_cmap_hold_key(&hold, whole_keys[11], 12);
    pthread_create(&taker, NULL, take_whole, &hold);
    wait_for(&closed_seen[2], "a hold of the whole map never closed the gate");
    nanosleep(&settle, NULL);
    if (atomic_load(&whole_taken))
        fail("a hold of the whole map ran beside a statement holding an entry", 0, 0);
    atomic_store(&entry_release, 1);
    pthread_join(holder, NULL);
    pthread_join(taker, NULL);
    if (!hold.whole || hold.held)
        fail("a hold asked for the whole map did not take it itself (whole, held)", hold.whole, hold.held);
    pthread_create(&beside, NULL, lock_beside, NULL);
    wait_for(&beside_started, "the statement beside the hold never began");
    nanosleep(&settle, NULL);
    if (atomic_load(&beside_done))
        fail("a statement on an entry ran while the map was held whole", 0, 0);
    uint64_t *one = wf_cmap_hold_slot(&hold, 0), *eleven = wf_cmap_hold_slot(&hold, 1);
    if (one[0] != 2 || eleven[0] != 0 || !held_keys(&hold)[1].fresh)
        fail("a hold of the whole map did not reach its keys' entries (present, absent)", one[0], eleven[0]);
    if (wf_cmap_count(whole_map) != 10)
        fail("a map held whole miscounted its entries (count, entries)", wf_cmap_count(whole_map), 10);
    one[0] = 0;
    eleven[0] = 42;
    if (!wf_cmap_hold_release(&hold, VALUE_TAG))
        fail("a release of held entries said it wrote nothing", 0, 0);
    pthread_join(beside, NULL);
    unsigned char first;
    if (read_present(user, whole_keys[1], 12, &first) || !read_present(user, whole_keys[11], 12, &first) ||
        first != 42)
        fail("a release under the whole map kept or removed an entry against its tag", first, 0);
    if (wf_cmap_count(whole_map) != 10)
        fail("a whole hold's release counted its entries wrong (count, entries)", wf_cmap_count(whole_map), 10);
    wf_cmap_hold_begin(&hold, whole_map);
    wf_cmap_hold_whole(&hold);
    wf_cmap_hold_take(user, &hold);
    if (!hold.whole || wf_cmap_count(whole_map) != 10)
        fail("a hold of the whole map with no key miscounted (whole, count)", hold.whole, wf_cmap_count(whole_map));
    if (wf_cmap_hold_release(&hold, VALUE_TAG))
        fail("a hold of the whole map with no key said it wrote", 0, 0);
    wf_cmap_destroy(whole_map);
}

/* A statement whose release starts a move stays inside the map until the
 * move has ended, so a hold of the whole map, which may then swap the map's
 * entries, waits for it rather than closing while the move is under way. */
static _Atomic int moved_hold_done;

static void *insert_until_move(void *arg) {
    wf_cmap *map = arg;
    test_driver = 1;
    wf_cmap_user *user = wf_cmap_user_at(map, 1);
    unsigned char bytes[16];
    table *first = atomic_load(&map->current);
    for (uint64_t k = 0; atomic_load(&map->current) == first; k++) {
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, bytes, counted_key(k, bytes), 0, &entry);
        slot[0] = 1;
        wf_cmap_unlock_entry(user, &entry, 0, 1);
    }
    return NULL;
}

static void *hold_after_move(void *arg) {
    wf_cmap *map = arg;
    test_driver = 2;
    wf_cmap_user *user = wf_cmap_user_at(map, 2);
    wf_cmap_hold(user);
    atomic_store(&moved_hold_done, 1);
    wf_cmap_unhold(user);
    return NULL;
}

static void holds_wait_out_moves(void) {
    struct timespec settle = {0, 2000000};
    wf_cmap *map = wf_cmap_create_entries(8, 8, 1);
    pause_map = map;
    atomic_store(&pause_reached, 0);
    atomic_store(&pause_released, 0);
    atomic_store(&moved_hold_done, 0);
    atomic_store(&closed_seen[2], 0);
    atomic_store(&pause_armed, 1);
    pthread_t mover, holder;
    pthread_create(&mover, NULL, insert_until_move, map);
    wait_for(&pause_reached, "no release started a move");
    pthread_create(&holder, NULL, hold_after_move, map);
    wait_for(&closed_seen[2], "a hold of the whole map never closed the gate");
    nanosleep(&settle, NULL);
    if (atomic_load(&moved_hold_done))
        fail("a hold of the whole map closed while a statement's move was under way", 0, 0);
    atomic_store(&pause_released, 1);
    pthread_join(mover, NULL);
    pthread_join(holder, NULL);
    pause_map = NULL;
    wf_cmap_destroy(map);
}

/* Swapping a held map's entries with another map's: each map then has the
 * other's entries, count and memory, which drain and free as any map's; a
 * statement waiting at the held map's gate during the swap reaches the
 * entries the map has after it; a hold of the held map's entries taken
 * before a swap leaves them to the other map; and the guards' watches stay
 * with each map. */
static wf_cmap *swap_map;
static _Atomic int swap_waited;

static void *lock_after_swap(void *arg) {
    (void)arg;
    test_driver = 1;
    wf_cmap_user *user = wf_cmap_user_at(swap_map, 1);
    unsigned char bytes[16];
    wf_cmap_entry entry;
    uint64_t *slot = wf_cmap_lock_entry(user, bytes, counted_key(0, bytes), 0, &entry);
    atomic_store(&swap_waited, (int)entry.fresh + 1);
    slot[0] = 77;
    wf_cmap_unlock_entry(user, &entry, 0, 1);
    return NULL;
}

static void put_counted(wf_cmap *map, unsigned user_index, uint64_t k, uint64_t value) {
    unsigned char bytes[16];
    wf_cmap_entry entry;
    wf_cmap_user *user = wf_cmap_user_at(map, user_index);
    uint64_t *slot = wf_cmap_lock_entry(user, bytes, counted_key(k, bytes), 0, &entry);
    slot[0] = value;
    wf_cmap_unlock_entry(user, &entry, 0, value != 0);
}

static uint64_t counted_value(wf_cmap *map, uint64_t k) {
    unsigned char bytes[16];
    wf_cmap_entry entry;
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    const uint64_t *slot = wf_cmap_read_entry(user, bytes, counted_key(k, bytes), 0, &entry);
    uint64_t value = entry.cell != NULL ? slot[0] : 0;
    wf_cmap_unread_entry(user, &entry, 0);
    return value;
}

static uint64_t drain_sum(wf_cmap *map, uint64_t *count) {
    uint64_t sum = 0;
    *count = 0;
    for (uint64_t *slot; (slot = wf_cmap_drain(map)) != NULL; ++*count)
        sum += slot[0];
    return sum;
}

static void maps_swap(void) {
    swap_map = wf_cmap_create_entries(8, 8, 0);
    wf_cmap *other = wf_cmap_create_entries(8, 8, 0);
    for (uint64_t k = 0; k < 5; k++)
        put_counted(swap_map, 0, k, 10 + k);
    put_counted(other, 0, 100, 200);
    put_counted(other, 0, 101, 201);
    wf_cmap_holding hold;
    wf_cmap_hold_begin(&hold, swap_map);
    wf_cmap_hold_whole(&hold);
    wf_cmap_hold_take(wf_cmap_user_at(swap_map, 0), &hold);
    atomic_store(&swap_waited, 0);
    pthread_t waiter;
    pthread_create(&waiter, NULL, lock_after_swap, NULL);
    struct timespec pause = {0, 100000};
    for (unsigned waited = 0; atomic_load(&swap_map->waiting) == 0; waited++) {
        if (waited == 10000)
            fail("a statement never waited at a held map's gate", 0, 0);
        nanosleep(&pause, NULL);
    }
    wf_cmap_swap(swap_map, other, VALUE_TAG);
    if (!wf_cmap_hold_release(&hold, VALUE_TAG))
        fail("a hold of the whole map that swapped its entries said it wrote nothing", 0, 0);
    pthread_join(waiter, NULL);
    if (atomic_load(&swap_waited) != 2 || counted_value(swap_map, 0) != 77)
        fail("a statement that waited out a swap missed the map's new entries (fresh, value)",
             (uint64_t)atomic_load(&swap_waited) - 1, counted_value(swap_map, 0));
    if (wf_cmap_count(swap_map) != 3 || counted_value(swap_map, 100) != 200 || counted_value(swap_map, 101) != 201)
        fail("a swap did not give the map the other's entries (count, value)", wf_cmap_count(swap_map),
             counted_value(swap_map, 100));
    if (wf_cmap_count(other) != 5)
        fail("a swap did not give the other map the held one's entries (count)", wf_cmap_count(other), 5);
    uint64_t drained, sum = drain_sum(other, &drained);
    if (drained != 5 || sum != 10 + 11 + 12 + 13 + 14)
        fail("a swapped-out map did not drain its entries (drained, sum)", drained, sum);
    wf_cmap_destroy(other);
    /* A hold taken before a swap */
    other = wf_cmap_create_entries(8, 8, 0);
    wf_cmap_hold_begin(&hold, swap_map);
    wf_cmap_hold_whole(&hold);
    unsigned char bytes[2][16];
    wf_cmap_hold_key(&hold, bytes[0], counted_key(100, bytes[0]));
    wf_cmap_hold_key(&hold, bytes[1], counted_key(102, bytes[1]));
    wf_cmap_hold_take(wf_cmap_user_at(swap_map, 0), &hold);
    *(uint64_t *)wf_cmap_hold_slot(&hold, 0) = 0;
    *(uint64_t *)wf_cmap_hold_slot(&hold, 1) = 9;
    wf_cmap_swap(swap_map, other, VALUE_TAG);
    /* The swap settled the hold's entries: none of the other map's cells is
     * left locked, which a later hold of the same keys would wait on for
     * ever once the entries come back. */
    {
        table *moved = atomic_load(&other->current);
        for (uint64_t i = 0; i < moved->capacity; i++)
            if (atomic_load(&moved->cells[i].key) & LOCKED)
                fail("a swap left a hold's cell locked in the other map (cell)", i, 0);
    }
    if (!wf_cmap_hold_release(&hold, VALUE_TAG))
        fail("a hold whose map was swapped said it wrote nothing", 0, 0);
    if (wf_cmap_count(swap_map) != 0 || counted_value(swap_map, 100) != 0)
        fail("a hold taken before a swap left entries in the map (count)", wf_cmap_count(swap_map), 0);
    /* Swapped back, the entries the hold settled are lockable again. */
    wf_cmap_swap(swap_map, other, VALUE_TAG);
    if (counted_value(swap_map, 102) != 9 || counted_value(swap_map, 100) != 0)
        fail("entries a swap settled are not as the hold left them (kept, removed)", counted_value(swap_map, 102),
             counted_value(swap_map, 100));
    wf_cmap_swap(swap_map, other, VALUE_TAG);
    sum = drain_sum(other, &drained);
    if (drained != 3 || sum != 9 + 201 + 77)
        fail("a hold taken before a swap did not leave its entries to the other map (drained, sum)", drained, sum);
    wf_cmap_destroy(other);
    /* The watches stay with the map */
    other = wf_cmap_create_entries(8, 8, 0);
    swap_map->watch.count = 1;
    wf_cmap_swap(swap_map, other, VALUE_TAG);
    if (swap_map->watch.count != 1 || other->watch.count != 0)
        fail("a swap moved a map's watches (kept, moved)", swap_map->watch.count, other->watch.count);
    swap_map->watch.count = 0;
    wf_cmap_destroy(other);
    /* A map swapped with itself keeps its entries */
    put_counted(swap_map, 0, 7, 70);
    wf_cmap_swap(swap_map, swap_map, VALUE_TAG);
    if (wf_cmap_count(swap_map) != 1 || counted_value(swap_map, 7) != 70)
        fail("a map swapped with itself lost its entries (count, value)", wf_cmap_count(swap_map),
             counted_value(swap_map, 7));
    wf_cmap_destroy(swap_map);
}

/* The keyed tables' functions (keyed_table.c): a table no guard watches
 * never enters a write's slow path; with a watch registered, a statement
 * that may have written enters it as it ends, and one that only read, or
 * held the table whole and only counted, does not; a whole hold that
 * swapped does. Each also does what the map's own function does. */
static void tables_wake_writers(void) {
    test_driver = 0;
    void *table = wf_cmap_create_entries(16, 8, 0);
    wf_table_entry entry;
    wf_cmap_holding hold;
    wf_key_set set;
    unsigned char watch[WF_WATCH_SIZE];
    atomic_store(&written_calls, 0);
    for (int watched = 0; watched < 2; watched++) {
        uint64_t before = atomic_load(&written_calls);
        uint64_t *slot = wf__table_lock_entry(table, (const unsigned char *)"key", 3, 0, &entry);
        slot[0] = 5;
        wf__table_unlock_entry(&entry, 1);
        if (atomic_load(&written_calls) != before + (uint64_t)watched)
            fail("a write's end disagrees with the table's watches (watched, wakes)", (uint64_t)watched,
                 atomic_load(&written_calls) - before);
        const uint64_t *read = wf__table_lock_entry(table, (const unsigned char *)"key", 3, 1, &entry);
        if (read[0] != 5)
            fail("a read of an entry missed its value", read[0], 5);
        wf__table_unlock_entry(&entry, 0);
        if (atomic_load(&written_calls) != before + (uint64_t)watched)
            fail("a read's end woke a table's watches (watched)", (uint64_t)watched, 0);
        wf__key_set_new(&set, 2);
        wf__key_set_insert(&set, (const unsigned char *)"two", 3);
        wf__key_set_insert(&set, (const unsigned char *)"one", 3);
        wf__table_hold_begin(&hold, table);
        if (wf__table_hold_keys(&hold, &set) != 0 || wf__table_hold_key(&hold, (const unsigned char *)"key", 3) != 2)
            fail("a hold's positions disagree with its keys", 0, 0);
        wf__table_hold_take(&hold);
        for (uint64_t i = 0; i < 2; i++)
            *(uint64_t *)wf__table_hold_slot(&hold, i) = (i == 0 ? 2u : 1u) + 10 * (uint64_t)watched;
        wf__table_hold_release(&hold, VALUE_TAG);
        wf__key_set_free(set.store);
        if (atomic_load(&written_calls) != before + 2 * (uint64_t)watched)
            fail("a hold's release disagrees with the table's watches (watched)", (uint64_t)watched, 0);
        wf__table_hold_begin(&hold, table);
        wf__table_hold_whole(&hold);
        wf__table_hold_take(&hold);
        if (wf__keyed_table_count(table, VALUE_TAG) != 3)
            fail("a table held whole miscounted (count, entries)", wf__keyed_table_count(table, VALUE_TAG), 3);
        wf__table_hold_release(&hold, VALUE_TAG);

        if (atomic_load(&written_calls) != before + 2 * (uint64_t)watched)
            fail("a whole hold that only counted woke a table's watches (watched)", (uint64_t)watched, 0);
        if (!watched)
            wf__watch_table(watch, table);
    }
    /* Held whole with keys, a count sees the statement's own writes: a
     * fresh key given a value counts, and a present key emptied does
     * not. */
    wf__table_hold_begin(&hold, table);
    if (wf__table_hold_key(&hold, (const unsigned char *)"new", 3) != 0 ||
        wf__table_hold_key(&hold, (const unsigned char *)"key", 3) != 1)
        fail("a whole hold's positions disagree with its keys", 0, 0);
    wf__table_hold_whole(&hold);
    wf__table_hold_take(&hold);
    *(uint64_t *)wf__table_hold_slot(&hold, 0) = 4;
    if (wf__keyed_table_count(table, VALUE_TAG) != 4)
        fail("a count under a whole hold missed the hold's new entry (count)", wf__keyed_table_count(table, VALUE_TAG), 4);
    *(uint64_t *)wf__table_hold_slot(&hold, 1) = 0;
    if (wf__keyed_table_count(table, VALUE_TAG) != 3)
        fail("a count under a whole hold kept the hold's emptied entry (count)", wf__keyed_table_count(table, VALUE_TAG), 3);
    *(uint64_t *)wf__table_hold_slot(&hold, 0) = 0;
    *(uint64_t *)wf__table_hold_slot(&hold, 1) = 5;
    wf__table_hold_release(&hold, VALUE_TAG);
    void *fresh = wf_cmap_create_entries(16, 8, 0);
    uint64_t before = atomic_load(&written_calls);
    wf__table_hold_begin(&hold, table);
    wf__table_hold_whole(&hold);
    wf__table_hold_take(&hold);
    wf__keyed_table_swap(table, fresh, VALUE_TAG);
    wf__table_hold_release(&hold, VALUE_TAG);
    if (atomic_load(&written_calls) != before + 1 || wf__keyed_table_count(table, VALUE_TAG) != 0 || wf__keyed_table_count(fresh, VALUE_TAG) != 3)
        fail("a whole hold that swapped did not wake the watches, or the swap moved no entries (wakes, count)",
             atomic_load(&written_calls) - before, wf__keyed_table_count(fresh, VALUE_TAG));
    uint64_t drained = 0;
    while (wf__keyed_table_drain(fresh) != NULL)
        drained++;
    if (drained != 3)
        fail("a swapped table did not drain its entries (drained)", drained, 3);
    wf__keyed_table_free(fresh);
    ((wf_cmap *)table)->watch.count = 0;
    wf__keyed_table_free(table);
}

/* A capacity past what any table can hold sizes the first table at the
 * limit, and the map works. */
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

/* Whole holds accept block-computed keys. Absent reads allocate nothing;
 * writes survive index growth and are counted before and after release. */
static void tables_held_selection(void) {
    wf_cmap_key_set_drop_spare();
    int64_t before = atomic_load(&blocks_out);
    wf_cmap *map = wf_cmap_create_entries(16, 8, 1);
    const unsigned char absent[] = "absent";
    wf_cmap_holding hold;
    wf__table_hold_begin(&hold, map);
    wf__table_hold_whole(&hold);
    wf__table_hold_take(&hold);
    uint64_t *none;
    uint64_t takes;
    takes = atomic_load(&allocations);
    for (unsigned i = 0; i < 1000; i++) {
        none = wf__table_held_entry(map, absent, 6, 0);
        if (none != NULL) fail("an absent whole-held read was not None", 1, i);
    }
    if (atomic_load(&allocations) != takes || hold.count != 0)
        fail("absent whole-held reads materialized cells", atomic_load(&allocations) - takes, hold.count);
    unsigned char key[12];
    for (unsigned i = 0; i < 256; i++) {
        uint64_t *slot = wf__table_held_entry(map, key, counted_key(i, key), 1);
        slot[0] = 1;
        slot[1] = i + 10;
    }
    if (wf__keyed_table_count(map, 0, 4, 0) != 256)
        fail("whole-held writes did not count through growth", wf__keyed_table_count(map, 0, 4, 0), 256);
    for (unsigned i = 0; i < 256; i++) {
        uint64_t *slot = wf__table_held_entry(map, key, counted_key(i, key), 0);
        if (slot[0] != 1 || slot[1] != i + 10)
            fail("whole-held growth lost a value", slot[1], i + 10);
    }
    wf_key_set set;
    wf__key_set_new(&set, 2);
    wf__key_set_insert(&set, key, counted_key(300, key));
    wf__key_set_insert(&set, key, counted_key(301, key));
    uint64_t entries[3];
    wf__table_held_entries(map, &set, entries, NULL);
    wf_cmap_holding *selected = (wf_cmap_holding *)(uintptr_t)entries[0];
    uint64_t *first = wf_cmap_hold_slot(selected, entries[1]);
    uint64_t *second = wf_cmap_hold_slot(selected, entries[1] + 1);
    if (entries[2] != 2 || first == second || first[0] != 0 || second[0] != 0)
        fail("in-block set selection had wrong positions or initial values", entries[2], first == second);
    first[0] = 1;
    first[1] = 99;
    second[0] = 0;
    if (wf__keyed_table_count(map, 0, 4, 0) != 257)
        fail("in-block set writes were not counted", wf__keyed_table_count(map, 0, 4, 0), 257);
    wf__table_hold_release(&hold, 0, 4, 0);
    wf__key_set_free(set.store);
    if (wf__keyed_table_count(map, 0, 4, 0) != 257)
        fail("whole-held release changed the count", wf__keyed_table_count(map, 0, 4, 0), 257);
    wf__table_hold_begin(&hold, map);
    wf__table_hold_whole(&hold);
    wf__table_hold_take(&hold);
    uint64_t *slot = wf__table_held_entry(map, key, counted_key(400, key), 1);
    slot[0] = 1;
    slot[1] = 44;
    if (wf__keyed_table_count(map, 0, 4, 0) != 258)
        fail("whole-held writes were not counted", wf__keyed_table_count(map, 0, 4, 0), 258);
    wf__table_hold_release(&hold, 0, 4, 0);
    wf__table_hold_begin(&hold, map);
    wf__table_hold_whole(&hold);
    wf__table_hold_take(&hold);
    slot = wf__table_held_entry(map, key, counted_key(400, key), 0);
    if (slot[0] != 1 || slot[1] != 44)
        fail("a write did not survive a later whole hold", slot[1], 44);
    wf__table_hold_release(&hold, 0, 4, 0);
    wf__keyed_table_free(map);
    wf_cmap_key_set_drop_spare();
    if (atomic_load(&blocks_out) != before)
        fail("whole-held selection leaked blocks", atomic_load(&blocks_out), before);
}

/* A read has no write side effects, before and after renewing a whole hold.
 * Snapshot the map, concurrent index and shared descriptor independently of
 * the read's value; a descriptor publication would fail even on an absent key. */
static void tables_read_selection(void) {
    wf_cmap_key_set_drop_spare();
    int64_t before = atomic_load(&blocks_out);
    wf_cmap *map = wf_cmap_create_entries(16, 8, 1);
    const unsigned char present[] = "present", absent[] = "absent";
    wf_key_set set;
    wf__key_set_new(&set, 2);
    wf__key_set_insert(&set, present, 7);
    wf__key_set_insert(&set, absent, 6);
    wf_cmap_holding outer;
    wf__table_hold_begin(&outer, map);
    wf__table_hold_whole(&outer);
    wf__table_hold_take(&outer);
    uint64_t *slot = wf__table_held_entry(map, present, 7, 1);
    slot[0] = 1;
    slot[1] = 7;
    for (unsigned retained = 0; retained < 2; retained++) {
        if (retained == 1) {
            wf__table_hold_release(&outer, 0, 4, 0);
            wf__table_hold_begin(&outer, map);
            wf__table_hold_whole(&outer);
            wf__table_hold_take(&outer);
        }
        unsigned char map_before[sizeof *map];
        memcpy(map_before, map, sizeof *map);
        table *index = atomic_load(&map->current);
        size_t bytes = (size_t)index->capacity * sizeof(cell);
        void *cells_before = malloc(bytes);
        if (cells_before == NULL) abort();
        memcpy(cells_before, index->cells, bytes);
        wf_cmap_holding hold_before;
        if (map->whole_hold != NULL) memcpy(&hold_before, map->whole_hold, sizeof hold_before);
        uint64_t takes = atomic_load(&allocations);
        for (unsigned i = 0; i < 100; i++) {
            slot = wf__table_held_entry(map, present, 7, 0);
            if (slot == NULL || slot[0] != 1 || slot[1] != 7)
                fail("a read selection lost a present value", slot == NULL, i);
            if (wf__table_held_entry(map, absent, 6, 0) != NULL)
                fail("an absent read selection returned a cell", 1, i);
            wf_cmap_holding selection;
            uint64_t entries[3];
            wf__table_held_entries(map, &set, entries, &selection);
            if (entries[1] != 0 || entries[2] != 2)
                fail("read-only entries have the wrong extent", entries[1], entries[2]);
            for (uint64_t k = 0; k < set.len; k++) {
                uint64_t length;
                const unsigned char *key = wf_cmap_key_set_key(&set, k, &length);
                slot = wf__table_hold_slot((void *)(uintptr_t)entries[0], k);
                int found = length == 7 && memcmp(key, present, 7) == 0;
                if (slot[0] != (uint64_t)found || (found && slot[1] != 7))
                    fail("read-only entries select the wrong slot", slot[0], found);
                if (!found && slot != map->none)
                    fail("an absent read-only entry is not shared None", 1, k);
            }
        }
        if (memcmp(map_before, map, sizeof *map) != 0 || memcmp(cells_before, index->cells, bytes) != 0 ||
            (map->whole_hold != NULL && memcmp(&hold_before, map->whole_hold, sizeof hold_before) != 0) ||
            atomic_load(&allocations) != takes)
            fail("read selections changed a map, index, shared hold or allocation count", retained, 0);
        free(cells_before);
    }
    wf__table_hold_release(&outer, 0, 4, 0);
    wf__key_set_free(set.store);
    wf__keyed_table_free(map);
    wf_cmap_key_set_drop_spare();
    if (atomic_load(&blocks_out) != before)
        fail("read selections leaked blocks", atomic_load(&blocks_out), before);
}

/* Independent observations of merge, read mode, and stable missing keys. */
static void shared_map_groups(void) {
    void *object = wf__shared_map_new(16, 8, 2);
    wf_cmap *map = *(wf_cmap **)((char *)object + WF_SHARED_STATE_OFFSET);
    wf_cmap_holding first, second;
    wf_atomic_target group[2] = {{object, &first}, {object, &second}};
    for (unsigned present = 0; present < 2; ++present) {
        if (present) {
            wf_table_entry entry;
            uint64_t *slot = wf__table_lock_entry(map, (const unsigned char *)"k", 1, 0, &entry);
            slot[0] = 1; slot[1] = 41;
            wf__table_unlock_entry(&entry, 1);
        }
        wf__table_hold_begin(&first, map); wf__table_hold_begin(&second, map);
        wf__table_hold_key(&first, (const unsigned char *)"k", 1);
        wf__table_hold_key(&second, (const unsigned char *)"k", 1);
        wf__table_hold_read(&first); wf__table_hold_read(&second);
        uint64_t allocations_before = atomic_load(&allocations);
        wf__atomic_group_take(group, 2);
        uint64_t *a = wf__table_hold_slot(&first, 0), *b = wf__table_hold_slot(&second, 0);
        if (a != b || a[0] != present || (present && a[1] != 41))
            fail("merged read targets did not share the expected slot", a == b, a[0]);
        if (atomic_load(&allocations) != allocations_before)
            fail("a read group allocated a missing cell", atomic_load(&allocations), allocations_before);
        if (!present && (a != map->none || map->whole_hold == NULL))
            fail("an absent read group did not stabilize shared None", a != map->none, map->whole_hold == NULL);
        if (present && atomic_load(&map->gate) != 0)
            fail("a present read group took an exclusive whole hold", atomic_load(&map->gate), 0);
        wf__atomic_group_release(group, 2);
        if (wf_cmap_count_held(map, 0, 4, 0) != present || wf_cmap_holds_whole(wf_cmap_user_at(map, test_driver)))
            fail("a merged group retained a hold or an absent cell", wf_cmap_count_held(map, 0, 4, 0), present);
    }
    wf__keyed_table_drain(map); wf__keyed_table_free(map); test_give(object);
}

/* The k a counted key's bytes name. */
static uint64_t counted_of(const unsigned char *bytes) {
    uint64_t k = 0;
    for (int i = 0; i < 8; i++)
        k |= (uint64_t)(bytes[4 + i] - '0') << (3 * i);
    return k;
}

/* Whether a scan's key x comes before y [SHARE-1]: by position, then by
 * bytes, a proper prefix first. */
static int scan_before(const unsigned char *x, uint64_t lx, const unsigned char *y, uint64_t ly) {
    uint64_t px = position_of(tag_of(x, lx)), py = position_of(tag_of(y, ly));
    if (px != py)
        return px < py;
    uint64_t shorter = lx < ly ? lx : ly;
    int order = shorter == 0 ? 0 : memcmp(x, y, (size_t)shorter);
    return order != 0 ? order < 0 : lx < ly;
}

/* A scan in steps, each under a whole hold of its own, against a plain
 * reference, while the keys change between steps so that the table grows,
 * shrinks and leaves removed cells: every step's cursor advances or ends
 * the scan, its keys are present, in its position range and in the scan's
 * order, no key comes twice, and every key present for the whole scan
 * comes once. A build that narrows the hashes gives many keys one position
 * and long runs of cells, which wrap at the table's end. */
static void scans_resume(void) {
    enum { KEYS = 700, ROUNDS = 24 };
    static uint8_t present[KEYS], throughout[KEYS], seen[KEYS];
    for (unsigned round = 0; round < ROUNDS; round++) {
        uint64_t state = 101 + round;
        wf_cmap *map = wf_cmap_create_entries(8, 8, 1);
        memset(present, 0, sizeof present);
        memset(seen, 0, sizeof seen);
        for (uint64_t k = 0; k < KEYS; k++)
            if (next(&state) % 3 != 0) {
                put_counted(map, 0, k, k + 1);
                present[k] = 1;
            }
        memcpy(throughout, present, sizeof present);
        /* Rounds alternate between growing and shrinking the map. */
        unsigned grow = round % 2 == 0;
        uint64_t count = 1 + round % 7, cursor = 0, steps = 0;
        for (;;) {
            wf_key_set set;
            wf__key_set_new(&set, 4);
            wf_cmap_holding hold;
            wf__table_hold_begin(&hold, map);
            wf__table_hold_whole(&hold);
            wf__table_hold_take(&hold);
            uint64_t step = wf__keyed_table_scan(map, cursor, count, &set, VALUE_TAG);
            wf__table_hold_release(&hold, VALUE_TAG);
            if (step != 0 && step <= cursor)
                fail("a scan step did not advance (cursor, next)", cursor, step);
            const unsigned char *last = NULL;
            uint64_t last_length = 0;
            for (uint64_t i = 0; i < set.len; i++) {
                uint64_t length;
                const unsigned char *key = wf_cmap_key_set_key(&set, i, &length);
                uint64_t k = counted_of(key), position = position_of(tag_of(key, length));
                if (length != 12 || k >= KEYS || !present[k])
                    fail("a scan step inserted a key the map lacks (key, step)", k, steps);
                if (position < cursor || (step != 0 && position >= step))
                    fail("a scan step inserted a key outside its positions (key, step)", k, steps);
                if (last != NULL && !scan_before(last, last_length, key, length))
                    fail("a scan step's keys are out of order (key, step)", k, steps);
                if (++seen[k] > 1)
                    fail("a scan inserted a key twice (key, round)", k, round);
                last = key;
                last_length = length;
            }
            wf__key_set_free(set.store);
            for (unsigned j = 0; j < 24; j++) {
                uint64_t k = next(&state) % KEYS;
                if (present[k] && (grow ? next(&state) % 4 == 0 : next(&state) % 4 != 0)) {
                    put_counted(map, 0, k, 0);
                    present[k] = 0;
                    throughout[k] = 0;
                } else if (!present[k] && (grow || next(&state) % 4 == 0)) {
                    put_counted(map, 0, k, k + 1);
                    present[k] = 1;
                }
            }
            if (++steps > 1000000)
                fail("a scan did not end (round)", round, 0);
            if (step == 0)
                break;
            cursor = step;
        }
        for (uint64_t k = 0; k < KEYS; k++)
            if (throughout[k] && seen[k] != 1)
                fail("a scan missed a key present throughout it (key, round)", k, round);
        wf_cmap_destroy(map);
    }
    wf_cmap_key_set_drop_spare();
}

/* A scan reads the map and writes none of it: under a hold whose own
 * entries are locked, some of them None, the map, its cells and the hold
 * are as they were, and the scan inserts exactly the entries a count under
 * the hold counts. */
static void scans_write_nothing(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 1);
    unsigned char bytes[16];
    for (uint64_t k = 0; k < 50; k++)
        put_counted(map, 0, k, k + 1);
    wf_cmap_holding hold;
    wf__table_hold_begin(&hold, map);
    wf__table_hold_whole(&hold);
    wf__table_hold_take(&hold);
    for (uint64_t k = 40; k < 60; k++) {
        uint64_t *slot = wf__table_held_entry(map, bytes, counted_key(k, bytes), 1);
        slot[0] = k % 2 == 0 ? 0 : k + 1;
    }
    unsigned char map_before[sizeof *map];
    memcpy(map_before, map, sizeof *map);
    table *index = atomic_load(&map->current);
    size_t cells = (size_t)index->capacity * sizeof(cell);
    void *cells_before = malloc(cells);
    if (cells_before == NULL)
        abort();
    memcpy(cells_before, index->cells, cells);
    unsigned char hold_before[sizeof hold];
    memcpy(hold_before, &hold, sizeof hold);
    wf_key_set set;
    wf__key_set_new(&set, 4);
    uint64_t cursor = 0;
    do
        cursor = wf__keyed_table_scan(map, cursor, 3, &set, VALUE_TAG);
    while (cursor != 0);
    if (memcmp(map_before, map, sizeof *map) != 0 || memcmp(cells_before, index->cells, cells) != 0 ||
        memcmp(hold_before, &hold, sizeof hold) != 0)
        fail("a scan changed the map, its cells or the hold", 0, 0);
    if (set.len != wf__keyed_table_count(map, VALUE_TAG) || set.len != 40 + 10)
        fail("a scan under a hold did not insert the entries it counts (inserted, counted)", set.len,
             wf__keyed_table_count(map, VALUE_TAG));
    free(cells_before);
    wf__key_set_free(set.store);
    wf__table_hold_release(&hold, VALUE_TAG);
    wf_cmap_destroy(map);
    wf_cmap_key_set_drop_spare();
}

/* What clears handed to their release: how many runs, entries and the sum
 * of their values. */
static unsigned cleared_runs;
static uint64_t cleared_entries, cleared_sum;
static wf_cmap *clear_source;

static void release_cleared(void *table) {
    if (atomic_load(&clear_source->gate) != 0 || clear_source->whole_hold != NULL)
        fail("a clear released entries before giving up the source hold", 0, 0);
    uint64_t drained;
    cleared_sum += drain_sum(table, &drained);
    cleared_entries += drained;
    cleared_runs++;
    wf__keyed_table_free(table);
}

/* A move keeps the freed cells only when they are the current table's size:
 * a map that grew keeps no spare of its old size, and one whose size holds
 * steady while keys come and go keeps a spare of its own size for the next
 * move, which it gives up when it grows again. */
static void spares_match_the_current_size(void) {
    wf_cmap *map = wf_cmap_create(1);
    wf_cmap_user *user = wf_cmap_enter(map);
    uint64_t got = 0;
    for (uint64_t k = 0; k < 20000; k++)
        wf_cmap_insert(user, key_of(k), k);
    wf_cmap_get(user, key_of(0), &got);
    table *now = atomic_load(&map->current);
    if (map->spare != NULL && map->spare_capacity != now->capacity)
        fail("a grown map kept a spare of another size (spare, current)", map->spare_capacity, now->capacity);
    uint64_t next = 20000;
    for (; next < 220000 || (map->spare == NULL && next < 420000); next++) {
        wf_cmap_remove(user, key_of(next - 20000));
        wf_cmap_insert(user, key_of(next), next);
        now = atomic_load(&map->current);
        if (map->spare != NULL && map->spare_capacity != now->capacity)
            fail("a steady map kept a spare of another size (spare, current)", map->spare_capacity, now->capacity);
    }
    if (map->spare == NULL)
        fail("a steady map never kept a spare for its next move", now->capacity, 0);
    if (wf_cmap_count(map) != 20000)
        fail("churn lost or gained keys (count)", wf_cmap_count(map), 20000);
    uint64_t steady = map->spare_capacity;
    for (uint64_t k = next; k < next + 60000; k++)
        wf_cmap_insert(user, key_of(k), k);
    wf_cmap_get(user, key_of(next), &got);
    now = atomic_load(&map->current);
    if (now->capacity == steady)
        fail("the map did not grow past its spare's size (capacity)", now->capacity, steady);
    if (map->spare != NULL && map->spare_capacity != now->capacity)
        fail("a regrown map kept its old spare (spare, current)", map->spare_capacity, now->capacity);
    if (wf_cmap_count(map) != 80000)
        fail("growth lost or gained keys (count)", wf_cmap_count(map), 80000);
    wf_cmap_leave(user);
    wf_cmap_destroy(map);
}

/* A clear under a whole hold empties the map at once, its own entries
 * included, keeps what the statement writes after it, and hands the old
 * entries to their release only once the hold is given up, leaking no
 * block. */
/* A map's reserve, the cells it keeps for the next move of their size, goes
 * on request: the release answers its bytes and changes no entry, a second
 * answers 0, one made while the map's own lock is held answers 0 and leaves
 * the reserve, and the map moves as before after a release. */
static void reserves_release_on_request(void) {
    wf_cmap *map = wf_cmap_create(1);
    wf_cmap_user *user = wf_cmap_enter(map);
    uint64_t got = 0;
    for (uint64_t k = 0; k < 20000; k++)
        wf_cmap_insert(user, key_of(k), k);
    uint64_t next = 20000;
    for (; map->spare == NULL && next < 420000; next++) {
        wf_cmap_remove(user, key_of(next - 20000));
        wf_cmap_insert(user, key_of(next), next);
    }
    if (map->spare == NULL)
        fail("a steady map never kept a reserve (keys inserted)", next, 0);
    uint64_t bytes = map->spare_capacity * sizeof(cell);
    atomic_store(&map->lock, 1);
    uint64_t locked = wf_cmap_release_reserve(map);
    if (locked != 0 || map->spare == NULL)
        fail("a release while the map's lock was held took the reserve (freed, kept)", locked, map->spare != NULL);
    atomic_store(&map->lock, 0);
    uint64_t freed = wf_cmap_release_reserve(map);
    if (freed != bytes || map->spare != NULL)
        fail("a release did not free the reserve (freed, reserve bytes)", freed, bytes);
    uint64_t again = wf_cmap_release_reserve(map);
    if (again != 0)
        fail("a second release freed bytes (freed, expected)", again, 0);
    if (wf_cmap_count(map) != 20000)
        fail("a release changed the count (count, expected)", wf_cmap_count(map), 20000);
    if (!wf_cmap_get(user, key_of(next - 1), &got) || got != next - 1)
        fail("a release lost an entry (value, expected)", got, next - 1);
    for (uint64_t end = next + 200000; next < end; next++) {
        wf_cmap_remove(user, key_of(next - 20000));
        wf_cmap_insert(user, key_of(next), next);
    }
    if (wf_cmap_count(map) != 20000)
        fail("churn after a release lost or gained keys (count, expected)", wf_cmap_count(map), 20000);
    wf_cmap_leave(user);
    wf_cmap_destroy(map);
}

static _Atomic int releasing_stop;
static _Atomic uint64_t released_bytes;

static void *release_reserves(void *arg) {
    wf_cmap **maps = (wf_cmap **)arg;
    while (!atomic_load(&releasing_stop)) {
        uint64_t freed = wf_cmap_release_reserve(maps[0]) + wf_cmap_release_reserve(maps[1]);
        atomic_fetch_add(&released_bytes, freed);
    }
    return NULL;
}

/* A release needs no statement, so it can meet a swap, which only needs no
 * statement inside either map: each reserve moves with its map or is
 * released, never both, so destroying both maps frees each array once.
 * Without the swap's locks, 2,000 rounds fail 5 of 5 runs under ASan and
 * under TSan (map-sanitizers.yml) but about 1 in 5 in a plain build, which
 * takes 20,000 rounds to fail reliably; the sanitizer builds are the ones
 * this race is left to, and the plain builds keep the shorter run. */
static void reserves_release_beside_swaps(void) {
    wf_cmap *maps[2] = {wf_cmap_create(1), wf_cmap_create(1)};
    wf_cmap_user *users[2] = {wf_cmap_enter(maps[0]), wf_cmap_enter(maps[1])};
    uint64_t next[2] = {0, 1000000};
    for (int m = 0; m < 2; m++)
        for (uint64_t k = 0; k < 64; k++, next[m]++)
            wf_cmap_insert(users[m], key_of(next[m]), next[m]);
    atomic_store(&releasing_stop, 0);
    atomic_store(&released_bytes, 0);
    pthread_t releaser;
    pthread_create(&releaser, NULL, release_reserves, maps);
    for (unsigned round = 0; round < 2000; round++) {
        for (int m = 0; m < 2; m++)
            for (unsigned step = 0; step < 16; step++, next[m]++) {
                wf_cmap_remove(users[m], key_of(next[m] - 64));
                wf_cmap_insert(users[m], key_of(next[m]), next[m]);
            }
        wf_cmap_swap(maps[0], maps[1], VALUE_TAG);
        wf_cmap_swap(maps[0], maps[1], VALUE_TAG);
    }
    atomic_store(&releasing_stop, 1);
    pthread_join(releaser, NULL);
    if (atomic_load(&released_bytes) == 0)
        fail("no release met a reserve beside the swaps (rounds)", 2000, 0);
    for (int m = 0; m < 2; m++)
        if (wf_cmap_count(maps[m]) != 64)
            fail("a map lost or gained keys beside releases and swaps (count, expected)", wf_cmap_count(maps[m]), 64);
    for (int m = 0; m < 2; m++) {
        wf_cmap_leave(users[m]);
        wf_cmap_destroy(maps[m]);
    }
}

static void maps_clear(void) {
    wf_cmap_key_set_drop_spare();
    int64_t before = atomic_load(&blocks_out);
    wf_cmap *map = wf_cmap_create_entries(8, 8, 1);
    unsigned char bytes[16];
    for (uint64_t k = 0; k < 100; k++)
        put_counted(map, 0, k, k + 1);
    cleared_runs = 0;
    cleared_entries = 0;
    cleared_sum = 0;
    clear_source = map;
    wf_cmap_holding hold;
    wf__table_hold_begin(&hold, map);
    wf__table_hold_whole(&hold);
    wf__table_hold_take(&hold);
    *(uint64_t *)wf__table_held_entry(map, bytes, counted_key(200, bytes), 1) = 7;
    *(uint64_t *)wf__table_held_entry(map, bytes, counted_key(5, bytes), 1) = 0;
    wf__keyed_table_clear(map, VALUE_TAG, release_cleared);
    if (wf__keyed_table_count(map, VALUE_TAG) != 0 || wf__table_held_entry(map, bytes, counted_key(3, bytes), 0) != NULL ||
        wf__table_held_entry(map, bytes, counted_key(200, bytes), 0) != NULL)
        fail("a clear left entries in the map (count)", wf__keyed_table_count(map, VALUE_TAG), 0);
    *(uint64_t *)wf__table_held_entry(map, bytes, counted_key(300, bytes), 1) = 9;
    wf__keyed_table_clear(map, VALUE_TAG, release_cleared);
    *(uint64_t *)wf__table_held_entry(map, bytes, counted_key(400, bytes), 1) = 11;
    if (cleared_runs != 0)
        fail("a clear released entries under the hold (runs)", cleared_runs, 0);
    wf__table_hold_release(&hold, VALUE_TAG);
    if (cleared_runs != 2 || cleared_entries != 101 || cleared_sum != 5050 - 6 + 7 + 9)
        fail("the clears' entries were not released once each (entries, sum)", cleared_entries, cleared_sum);
    if (wf_cmap_count(map) != 1 || counted_value(map, 400) != 11 || counted_value(map, 3) != 0)
        fail("a write after a clear was not kept (count, value)", wf_cmap_count(map), counted_value(map, 400));
    wf_cmap_destroy(map);
    wf_cmap_key_set_drop_spare();
    if (atomic_load(&blocks_out) != before)
        fail("clears leaked blocks (out, before)", atomic_load(&blocks_out), before);
}

/* Empty maps finish at once, and a sparse map fits in a step of its count.
 * Distinct homes in the upper half, before the last cell, keep the old cell
 * budget from reaching any key in either hash build and leave homes after
 * the last key, while a small step still stops at a key. */
static void scans_sparse(void) {
    enum { KEYS = 5 };
    wf_cmap *map = wf_cmap_create_entries(8, 8, 1u << 17);
    wf_cmap_holding hold;
    wf__table_hold_begin(&hold, map);
    wf__table_hold_whole(&hold);
    wf__table_hold_take(&hold);
    wf_key_set set;
    wf__key_set_new(&set, 4);
    uint64_t step = wf__keyed_table_scan(map, 0, 10, &set, VALUE_TAG);
    if (step != 0 || set.len != 0)
        fail("an empty sparse map did not finish its scan (next, keys)", step, set.len);
    wf__key_set_free(set.store);
    wf__table_hold_release(&hold, VALUE_TAG);

    table *index = atomic_load(&map->current);
    uint64_t chosen[KEYS], homes[KEYS], added = 0;
    unsigned char bytes[16];
    for (uint64_t k = 0; added < KEYS; k++) {
        uint64_t length = counted_key(k, bytes);
        uint64_t home = start_of(index, tag_of(bytes, length));
        if (home < index->capacity / 2 || home == index->capacity - 1)
            continue;
        uint64_t i = 0;
        while (i < added && homes[i] != home)
            i++;
        if (i != added)
            continue;
        chosen[added] = k;
        homes[added++] = home;
        put_counted(map, 0, k, k + 1);
    }
    wf__table_hold_begin(&hold, map);
    wf__table_hold_whole(&hold);
    wf__table_hold_take(&hold);
    wf__key_set_new(&set, 4);
    step = wf__keyed_table_scan(map, 0, KEYS, &set, VALUE_TAG);
    uint64_t seen = 0;
    for (uint64_t i = 0; i < set.len; i++) {
        uint64_t length;
        const unsigned char *key = wf_cmap_key_set_key(&set, i, &length);
        if (length == 12)
            for (uint64_t j = 0; j < KEYS; j++)
                if (counted_of(key) == chosen[j])
                    seen |= 1ull << j;
    }
    if (step != 0 || set.len != KEYS || seen != (1ull << KEYS) - 1)
        fail("a sparse scan did not finish with every key (next, keys)", step, set.len);
    wf__key_set_free(set.store);

    wf__key_set_new(&set, 4);
    step = wf__keyed_table_scan(map, 0, 1, &set, VALUE_TAG);
    if (step == 0 || set.len != 1)
        fail("a sparse scan did not stop at its first key (next, keys)", step, set.len);
    wf__key_set_free(set.store);

    clear_source = map;
    cleared_runs = 0;
    cleared_entries = 0;
    cleared_sum = 0;
    wf__keyed_table_clear(map, VALUE_TAG, release_cleared);
    wf__key_set_new(&set, 4);
    step = wf__keyed_table_scan(map, 0, 10, &set, VALUE_TAG);
    if (step != 0 || set.len != 0)
        fail("a cleared map did not finish its scan (next, keys)", step, set.len);
    wf__key_set_free(set.store);
    wf__table_hold_release(&hold, VALUE_TAG);
    wf_cmap_destroy(map);
    wf_cmap_key_set_drop_spare();
}

/* A hold's keys are the node's bytes from its take on: the statement may
 * change the bytes it named a key by, and a move under the whole hold
 * finds the entry again by the node's. */
static void holds_keep_their_bytes(void) {
    wf_cmap *map = wf_cmap_create_entries(8, 8, 1);
    unsigned char name[16], bytes[16];
    wf_cmap_holding hold;
    wf__table_hold_begin(&hold, map);
    wf__table_hold_whole(&hold);
    wf__table_hold_key(&hold, name, counted_key(1, name));
    wf__table_hold_take(&hold);
    *(uint64_t *)wf__table_hold_slot(&hold, 0) = 5;
    counted_key(2, name);
    for (uint64_t k = 10; k < 400; k++)
        *(uint64_t *)wf__table_held_entry(map, bytes, counted_key(k, bytes), 1) = k;
    if (*(uint64_t *)wf__table_hold_slot(&hold, 0) != 5)
        fail("a held entry lost its value across a move", *(uint64_t *)wf__table_hold_slot(&hold, 0), 5);
    wf__table_hold_release(&hold, VALUE_TAG);
    if (counted_value(map, 1) != 5 || counted_value(map, 2) != 0 || wf_cmap_count(map) != 391)
        fail("a held entry is not under its own key after a move (value, count)", counted_value(map, 1),
             wf_cmap_count(map));
    wf_cmap_destroy(map);
}

/* A second driver inserts the very key an absent statement observed, or
 * updates its present payload through the new exclusive-existing path. */
typedef struct {
    wf_cmap *map;
    uint32_t flags;
    _Atomic int done;
    int upgraded;
} present_racer;

static void *present_race(void *argument) {
    present_racer *race = argument;
    test_driver = 1;
    wf_table_entry entry;
    uint64_t *slot = wf__table_lock_entry(race->map, (const unsigned char *)"key", 3, race->flags, &entry);
    race->upgraded = entry.inner.upgraded != 0;
    slot[0] = 1;
    slot[1] += 1;
    wf__table_unlock_entry(&entry, 1);
    atomic_store(&race->done, 1);
    return NULL;
}

static void entries_noninserting(void) {
    test_driver = 0;
    wf_cmap *map = wf_cmap_create_entries(16, 8, 0);
    wf_cmap_user *u = wf_cmap_user_at(map, 0);
    wf_table_entry entry;
    uint64_t allocations_before = atomic_load(&allocations);
    int64_t used, live;
    uint64_t *slot = wf__table_lock_entry(map, (const unsigned char *)"key", 3, 4, &entry);
    totals(map, &used, &live);
    if (slot != map->none || slot[0] != 0 || entry.inner.cell != NULL || used != 0 || live != 0 ||
        u->chunks != NULL || u->cursor != NULL || u->room != 0 || atomic_load(&allocations) != allocations_before)
        fail("non-inserting miss claimed a cell or allocated a node", (uint64_t)used, (uint64_t)live);
    table *t = atomic_load(&map->current);
    for (uint64_t i = 0; i < t->capacity; i++)
        if (atomic_load(&t->cells[i].key) != EMPTY)
            fail("non-inserting miss changed a cell", i, 0);
    present_racer race = {.map = map, .flags = 0};
    pthread_t thread;
    if (pthread_create(&thread, NULL, present_race, &race) != 0) abort();
    pthread_join(thread, NULL); /* insertion must finish before the miss releases */
    if (slot[0] != 0) fail("insertion changed shared None", slot[0], 0);
    map->watch.count = 1;
    uint64_t wakes = atomic_load(&written_calls);
    wf__table_unlock_entry(&entry, 0);
    if (atomic_load(&written_calls) != wakes)
        fail("non-inserting miss reported a map write", atomic_load(&written_calls), wakes);
    map->watch.count = 0;
    slot = wf__table_lock_entry(map, (const unsigned char *)"key", 3, 4, &entry);
    if (slot == map->none || slot[0] != 1 || slot[1] != 1 || entry.inner.cell == NULL ||
        !(atomic_load(&((cell *)entry.inner.cell)->key) & LOCKED))
        fail("non-inserting hit lost insertion or lacks exclusive lock", slot[0], slot[1]);
    slot[1] = 7;
    wf__table_unlock_entry(&entry, 1);
    const uint64_t *read = wf__table_lock_entry(map, (const unsigned char *)"key", 3, 1, &entry);
    if (read[1] != 7 || wf_cmap_count(map) != 1)
        fail("non-inserting update was not retained", read[1], wf_cmap_count(map));
    wf__table_unlock_entry(&entry, 1);

    /* Exhausted patience must upgrade, wait for the old holder, then update. */
    slot = wf__table_lock_entry(map, (const unsigned char *)"key", 3, 0, &entry);
    patience[1] = 0;
    atomic_store(&closed_seen[1], 0);
    race.flags = 4;
    atomic_store(&race.done, 0);
    if (pthread_create(&thread, NULL, present_race, &race) != 0) abort();
    while (!atomic_load(&closed_seen[1])) sched_yield();
    if (atomic_load(&race.done)) fail("non-inserting writer passed a locked hit", 0, 0);
    wf__table_unlock_entry(&entry, 1);
    pthread_join(thread, NULL);
    if (!race.upgraded) fail("non-inserting writer did not upgrade after impatience", 0, 0);
    set_patience(PATIENCE, PATIENCE);

    /* A miss beside another target holds absence until release. */
    slot = wf__table_lock_entry(map, (const unsigned char *)"absent", 6, 6, &entry);
    if (slot != map->none || !entry.inner.upgraded || !wf_cmap_holds_whole(u))
        fail("non-inserting multi-target miss did not stabilize absence", 0, 0);
    race.flags = 0;
    atomic_store(&race.done, 0);
    if (pthread_create(&thread, NULL, present_race, &race) != 0) abort();
    while (atomic_load(&map->waiting) == 0) sched_yield();
    if (atomic_load(&race.done)) fail("writer passed stable absence hold", 0, 0);
    wf__table_unlock_entry(&entry, 0);
    pthread_join(thread, NULL);
    if (wf_cmap_holds_whole(u)) fail("miss release retained whole hold", 0, 0);

    /* An enclosing whole hold remains owned by its caller, hit or miss. */
    wf_cmap_hold(u);
    slot = wf__table_lock_entry(map, (const unsigned char *)"absent", 6, 6, &entry);
    if (slot != map->none || entry.inner.upgraded || !entry.held)
        fail("held miss acquired another whole hold", 0, 0);
    wf__table_unlock_entry(&entry, 0);
    slot = wf__table_lock_entry(map, (const unsigned char *)"key", 3, 4, &entry);
    if (slot[1] != 9) fail("racing payload updates were lost", slot[1], 9);
    wf__table_unlock_entry(&entry, 1);
    if (!wf_cmap_holds_whole(u)) fail("entry release gave away enclosing whole hold", 0, 0);
    wf_cmap_unhold(u);
    wf_cmap_destroy(map);
}

/* PRE-2's requests outside the host pool: the exact mapping threshold,
 * live nodes rather than chunk capacity, reuse, and drain after the value
 * is released. Script moves to isolate a freed other-size table and
 * replaced and reused spare arrays without a second large insertion
 * workload. */
static void heap_accounting(void) {
    const int64_t mib = 1024 * 1024;
    int64_t before = atomic_load(&mapped_bytes_out);
    int64_t blocks_before = atomic_load(&blocks_out);
    wf_cmap *map = wf_cmap_create_entries(8, 8, 65536);
    wf_cmap_user *u = wf_cmap_user_at(map, 0);
    if (atomic_load(&mapped_bytes_out) != before + 2 * mib)
        fail("host cells are not counted by their requested bytes", atomic_load(&mapped_bytes_out) - before, 2 * mib);
    unsigned char key[3] = {1, 2, 3};
    for (unsigned round = 0; round < 2; round++) {
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(u, key, sizeof key, 0, &entry);
        *slot = 11;
        wf_cmap_unlock_entry(u, &entry, 0, 1);
        if (atomic_load(&mapped_bytes_out) != before + 2 * mib + 32)
            fail("new or reused node counted chunk reserves", atomic_load(&mapped_bytes_out) - before, 2 * mib + 32);
        slot = wf_cmap_lock_entry(u, key, sizeof key, 0, &entry);
        *slot = 0;
        wf_cmap_unlock_entry(u, &entry, 0, 0);
        if (atomic_load(&mapped_bytes_out) != before + 2 * mib)
            fail("a free-list node is still counted", atomic_load(&mapped_bytes_out) - before, 2 * mib);
    }
    wf_cmap_entry entry;
    uint64_t *small = wf_cmap_lock_entry(u, key, sizeof key, 0, &entry);
    *small = 11;
    wf_cmap_unlock_entry(u, &entry, 0, 1);
    unsigned char long_key[600] = {0};
    uint64_t *large = wf_cmap_lock_entry(u, long_key, sizeof long_key, 0, &entry);
    *large = 99;
    wf_cmap_unlock_entry(u, &entry, 0, 1);
    if (atomic_load(&mapped_bytes_out) != before + 2 * mib + 32)
        fail("a pool node was also counted outside the pool", atomic_load(&mapped_bytes_out) - before, 2 * mib + 32);

    table *old = use_current(u);
    start_move_for(map, old, 65536);
    finish_move(map, old);
    reclaim(map);
    if (atomic_load(&mapped_bytes_out) != before + 6 * mib + 32 || map->spare != NULL)
        fail("a pinned retired table lost its accounting", atomic_load(&mapped_bytes_out) - before, 6 * mib + 32);
    /* The released 2 MiB table is not the current size, so it is freed
     * rather than kept as a spare; the same-size moves below keep one. */
    use_current(u);
    if (atomic_load(&mapped_bytes_out) != before + 4 * mib + 32 || map->spare != NULL)
        fail("a grown map's other-size table stayed counted (bytes, expected)", atomic_load(&mapped_bytes_out) - before, 4 * mib + 32);
    for (unsigned round = 0; round < 2; round++) {
        old = use_current(u);
        start_move_for(map, old, 65536);
        finish_move(map, old);
        use_current(u);
        if (atomic_load(&mapped_bytes_out) != before + 8 * mib + 32)
            fail("replacing or reusing a spare miscounted its request", atomic_load(&mapped_bytes_out) - before, 8 * mib + 32);
    }
    int64_t live_blocks = atomic_load(&blocks_out);
    for (uint64_t *slot; (slot = wf_cmap_drain(map)) != NULL;) {
        if (slot == small && atomic_load(&mapped_bytes_out) != before + 8 * mib + 32)
            fail("a draining node stopped counting before its value was released", atomic_load(&mapped_bytes_out) - before, 8 * mib + 32);
    }
    if (atomic_load(&mapped_bytes_out) != before + 8 * mib || atomic_load(&blocks_out) != live_blocks - 1)
        fail("draining did not release small and pool nodes once", atomic_load(&mapped_bytes_out) - before, 8 * mib);
    wf_cmap_destroy(map);
    if (atomic_load(&mapped_bytes_out) != before || atomic_load(&blocks_out) != blocks_before)
        fail("destroy leaked map storage", atomic_load(&mapped_bytes_out), before);

    /* Native slots with no owned payload need no caller-side drain. */
    map = wf_cmap_create_entries(8, 8, 1);
    u = wf_cmap_user_at(map, 0);
    small = wf_cmap_lock_entry(u, key, sizeof key, 0, &entry);
    *small = 11;
    wf_cmap_unlock_entry(u, &entry, 0, 1);
    large = wf_cmap_lock_entry(u, long_key, sizeof long_key, 0, &entry);
    *large = 99;
    wf_cmap_unlock_entry(u, &entry, 0, 1);
    wf_cmap_destroy(map);
    if (atomic_load(&mapped_bytes_out) != before || atomic_load(&blocks_out) != blocks_before)
        fail("direct destroy leaked small or pool nodes", atomic_load(&mapped_bytes_out), before);
}

int main(int argc, char **argv) {
    /* Writers that wait on each other in a cycle fail the test here rather
     * than at the gate's limit. */
    alarm(120);
    set_patience(PATIENCE, PATIENCE);
    if (argc == 2 && strcmp(argv[1], "selection") == 0) {
        tables_held_selection();
        tables_read_selection();
        shared_map_groups();
        if (atomic_load(&mapped_bytes_out) != 0)
            fail("selection checks leaked counted map storage", atomic_load(&mapped_bytes_out), 0);
        puts("concurrent-map-test: selection checks passed");
        return 0;
    }
    if (argc != 1) return 2;
    entries_noninserting();
    if (ENTRY_TESTS) {
        heap_accounting();
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
        key_sets();
        holds_sequential();
        holds_sort_a_set();
        holds_positions();
        holds_follow_moves();
        locks_follow_moves();
        locks_read_no_freed_node();
        holds_give_back_counted();
        holds_in_one_order();
        holds_whole();
        holds_wait_out_moves();
        maps_swap();
        maps_clear();
        scans_resume();
        scans_sparse();
        scans_write_nothing();
        holds_keep_their_bytes();
        tables_wake_writers();
        tables_held_selection();
        tables_read_selection();
        shared_map_groups();
        holds_move_amounts(0, PATIENCE);
        holds_move_amounts(1, PATIENCE);
        holds_move_amounts(1, 0);
        holds_across_maps();
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
    reserves_release_on_request();
    reserves_release_beside_swaps();
    spares_match_the_current_size();
    if (atomic_load(&mapped_bytes_out) != 0)
        fail("map checks leaked counted map storage", atomic_load(&mapped_bytes_out), 0);
    printf("concurrent-map-test: all checks passed\n");
    return 0;
}
