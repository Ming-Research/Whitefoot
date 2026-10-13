/* The sharded-state runtime with a counted host pool. The lock operations
 * below spell the same acquire/release protocol the emitter inlines: only
 * a contended acquisition enters wf__shards_wait. Whole holds take those
 * same locks in index order. Tests race complete snapshots and buffer
 * swap-outs against writers, then check exact totals and zero live grants.
 * The sanitizer builds check this C protocol; conformance exercises its
 * generated LLVM implementation and compiler-derived cleanup. */
#define _POSIX_C_SOURCE 200809L
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "sharded_state.c"

enum { WRITERS = 4, ITERATIONS = 4000 };
static _Atomic int64_t live_bytes;
static _Atomic uint64_t live_blocks;
static _Atomic uint64_t waits;
static uint64_t driver_count = WRITERS;
_Thread_local unsigned wf__map_user;

static void check(int condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "sharded-state-test: %s\n", message);
        exit(1);
    }
}

uint64_t wf__context_driver_count(void) { return driver_count; }
uint64_t wf__runtime_granted(uint64_t bytes) { return (bytes + 63u) / 64u * 64u; }
void *wf__runtime_take(uint64_t bytes) {
    uint64_t granted = wf__runtime_granted(bytes);
    void *block = NULL;
    check(posix_memalign(&block, 64u, (size_t)granted) == 0, "host allocation failed");
    atomic_fetch_add_explicit(&live_blocks, 1u, memory_order_relaxed);
    atomic_fetch_add_explicit(&live_bytes, (int64_t)granted, memory_order_relaxed);
    return block;
}
void wf__runtime_give(void *block, uint64_t bytes) {
    if (block == NULL)
        return;
    atomic_fetch_sub_explicit(&live_blocks, 1u, memory_order_relaxed);
    atomic_fetch_sub_explicit(&live_bytes, (int64_t)wf__runtime_granted(bytes), memory_order_relaxed);
    free(block);
}
void wf__runtime_yield(void) {
    atomic_fetch_add_explicit(&waits, 1u, memory_order_relaxed);
    sched_yield();
}
_Noreturn void wf__runtime_exhausted(void) { abort(); }

static wf_shard_lock *lock_at(wf_shards *object, uint64_t index) {
    return (wf_shard_lock *)((unsigned char *)object + WF_SHARDS_HEADER_SIZE + index * object->stride);
}
static void *value_at(wf_shards *object, uint64_t index) {
    return (unsigned char *)lock_at(object, index) + WF_SHARDS_VALUE_OFFSET;
}
static void take(wf_shard_lock *lock) {
    uint64_t ticket = atomic_fetch_add_explicit(&lock->next, 1u, memory_order_relaxed);
    if (atomic_load_explicit(&lock->serving, memory_order_acquire) != ticket)
        wf__shards_wait(lock, ticket);
}
static void give(wf_shard_lock *lock) {
    uint64_t ticket = atomic_load_explicit(&lock->serving, memory_order_relaxed);
    atomic_store_explicit(&lock->serving, ticket + 1u, memory_order_release);
}
static void hold(wf_shards *object) {
    for (uint64_t i = 0; i < object->len; ++i)
        take(lock_at(object, i));
}
static void unhold(wf_shards *object) {
    for (uint64_t i = 0; i < object->len; ++i)
        give(lock_at(object, i));
}
static int last_handle(wf_shards *object) {
    return atomic_fetch_sub_explicit(&object->handles, 1u, memory_order_acq_rel) == 1u;
}

/* Reserve the two tickets before starting their waiters in reverse order:
 * the later ticket must still follow the earlier one after release. */
typedef struct {
    wf_shard_lock *lock;
    uint64_t ticket;
    _Atomic unsigned *entered;
} queued;
static void *queued_waiter(void *argument) {
    queued *waiter = argument;
    wf__shards_wait(waiter->lock, waiter->ticket);
    check(atomic_fetch_add_explicit(waiter->entered, 1u, memory_order_relaxed)
        == waiter->ticket - 1u, "ticket order was bypassed");
    give(waiter->lock);
    return NULL;
}
static void layout_and_ticket_order(void) {
    driver_count = 1;
    wf_shards *single = wf__shards_new(65u);
    check(single->len == 1u && single->stride == 128u, "single-driver length or stride");
    check(((uintptr_t)single & 63u) == 0u, "header alignment");
    check(((uintptr_t)lock_at(single, 0) & 63u) == 0u, "shard alignment");
    check(((uintptr_t)value_at(single, 0) & 15u) == 0u, "value alignment");
    check(single->identity == (uint64_t)(uintptr_t)single, "object identity");
    unsigned char header[WF_SHARDS_HEADER_SIZE];
    memcpy(header, single, sizeof(header));
    take(lock_at(single, 0));
    _Atomic unsigned entered = 0;
    queued waiters[2];
    pthread_t threads[2];
    for (unsigned i = 0; i < 2; ++i) {
        waiters[i].lock = lock_at(single, 0);
        waiters[i].ticket = atomic_fetch_add_explicit(&waiters[i].lock->next, 1u, memory_order_relaxed);
        waiters[i].entered = &entered;
    }
    uint64_t before = atomic_load_explicit(&waits, memory_order_relaxed);
    check(pthread_create(&threads[1], NULL, queued_waiter, &waiters[1]) == 0, "later waiter start");
    check(pthread_create(&threads[0], NULL, queued_waiter, &waiters[0]) == 0, "first waiter start");
    while (atomic_load_explicit(&waits, memory_order_relaxed) == before)
        sched_yield();
    check(atomic_load_explicit(&entered, memory_order_relaxed) == 0u, "entered a held shard");
    give(lock_at(single, 0));
    for (unsigned i = 0; i < 2; ++i)
        check(pthread_join(threads[i], NULL) == 0, "ticket waiter join");
    check(atomic_load_explicit(&entered, memory_order_relaxed) == 2u, "lost a ticket waiter");
    check(memcmp(header, single, sizeof(header)) == 0, "lock operation wrote the object header");
    check(last_handle(single), "single last handle");
    wf__shards_free(single);
    driver_count = WRITERS;
}

typedef struct {
    uint64_t count;
    uint64_t twice;
} counter;
typedef struct {
    uint64_t *data;
    uint64_t len;
    uint64_t cap;
} buffer;
typedef struct {
    wf_shards *object;
    _Atomic unsigned start;
    _Atomic unsigned done;
    _Atomic uint64_t committed;
    uint64_t drained_count;
    uint64_t drained_sum;
    unsigned buffers;
} trial;
typedef struct { trial *test; unsigned user; } writer;

static void append(buffer *value, uint64_t item) {
    if (value->len == value->cap) {
        uint64_t cap = value->cap == 0 ? 2u : value->cap * 2u;
        uint64_t *data = wf__runtime_take(cap * sizeof(*data));
        if (value->len != 0)
            memcpy(data, value->data, (size_t)value->len * sizeof(*data));
        wf__runtime_give(value->data, value->cap * sizeof(*data));
        value->data = data;
        value->cap = cap;
    }
    value->data[value->len++] = item;
}
static void release_buffer(trial *test, buffer *value) {
    for (uint64_t i = 0; i < value->len; ++i) {
        uint64_t item = value->data[i];
        check((item >> 32u) < WRITERS && (uint32_t)item <= ITERATIONS, "corrupt buffer item");
        test->drained_sum += item;
    }
    test->drained_count += value->len;
    wf__runtime_give(value->data, value->cap * sizeof(*value->data));
    *value = (buffer){0};
}
static void *write_shard(void *argument) {
    writer *writer_state = argument;
    trial *test = writer_state->test;
    wf__map_user = writer_state->user;
    uint64_t index = wf__map_user % test->object->len;
    while (atomic_load_explicit(&test->start, memory_order_acquire) == 0u)
        sched_yield();
    for (uint64_t i = 0; i < ITERATIONS; ++i) {
        wf_shard_lock *lock = lock_at(test->object, index);
        take(lock);
        if (test->buffers) {
            append(value_at(test->object, index), ((uint64_t)wf__map_user << 32u) | i);
        } else {
            counter *value = value_at(test->object, index);
            value->count += 1u;
            value->twice += 2u;
        }
        atomic_fetch_add_explicit(&test->committed, 1u, memory_order_relaxed);
        give(lock);
    }
    atomic_fetch_add_explicit(&test->done, 1u, memory_order_release);
    return NULL;
}
static void *read_whole(void *argument) {
    trial *test = argument;
    buffer removed[2u * WRITERS];
    atomic_store_explicit(&test->start, 1u, memory_order_release);
    unsigned rounds = 0;
    do {
        hold(test->object);
        uint64_t total = 0;
        for (uint64_t i = 0; i < test->object->len; ++i) {
            if (test->buffers) {
                buffer *value = value_at(test->object, i);
                removed[i] = *value;
                *value = (buffer){0};
            } else {
                counter *value = value_at(test->object, i);
                check(value->twice == 2u * value->count, "whole reader saw a partial shard update");
                total += value->count;
            }
        }
        if (!test->buffers)
            check(total == atomic_load_explicit(&test->committed, memory_order_relaxed),
                "whole snapshot missed a committed update");
        unhold(test->object);
        if (test->buffers)
            for (uint64_t i = 0; i < test->object->len; ++i)
                release_buffer(test, &removed[i]);
        ++rounds;
    } while (atomic_load_explicit(&test->done, memory_order_acquire) != WRITERS || rounds < 100u);
    return NULL;
}
static void race(unsigned buffers) {
    trial test = {0};
    test.buffers = buffers;
    test.object = wf__shards_new(buffers ? sizeof(buffer) : sizeof(counter));
    check(test.object->len == 2u * WRITERS, "multi-driver length");
    uint64_t object_grant = atomic_load_explicit(&live_bytes, memory_order_relaxed);
    check(atomic_load_explicit(&live_blocks, memory_order_relaxed) == 1u,
        "shards used more than one allocation");
    check(object_grant == test.object->pool_bytes, "object grant accounting");
    for (uint64_t i = 0; i < test.object->len; ++i) {
        check(((uintptr_t)lock_at(test.object, i) & 63u) == 0u, "shard stride alignment");
        if (buffers) {
            buffer *value = value_at(test.object, i);
            *value = (buffer){.data = wf__runtime_take(2u * sizeof(uint64_t)), .cap = 2u};
        } else {
            *(counter *)value_at(test.object, i) = (counter){0};
        }
    }
    writer writers[WRITERS];
    pthread_t threads[WRITERS], reader;
    for (unsigned i = 0; i < WRITERS; ++i) {
        writers[i] = (writer){.test = &test, .user = i};
        check(pthread_create(&threads[i], NULL, write_shard, &writers[i]) == 0, "writer start");
    }
    check(pthread_create(&reader, NULL, read_whole, &test) == 0, "reader start");
    for (unsigned i = 0; i < WRITERS; ++i)
        check(pthread_join(threads[i], NULL) == 0, "writer join");
    check(pthread_join(reader, NULL) == 0, "reader join");
    if (buffers) {
        /* Leave owned contents for the last handle's cleanup even if the
         * reader's final swap emptied every writer slot. */
        for (unsigned i = 0; i < WRITERS; ++i)
            append(value_at(test.object, i), ((uint64_t)i << 32u) | ITERATIONS);
    } else {
        hold(test.object);
        uint64_t total = 0;
        for (uint64_t i = 0; i < test.object->len; ++i)
            total += ((counter *)value_at(test.object, i))->count;
        unhold(test.object);
        check(total == WRITERS * ITERATIONS, "final counter total");
    }
    check(last_handle(test.object), "object last handle");
    if (buffers) {
        for (uint64_t i = 0; i < test.object->len; ++i)
            release_buffer(&test, value_at(test.object, i));
        uint64_t per_writer = ITERATIONS + 1u;
        uint64_t expected = ((uint64_t)WRITERS * (WRITERS - 1u) / 2u << 32u) * per_writer
            + WRITERS * ((uint64_t)ITERATIONS * per_writer / 2u);
        check(test.drained_count == WRITERS * per_writer, "buffer swap and final cleanup lost items");
        check(test.drained_sum == expected, "buffer contents changed during swap and cleanup");
    }
    check((uint64_t)atomic_load_explicit(&live_bytes, memory_order_relaxed) == object_grant,
        "owned content survived final cleanup");
    wf__shards_free(test.object);
    check(atomic_load_explicit(&live_bytes, memory_order_relaxed) == 0,
        "live bytes remained after object release");
    check(atomic_load_explicit(&live_blocks, memory_order_relaxed) == 0u,
        "live blocks remained after object release");
}

int main(void) {
    layout_and_ticket_order();
    race(0);
    race(1);
    puts("sharded-state-test: passed");
    return 0;
}
