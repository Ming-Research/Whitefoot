/* Shared-take count witness; see the pre-measurement protocol in
 * research/investigations/shared-take-fairness/DESIGN.md.
 *
 * Includes the private bridge as cancel_test.c does to place one real
 * context on each driver and dispatch its handwritten frame retries. X
 * holds a real map entry across wf__shared_take; each Y uses the emitted
 * wf__shared_acquire/unlock entries, including park, wake and handed holds.
 * Driver placement/stealing and compiled coroutine construction are outside
 * this witness. The bridge hook counts unsuccessful take probes and, only
 * in the scheduled control, models preemption after claim publication.
 *
 * Usage: shared-take-witness N CAP TRIAL [--scheduled].
 * N = 0,1,2,8; CAP = 0 for N=0; --scheduled requires N=1 and CAP=65.
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
    _Atomic unsigned claim_ready, allow_claim, holding, release_hold, probes;
    wf_shared_take_claim *claim;
} witness_executor;

static witness_executor executors[MAX_Y + 1];
static void *object, *map_object, *table;
static unsigned n_y, scheduled, arbitration;
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

/* Natural trials only count misses. The scheduled control preempts X after
 * its first confirmed miss, outside O's lock and after claim publication.
 * Y runs until a real handoff reserves O for X, or all 65 statements finish.
 * The production algorithm therefore resumes after one Y completion; the
 * no-escalation mutant resumes after 65, exceeding B(1)=64 with exit 2.
 * Both builds run this same fixture. It guards claim escalation, failing on
 * the old spinner for the count criterion rather than a timeout or crash. */
static void wf_shared_take_witness_missed(void *attempted,
                                         wf_shared_take_claim *claim) {
    if (attempted != object) return;
    missed += 1u;
    if (arbitration) {
        witness_executor *executor = &executors[wf_executor_index];
        if (missed == 1u) {
            executor->claim = claim;
            atomic_store_explicit(&executor->claim_ready, 1u, memory_order_release);
        }
        atomic_fetch_add_explicit(&executor->probes, 1u, memory_order_release);
        while (atomic_load_explicit(&executor->allow_claim, memory_order_acquire) == 0u)
            wf_prim_yield();
        return;
    }
    if (missed == 1u) {
        first_count = atomic_load_explicit(&y_completed, memory_order_relaxed);
        atomic_store_explicit(&first_failure, 1u, memory_order_release);
        if (scheduled) {
            while (atomic_load_explicit(&claim->granted, memory_order_acquire) == 0u &&
                   atomic_load_explicit(&y_completed, memory_order_relaxed) < 65u)
                wf_prim_yield();
        }
    }
}

static uint64_t *object_count(void) {
    return (uint64_t *)((unsigned char *)object + WF_SHARED_STATE_OFFSET);
}

/* OS yielding here services only setup or a parked handwritten frame.
 * Natural trials never withhold wf__shared_take's running X. */
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

static void *arbitration_take(void *opaque) {
    witness_executor *executor = opaque;
    bind_executor(executor);
    wf__shared_take(object, 1u);
    atomic_store_explicit(&executor->holding, 1u, memory_order_release);
    await_flag(&executor->release_hold);
    wf__shared_unlock(object, 1u);
    unbind_executor();
    return NULL;
}

static void park_executor(unsigned index, uint32_t write, int due_handoff) {
    witness_executor *executor = &executors[index];
    bind_executor(executor);
    CHECK(wf__shared_acquire(object, write, &executor->frame) == 1);
    CHECK(wf_context_parked_away != 0);
    wf_context_current = NULL;
    wf_context_commit_wake(&executor->context);
    /* Fixtures name the already-due handoff boundary; shared_object_test
     * separately observes real vain wakes through the ordinary dispatcher. */
    if (due_handoff) executor->context.shared_woken = WF_SHARED_HANDOFF;
    bind_executor(&executors[0]);
}

static void resume_executor(unsigned index, uint32_t write) {
    witness_executor *executor = &executors[index];
    CHECK(wf_run_take(&executor->driver) == &executor->context);
    CHECK(executor->context.resume == &executor->frame);
    executor->context.resume = NULL;
    executor->context.passes = 0u;
    bind_executor(executor);
    CHECK(wf__shared_acquire(object, write, &executor->frame) == 0);
}

/* A returned borrow does not make a new physical release: the parked
 * grantee still owns its hold. With two pending claims only the head may
 * borrow it. Removing that head check lets X2 reserve a borrowed hold for
 * the paused X1, which the borrowed-state assertion detects; not permitting
 * queued borrowing leaves both claims waiting behind a grantee
 * on the kept driver, which this fixture reports as an invalid stalled run.
 * Existing shared_object_test covers the subsequent grantee claim veto. */
static void arbitration_borrow_check(void) {
    bind_executor(&executors[0]);
    CHECK(wf__shared_acquire(object, 1u, &executors[0].frame) == 0);
    park_executor(4u, 1u, 1);
    wf__shared_unlock(object, 1u);
    wf_shared *shared = object;
    CHECK(atomic_load_explicit(&shared->granted, memory_order_relaxed) == &executors[4].context);
    wf__shared_take(object, 1u);
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 1u);
    for (unsigned index = 1; index <= 2u; ++index) {
        witness_executor *executor = &executors[index];
        atomic_store_explicit(&executor->claim_ready, 0u, memory_order_relaxed);
        atomic_store_explicit(&executor->allow_claim, 0u, memory_order_relaxed);
        atomic_store_explicit(&executor->holding, 0u, memory_order_relaxed);
        atomic_store_explicit(&executor->release_hold, 0u, memory_order_relaxed);
        atomic_store_explicit(&executor->probes, 0u, memory_order_relaxed);
        CHECK(pthread_create(&executor->thread, NULL, arbitration_take, executor) == 0);
        await_flag(&executor->claim_ready);
    }
    wf__shared_unlock(object, 1u);
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 0u);
    atomic_store_explicit(&executors[2].allow_claim, 1u, memory_order_release);
    while (atomic_load_explicit(&executors[2].probes, memory_order_acquire) < 2u &&
           atomic_load_explicit(&executors[2].holding, memory_order_acquire) == 0u)
        wf_prim_yield();
    CHECK(atomic_load_explicit(&executors[2].holding, memory_order_acquire) == 0u);
    CHECK(atomic_load_explicit(&executors[2].claim->granted, memory_order_acquire) == 0u);
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 0u);
    atomic_store_explicit(&executors[1].allow_claim, 1u, memory_order_release);
    await_flag(&executors[1].holding);
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 1u);
    CHECK(atomic_load_explicit(&executors[2].holding, memory_order_acquire) == 0u);
    atomic_store_explicit(&executors[1].release_hold, 1u, memory_order_release);
    CHECK(pthread_join(executors[1].thread, NULL) == 0);
    await_flag(&executors[2].holding);
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 1u);
    atomic_store_explicit(&executors[2].release_hold, 1u, memory_order_release);
    CHECK(pthread_join(executors[2].thread, NULL) == 0);
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 0u);
    CHECK(atomic_load_explicit(&shared->granted, memory_order_relaxed) == &executors[4].context);
    resume_executor(4u, 1u);
    wf__shared_unlock(object, 1u);
}

/* Protects FIFO claims, draining initial readers, and alternating physical
 * parked grants with take claims. A reversed queue admits X2 first; reader
 * barging makes the late read acquire instead of park; omitting the forced
 * spinner turn gives R2 the hold ahead of X1. The original runtime has no
 * claim queue and fails the FIFO-publication assertion in this fixture. */
static int arbitration_check(void) {
    arbitration = 1u;
    CHECK(pthread_create(&watchdog_thread, NULL, watchdog, NULL) == 0);
    object = wf__shared_new(sizeof(uint64_t));
    for (unsigned index = 0; index < 6u; ++index)
        executor_init(&executors[index], index);
    bind_executor(&executors[0]);
    CHECK(wf__shared_acquire(object, 0u, &executors[0].frame) == 0);
    CHECK(wf__shared_acquire(object, 0u, &executors[0].frame) == 0);
    park_executor(4u, 1u, 1);
    park_executor(5u, 1u, 1);
    for (unsigned index = 1; index <= 2u; ++index) {
        CHECK(pthread_create(&executors[index].thread, NULL, arbitration_take,
                             &executors[index]) == 0);
        await_flag(&executors[index].claim_ready);
    }
    wf_shared *shared = object;
    wf_spin_lock(&shared->lock);
    wf_shared_take_claim *tail = atomic_load_explicit(&shared->take_tail, memory_order_relaxed);
    CHECK(tail == executors[2].claim && tail->next == executors[1].claim);
    CHECK(tail->next->next == tail);
    wf_spin_unlock(&shared->lock);
    park_executor(3u, 0u, 0);
    wf__shared_unlock(object, 0u);
    CHECK(atomic_load_explicit(&shared->holders, memory_order_relaxed) == 1u);
    CHECK(wf_run_take(&executors[4].driver) == NULL);
    wf__shared_unlock(object, 0u);
    CHECK(atomic_load_explicit(&shared->granted, memory_order_relaxed) == &executors[4].context);
    CHECK(atomic_load_explicit(&executors[1].claim->granted, memory_order_acquire) == 0u);
    resume_executor(4u, 1u);
    CHECK(atomic_load_explicit(&executors[1].claim->granted, memory_order_acquire) == 0u);
    wf__shared_unlock(object, 1u);
    CHECK(atomic_load_explicit(&executors[1].claim->granted, memory_order_acquire) == 1u);
    CHECK(atomic_load_explicit(&executors[2].claim->granted, memory_order_acquire) == 0u);
    CHECK(wf_run_take(&executors[5].driver) == NULL);
    atomic_store_explicit(&executors[1].allow_claim, 1u, memory_order_release);
    await_flag(&executors[1].holding);
    CHECK(wf_run_take(&executors[3].driver) == NULL);
    atomic_store_explicit(&executors[1].release_hold, 1u, memory_order_release);
    CHECK(pthread_join(executors[1].thread, NULL) == 0);
    CHECK(atomic_load_explicit(&shared->granted, memory_order_relaxed) == &executors[5].context);
    resume_executor(5u, 1u);
    wf__shared_unlock(object, 1u);
    CHECK(atomic_load_explicit(&executors[2].claim->granted, memory_order_acquire) == 1u);
    atomic_store_explicit(&executors[2].allow_claim, 1u, memory_order_release);
    await_flag(&executors[2].holding);
    atomic_store_explicit(&executors[2].release_hold, 1u, memory_order_release);
    CHECK(pthread_join(executors[2].thread, NULL) == 0);
    resume_executor(3u, 0u);
    wf__shared_unlock(object, 0u);
    arbitration_borrow_check();
    CHECK(shared->waiting_tail == NULL);
    CHECK(atomic_load_explicit(&shared->take_tail, memory_order_relaxed) == NULL);
    CHECK(atomic_load_explicit(&shared->holders, memory_order_relaxed) == 0u);
    CHECK(atomic_load_explicit(&shared->granted, memory_order_relaxed) == NULL);
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 0u);
    unbind_executor();
    CHECK(wf__shared_release(object) != 0);
    wf__shared_free(object);
    for (unsigned index = 0; index < 6u; ++index)
        CHECK(wf_completion_runtime_destroy(executors[index].driver.runtime) == 0);
    atomic_store_explicit(&watchdog_done, 1u, memory_order_release);
    CHECK(pthread_join(watchdog_thread, NULL) == 0);
    puts("shared-take-arbitration: all checks passed");
    return 0;
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "--arbitration") == 0)
        return arbitration_check();
    CHECK(argc == 4 || argc == 5);
    if (argc == 5) {
        CHECK(strcmp(argv[4], "--scheduled") == 0);
        scheduled = 1u;
    }
    uint64_t requested_n = argument(argv[1]);
    CHECK(requested_n == 0u || requested_n == 1u || requested_n == 2u || requested_n == 8u);
    n_y = (unsigned)requested_n;
    cap = argument(argv[2]);
    trial = argument(argv[3]);
    CHECK((n_y == 0u && cap == 0u) || (n_y != 0u && cap != 0u));
    CHECK(!scheduled || (n_y == 1u && cap == 65u));
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
    CHECK(shared->waiting_tail == NULL);
    CHECK(atomic_load_explicit(&shared->take_tail, memory_order_relaxed) == NULL);
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
