/* Deterministic D ownership schedules. The private bridge exposes the same
 * scope boundaries its driver loop uses; no production test branch exists.
 * driver_service_frames.c supplies handwritten coroutine entries, as the
 * shared-object probe does. The watchdog only diagnoses a hang: no elapsed
 * time selects a verdict, and every timer instant is explicit test data. */
#define _GNU_SOURCE
#include "bridge.c"
#include "../runtime_test_guard.h"
#include <pthread.h>

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "driver-service-test:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

static wf_driver assistant;
static _Atomic unsigned computing, release_compute, ran_timer;
enum { FRAME_COMPUTE, FRAME_TIMER, FRAME_EARLY_WAKE };
typedef struct service_frame {
    unsigned kind;
    int done;
    uint64_t group[2];
} service_frame;

void wf_driver_service_test_resume(void *opaque) {
    service_frame *frame = opaque;
    CHECK(wf_driver_service == NULL);
    CHECK((atomic_load(&wf_driver_self->service_token) & WF_DRIVER_PHASE_MASK)
          == WF_DRIVER_OUTSIDE);
    if (frame->kind == FRAME_COMPUTE) {
        CHECK(wf__driver_index() == 0u);
        atomic_store(&computing, 1u);
        while (!atomic_load(&release_compute)) wf_prim_yield();
        CHECK(wf__driver_index() == 0u);
        CHECK(wf__context_join_wait(frame->group, frame) == 1);
    } else if (frame->kind == FRAME_TIMER) {
        CHECK(wf__driver_index() == 1u);
        CHECK(wf_context_current == &wf_context_root);
        CHECK(wf_context_current->driver == &assistant);
        CHECK(wf_bridge_record_state((wf_completion_record *)wf_context_current->operation.bytes)
              == WF_COMPLETION_DONE);
        /* Service authority has been returned before a detached context runs. */
        CHECK((atomic_load(&wf_driver_root.service_token) & WF_DRIVER_PHASE_MASK)
              == WF_DRIVER_OUTSIDE);
        atomic_fetch_add(&ran_timer, 1u);
        frame->done = 1;
        atomic_store(&release_compute, 1u);
        atomic_store(&wf_drivers_stopping, 1u);
    } else {
        CHECK(wf__context_join_wait(frame->group, frame) == 1);
        wf_group_finish(frame->group);
        CHECK(atomic_load(&wf_context_current->wake_gate) == WF_WAKE_EARLY);
        CHECK(atomic_load(&wf_driver_self->run_count) == 0u);
        CHECK(!wf_driver_steal(&assistant));
    }
}

int wf_driver_service_test_done(void *opaque) {
    return ((service_frame *)opaque)->done;
}

static void fixture_begin(void) {
    memset(&wf_driver_root, 0, sizeof(wf_driver_root));
    memset(&wf_context_root, 0, sizeof(wf_context_root));
    memset(&assistant, 0, sizeof(assistant));
    memset(wf_driver_stats, 0, sizeof(wf_driver_stats));
    atomic_store(&wf_driver_stats_count, 2u);
    atomic_flag_clear(&wf_driver_root.run_lock);
    atomic_flag_clear(&assistant.run_lock);
    wf_driver_root.runtime = &wf_bridge_runtime;
    assistant.runtime = &assistant.own_runtime;
    assistant.index = 1u;
    CHECK(wf_completion_runtime_init(assistant.runtime) == 0);
    atomic_store(&wf_drivers[0], &wf_driver_root);
    atomic_store(&wf_drivers[1], &assistant);
    atomic_store(&wf_driver_count, 2u);
    atomic_store(&wf_drivers_stopping, 0u);
    atomic_store(&wf_context_root_done, 0u);
    atomic_store(&wf_drivers_idle, 0u);
    atomic_store(&wf_drivers_searching, 0u);
    wf_driver_self = &wf_driver_root;
    wf_driver_service = &wf_driver_root;
    wf_context_current = NULL;
    wf_context_root.driver = &wf_driver_root;
}

static void fixture_end(void) {
    CHECK(atomic_load(&wf_drivers_moving) == 0u);
    atomic_store(&wf_driver_count, 1u);
    while (atomic_load(&wf_drivers_notifying)) wf_prim_yield();
    atomic_store(&wf_drivers[1], NULL);
    CHECK(wf_completion_runtime_destroy(assistant.runtime) == 0);
    wf_pool_give(wf_driver_root.timers, wf_driver_root.timer_bytes);
    wf_pool_give(assistant.timers, assistant.timer_bytes);
    wf_driver_self = NULL;
    wf_driver_service = NULL;
    wf_context_current = NULL;
    atomic_store(&wf_driver_count, 0u);
    atomic_store(&wf_drivers[0], NULL);
}

typedef struct startup_park {
    uint64_t epoch;
    enum wf_completion_park_result result;
} startup_park;

static void *startup_park_thread(void *opaque) {
    startup_park *park = opaque;
    park->result = wf_completion_park_if_unchanged(
        assistant.runtime, park->epoch, UINT32_MAX);
    return NULL;
}

static void startup_publication(void) {
    fixture_begin();
    for (unsigned parked_first = 0; parked_first < 2; ++parked_first) {
        atomic_store(&wf_driver_count, 1u);
        startup_park park = {.epoch = wf_completion_wake_epoch(assistant.runtime)};
        CHECK(wf_driver_wait_ms(&assistant) == UINT32_MAX);
        pthread_t thread;
        if (parked_first) {
            CHECK(pthread_create(&thread, NULL, startup_park_thread, &park) == 0);
            while (!wf_completion_parked_scheduler_count(assistant.runtime)) wf_prim_yield();
        }
        /* The production startup boundary must close both schedules, even
         * when the new executor chose an infinite wait before publication. */
        wf_driver_publish_started(&assistant);
        CHECK(wf_completion_wake_epoch(assistant.runtime) != park.epoch);
        CHECK(wf_driver_wait_ms(&assistant) == WF_DRIVER_ASSIST_WAIT_MS);
        if (parked_first) {
            CHECK(pthread_join(thread, NULL) == 0);
            CHECK(park.result == WF_COMPLETION_PARK_WOKEN
                  || park.result == WF_COMPLETION_PARK_EPOCH_CHANGED);
        } else {
            CHECK(wf_completion_park_if_unchanged(assistant.runtime, park.epoch, UINT32_MAX)
                  == WF_COMPLETION_PARK_EPOCH_CHANGED);
        }
    }
    fixture_end();
}

static void steal_attribution(void) {
    fixture_begin();
    wf_context contexts[3] = {0};
    for (unsigned i = 0; i < 3; ++i) wf_run_push(&wf_driver_root, &contexts[i]);
    CHECK(wf_driver_steal(&assistant));
    CHECK(atomic_load(&wf_driver_stats[1].stolen_contexts) == 2u);
    CHECK(atomic_load(&wf_driver_stats[1].borrows) == 0u);
    CHECK(wf_run_take(&assistant) == &contexts[0]);
    CHECK(wf_run_take(&assistant) == &contexts[1]);
    CHECK(wf_run_take(&wf_driver_root) == &contexts[2]);
    CHECK(!wf_driver_steal(&assistant));
    CHECK(atomic_load(&wf_driver_stats[1].stolen_contexts) == 2u);
    /* Reporting must retain every started driver's counters after teardown
     * reduces the active count, and count contexts rather than steal passes. */
    atomic_store(&wf_driver_count, 1u);
    char report[512];
    CHECK(wf_driver_report(1u, report, sizeof(report)));
    CHECK(strcmp(report, "driver: index=1 borrow_attempts=0 borrows=0 borrowed_sleeps=0 "
          "borrowed_terminals=0 stolen_contexts=2") == 0);
    CHECK(!wf_driver_report(2u, report, sizeof(report)));
    CHECK(!wf_driver_report(1u, report, 1u));
    fixture_end();
}

typedef struct borrow_race {
    _Atomic unsigned start, claimed, release;
    unsigned won;
} borrow_race;

static void *borrow_thread(void *opaque) {
    borrow_race *race = opaque;
    uint64_t outside;
    while (!atomic_load(&race->start)) wf_prim_yield();
    race->won = (unsigned)wf_driver_try_borrow(&wf_driver_root, &outside);
    if (race->won) wf_driver_root.runs_since_reap = 17u;
    atomic_store(&race->claimed, 1u);
    while (!atomic_load(&race->release)) wf_prim_yield();
    if (race->won) wf_driver_release_borrow(&wf_driver_root, outside);
    return NULL;
}

static void ownership_races(void) {
    fixture_begin();
    for (unsigned borrower_first = 0; borrower_first < 2; ++borrower_first) {
        borrow_race race = {0};
        pthread_t thread;
        uint64_t outside = wf_driver_leave_service(&wf_driver_root);
        CHECK(pthread_create(&thread, NULL, borrow_thread, &race) == 0);
        if (!borrower_first) CHECK(wf_driver_try_return(&wf_driver_root, outside));
        atomic_store(&race.start, 1u);
        while (!atomic_load(&race.claimed)) wf_prim_yield();
        CHECK(race.won == borrower_first);
        if (borrower_first) {
            CHECK(!wf_driver_try_return(&wf_driver_root, outside));
            uint64_t unused;
            CHECK(!wf_driver_try_borrow(&wf_driver_root, &unused));
        }
        atomic_store(&race.release, 1u);
        if (borrower_first) {
            wf_driver_return_service(&wf_driver_root, outside);
            CHECK(wf_driver_root.runs_since_reap == 17u);
        } else wf_driver_service = &wf_driver_root;
        CHECK(pthread_join(thread, NULL) == 0);
        uint64_t later = wf_driver_leave_service(&wf_driver_root);
        CHECK(later != outside);
        CHECK(!wf_driver_try_return(&wf_driver_root, outside));
        wf_driver_return_service(&wf_driver_root, later);
    }
    for (unsigned phase = WF_DRIVER_SERVICE; phase <= WF_DRIVER_STOPPED; ++phase) {
        if (phase == WF_DRIVER_OUTSIDE) continue;
        uint64_t token = wf_driver_root.departure | phase;
        atomic_store(&wf_driver_root.service_token, token);
        uint64_t unused;
        CHECK(!wf_driver_try_borrow(&wf_driver_root, &unused));
        CHECK(atomic_load(&wf_driver_root.service_token) == token);
    }
    fixture_end();
}

static wf_completion_record *park_record(wf_context *context, unsigned route, uint64_t at) {
    wf_completion_record *record = (wf_completion_record *)context->operation.bytes;
    memset(record, 0, sizeof(*record));
    wf_completion_record_init(record);
    record->route = route;
    record->request.kind = route == WF_COMPLETION_ROUTE_TIMER ? WF_FILE_SLEEP : WF_FILE_READ;
    atomic_store(&record->deadline, at);
    context->driver = &wf_driver_root;
    context->record = record;
    wf_context_adopt_wait(&wf_driver_root, context);
    return record;
}

static void *compute_thread(void *opaque) {
    wf_driver_self = &wf_driver_root;
    wf_driver_service = &wf_driver_root;
    wf_context_run(&wf_driver_root, opaque);
    wf_driver_self = NULL;
    wf_driver_service = NULL;
    return NULL;
}

static void timer_migration(void) {
    fixture_begin();
    service_frame timer = {.kind = FRAME_TIMER};
    service_frame compute = {.kind = FRAME_COMPUTE, .group = {1, 0}};
    wf_context child = {.root = &compute, .resume = &compute, .driver = &wf_driver_root};
    wf_context_root.root = &timer;
    wf_context_root.resume = &timer;
    wf_completion_record *record = park_record(&wf_context_root, WF_COMPLETION_ROUTE_TIMER, 1u);
    /* A peer that finds no OUTSIDE role must still have a finite park: the
     * owner may depart just after its scan. No elapsed-time assertion. */
    CHECK(!wf_driver_assist(&assistant));
    CHECK(wf_driver_wait_ms(&assistant) == WF_DRIVER_ASSIST_WAIT_MS);
    CHECK(atomic_load(&wf_driver_stats[1].borrow_attempts) == 1u);
    CHECK(atomic_load(&wf_driver_stats[1].borrows) == 0u);
    atomic_store(&computing, 0u);
    atomic_store(&release_compute, 0u);
    atomic_store(&ran_timer, 0u);
    pthread_t thread;
    CHECK(pthread_create(&thread, NULL, compute_thread, &child) == 0);
    while (!atomic_load(&computing)) wf_prim_yield();
    wf_driver_self = &assistant;
    wf_driver_service = &assistant;
    /* The production idle loop must discover, detach and run the root.
     * The computing invocation does not return until that continuation runs. */
    wf_context_drive(&assistant);
    CHECK(pthread_join(thread, NULL) == 0);
    CHECK(atomic_load(&ran_timer) == 1u);
    CHECK(atomic_load(&wf_context_root_done) == 1u);
    CHECK(record->result.value == 0 && record->result.error_code == 0);
    CHECK(wf_driver_root.parked == NULL && wf_driver_root.timer_count == 0u);
    CHECK(atomic_load(&wf_driver_root.host_waits) == 0u);
    CHECK(wf_run_take(&wf_driver_root) == NULL && wf_run_take(&assistant) == NULL);
    CHECK(wf_context_root.driver == &assistant);
    CHECK(atomic_load(&wf_driver_stats[1].borrows) == 1u);
    CHECK(atomic_load(&wf_driver_stats[1].borrowed_sleeps) == 1u);
    CHECK(atomic_load(&wf_driver_stats[1].borrowed_terminals) == 0u);
    CHECK(atomic_load(&wf_driver_stats[1].stolen_contexts) == 0u);
    compute.group[1] = 0;
    fixture_end();
}

static void *publish_thread(void *opaque) {
    wf_completion_record *record = opaque;
    record->result.kind = WF_FILE_READ;
    record->result.value = 7;
    wf_completion_record_complete(record);
    return NULL;
}

static void publication_during_borrow(void) {
    fixture_begin();
    wf_context context = {0};
    wf_completion_record *record = park_record(&context, WF_COMPLETION_ROUTE_FILE_ADAPTER, 0);
    uint64_t outside = wf_driver_leave_service(&wf_driver_root), borrowed;
    CHECK(wf_driver_try_borrow(&wf_driver_root, &borrowed));
    CHECK(borrowed == outside);
    CHECK(wf_driver_service_only(&assistant, &wf_driver_root, 0) == NULL);
    pthread_t publisher;
    CHECK(pthread_create(&publisher, NULL, publish_thread, record) == 0);
    CHECK(pthread_join(publisher, NULL) == 0);
    /* Completion arrived after a borrowed scan, while the token was still
     * borrowed. A later bounded scan must find it without losing the cursor. */
    wf_context *ready = wf_driver_service_only(&assistant, &wf_driver_root, 0);
    CHECK(ready == &context && ready->next == NULL);
    CHECK(wf_driver_service_only(&assistant, &wf_driver_root, 0) == NULL);
    CHECK(atomic_load(&wf_driver_root.run_count) == 0u);
    CHECK(atomic_load(&assistant.run_count) == 0u);
    CHECK(context.record == NULL && record->result.value == 7);
    CHECK(atomic_load(&wf_driver_stats[1].borrowed_terminals) == 1u);
    CHECK(atomic_load(&wf_driver_stats[1].borrowed_sleeps) == 0u);
    wf_driver_release_borrow(&wf_driver_root, borrowed);
    ready->driver = &assistant;
    wf_run_push(&assistant, ready);
    wf_driver_return_service(&wf_driver_root, outside);
    CHECK(!wf_context_harvest(&wf_driver_root));
    CHECK(wf_run_take(&assistant) == &context);
    CHECK(wf_run_take(&assistant) == NULL);
    fixture_end();
}

static void bounded_scan(void) {
    fixture_begin();
    wf_context contexts[WF_BRIDGE_REAP_BUDGET + 1u] = {0};
    /* The due timer is beyond the first batch of nonterminal engine work. */
    wf_completion_record *timer = park_record(&contexts[0], WF_COMPLETION_ROUTE_TIMER, 1u);
    for (unsigned i = 1; i <= WF_BRIDGE_REAP_BUDGET; ++i)
        (void)park_record(&contexts[i], WF_COMPLETION_ROUTE_FILE_ADAPTER, 1u);
    uint64_t outside = wf_driver_leave_service(&wf_driver_root), borrowed;
    CHECK(wf_driver_try_borrow(&wf_driver_root, &borrowed));
    CHECK(wf_driver_service_only(&assistant, &wf_driver_root, 1u) == NULL);
    CHECK(wf_driver_service_only(&assistant, &wf_driver_root, 1u) == &contexts[0]);
    CHECK(wf_bridge_record_state(timer) == WF_COMPLETION_DONE);
    /* A borrower neither executes queued host work nor cancels through TLS. */
    for (unsigned i = 1; i <= WF_BRIDGE_REAP_BUDGET; ++i) {
        wf_completion_record *record = contexts[i].record;
        CHECK(wf_bridge_record_state(record) == WF_COMPLETION_PENDING);
        CHECK(atomic_load(&record->deadline) == 1u);
        wf_context_unpark(&wf_driver_root, &contexts[i]);
        wf_context_host_wait_ended(&wf_driver_root);
    }
    CHECK(wf_driver_root.borrow_cursor == NULL);
    wf_driver_release_borrow(&wf_driver_root, borrowed);
    wf_driver_return_service(&wf_driver_root, outside);
    fixture_end();
}

static void adoption_and_early_wake(void) {
    fixture_begin();
    wf_context context = {0};
    wf_cancel *source = wf__cancel_new();
    wf_completion_record *record = (wf_completion_record *)context.operation.bytes;
    wf_context_current = &context;
    context.driver = &wf_driver_root;
    uint64_t outside = wf_driver_leave_service(&wf_driver_root), borrowed;
    CHECK(wf__completion_sleep_watched_submit(UINT64_MAX - 1u, source, record) == 2);
    CHECK(wf__context_wait(record, &context) == 1);
    CHECK(wf_driver_try_borrow(&wf_driver_root, &borrowed));
    CHECK(wf_driver_root.cancel_waits == NULL && wf_driver_root.parked == NULL);
    CHECK(wf_driver_root.timer_count == 0u);
    CHECK(wf_driver_service_only(&assistant, &wf_driver_root, UINT64_MAX - 1u) == NULL);
    wf_driver_release_borrow(&wf_driver_root, borrowed);
    /* Model a firing already consumed before the wait is adopted. */
    atomic_store(&source->fired, 1u);
    atomic_store(&wf_driver_root.cancel_pending, 0u);
    wf_context_current = NULL;
    wf_driver_return_service(&wf_driver_root, outside);
    wf_context_adopt_wait(&wf_driver_root, &context);
    CHECK(atomic_load(&wf_driver_root.cancel_pending) == 1u);
    outside = wf_driver_leave_service(&wf_driver_root);
    CHECK(wf_driver_try_borrow(&wf_driver_root, &borrowed));
    CHECK(wf_driver_service_only(&assistant, &wf_driver_root, 0u) == &context);
    CHECK(wf__completion_cancelled(record));
    CHECK(atomic_load(&wf_driver_stats[1].borrowed_sleeps) == 0u);
    CHECK(atomic_load(&wf_driver_stats[1].borrowed_terminals) == 0u);
    CHECK(wf_driver_root.cancel_waits == NULL && context.timer_slot == 0u);
    wf_driver_release_borrow(&wf_driver_root, borrowed);
    wf_driver_return_service(&wf_driver_root, outside);
    CHECK(wf__shared_release(source));
    wf__shared_free(source);

    service_frame early = {.kind = FRAME_EARLY_WAKE, .group = {1, 0}};
    context.root = &early;
    context.resume = &early;
    wf_context_run(&wf_driver_root, &context);
    CHECK(atomic_load(&context.wake_gate) == WF_WAKE_NONE);
    CHECK(wf_run_take(&wf_driver_root) == &context);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    fixture_end();
}

int main(void) {
    wf_test_guard_start(30);
    wf_bridge_require();
    CHECK(!wf_bridge_ring_ready()); /* deterministic service-only fixture */
    wf_test_guard_phase("startup publication wakes an obsolete unbounded park");
    startup_publication();
    wf_test_guard_phase("ready-queue steals have separate attribution");
    steal_attribution();
    wf_test_guard_phase("return versus borrow, excluded phases and stale epochs");
    ownership_races();
    wf_test_guard_phase("due timer runs on assisting driver while owner computes");
    timer_migration();
    wf_test_guard_phase("completion publication during borrowing");
    publication_during_borrow();
    wf_test_guard_phase("bounded scan and engine work left untouched");
    bounded_scan();
    wf_test_guard_phase("post-unwind registration and early external wake");
    adoption_and_early_wake();
    wf_test_guard_finish();
    return 0;
}
