/* Deterministic borrow and whole-role handoff schedules. The private bridge exposes the same
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
enum { FRAME_COMPUTE, FRAME_TIMER, FRAME_EARLY_WAKE, FRAME_HANDOFF_COMPUTE,
    FRAME_HANDOFF_TIMER, FRAME_LAUNCHER, FRAME_LAUNCHER_CHILD, FRAME_FAIR };
static _Atomic unsigned destroyed_child, destroyed_root;
static pthread_t launcher_thread;
static wf_cmap *handoff_map;
typedef struct service_frame {
    unsigned kind;
    int done;
    unsigned stage, handback;
    uint64_t group[2];
} service_frame;

static void *start_frame(void *arguments) { return arguments; }

void wf_driver_service_test_destroy(void *opaque) {
    service_frame *frame = opaque;
    if (frame->kind == FRAME_LAUNCHER) {
        CHECK(pthread_equal(pthread_self(), launcher_thread));
        CHECK(atomic_load(&wf_monitor_exited));
        CHECK(atomic_load(&wf_executors[1].exited));
        CHECK(!wf_invocations_active());
        atomic_fetch_add(&destroyed_root, 1u);
    } else if (frame->kind == FRAME_LAUNCHER_CHILD || frame->kind == FRAME_HANDOFF_COMPUTE) {
        atomic_fetch_add(&destroyed_child, 1u);
    }
}

void wf_driver_service_test_resume(void *opaque) {
    service_frame *frame = opaque;
    CHECK(wf_driver_service == NULL);
    CHECK((wf_context_outside & WF_DRIVER_PHASE_MASK) == WF_DRIVER_OUTSIDE);
    /* Reassignment may win even between OUTSIDE and this function's entry. */
    if (frame->kind == FRAME_FAIR) {
        for (unsigned pass = 1u; pass <= WF_CONTEXT_YIELD_PASSES; ++pass)
            CHECK(wf__context_pass(frame) == (pass == WF_CONTEXT_YIELD_PASSES));
        CHECK(wf_context_parked_away == WF_SUSPEND_FAIR);
        CHECK(!atomic_load(&wf_context_current->wake_gate));
        frame->stage += 1u;
    } else if (frame->kind == FRAME_HANDOFF_COMPUTE) {
        CHECK(wf__driver_index() == 0u);
        wf_cmap_user *user = wf_cmap_user_at(handoff_map, wf__driver_index());
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, (const unsigned char *)"old", 3u, 0, &entry);
        *slot = 19u;
        atomic_store(&computing, 1u);
        while (!atomic_load(&release_compute)) wf_prim_yield();
        CHECK(wf__driver_index() == 0u && *slot == 19u);
        wf_cmap_unlock_entry(user, &entry, 0, 1);
        CHECK(wf_driver_service == NULL);
        if (frame->handback == WF_HAND_BACK_COMPLETE) frame->done = 1;
        else if (frame->handback == WF_HAND_BACK_WAIT) {
            wf_completion_record *record = wf_bridge_begin(wf_context_current->operation.bytes);
            record->route = WF_COMPLETION_ROUTE_FILE_ADAPTER;
            CHECK(wf__context_wait(record, frame));
        } else CHECK(wf__context_pass(frame));
    } else if (frame->kind == FRAME_HANDOFF_TIMER) {
        CHECK(wf__driver_index() == 1u);
        CHECK(wf_driver_self == &wf_driver_root);
        wf_cmap_user *user = wf_cmap_user_at(handoff_map, wf__driver_index());
        CHECK(user != wf_cmap_user_at(handoff_map, 0u));
        wf_cmap_entry entry;
        uint64_t *slot = wf_cmap_lock_entry(user, (const unsigned char *)"new", 3u, 0, &entry);
        *slot = 23u;
        wf_cmap_unlock_entry(user, &entry, 0, 1);
        CHECK(!atomic_load(&release_compute));
        atomic_fetch_add(&ran_timer, 1u);
        frame->done = 1;
    } else if (frame->kind == FRAME_LAUNCHER) {
        if (frame->stage == 0u) {
            CHECK(wf__driver_index() == 0u);
            service_frame *child = wf__context_prepare(sizeof(*child));
            *child = (service_frame){.kind = FRAME_LAUNCHER_CHILD};
            wf__context_launch(frame->group, child, start_frame);
            CHECK(wf_executor_count == 2u && atomic_load(&wf_executors[1].ready));
            /* Controlled instant: the loop takes the queued child before
             * its next idle reap, so the timer remains pending in compute. */
            wf_completion_record *record = wf_bridge_begin(wf_context_current->operation.bytes);
            record->request.kind = WF_FILE_SLEEP;
            record->route = WF_COMPLETION_ROUTE_TIMER;
            atomic_store(&record->deadline, 1u);
            frame->stage = 1u;
            CHECK(wf__context_wait(record, frame));
        } else if (frame->stage == 1u) {
            CHECK(wf__driver_index() == 1u);
            atomic_fetch_add(&ran_timer, 1u);
            frame->stage = 2u;
            CHECK(wf__context_join_wait(frame->group, frame));
        } else {
            CHECK(wf__driver_index() == 1u);
            CHECK(frame->group[0] == 0u);
            frame->done = 1;
        }
    } else if (frame->kind == FRAME_LAUNCHER_CHILD) {
        CHECK(wf__driver_index() == 0u);
        atomic_store(&computing, 1u);
        while (!atomic_load(&release_compute)) wf_prim_yield();
        frame->done = 1;
    } else if (frame->kind == FRAME_COMPUTE) {
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
    memset(wf_executors, 0, sizeof(wf_executors));
    wf_executor_count = 0u;
    wf_monitor_started = 0u;
    wf__map_user = 0u;
    atomic_flag_clear(&wf_executor_lock);
    for (unsigned i = 0; i < WF_EXECUTOR_LIMIT; ++i)
        atomic_store(&wf_invocations[i].sequence, 0u);
    atomic_store(&destroyed_child, 0u);
    atomic_store(&destroyed_root, 0u);
    atomic_store(&wf_driver_stats_count, 2u);
    atomic_flag_clear(&wf_driver_root.run_lock);
    atomic_flag_clear(&wf_driver_root.ingress_lock);
    atomic_flag_clear(&assistant.run_lock);
    atomic_flag_clear(&assistant.ingress_lock);
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

/* The emitted inline boundary has already incremented the context's word.
 * Exercise the same cold helper's no-peer, threshold and displacement cases
 * with explicit tokens and queue contents, without a timing assumption. */
static void counted_atomic_boundaries(void) {
    uint64_t saved_outside = wf_context_outside;
    int saved_parked = wf_context_parked_away;
    fixture_begin();
    service_frame frame = {0};
    wf_context peer = {.driver = &wf_driver_root};
    wf_context_outside = WF_DRIVER_EPOCH_STEP | WF_DRIVER_OUTSIDE;
    atomic_store(&wf_driver_root.service_token, wf_context_outside);
    wf_context_set_current(&wf_context_root);
    wf_context_parked_away = 0;
    CHECK(wf__context_boundary.passes == &wf_context_root.passes);
    CHECK(wf__context_boundary.service_token == &wf_driver_root.service_token);
    CHECK(wf__context_boundary.outside == wf_context_outside);

    *wf__context_boundary.passes = WF_CONTEXT_YIELD_PASSES - 1u;
    CHECK(wf__context_pass(&frame) == 0);
    CHECK(wf_context_root.passes == 0u);
    *wf__context_boundary.passes = WF_CONTEXT_YIELD_PASSES;
    CHECK(wf__context_pass_counted(&frame) == 0);
    CHECK(wf_context_root.passes == 0u && !wf_context_parked_away);

    wf_run_push(&wf_driver_root, &peer);
    *wf__context_boundary.passes = WF_CONTEXT_YIELD_PASSES - 1u;
    CHECK(wf__context_pass_counted(&frame) == 0);
    CHECK(wf_context_root.passes == WF_CONTEXT_YIELD_PASSES - 1u);
    *wf__context_boundary.passes += 1u;
    CHECK(wf__context_pass_counted(&frame) == 1);
    CHECK(wf_context_root.passes == 0u && wf_context_root.resume == &frame);
    CHECK(wf_context_parked_away);
    CHECK(wf_context_parked_away == WF_SUSPEND_FAIR);
    CHECK(atomic_load(&wf_context_root.wake_gate) == WF_WAKE_NONE);
    CHECK(atomic_load(&wf_driver_root.run_count) == 1u);
    wf_context_root.disposition = WF_HAND_BACK_FAIR;
    wf_context_adopt(&wf_driver_root, &wf_context_root);
    CHECK(wf_run_take(&wf_driver_root) == &peer);
    CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);

    wf_context_parked_away = 0;
    *wf__context_boundary.passes = 1u;
    atomic_store(&wf_driver_root.service_token,
        WF_DRIVER_EPOCH_STEP | WF_DRIVER_BORROWED_SERVICE);
    CHECK(wf__context_pass_counted(&frame) == 0);
    CHECK(wf_context_root.passes == 1u && !wf_context_parked_away);
    atomic_store(&wf_driver_root.service_token,
        (2u * WF_DRIVER_EPOCH_STEP) | WF_DRIVER_OUTSIDE);
    CHECK(wf__context_pass_counted(&frame) == 1);
    CHECK(wf_context_root.passes == 1u && wf_context_parked_away);
    CHECK(wf_context_root.resume == &frame);
    wf_context_root.disposition = WF_HAND_BACK_FAIR;
    wf_context_adopt(&wf_driver_root, &wf_context_root);
    CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    wf_context_outside = saved_outside;
    wf_context_parked_away = saved_parked;
    fixture_end();
    wf_context_set_current(NULL);
}

/* A lone busy context still has to return for local service. A sleeping
 * reader is absent from the run queue until that service expires its timer. */
static void solitary_service_boundaries(void) {
    fixture_begin();
    service_frame frame = {0};
    wf_context_outside = WF_DRIVER_EPOCH_STEP | WF_DRIVER_OUTSIDE;
    atomic_store(&wf_driver_root.service_token, wf_context_outside);
    wf_context_set_current(&wf_context_root);
    for (unsigned pending = 0; pending < 2u; ++pending) {
        atomic_store(&wf_driver_root.host_waits, pending == 0u);
        atomic_store(&wf_driver_root.ingress_count, pending == 1u);
        wf_context_root.passes = WF_CONTEXT_YIELD_PASSES;
        wf_context_parked_away = WF_SUSPEND_NONE;
        CHECK(!atomic_load(&wf_driver_root.run_count));
        CHECK(wf__context_pass_counted(&frame));
        CHECK(wf_context_parked_away == WF_SUSPEND_FAIR);
        CHECK(!atomic_load(&wf_context_root.wake_gate));
        CHECK(!atomic_load(&wf_driver_root.run_count));
        wf_context_root.disposition = WF_HAND_BACK_FAIR;
        wf_context_adopt(&wf_driver_root, &wf_context_root);
        CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
    }
    atomic_store(&wf_driver_root.host_waits, 0u);
    atomic_store(&wf_driver_root.ingress_count, 0u);
    wf_context_parked_away = WF_SUSPEND_NONE;
    fixture_end();
    wf_context_set_current(NULL);
}

/* Scope boundaries are the production collector's helpers, with an invocation
 * explicitly placed between the two collections. A Boolean or single collect
 * misses the complete 0 -> 1 -> 2 transition. */
static void invocation_collections(void) {
    fixture_begin();
    uint64_t sequence[WF_EXECUTOR_LIMIT];
    wf_quiescence_lock();
    wf_driver_idle_locked(&wf_driver_root);
    wf_driver_idle_locked(&assistant);
    CHECK(wf_contexts_stuck_locked());
    CHECK(wf_invocations_collect(sequence));
    uint64_t changes = atomic_load(&wf_drivers_changes);
    wf__map_user = WF_EXECUTOR_LIMIT - 1u;
    wf_invocation_begin();
    CHECK(!wf_invocations_unchanged(sequence));
    CHECK(!wf_contexts_stuck_locked());
    wf__map_user = 0u;
    wf_invocation_begin();
    wf_invocation_end();
    CHECK(!wf_contexts_stuck_locked());
    CHECK(atomic_load(&wf_invocations[WF_EXECUTOR_LIMIT - 1u].sequence) & 1u);
    wf__map_user = WF_EXECUTOR_LIMIT - 1u;
    wf_invocation_end();
    CHECK(!wf_invocations_unchanged(sequence));
    CHECK(!wf_contexts_stuck_since(sequence, changes));
    CHECK(wf_invocations_collect(sequence));
    CHECK(wf_invocations_unchanged(sequence));
    CHECK(wf_contexts_stuck_locked());
    wf_quiescence_unlock();
    /* Shutdown also enumerates beyond the published executor count. */
    wf_invocation_begin();
    CHECK(!wf_drivers_quiescent());
    CHECK(wf_driver_poll_wait_ms(&wf_driver_root) == 1);
    wf_invocation_end();
    CHECK(wf_drivers_quiescent());
    wf__map_user = 0u;
    fixture_end();
}

/* Empty maintenance retains logical idleness. Failed borrow/steal probes
 * finish under the cold mutex, so successive scans cannot chase their churn. */
static void idle_quiescence(void) {
    fixture_begin();
    wf_quiescence_lock();
    wf_driver_idle_locked(&wf_driver_root);
    wf_driver_idle_locked(&assistant);
    uint64_t changes = atomic_load(&wf_drivers_changes);
    wf_quiescence_unlock();
    for (unsigned poll = 0; poll < 8u; ++poll) {
        CHECK(!wf_driver_steal(&wf_driver_root));
        CHECK(!wf_driver_assist(&wf_driver_root));
        wf_quiescence_lock();
        wf_driver_idle_locked(&wf_driver_root);
        CHECK(wf_contexts_stuck_locked());
        CHECK(atomic_load(&wf_drivers_changes) == changes);
        CHECK(atomic_load(&wf_drivers_idle) == 2u);
        wf_quiescence_unlock();
    }
    wf_context ready = {.driver = &wf_driver_root};
    wf_run_push(&wf_driver_root, &ready);
    CHECK(atomic_load(&wf_driver_root.idle));
    CHECK(wf_run_take(&wf_driver_root) == &ready);
    CHECK(!wf_contexts_stuck(&wf_driver_root));
    CHECK(!atomic_load(&wf_driver_root.idle));
    CHECK(atomic_load(&wf_drivers_idle) == 1u);
    /* A zero local searcher must not retire another driver's wake claim. */
    atomic_store(&wf_drivers_searching, 1u);
    atomic_store(&assistant.searcher, 1u);
    wf_driver_end_search(&wf_driver_root, 0);
    CHECK(atomic_load(&wf_drivers_searching));
    wf_driver_end_search(&assistant, 0);
    CHECK(!atomic_load(&wf_drivers_searching));
    /* A claim after the fast zero observation survives until the target's
     * next empty-loop end-search, even when no context was found. */
    wf_driver_end_search(&assistant, 0);
    wf_drivers_wake_one();
    CHECK(atomic_load(&assistant.searcher));
    CHECK(atomic_load(&wf_drivers_searching));
    CHECK(!atomic_load(&assistant.idle));
    wf_driver_end_search(&assistant, 0);
    CHECK(!atomic_load(&assistant.searcher));
    CHECK(!atomic_load(&wf_drivers_searching));
    fixture_end();
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
          "borrowed_terminals=0 stolen_contexts=2 reassignments=0 ingress_commits=0 reserve_misses=0") == 0);
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

/* No monitor or second executor participates. Force the parked sleep's
 * timer instant to zero, which is due for every monotonic clock result. The
 * production reap interval alone must make it ready by its 64th resume. */
static void solitary_sleep_reap(void) {
    fixture_begin();
    service_frame busy = {.kind = FRAME_FAIR};
    wf_context child = {.driver = &wf_driver_root, .root = &busy, .resume = &busy};
    wf_completion_record *record = park_record(&wf_context_root, WF_COMPLETION_ROUTE_TIMER, 1u);
    CHECK(wf_driver_root.timer_count == 1u);
    wf_driver_root.timers[0]->timer_at = 0u;
    uint64_t changes = atomic_load(&wf_drivers_changes);
    for (unsigned run = 1u; run <= WF_DRIVER_REAP_RUNS; ++run) {
        CHECK(!atomic_load(&wf_driver_root.run_count));
        CHECK(wf_context_run(&wf_driver_root, &child));
        CHECK(busy.stage == run);
        CHECK(atomic_load(&wf_invocations[0].sequence) == 2u * run);
        CHECK(atomic_load(&wf_drivers_changes) == changes);
        if (run < WF_DRIVER_REAP_RUNS) {
            CHECK(wf_bridge_record_state(record) == WF_COMPLETION_PENDING);
            CHECK(wf_run_take(&wf_driver_root) == &child);
        }
    }
    CHECK(wf_bridge_record_state(record) == WF_COMPLETION_DONE);
    CHECK(!atomic_load(&wf_driver_root.host_waits));
    CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
    CHECK(wf_run_take(&wf_driver_root) == &child);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    CHECK(!wf_invocations_active());
    fixture_end();
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
    wf__map_user = 1u;
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

/* This fixture grants a spare without starting a worker, letting the test
 * choose exactly when RESERVED is acquired. Its clock is explicit data. */
static void handoff_fixture(void) {
    fixture_begin();
    atomic_store(&wf_driver_count, 1u);
    atomic_store(&wf_drivers[1], NULL);
    wf_executor_init(0u, NULL);
    wf_executor_init(1u, NULL);
    wf_executor_count = 2u;
    wf_executors[1].available = 1u;
}

static void handoff_fixture_end(void) {
    CHECK(!atomic_load(&wf_driver_root.ingress_count));
    CHECK(!wf_invocations_active());
    CHECK(wf_completion_runtime_destroy(&wf_executors[0].wake) == 0);
    CHECK(wf_completion_runtime_destroy(&wf_executors[1].wake) == 0);
    fixture_end();
}

static void take_reservation(unsigned index) {
    wf_spin_lock(&wf_executor_lock);
    CHECK(wf_executors[index].assignment == &wf_driver_root);
    wf_executors[index].assignment = NULL;
    wf_spin_unlock(&wf_executor_lock);
    wf__map_user = index;
    wf_driver_acquire_reserved(&wf_driver_root);
}

typedef struct monitor_race {
    _Atomic unsigned start, finished;
    int won;
} monitor_race;

static void *monitor_racer(void *opaque) {
    monitor_race *race = opaque;
    while (!atomic_load(&race->start)) wf_prim_yield();
    race->won = wf_driver_reassign(&wf_driver_root, WF_HANDOFF_TAU_NS + 2u);
    atomic_store(&race->finished, 1u);
    return NULL;
}

static void reassignment_races(void) {
    handoff_fixture();
    wf_context timer = {0};
    (void)park_record(&timer, WF_COMPLETION_ROUTE_TIMER, 1u);
    uint64_t prior = 0u;
    for (unsigned monitor_first = 0; monitor_first < 2u; ++monitor_first) {
        for (unsigned repeat = 0; repeat < 3u; ++repeat) {
            unsigned spare = 1u - wf__map_user;
            wf_executors[wf__map_user].available = 0u;
            wf_executors[spare].available = 1u;
            uint64_t outside = wf_driver_leave_service(&wf_driver_root);
            CHECK(!wf_driver_reassign(&wf_driver_root, WF_HANDOFF_TAU_NS + 1u));
            atomic_store(&wf_drivers_idle, 1u);
            CHECK(!wf_driver_reassign(&wf_driver_root, WF_HANDOFF_TAU_NS + 2u));
            atomic_store(&wf_drivers_idle, 0u);
            uint64_t borrowed;
            CHECK(wf_driver_try_borrow(&wf_driver_root, &borrowed));
            CHECK(!wf_driver_reassign(&wf_driver_root, WF_HANDOFF_TAU_NS + 2u));
            wf_driver_release_borrow(&wf_driver_root, borrowed);
            monitor_race race = {0};
            pthread_t thread;
            CHECK(pthread_create(&thread, NULL, monitor_racer, &race) == 0);
            if (!monitor_first) CHECK(wf_driver_return_service(&wf_driver_root, outside));
            atomic_store(&race.start, 1u);
            while (!atomic_load(&race.finished)) wf_prim_yield();
            CHECK(race.won == (int)monitor_first);
            if (monitor_first) {
                CHECK(!wf_driver_return_service(&wf_driver_root, outside));
                CHECK(atomic_load(&wf_drivers_moving) == 1u);
                CHECK(!wf_driver_try_borrow(&wf_driver_root, &borrowed));
                take_reservation(spare);
                CHECK(!wf_driver_try_return(&wf_driver_root, outside));
            }
            if (prior) CHECK(!wf_driver_try_return(&wf_driver_root, prior));
            prior = outside;
            CHECK(pthread_join(thread, NULL) == 0);
        }
    }
    /* A stale hint from a completed borrow cannot authorize transfer. */
    uint64_t outside = wf_driver_leave_service(&wf_driver_root), borrowed;
    CHECK(wf_driver_try_borrow(&wf_driver_root, &borrowed));
    wf_context_unpark(&wf_driver_root, &timer);
    wf_context_host_wait_ended(&wf_driver_root);
    wf_driver_release_borrow(&wf_driver_root, borrowed);
    wf_executors[wf__map_user].available = 0u;
    wf_executors[1u - wf__map_user].available = 1u;
    CHECK(!wf_driver_reassign(&wf_driver_root, WF_HANDOFF_TAU_NS + 2u));
    CHECK(wf_driver_return_service(&wf_driver_root, outside));
    handoff_fixture_end();
}

static void displaced_dispositions(void) {
    for (unsigned disposition = WF_HAND_BACK_WAIT; disposition <= WF_HAND_BACK_COMPLETE; ++disposition) {
        handoff_fixture();
        handoff_map = wf_cmap_create_entries(8u, 8u, 64u);
        service_frame timer = {.kind = FRAME_HANDOFF_TIMER};
        service_frame compute = {.kind = FRAME_HANDOFF_COMPUTE, .handback = disposition};
        uint64_t group[2] = {1u, 0u};
        size_t granted;
        wf_context *child = wf_pool_take(sizeof(*child), &granted);
        memset(child, 0, sizeof(*child));
        child->pool_bytes = granted;
        child->root = child->resume = &compute;
        child->driver = &wf_driver_root;
        child->group = group;
        atomic_store(&wf_context_live, 1u);
        wf_context_root.root = wf_context_root.resume = &timer;
        wf_completion_record *record = park_record(&wf_context_root, WF_COMPLETION_ROUTE_TIMER, 1u);
        atomic_store(&computing, 0u);
        atomic_store(&release_compute, 0u);
        atomic_store(&ran_timer, 0u);
        pthread_t thread;
        CHECK(pthread_create(&thread, NULL, compute_thread, child) == 0);
        while (!atomic_load(&computing)) wf_prim_yield();
        wf_executors[1].available = 0u;
        CHECK(!wf_driver_reassign(&wf_driver_root, WF_HANDOFF_TAU_NS + 2u));
        CHECK(atomic_load(&wf_driver_stats[0].reserve_misses) == 1u);
        wf_executors[1].available = 1u;
        CHECK(wf_monitor_scan(WF_HANDOFF_TAU_NS + 2u) == WF_MONITOR_PERIOD_MS);
        CHECK(atomic_load(&wf_driver_stats[0].reassignments) == 1u);
        take_reservation(1u);
        CHECK(wf_driver_expire_wait(&wf_driver_root, &wf_context_root,
            WF_HANDOFF_TAU_NS + 2u, 0));
        CHECK(wf_bridge_record_state(record) == WF_COMPLETION_DONE);
        CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
        CHECK(wf_context_run(&wf_driver_root, &wf_context_root));
        CHECK(atomic_load(&ran_timer) == 1u);
        CHECK(atomic_load(&wf_invocations[0].sequence) & 1u);
        CHECK(!(atomic_load(&wf_invocations[1].sequence) & 1u));
        /* Every logical role looks idle, but the old executor holds a map
         * entry and can still make progress. It must not be declared stuck. */
        wf_quiescence_lock();
        wf_driver_idle_locked(&wf_driver_root);
        wf_quiescence_unlock();
        CHECK(!wf_contexts_stuck(&wf_driver_root));
        atomic_store(&release_compute, 1u);
        CHECK(pthread_join(thread, NULL) == 0);
        CHECK(atomic_load(&wf_driver_root.ingress_count) == 1u);
        CHECK(atomic_load(&wf_driver_stats[0].ingress_commits) == 1u);
        CHECK(!wf_contexts_stuck(&wf_driver_root));
        CHECK(wf_driver_adopt_ingress(&wf_driver_root));
        CHECK(!wf_driver_adopt_ingress(&wf_driver_root));
        if (disposition == WF_HAND_BACK_COMPLETE) {
            CHECK(group[0] == 0u && atomic_load(&wf_context_live) == 0u);
            CHECK(atomic_load(&destroyed_child) == 1u);
        } else {
            if (disposition == WF_HAND_BACK_WAIT) {
                CHECK(wf_driver_root.parked == child);
                CHECK(atomic_load(&wf_driver_root.host_waits) == 1u);
                wf_completion_record_complete(child->record);
            }
            CHECK(wf_run_take(&wf_driver_root) == child);
            wf_pool_give(child, granted);
            atomic_store(&wf_context_live, 0u);
        }
        CHECK(wf_run_take(&wf_driver_root) == NULL);
        wf_quiescence_lock();
        wf_driver_idle_locked(&wf_driver_root);
        wf_quiescence_unlock();
        CHECK(wf_contexts_stuck(&wf_driver_root));
        wf_cmap_destroy(handoff_map);
        handoff_map = NULL;
        handoff_fixture_end();
    }
}

static void root_ingress_quiescence(void) {
    handoff_fixture();
    wf_context_root.disposition = WF_HAND_BACK_COMPLETE;
    wf_invocation_begin();
    wf_context_ingress(&wf_driver_root, &wf_context_root);
    CHECK(!wf_drivers_quiescent());
    /* Pause an owner between the two production adoption steps: publishing
     * root_done is not permission to tear down the ingress it came through. */
    wf_spin_lock(&wf_driver_root.ingress_lock);
    CHECK(wf_driver_root.ingress_head == &wf_context_root);
    wf_driver_root.ingress_head = wf_driver_root.ingress_tail = NULL;
    wf_spin_unlock(&wf_driver_root.ingress_lock);
    wf_context_adopt(&wf_driver_root, &wf_context_root);
    CHECK(atomic_load(&wf_context_root_done));
    CHECK(!wf_drivers_quiescent());
    atomic_fetch_add(&wf_drivers_changes, 1u);
    atomic_fetch_sub(&wf_driver_root.ingress_count, 1u);
    CHECK(!wf_drivers_quiescent());
    uint64_t epoch = wf_completion_wake_epoch(wf_driver_root.runtime);
    wf_context_retire(&wf_driver_root, 0);
    CHECK(wf_completion_wake_epoch(wf_driver_root.runtime) != epoch);
    CHECK(wf_drivers_quiescent());
    handoff_fixture_end();
}

static void readiness_ingress_bound(void) {
    handoff_fixture();
    CHECK(wf_driver_poll_wait_ms(&wf_driver_root) == -1);
    wf_invocation_begin();
    CHECK(wf_driver_poll_wait_ms(&wf_driver_root) == 1);
#if !defined(_WIN32)
    int descriptors[2];
    CHECK(pipe(descriptors) == 0);
    wf_context polling = {.poll_descriptor = descriptors[0], .poll_events = WF_FILE_READABLE};
    wf_driver_root.polling = &polling;
    wf_driver_root.polling_count = 1u;
    /* No descriptor ever becomes ready. The production-selected finite
     * wait must return so that a later ingress can be adopted. */
    CHECK(!wf_context_poll(&wf_driver_root, wf_driver_poll_wait_ms(&wf_driver_root)));
    wf_driver_root.polling = NULL;
    wf_driver_root.polling_count = 0u;
    CHECK(close(descriptors[0]) == 0 && close(descriptors[1]) == 0);
#endif
    wf_context ready = {.driver = &wf_driver_root, .disposition = WF_HAND_BACK_READY};
    wf_context_ingress(&wf_driver_root, &ready);
    wf_invocation_end();
    CHECK(wf_driver_poll_wait_ms(&wf_driver_root) == 0);
    CHECK(wf_driver_adopt_ingress(&wf_driver_root));
    CHECK(wf_run_take(&wf_driver_root) == &ready);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    handoff_fixture_end();
}

static void *launcher_main(void *opaque) {
    launcher_thread = pthread_self();
    wf__context_root_begin();
    wf__context_root_run(opaque);
    return NULL;
}

static void root_takeover_cleanup(void) {
    /* Actual warm-spare, monitor and launcher lifecycle, with barriers in
     * the handwritten frames. No elapsed time selects success. */
    fixture_begin();
    fixture_end();
    atomic_store(&wf_monitor_stopping, 0u);
    atomic_store(&wf_monitor_exited, 0u);
    atomic_store(&computing, 0u);
    atomic_store(&release_compute, 0u);
    atomic_store(&ran_timer, 0u);
    service_frame root = {.kind = FRAME_LAUNCHER};
    pthread_t thread;
    CHECK(pthread_create(&thread, NULL, launcher_main, &root) == 0);
    while (!atomic_load(&computing) || !atomic_load(&ran_timer)
        || !atomic_load(&wf_driver_root.idle)) wf_prim_yield();
    CHECK(!wf_contexts_stuck(&wf_driver_root));
    CHECK(!atomic_load(&destroyed_child) && !atomic_load(&destroyed_root));
    atomic_store(&release_compute, 1u);
    CHECK(pthread_join(thread, NULL) == 0);
    CHECK(atomic_load(&destroyed_child) == 1u && atomic_load(&destroyed_root) == 1u);
    CHECK(atomic_load(&wf_driver_stats[0].reassignments) == 1u);
    CHECK(atomic_load(&wf_driver_stats[0].ingress_commits) == 1u);
    CHECK(!atomic_load(&wf_drivers_moving) && !wf_invocations_active());
    CHECK(!wf_driver_root.ingress_head && !wf_driver_root.parked);
    wf_pool_give(wf_driver_root.timers, wf_driver_root.timer_bytes);
}

int main(void) {
    wf_test_guard_start(30);
    wf_bridge_require();
    CHECK(!wf_bridge_ring_ready()); /* deterministic service-only fixture */
    wf_test_guard_phase("counted atomic boundaries share threshold and displacement handling");
    counted_atomic_boundaries();
    wf_test_guard_phase("lone busy context services sleepers and displaced ingress");
    solitary_service_boundaries();
    wf_test_guard_phase("64 lone-context resumptions service an expired sleep without monitor");
    solitary_sleep_reap();
    wf_test_guard_phase("executor generations reject complete intervening invocations");
    invocation_collections();
    wf_test_guard_phase("persistent idle scans and searcher claim ownership");
    idle_quiescence();
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
    wf_test_guard_phase("return versus monitor, borrow exclusion and repeated stale epochs");
    reassignment_races();
    wf_test_guard_phase("timer takeover, physical map users and exactly-once ingress dispositions");
    displaced_dispositions();
    wf_test_guard_phase("root COMPLETE ingress stays live until adoption retires");
    root_ingress_quiescence();
    wf_test_guard_phase("descriptor polling observes displaced ingress without a ready descriptor");
    readiness_ingress_bound();
    wf_test_guard_phase("root takeover, displaced false-stuck prevention and launcher cleanup once");
    root_takeover_cleanup();
    wf_test_guard_finish();
    return 0;
}
