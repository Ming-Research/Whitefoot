/* One-shot control verdicts on two native threads representing two drivers.
 * The private bridge inclusion supplies driver/context identities, as in
 * cancel_test.c; it adds no test-selected path to production code.
 *
 * Before decision cells exist this probe fails to compile/link their ABI.
 * With the ABI present it distinguishes early publication from publication
 * only at statement end, rejects a lost/changed winner, catches a claim that
 * takes the shared-header lock, and exercises retained-handle lifetime.
 * The 100 ms work window is a semantic witness, not a performance sample;
 * barriers, rather than the host's speed, determine observation order. */
#define _GNU_SOURCE
#include "bridge.c"
#include "../runtime_test_guard.h"
#include <pthread.h>

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "decision-cell-test:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

/* Single-use two-party barrier; usable on macOS as well as Linux. */
typedef struct { _Atomic unsigned arrivals; } test_barrier;

static void meet(test_barrier *barrier) {
    atomic_fetch_add_explicit(&barrier->arrivals, 1u, memory_order_acq_rel);
    while (atomic_load_explicit(&barrier->arrivals, memory_order_acquire) != 2u)
        wf_prim_yield();
}

enum schedule { EARLY_WRITTEN, END_WRITTEN, KILL_FIRST };
typedef struct {
    void *cell;
    void *held;
    enum schedule order;
    test_barrier begin;
    test_barrier observed;
    test_barrier killed;
    uint32_t seen;
    uint32_t kill_result;
} test_race;

static wf_driver other_driver;

static void release_cell(void *cell) {
    if (wf__shared_release(cell)) wf__shared_free(cell);
}

static void *other_context(void *opaque) {
    test_race *race = opaque;
    wf_context context = {0};
    context.driver = &other_driver;
    wf_driver_self = &other_driver;
    wf_context_current = &context;
    if (race->order == KILL_FIRST) {
        race->kill_result = wf__claim_killed(race->cell);
        meet(&race->killed);
    }
    meet(&race->begin);
    /* A still holds X: B's result must not wait for A's statement end. */
    CHECK(atomic_load(&((wf_shared *)race->held)->holders) == WF_SHARED_WRITER);
    race->seen = wf__decision_cell_read(race->cell);
    if (race->order == EARLY_WRITTEN)
        race->kill_result = wf__claim_killed(race->cell);
    meet(&race->observed);
    release_cell(race->cell);
    wf_context_current = NULL;
    wf_driver_self = NULL;
    return NULL;
}

static void compute_window(void) {
    uint64_t start = wf_prim_monotonic_us();
    CHECK(start != 0);
    volatile uint64_t work = 1;
    do {
        for (unsigned i = 0; i < 1024; ++i)
            work = work * UINT64_C(6364136223846793005) + 1;
    } while (wf_prim_monotonic_us() - start < UINT64_C(100000));
}

/* A accepts a no-op write only after a Written verdict. EARLY_WRITTEN
 * records Written while X remains held; END_WRITTEN is the control that
 * waits until A ends, so B must see Open; KILL_FIRST prevents acceptance.
 * Publishing at unlock would fail EARLY_WRITTEN; treating an Open read as
 * a promise or ignoring a losing claim would fail KILL_FIRST. */
static void ordered_race(enum schedule order) {
    test_race race = { .cell = wf__decision_cell_new(),
                       .held = wf__shared_new(sizeof(uint32_t)), .order = order };
    uint32_t *value = (uint32_t *)((unsigned char *)race.held + WF_SHARED_STATE_OFFSET);
    *value = 7;
    CHECK(wf__decision_cell_read(race.cell) == WF_DECISION_OPEN);
    wf__shared_share(race.cell);
    pthread_t thread;
    CHECK(pthread_create(&thread, NULL, other_context, &race) == 0);
    if (order == KILL_FIRST) meet(&race.killed);
    wf__shared_take(race.held, 1u);
    uint32_t written = WF_DECISION_OPEN;
    unsigned accepted = 0;
    if (order != END_WRITTEN) {
        written = wf__claim_written(race.cell);
        if (written == WF_DECISION_WRITTEN) {
            *value = 7; /* Even an unchanged value is an accepted write. */
            accepted++;
        }
    }
    meet(&race.begin);
    compute_window();
    meet(&race.observed);
    wf__shared_unlock(race.held, 1u);
    if (order == END_WRITTEN) written = wf__claim_written(race.cell);
    CHECK(pthread_join(thread, NULL) == 0);
    if (order == KILL_FIRST) {
        CHECK(race.seen == WF_DECISION_KILLED);
        CHECK(race.kill_result == WF_DECISION_KILLED);
        CHECK(written == WF_DECISION_KILLED && accepted == 0);
    } else {
        uint32_t expected = order == EARLY_WRITTEN ? WF_DECISION_WRITTEN : WF_DECISION_OPEN;
        CHECK(written == WF_DECISION_WRITTEN);
        CHECK(race.seen == expected);
        if (order == EARLY_WRITTEN) {
            CHECK(race.kill_result == WF_DECISION_WRITTEN);
            CHECK(accepted == 1);
        }
    }
    CHECK(*value == 7);
    /* Both repeated claims return the same absorbing winner. */
    CHECK(wf__claim_written(race.cell) == written);
    CHECK(wf__claim_killed(race.cell) == written);
    CHECK(wf__decision_cell_read(race.cell) == written);
    release_cell(race.cell);
    CHECK(wf__shared_release(race.held));
    wf__shared_free(race.held);
}

/* Holding the otherwise unused header lock makes accidental acquisition in
 * a read/claim fail the watchdog. Releasing the original handle before the
 * alias is used catches a missing retain or a premature free. */
static void independent_word(void) {
    void *cell = wf__decision_cell_new();
    wf__shared_share(cell);
    CHECK(!wf__shared_release(cell));
    wf_shared *header = cell;
    CHECK(!atomic_flag_test_and_set(&header->lock));
    CHECK(wf__decision_cell_read(cell) == WF_DECISION_OPEN);
    CHECK(wf__claim_written(cell) == WF_DECISION_WRITTEN);
    CHECK(wf__claim_killed(cell) == WF_DECISION_WRITTEN);
    CHECK(wf__decision_cell_read(cell) == WF_DECISION_WRITTEN);
    atomic_flag_clear(&header->lock);
    CHECK(wf__shared_release(cell));
    wf__shared_free(cell);
}

int main(void) {
    wf_test_guard_start(10);
    wf__context_root_begin();
    other_driver.runtime = &other_driver.own_runtime;
    CHECK(wf_completion_runtime_init(other_driver.runtime) == 0);
    atomic_store(&wf_drivers[1], &other_driver);
    atomic_store(&wf_driver_count, 2u);
    wf_test_guard_phase("decision cell: Written visible while X remains held");
    ordered_race(EARLY_WRITTEN);
    wf_test_guard_phase("decision cell: publication at statement end control");
    ordered_race(END_WRITTEN);
    wf_test_guard_phase("decision cell: Killed wins before the no-op write");
    ordered_race(KILL_FIRST);
    wf_test_guard_phase("decision cell: independent atomic word and retained alias");
    independent_word();
    atomic_store(&wf_driver_count, 1u);
    while (atomic_load(&wf_drivers_notifying) != 0u) wf_prim_yield();
    atomic_store(&wf_drivers[1], NULL);
    CHECK(wf_completion_runtime_destroy(other_driver.runtime) == 0);
    wf_test_guard_finish();
    return 0;
}
