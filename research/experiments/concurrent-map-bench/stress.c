/* Serves concurrent-map-bench: the linearizability mode of DESIGN.md's
 * checks. Threads run random operations on a few keys, each recorded with
 * the instants it was called and returned; then every key's history is
 * checked against the sequential map, which suffices because
 * linearizability is local to each object (Herlihy and Wing). The search is
 * Wing and Gong's, with Lowe's memoization of linearized sets and states,
 * as Porcupine implements it.
 *
 *   stress-<prefix> [--threads 4] [--keys 4] [--ops 4000] [--rounds 20]
 *
 * Exits 0 when every history of every round is linearizable, 1 naming the
 * first that is not.
 */
#define _GNU_SOURCE
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "cmap.h"
#include "workload.h"

enum { GET, INSERT, REMOVE, UPDATE };

typedef struct {
    uint64_t call, ret; /* nanoseconds */
    uint64_t arg;       /* the value an insert stores */
    uint64_t out;       /* the value a get found */
    int kind, key, result;
} op_t;

typedef struct {
    op_t *ops;
    unsigned count;
    unsigned thread;
} log_t;

static cm_map *map;
static unsigned nkeys, nops;
static unsigned round_keys; /* the keys of the current round */
static _Atomic int go;

static uint64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000ull + (uint64_t)t.tv_nsec;
}

static void *worker(void *arg) {
    log_t *log = arg;
    uint64_t state = wl_mix64(0x57AE55ull ^ log->thread);
    CM(enter)(map);
    while (!atomic_load(&go)) {
    }
    for (unsigned i = 0; i < nops; i++) {
        uint64_t r = wl_next(&state);
        op_t *o = &log->ops[i];
        o->key = (int)((r >> 8) % round_keys);
        o->kind = (int)(r & 3);
        /* Insert values are distinct and far apart, so that no run of
         * updates makes one value look like another. */
        o->arg = ((uint64_t)(log->thread + 1) << 40) | ((uint64_t)i << 12);
        uint64_t key = wl_key((uint64_t)o->key);
        uint64_t v = 0;
        o->call = now_ns();
        switch (o->kind) {
        case GET:
            o->result = CM(get)(map, key, &v);
            o->out = v;
            break;
        case INSERT:
            o->result = CM(insert)(map, key, o->arg);
            break;
        case REMOVE:
            o->result = CM(remove)(map, key);
            break;
        default:
            o->result = CM(update)(map, key);
            break;
        }
        o->ret = now_ns();
    }
    CM(leave)(map);
    return NULL;
}

/* The sequential map at one key: present and its value. */
typedef struct {
    int present;
    uint64_t value;
} reg_t;

/* Applies o to s; 0 when o's recorded result is not what the sequential
 * map would answer. */
static int apply(reg_t *s, const op_t *o) {
    switch (o->kind) {
    case GET:
        if (o->result != s->present)
            return 0;
        return !o->result || o->out == s->value;
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
        if (s->present)
            s->value++;
        return 1;
    }
}

/* An entry of the event list: a call or a return of operation id. */
typedef struct entry {
    struct entry *prev, *next, *match; /* a call's match is its return */
    uint64_t time;
    int id, is_call;
} entry_t;

typedef struct {
    uint64_t *bits;
    reg_t state;
} seen_t;

static int cmp_entry(const void *a, const void *b) {
    const entry_t *x = *(entry_t *const *)a, *y = *(entry_t *const *)b;
    if (x->time != y->time)
        return x->time < y->time ? -1 : 1;
    /* At equal instants calls come first, the permissive order. */
    return y->is_call - x->is_call;
}

static uint64_t hash_seen(const uint64_t *bits, unsigned words, reg_t s) {
    uint64_t h = (uint64_t)s.present * 31 + s.value;
    for (unsigned i = 0; i < words; i++)
        h = wl_mix64(h ^ bits[i]);
    return h;
}

/* Checks one key's operations; 1 when some order consistent with real time
 * explains every result. */
static int linearizable(op_t **ops, unsigned n) {
    if (n == 0)
        return 1;
    entry_t *entries = calloc(2 * n, sizeof *entries);
    entry_t **order = malloc(2 * n * sizeof *order);
    for (unsigned i = 0; i < n; i++) {
        entries[2 * i] = (entry_t){.time = ops[i]->call, .id = (int)i, .is_call = 1};
        entries[2 * i + 1] = (entry_t){.time = ops[i]->ret, .id = (int)i, .is_call = 0};
        entries[2 * i].match = &entries[2 * i + 1];
        order[2 * i] = &entries[2 * i];
        order[2 * i + 1] = &entries[2 * i + 1];
    }
    qsort(order, 2 * n, sizeof *order, cmp_entry);
    entry_t head = {0};
    entry_t *last = &head;
    for (unsigned i = 0; i < 2 * n; i++) {
        last->next = order[i];
        order[i]->prev = last;
        last = order[i];
    }
    unsigned words = (n + 63) / 64;
    uint64_t *bits = calloc(words, sizeof *bits);
    size_t cache_cap = 1u << 16, cache_used = 0;
    seen_t *cache = calloc(cache_cap, sizeof *cache);
    typedef struct {
        entry_t *call;
        reg_t state;
    } frame_t;
    frame_t *stack = malloc(n * sizeof *stack);
    unsigned depth = 0;
    reg_t state = {0, 0};
    entry_t *e = head.next;
    int ok = 1;
    while (head.next != NULL) {
        if (e != NULL && e->is_call) {
            reg_t next = state;
            int fits = apply(&next, ops[e->id]);
            int fresh = 0;
            if (fits) {
                bits[e->id / 64] |= 1ull << (e->id % 64);
                uint64_t h = hash_seen(bits, words, next);
                size_t slot = h & (cache_cap - 1);
                fresh = 1;
                while (cache[slot].bits != NULL) {
                    if (cache[slot].state.present == next.present && cache[slot].state.value == next.value &&
                        memcmp(cache[slot].bits, bits, words * sizeof *bits) == 0) {
                        fresh = 0;
                        break;
                    }
                    slot = (slot + 1) & (cache_cap - 1);
                }
                if (fresh) {
                    cache[slot].bits = malloc(words * sizeof *bits);
                    memcpy(cache[slot].bits, bits, words * sizeof *bits);
                    cache[slot].state = next;
                    if (++cache_used * 2 > cache_cap) {
                        /* Grow the memo table, rehashing every set. */
                        size_t old_cap = cache_cap;
                        seen_t *old = cache;
                        cache_cap *= 2;
                        cache = calloc(cache_cap, sizeof *cache);
                        for (size_t k = 0; k < old_cap; k++) {
                            if (old[k].bits == NULL)
                                continue;
                            size_t s = hash_seen(old[k].bits, words, old[k].state) & (cache_cap - 1);
                            while (cache[s].bits != NULL)
                                s = (s + 1) & (cache_cap - 1);
                            cache[s] = old[k];
                        }
                        free(old);
                    }
                } else {
                    bits[e->id / 64] &= ~(1ull << (e->id % 64));
                }
            }
            if (fits && fresh) {
                /* Lift the operation out of the list and continue from the
                 * start. */
                stack[depth++] = (frame_t){e, state};
                state = next;
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
            /* A return reached before its call was lifted, or the end:
             * undo the last lift and try the next call after it. */
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
    for (size_t k = 0; k < cache_cap; k++)
        free(cache[k].bits);
    free(cache);
    free(stack);
    free(bits);
    free(order);
    free(entries);
    return ok;
}

int main(int argc, char **argv) {
    unsigned threads = 4, rounds = 20;
    nkeys = 4;
    nops = 4000;
    for (int i = 1; i + 1 < argc; i += 2) {
        unsigned v = (unsigned)atoi(argv[i + 1]);
        if (!strcmp(argv[i], "--threads"))
            threads = v;
        else if (!strcmp(argv[i], "--keys"))
            nkeys = v;
        else if (!strcmp(argv[i], "--ops"))
            nops = v;
        else if (!strcmp(argv[i], "--rounds"))
            rounds = v;
    }
    if (threads == 0 || threads > 64 || nkeys == 0 || nops == 0)
        return 2;
    for (unsigned round = 0; round < rounds; round++) {
        /* Alternate few keys on a map sized for them, where operations
         * contend, with many keys on a map grown from empty, where they
         * cross table moves. */
        round_keys = round % 2 ? nkeys : 64 * nkeys;
        map = CM(create)(round % 2 ? nkeys : 0);
        pthread_t t[64];
        log_t logs[64];
        atomic_store(&go, 0);
        for (unsigned i = 0; i < threads; i++) {
            logs[i] = (log_t){calloc(nops, sizeof(op_t)), nops, i + round * 64};
            pthread_create(&t[i], NULL, worker, &logs[i]);
        }
        atomic_store(&go, 1);
        for (unsigned i = 0; i < threads; i++)
            pthread_join(t[i], NULL);
        op_t **per_key = malloc((size_t)threads * nops * sizeof *per_key);
        for (unsigned k = 0; k < round_keys; k++) {
            unsigned n = 0;
            for (unsigned i = 0; i < threads; i++)
                for (unsigned j = 0; j < nops; j++)
                    if (logs[i].ops[j].key == (int)k)
                        per_key[n++] = &logs[i].ops[j];
            if (!linearizable(per_key, n)) {
                printf("stress-%s: round %u key %u is not linearizable (%u operations)\n", CM(name)(), round, k, n);
                return 1;
            }
        }
        free(per_key);
        for (unsigned i = 0; i < threads; i++)
            free(logs[i].ops);
        CM(destroy)(map);
    }
    printf("stress-%s: %u rounds of %u threads x %u operations on %u or %u keys linearizable\n", CM(name)(),
           rounds, threads, nops, nkeys, 64 * nkeys);
    return 0;
}
