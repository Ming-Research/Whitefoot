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
 *   refuse a history that is not linearizable and to accept one that is.
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

#include "concurrent_map.h"

#define THREADS 4

static uint64_t mix64(uint64_t z) {
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ull;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBull;
    return z ^ (z >> 31);
}

static uint64_t next(uint64_t *state) {
    *state += 0x9E3779B97F4A7C15ull;
    return mix64(*state);
}

/* Distinct nonzero keys: the finalizer is a bijection that maps only zero
 * to zero. */
static uint64_t key_of(uint64_t index) { return mix64(index + 1); }

static void fail(const char *what, unsigned long long a, unsigned long long b) {
    printf("concurrent-map-test: %s (%llu, %llu)\n", what, a, b);
    exit(1);
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
    wf_cmap_enter(map);
    uint64_t state = 7;
    for (unsigned i = 0; i < OPS; i++) {
        uint64_t r = next(&state);
        unsigned k = (unsigned)((r >> 8) % KEYS);
        uint64_t key = key_of(k), got = 0;
        int result;
        switch (r & 3) {
        case 0:
            result = wf_cmap_get(map, key, &got);
            if (result != present[k] || (result && got != value[k]))
                fail("a get disagrees with the reference", k, got);
            break;
        case 1:
            result = wf_cmap_insert(map, key, r);
            if (result != !present[k])
                fail("an insert disagrees with the reference", k, (unsigned long long)result);
            present[k] = 1;
            value[k] = r;
            break;
        case 2:
            result = wf_cmap_remove(map, key);
            if (result != present[k])
                fail("a remove disagrees with the reference", k, (unsigned long long)result);
            present[k] = 0;
            break;
        default:
            result = wf_cmap_update(map, key, add_one, NULL);
            if (result != present[k])
                fail("an update disagrees with the reference", k, (unsigned long long)result);
            value[k] += (uint64_t)present[k];
            break;
        }
    }
    for (unsigned k = 0; k < KEYS; k++) {
        uint64_t got = 0;
        int result = wf_cmap_get(map, key_of(k), &got);
        if (result != present[k] || (result && got != value[k]))
            fail("a key's final state disagrees with the reference", k, got);
    }
    wf_cmap_leave(map);
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
    wf_cmap_enter(w->map);
    for (unsigned i = 0; i < 200000; i++) {
        uint64_t r = next(&state);
        if (!w->churn) {
            w->updates += (uint64_t)wf_cmap_update(w->map, key_of((r >> 8) % SHARED_KEYS), add_one, NULL);
        } else {
            uint64_t key = key_of((r >> 8) % (2 * SHARED_KEYS));
            if (r & 1)
                w->inserted += (uint64_t)wf_cmap_insert(w->map, key, r);
            else
                w->removed += (uint64_t)wf_cmap_remove(w->map, key);
        }
    }
    wf_cmap_leave(w->map);
    return NULL;
}

/* Runs THREADS workers on a map holding the keys below SHARED_KEYS, with
 * value equal to their index, and checks what is left. */
static void concurrent(int churn) {
    wf_cmap *map = wf_cmap_create(churn ? 1 : SHARED_KEYS);
    wf_cmap_enter(map);
    for (uint64_t i = 0; i < SHARED_KEYS; i++)
        wf_cmap_insert(map, key_of(i), i);
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
        if (wf_cmap_get(map, key_of(i), &v)) {
            live++;
            sum += v;
        }
    }
    if (!churn && (live != SHARED_KEYS || sum != (uint64_t)SHARED_KEYS * (SHARED_KEYS - 1) / 2 + updates))
        fail("an update was lost", live, sum);
    if (churn && live != SHARED_KEYS + inserted - removed)
        fail("the live count after churn is wrong", live, SHARED_KEYS + inserted - removed);
    wf_cmap_leave(map);
    wf_cmap_destroy(map);
}

/* Linearizability. */

enum { GET, INSERT, REMOVE, UPDATE };

typedef struct {
    uint64_t call, ret, arg, out;
    int kind, key, result;
} op_t;

typedef struct {
    wf_cmap *map;
    op_t *ops;
    unsigned count, thread, keys;
    _Atomic int *go;
} history_t;

static uint64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000ull + (uint64_t)t.tv_nsec;
}

static void *record(void *arg) {
    history_t *h = arg;
    uint64_t state = mix64(0x57AE55ull ^ h->thread);
    wf_cmap_enter(h->map);
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
        o->call = now_ns();
        switch (o->kind) {
        case GET:
            o->result = wf_cmap_get(h->map, key, &v);
            o->out = v;
            break;
        case INSERT:
            o->result = wf_cmap_insert(h->map, key, o->arg);
            break;
        case REMOVE:
            o->result = wf_cmap_remove(h->map, key);
            break;
        default:
            o->result = wf_cmap_update(h->map, key, add_one, NULL);
            break;
        }
        o->ret = now_ns();
    }
    wf_cmap_leave(h->map);
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
            if (!linearizable(per_key, n))
                fail("a key's history is not linearizable (round, key)", round, k);
        }
        free(per_key);
        for (unsigned i = 0; i < THREADS; i++)
            free(h[i].ops);
        wf_cmap_destroy(map);
    }
}

int main(void) {
    checker_self_test();
    sequential();
    concurrent(0);
    concurrent(1);
    histories(20);
    printf("concurrent-map-test: all checks passed\n");
    return 0;
}
