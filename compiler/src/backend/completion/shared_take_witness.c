/* Phase-one count witness; see the pre-measurement protocol in
 * research/investigations/shared-take-fairness/DESIGN.md.
 *
 * Includes the private bridge as cancel_test.c does to place one real
 * context on each driver and dispatch its handwritten frame retries. X
 * holds a real map entry across wf__shared_take; each Y uses the emitted
 * wf__shared_acquire/unlock entries, including park, wake and handed holds.
 * Driver placement/stealing and compiled coroutine construction are outside
 * this witness. The only bridge hook counts unsuccessful take probes.
 *
 * Usage: shared-take-witness N K TRIAL (N = 0,1,2,8; K = 0 for N=0).
 * Exit 0: selected bound passed; 2: selected bound exceeded; 1: invalid
 * harness/run. The standalone research caller handles exit 2 explicitly.
 */
#if defined(__linux__)
#define _GNU_SOURCE
#endif
#define WF_SHARED_TAKE_WITNESS 1
#include "bridge.c"

#include <inttypes.h>
#include <pthread.h>
#include <time.h>

enum { MAX_Y = 8, BOUND_PER_Y = 64, DEADLINE_SECONDS = 60 };

typedef struct witness_executor {
    wf_driver driver;
    wf_context context;
    pthread_t thread;
    unsigned index;
    int frame;
} witness_executor;

static witness_executor executors[MAX_Y + 1];
static void *object, *map_object, *table;
static unsigned n_y;
static uint64_t cap, trial;
static _Atomic unsigned initial_held, first_failure, watchdog_done;
static _Atomic uint64_t y_completed;
static uint64_t first_count, acquired_count, failed_probes;
static unsigned drained_at_acquire;
static _Thread_local uint64_t missed;
static pthread_t watchdog_thread;

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "shared-take-witness:%d: %s\n", __LINE__, #test); \
    fflush(stderr); exit(1); \
} } while (0)

/* This hook observes only X's failed probes. It never waits, yields, delays
 * or changes admission state. Publishing the first failure ends setup; all
 * subsequent contention is unrestricted by the harness. */
static void wf_shared_take_witness_missed(void *attempted) {
    if (attempted != object) return;
    missed += 1u;
    if (missed == 1u) {
        first_count = atomic_load_explicit(&y_completed, memory_order_relaxed);
        atomic_store_explicit(&first_failure, 1u, memory_order_release);
    }
}

static uint64_t *object_count(void) {
    return (uint64_t *)((unsigned char *)object + WF_SHARED_STATE_OFFSET);
}

/* OS yielding here services only setup or a parked handwritten frame.
 * wf__shared_take's running X is never withheld by the harness. */
static void await_flag(_Atomic unsigned *flag) {
    while (atomic_load_explicit(flag, memory_order_acquire) == 0u)
        wf_prim_yield();
}

static void bind_executor(witness_executor *executor) {
    wf_executor_index = executor->index;
    wf_driver_self = &executor->driver;
    wf_driver_service = &executor->driver;
    wf_context_current = &executor->context;
    wf_context_outside = 0u;
    wf_context_parked_away = 0;
}

static void unbind_executor(void) {
    wf_context_current = NULL;
    wf_driver_service = NULL;
    wf_driver_self = NULL;
}

/* The native acquire frame has unwound when it answers 1. Commit its wake
 * gate just as the real dispatcher does: an early wake then enqueues it;
 * a later wake owns the enqueue. Retry only after its ready-queue removal. */
static void y_acquire(witness_executor *executor) {
    while (wf__shared_acquire(object, 1u, &executor->frame)) {
        CHECK(wf_context_parked_away != 0);
        wf_context_current = NULL;
        wf_context_commit_wake(&executor->context);
        wf_context *ready;
        do {
            ready = wf_run_take(&executor->driver);
            if (ready == NULL) wf_prim_yield();
        } while (ready == NULL);
        CHECK(ready == &executor->context);
        CHECK(ready->resume == &executor->frame);
        ready->resume = NULL;
        ready->passes = 0u;
        wf_context_parked_away = 0;
        wf_context_current = ready;
    }
}

static void y_complete(void) {
    *object_count() += 1u;
    /* Completion means the body has taken effect while O is still held.
     * X's acquisition snapshot cannot race this counter's increment. */
    atomic_fetch_add_explicit(&y_completed, 1u, memory_order_relaxed);
    wf__shared_unlock(object, 1u);
}

static void *y_thread(void *opaque) {
    witness_executor *executor = opaque;
    bind_executor(executor);
    uint64_t remaining = cap;
    if (executor->index == 1u) {
        y_acquire(executor);
        atomic_store_explicit(&initial_held, 1u, memory_order_release);
        /* Setup holder only: ensures one observed failed X probe. This
         * artificial setup hold is excluded from any source-program claim;
         * its completion counts as the first of Y0's K statements. */
        await_flag(&first_failure);
        y_complete();
        remaining -= 1u;
    } else {
        await_flag(&first_failure);
    }
    for (uint64_t statement = 0; statement < remaining; ++statement) {
        y_acquire(executor);
        y_complete();
    }
    CHECK(atomic_load_explicit(&executor->context.shared_granted, memory_order_relaxed) == 0u);
    CHECK(wf_run_take(&executor->driver) == NULL);
    unbind_executor();
    return NULL;
}

static void *x_thread(void *opaque) {
    witness_executor *executor = opaque;
    bind_executor(executor);
    if (n_y != 0u) await_flag(&initial_held);
    _Alignas(WF_TABLE_ENTRY_ALIGN) unsigned char record[WF_TABLE_ENTRY_SIZE];
    uint64_t *entry = wf__table_lock_entry(table, (const unsigned char *)"E", 1u, 0u,
                                          (struct wf_table_entry *)record);
    CHECK(entry != NULL && entry[0] == 1u && entry[1] == 17u);
    wf__shared_take(object, 1u);
    acquired_count = atomic_load_explicit(&y_completed, memory_order_relaxed);
    failed_probes = missed;
    drained_at_acquire = n_y != 0u && acquired_count == cap * n_y;
    CHECK(entry[0] == 1u && entry[1] == 17u);
    entry[1] = 18u;
    *object_count() += 1u;
    wf__shared_unlock(object, 1u);
    wf__table_unlock_entry((struct wf_table_entry *)record, 1u);
    unbind_executor();
    return NULL;
}

/* Elapsed time is only an invalid-run watchdog; it selects no fairness
 * verdict. A timeout supplies no overtaking measurement or bound failure. */
static void *watchdog(void *unused) {
    (void)unused;
    struct timespec start, now, pause = {0, 100000000};
    CHECK(clock_gettime(CLOCK_MONOTONIC, &start) == 0);
    while (atomic_load_explicit(&watchdog_done, memory_order_acquire) == 0u) {
        CHECK(clock_gettime(CLOCK_MONOTONIC, &now) == 0);
        if (now.tv_sec - start.tv_sec >= DEADLINE_SECONDS) {
            fputs("shared-take-witness: invalid run: watchdog expired\n", stderr);
            fflush(stderr);
            _Exit(1);
        }
        (void)nanosleep(&pause, NULL);
    }
    return NULL;
}

static uint64_t argument(const char *text) {
    char *end;
    CHECK(*text >= '0' && *text <= '9');
    errno = 0;
    unsigned long long value = strtoull(text, &end, 10);
    CHECK(errno == 0 && *end == '\0' && value <= UINT64_MAX);
    return (uint64_t)value;
}

static void executor_init(witness_executor *executor, unsigned index) {
    executor->index = index;
    executor->driver.index = index;
    executor->driver.runtime = &executor->driver.own_runtime;
    atomic_flag_clear(&executor->driver.run_lock);
    atomic_flag_clear(&executor->driver.ingress_lock);
    CHECK(wf_completion_runtime_init(executor->driver.runtime) == 0);
    executor->context.driver = &executor->driver;
}

int main(int argc, char **argv) {
    CHECK(argc == 4);
    uint64_t requested_n = argument(argv[1]);
    CHECK(requested_n == 0u || requested_n == 1u || requested_n == 2u || requested_n == 8u);
    n_y = (unsigned)requested_n;
    cap = argument(argv[2]);
    trial = argument(argv[3]);
    CHECK((n_y == 0u && cap == 0u) || (n_y != 0u && cap != 0u));
    CHECK(cap <= (UINT64_MAX - 1u) / MAX_Y);
    CHECK(pthread_create(&watchdog_thread, NULL, watchdog, NULL) == 0);

    object = wf__shared_new(sizeof(uint64_t));
    *object_count() = 0u;
    map_object = wf__shared_map_new(2u * sizeof(uint64_t), _Alignof(uint64_t), 0u);
    table = *(void **)((unsigned char *)map_object + WF_SHARED_STATE_OFFSET);
    _Alignas(WF_TABLE_ENTRY_ALIGN) unsigned char record[WF_TABLE_ENTRY_SIZE];
    uint64_t *entry = wf__table_lock_entry(table, (const unsigned char *)"E", 1u, 0u,
                                          (struct wf_table_entry *)record);
    CHECK(entry != NULL);
    entry[0] = 1u;
    entry[1] = 17u;
    wf__table_unlock_entry((struct wf_table_entry *)record, 1u);

    for (unsigned index = 0u; index <= n_y; ++index)
        executor_init(&executors[index], index);
    for (unsigned index = 1u; index <= n_y; ++index)
        CHECK(pthread_create(&executors[index].thread, NULL, y_thread, &executors[index]) == 0);
    CHECK(pthread_create(&executors[0].thread, NULL, x_thread, &executors[0]) == 0);
    for (unsigned index = 0u; index <= n_y; ++index)
        CHECK(pthread_join(executors[index].thread, NULL) == 0);

    uint64_t total = atomic_load_explicit(&y_completed, memory_order_relaxed);
    CHECK(total == cap * n_y && *object_count() == total + 1u);
    wf_shared *shared = object;
    CHECK(atomic_load_explicit(&shared->holders, memory_order_relaxed) == 0u);
    CHECK(shared->waiting_head == NULL && shared->waiting_tail == NULL);
    CHECK(atomic_load_explicit(&shared->granted, memory_order_relaxed) == NULL);
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 0u);
    CHECK(first_count <= acquired_count && acquired_count <= total);
    if (n_y != 0u) {
        CHECK(atomic_load_explicit(&first_failure, memory_order_acquire) == 1u);
        CHECK(first_count == 0u && failed_probes != 0u);
    } else {
        CHECK(failed_probes == 0u && acquired_count == 0u && first_count == 0u);
    }
    entry = wf__table_lock_entry(table, (const unsigned char *)"E", 1u, 1u,
                                 (struct wf_table_entry *)record);
    CHECK(entry != NULL && entry[0] == 1u && entry[1] == 18u);
    wf__table_unlock_entry((struct wf_table_entry *)record, 1u);
    CHECK(wf__keyed_table_count(table, 0u, sizeof(uint64_t), 0u) == 1u);
    while (wf__keyed_table_drain(table) != NULL) {
    }
    wf__keyed_table_free(table);
    CHECK(wf__shared_release(map_object) != 0);
    wf__shared_free(map_object);
    CHECK(wf__shared_release(object) != 0);
    wf__shared_free(object);
    for (unsigned index = 0u; index <= n_y; ++index)
        CHECK(wf_completion_runtime_destroy(executors[index].driver.runtime) == 0);
    atomic_store_explicit(&watchdog_done, 1u, memory_order_release);
    CHECK(pthread_join(watchdog_thread, NULL) == 0);

    uint64_t overtakes = acquired_count - first_count;
    uint64_t bound = (uint64_t)BOUND_PER_Y * n_y;
    int exceeded = overtakes > bound;
    puts("mode,n,cap,trial,bound,y_at_first_fail,y_at_acquire,overtakes,failed_probes,y_total,drained_before_acquire,result");
    printf("%s,%u,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64
           ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%u,%s\n",
           n_y == 0u ? "control" : "contended", n_y, cap, trial, bound, first_count,
           acquired_count, overtakes, failed_probes, total, drained_at_acquire,
           exceeded ? "bound_exceeded" : "bound_pass");
    CHECK(fflush(stdout) == 0);
    return exceeded ? 2 : 0;
}
