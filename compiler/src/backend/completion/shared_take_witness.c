/* Shared-take count witness and claim arbitration fixtures; see the
 * protocol in research/investigations/shared-take-fairness/DESIGN.md.
 *
 * Includes the private bridge as cancel_test.c does to place one real
 * context on each driver and dispatch its handwritten frame retries. X
 * holds a real map entry across wf__shared_take; each Y uses the emitted
 * wf__shared_acquire/unlock entries, including park, wake and handed holds.
 * Driver placement/stealing and compiled coroutine construction are outside
 * this witness. The bridge hook counts unsuccessful take looks and, only
 * in the scheduled control, models preemption after the claim is queued.
 *
 * Usage: shared-take-witness N CAP TRIAL [--scheduled]
 *        shared-take-witness --arbitration [order|newcomer|borrow|reservation]
 * N = 0,1,2,8; CAP = 0 for N=0; --scheduled requires N=1 and CAP=65.
 * Count mode exits 0 when the bound passed, 2 when it was exceeded and 1
 * for an invalid run. Arbitration mode runs every case, or the one named,
 * and exits 1 with a message naming the first broken rule.
 */
#if defined(__linux__)
#define _GNU_SOURCE
#endif
#define WF_SHARED_TAKE_WITNESS 1
#include "bridge.c"

#include <inttypes.h>
#include <pthread.h>
#include <time.h>

/* A shared object whose state is a map, as `shared_new(map_new(capacity))`
 * makes one. */
static void *witness_shared_map(uint64_t slot_size, uint64_t slot_align, uint64_t capacity) {
    void *object = wf__shared_new(sizeof(void *));
    *(void **)((unsigned char *)object + WF_SHARED_STATE_OFFSET) = wf__map_new(slot_size, slot_align, capacity);
    return object;
}

enum { MAX_Y = 8, BOUND_PER_Y = 64, DEADLINE_SECONDS = 60 };
/* The arbitration fixtures' executors, and how many more failed looks a
 * claim allowed to run makes before the fixture decides it did not hold. */
enum { ARBITRATION_EXECUTORS = 6, LOOKS_WITHOUT_BORROW = 3 };

typedef struct witness_executor {
    wf_driver driver;
    wf_context context;
    pthread_t thread;
    unsigned index;
    int frame;
    uint32_t write;
    /* Arbitration fixtures: the take's live claim once it missed, whether
     * the fixture lets it look again, its failed looks, whether it holds
     * the object, the fixture's word to unlock it, and whether a miss is
     * itself a broken rule. */
    _Atomic unsigned claim_ready, allow_claim, holding, release_hold, probes, must_not_miss;
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

/* A broken arbitration rule, named by its message; the mutant table in
 * research/investigations/shared-take-fairness/DESIGN.md lists these. */
#define REQUIRE(test, message) do { if (!(test)) { \
    fprintf(stderr, "shared-take-witness: %s\n", message); \
    fflush(stderr); exit(1); \
} } while (0)

/* Natural trials only count misses. The scheduled control preempts X after
 * its first confirmed miss, outside O's lock and after its claim is queued.
 * Y runs until an unlock grants X's claim, or all 65 statements finish.
 * The production algorithm therefore resumes after one Y completion; the
 * no-escalation build resumes after 65, exceeding B(1)=64 with exit 2.
 * Both builds run this same fixture. It guards the claim, failing on the
 * old spinner for the count criterion rather than a timeout or crash. */
static void wf_shared_take_witness_missed(void *attempted,
                                         wf_shared_take_claim *claim) {
    if (attempted != object) return;
    missed += 1u;
    if (arbitration) {
        witness_executor *executor = &executors[wf_executor_index];
        REQUIRE(atomic_load_explicit(&executor->must_not_miss, memory_order_relaxed) == 0u,
                "a take did not borrow the hold handed to a parked context");
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

/* ----------------------------------------------- arbitration fixtures */

/* A take on its own thread, standing for a statement on another driver
 * that holds an earlier target; it keeps the object until told. */
static void *arbitration_take(void *opaque) {
    witness_executor *executor = opaque;
    bind_executor(executor);
    wf__shared_take(object, executor->write);
    atomic_store_explicit(&executor->holding, 1u, memory_order_release);
    await_flag(&executor->release_hold);
    wf__shared_unlock(object, executor->write);
    unbind_executor();
    return NULL;
}

/* Starts a take on executor `index` that is expected to miss, and waits
 * until it has missed and waits for the fixture's word, or, against the
 * rule the caller then checks, holds the object at once. */
static void start_claim(unsigned index, uint32_t write) {
    witness_executor *executor = &executors[index];
    executor->write = write;
    executor->claim = NULL;
    atomic_store_explicit(&executor->claim_ready, 0u, memory_order_relaxed);
    atomic_store_explicit(&executor->allow_claim, 0u, memory_order_relaxed);
    atomic_store_explicit(&executor->holding, 0u, memory_order_relaxed);
    atomic_store_explicit(&executor->release_hold, 0u, memory_order_relaxed);
    atomic_store_explicit(&executor->probes, 0u, memory_order_relaxed);
    CHECK(pthread_create(&executor->thread, NULL, arbitration_take, executor) == 0);
    while (atomic_load_explicit(&executor->claim_ready, memory_order_acquire) == 0u
           && atomic_load_explicit(&executor->holding, memory_order_acquire) == 0u)
        wf_prim_yield();
}

/* Lets a claim's take look again and waits until it holds the object or
 * has looked LOOKS_WITHOUT_BORROW more times; answers whether it holds. */
static int claim_runs(unsigned index) {
    witness_executor *executor = &executors[index];
    unsigned before = atomic_load_explicit(&executor->probes, memory_order_acquire);
    atomic_store_explicit(&executor->allow_claim, 1u, memory_order_release);
    while (atomic_load_explicit(&executor->holding, memory_order_acquire) == 0u
           && atomic_load_explicit(&executor->probes, memory_order_acquire) - before < LOOKS_WITHOUT_BORROW)
        wf_prim_yield();
    return atomic_load_explicit(&executor->holding, memory_order_acquire) != 0u;
}

static void end_claim(unsigned index) {
    atomic_store_explicit(&executors[index].release_hold, 1u, memory_order_release);
    CHECK(pthread_join(executors[index].thread, NULL) == 0);
}

/* Whether an unlock granted the claim of a take still waiting in the
 * fixture's hook, whose record is therefore live. */
static unsigned granted_to(unsigned index) {
    wf_shared_take_claim *claim = executors[index].claim;
    return claim != NULL && atomic_load_explicit(&claim->granted, memory_order_acquire) != 0u;
}

/* The object's claims in queue order, oldest first, under its lock. */
static unsigned claims_in_order(unsigned first, unsigned second) {
    wf_shared *shared = object;
    wf_spin_lock(&shared->lock);
    wf_shared_take_claim *tail = wf_shared_claims(shared);
    unsigned ordered = tail == executors[second].claim && tail->next == executors[first].claim
        && tail->next->next == tail;
    wf_spin_unlock(&shared->lock);
    return ordered;
}

static unsigned any_claim(void) {
    wf_shared *shared = object;
    wf_spin_lock(&shared->lock);
    unsigned queued = wf_shared_claims(shared) != NULL;
    wf_spin_unlock(&shared->lock);
    return queued;
}

/* Parks executor `index`'s context on the object; `due` primes the count of
 * vain wakes so the next unlock hands it the object. shared_object_test
 * observes real vain wakes through the ordinary dispatcher. */
static void park_executor(unsigned index, uint32_t write, int due) {
    witness_executor *executor = &executors[index];
    bind_executor(executor);
    CHECK(wf__shared_acquire(object, write, &executor->frame) == 1);
    CHECK(wf_context_parked_away != 0);
    wf_context_current = NULL;
    wf_context_commit_wake(&executor->context);
    if (due) executor->context.shared_woken = WF_SHARED_HANDOFF;
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

static void arbitration_begin(void) {
    object = wf__shared_new(sizeof(uint64_t));
    for (unsigned index = 0; index < ARBITRATION_EXECUTORS; ++index)
        executor_init(&executors[index], index);
    bind_executor(&executors[0]);
}

static void arbitration_end(void) {
    wf_shared *shared = object;
    bind_executor(&executors[0]);
    CHECK(shared->waiting_tail == NULL);
    CHECK(shared->arbiter == 0u);
    CHECK(shared->take_turn == 0u);
    CHECK(atomic_load_explicit(&shared->holders, memory_order_relaxed) == 0u);
    CHECK(atomic_load_explicit(&shared->granted, memory_order_relaxed) == NULL);
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 0u);
    for (unsigned index = 0; index < ARBITRATION_EXECUTORS; ++index) {
        CHECK(wf_run_take(&executors[index].driver) == NULL);
        CHECK(executors[index].context.shared_holds == 0u);
    }
    unbind_executor();
    CHECK(wf__shared_release(object) != 0);
    wf__shared_free(object);
    for (unsigned index = 0; index < ARBITRATION_EXECUTORS; ++index) {
        CHECK(wf_completion_runtime_destroy(executors[index].driver.runtime) == 0);
        memset(&executors[index], 0, sizeof(executors[index]));
    }
}

/* Claims queue in arrival order behind readers already holding the object,
 * refuse a reader that arrives after them, and take turns with parked
 * contexts' hand-offs: the parked head gets its hand-off first, then the
 * oldest claim, then the next due parked context, then the next claim. */
static void arbitration_order(void) {
    arbitration_begin();
    wf_shared *shared = object;
    CHECK(wf__shared_acquire(object, 0u, &executors[0].frame) == 0);
    CHECK(wf__shared_acquire(object, 0u, &executors[0].frame) == 0);
    park_executor(4u, 1u, 1);
    park_executor(5u, 1u, 1);
    start_claim(1u, 1u);
    REQUIRE(any_claim(), "a take that missed queued no claim");
    start_claim(2u, 1u);
    REQUIRE(claims_in_order(1u, 2u), "claims did not queue in arrival order");
    /* A reader arriving while readers hold the object and claims wait. */
    bind_executor(&executors[3]);
    REQUIRE(wf__shared_acquire(object, 0u, &executors[3].frame) == 1,
            "a reader overtook a waiting claim");
    CHECK(wf_context_parked_away != 0);
    wf_context_current = NULL;
    wf_context_commit_wake(&executors[3].context);
    bind_executor(&executors[0]);
    wf__shared_unlock(object, 0u);
    CHECK(atomic_load_explicit(&shared->holders, memory_order_relaxed) == 1u);
    CHECK(granted_to(1u) == 0u && wf_run_take(&executors[4].driver) == NULL);
    wf__shared_unlock(object, 0u);
    REQUIRE(atomic_load_explicit(&shared->granted, memory_order_relaxed) == &executors[4].context
            && granted_to(1u) == 0u,
            "a claim was granted ahead of the parked head's due hand-off");
    resume_executor(4u, 1u);
    wf__shared_unlock(object, 1u);
    REQUIRE(granted_to(1u) != 0u && wf_run_take(&executors[5].driver) == NULL,
            "a parked context was handed the object again while a claim waited");
    REQUIRE(granted_to(2u) == 0u, "a release granted a later claim first");
    REQUIRE(claim_runs(1u), "a granted claim did not hold the object");
    CHECK(wf_run_take(&executors[3].driver) == NULL);
    end_claim(1u);
    REQUIRE(atomic_load_explicit(&shared->granted, memory_order_relaxed) == &executors[5].context
            && granted_to(2u) == 0u,
            "claims took the object twice in a row while a parked context was due");
    resume_executor(5u, 1u);
    wf__shared_unlock(object, 1u);
    REQUIRE(granted_to(2u) != 0u, "a parked context was handed the object again while a claim waited");
    REQUIRE(claim_runs(2u), "a granted claim did not hold the object");
    end_claim(2u);
    resume_executor(3u, 0u);
    wf__shared_unlock(object, 0u);
    arbitration_end();
}

/* A newcomer's take while a claim waits queues behind it rather than
 * borrowing a hold handed to a parked context, or reading beside readers. */
static void arbitration_newcomer(void) {
    arbitration_begin();
    wf_shared *shared = object;
    CHECK(wf__shared_acquire(object, 1u, &executors[0].frame) == 0);
    park_executor(4u, 1u, 1);
    start_claim(1u, 1u);
    wf__shared_unlock(object, 1u);
    CHECK(atomic_load_explicit(&shared->granted, memory_order_relaxed) == &executors[4].context);
    CHECK(granted_to(1u) == 0u);
    /* The hold is lendable and the oldest claim is held back: a newcomer
     * that borrowed now would overtake it. */
    start_claim(2u, 1u);
    REQUIRE(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 0u
            && claims_in_order(1u, 2u),
            "a newcomer take borrowed ahead of a waiting claim");
    REQUIRE(claim_runs(1u), "the oldest claim did not borrow the hold handed to a parked context");
    end_claim(1u);
    REQUIRE(claim_runs(2u), "the next claim did not borrow the returned hold");
    end_claim(2u);
    resume_executor(4u, 1u);
    wf__shared_unlock(object, 1u);
    /* Readers hold the object and a writer's claim waits behind them: a
     * reading newcomer's take queues too. */
    bind_executor(&executors[0]);
    CHECK(wf__shared_acquire(object, 0u, &executors[0].frame) == 0);
    start_claim(1u, 1u);
    start_claim(2u, 0u);
    REQUIRE(atomic_load_explicit(&shared->holders, memory_order_relaxed) == 1u
            && claims_in_order(1u, 2u),
            "a newcomer take read ahead of a waiting claim");
    wf__shared_unlock(object, 0u);
    REQUIRE(granted_to(1u) != 0u && granted_to(2u) == 0u, "a release granted a later claim first");
    REQUIRE(claim_runs(1u), "a granted claim did not hold the object");
    end_claim(1u);
    REQUIRE(claim_runs(2u), "a granted claim did not hold the object");
    end_claim(2u);
    arbitration_end();
}

/* A take with no claim before it borrows a hold handed to a parked context;
 * while that borrow is under way two takes queue claims, and once it ends
 * only the oldest borrows, then the next: borrowing never waits for the
 * parked context, which may wait on the very driver a claim keeps, and a
 * borrow's end gives the parked context its hold back. */
static void arbitration_borrow(void) {
    arbitration_begin();
    wf_shared *shared = object;
    CHECK(wf__shared_acquire(object, 1u, &executors[0].frame) == 0);
    park_executor(4u, 1u, 1);
    wf__shared_unlock(object, 1u);
    CHECK(atomic_load_explicit(&shared->granted, memory_order_relaxed) == &executors[4].context);
    atomic_store_explicit(&executors[0].must_not_miss, 1u, memory_order_relaxed);
    wf__shared_take(object, 1u);
    atomic_store_explicit(&executors[0].must_not_miss, 0u, memory_order_relaxed);
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 1u);
    start_claim(1u, 1u);
    start_claim(2u, 1u);
    CHECK(claims_in_order(1u, 2u));
    wf__shared_unlock(object, 1u);
    REQUIRE(atomic_load_explicit(&shared->granted, memory_order_relaxed) == &executors[4].context,
            "a borrow's end took the parked context's hold away");
    REQUIRE(!claim_runs(2u) && granted_to(2u) == 0u
            && atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 0u,
            "a later claim borrowed ahead of the oldest");
    REQUIRE(claim_runs(1u), "the oldest claim did not borrow the hold handed to a parked context");
    CHECK(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 1u);
    CHECK(atomic_load_explicit(&executors[2].holding, memory_order_acquire) == 0u);
    end_claim(1u);
    await_flag(&executors[2].holding);
    REQUIRE(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 1u,
            "the next claim did not borrow the returned hold");
    end_claim(2u);
    REQUIRE(atomic_load_explicit(&shared->borrowed, memory_order_relaxed) == 0u
            && atomic_load_explicit(&shared->granted, memory_order_relaxed) == &executors[4].context,
            "a borrow's end took the parked context's hold away");
    resume_executor(4u, 1u);
    wf__shared_unlock(object, 1u);
    arbitration_end();
}

/* A guard's reservation, owned by a context that has not retried yet (its
 * guard may need the entry the claim's owner holds), refuses a parked
 * newcomer but not a claim: the unlock grants the claim and wakes no one.
 * The queue keeps the reservation while it names a claim, and gives it
 * back when the last claim is granted; the owner's own retry then ends it
 * and the newcomer gets its turn. The reservation is installed as a
 * notification installs it; shared_object_test covers notifications. */
static void arbitration_reservation(void) {
    arbitration_begin();
    wf_shared *shared = object;
    wf_context *owner = &executors[5].context;
    CHECK(wf__shared_acquire(object, 1u, &executors[0].frame) == 0);
    wf_spin_lock(&shared->lock);
    wf_shared_set_reservation(shared, owner);
    owner->shared_turn = shared;
    owner->guard_reserved = 1u;
    wf_spin_unlock(&shared->lock);
    park_executor(4u, 1u, 0);
    start_claim(1u, 1u);
    wf_spin_lock(&shared->lock);
    unsigned kept = wf_shared_reservation(shared) == owner && wf_shared_claims(shared) != NULL;
    wf_spin_unlock(&shared->lock);
    REQUIRE(kept, "a claim's queue lost the object's reservation");
    wf__shared_unlock(object, 1u);
    REQUIRE(granted_to(1u) != 0u, "a claim waited behind a guard's reservation");
    CHECK(wf_run_take(&executors[4].driver) == NULL);
    REQUIRE(shared->arbiter == (uintptr_t)owner,
            "the last claim's grant lost the object's reservation");
    REQUIRE(claim_runs(1u), "a granted claim did not hold the object");
    end_claim(1u);
    CHECK(wf_run_take(&executors[4].driver) == NULL);
    bind_executor(&executors[5]);
    CHECK(wf__shared_acquire(object, 1u, &executors[5].frame) == 0);
    wf__shared_unlock(object, 1u);
    CHECK(owner->shared_turn == NULL && owner->guard_reserved == 0u);
    resume_executor(4u, 1u);
    wf__shared_unlock(object, 1u);
    arbitration_end();
}

static const struct {
    const char *name;
    void (*run)(void);
} arbitration_cases[] = {
    {"order", arbitration_order},
    {"newcomer", arbitration_newcomer},
    {"borrow", arbitration_borrow},
    {"reservation", arbitration_reservation},
};

static int arbitration_check(const char *selected) {
    unsigned ran = 0u;
    arbitration = 1u;
    CHECK(pthread_create(&watchdog_thread, NULL, watchdog, NULL) == 0);
    for (size_t index = 0; index < sizeof(arbitration_cases) / sizeof(arbitration_cases[0]); ++index) {
        if (selected != NULL && strcmp(selected, arbitration_cases[index].name) != 0) continue;
        arbitration_cases[index].run();
        printf("shared-take-arbitration: %s passed\n", arbitration_cases[index].name);
        ran += 1u;
    }
    CHECK(ran != 0u);
    atomic_store_explicit(&watchdog_done, 1u, memory_order_release);
    CHECK(pthread_join(watchdog_thread, NULL) == 0);
    CHECK(fflush(stdout) == 0);
    return 0;
}

int main(int argc, char **argv) {
    if (argc >= 2 && strcmp(argv[1], "--arbitration") == 0) {
        CHECK(argc <= 3);
        return arbitration_check(argc == 3 ? argv[2] : NULL);
    }
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
    map_object = witness_shared_map(2u * sizeof(uint64_t), _Alignof(uint64_t), 0u);
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
    CHECK(shared->arbiter == 0u);
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
