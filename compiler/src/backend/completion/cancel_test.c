/* Cancellation runtime probe: includes the private bridge so a deterministic
 * schedule can observe registration, park, notification-before-sleep, and
 * exactly one ready-queue insertion. The other pthread stands for another
 * driver's firing context, as in shared_object_test.c. Socketpairs carry
 * real receives; the silent peer remains open throughout cancellation.
 * No test-specific branch is compiled into the runtime. */
#define _GNU_SOURCE
/* These races observe broadcast wake deduplication, rather than age-based
 * selection. Keep them below the age threshold regardless of host speed;
 * shared_object_test.c supplies the clock and separately covers an aged,
 * reserved guard completing through its cancellation alternative. */
#define WF_GUARD_TURN_AGE_NS UINT64_MAX
#include "bridge.c"
#include "../ordinary_values.c"
#include "../runtime_test_guard.h"
#include <pthread.h>
#include <netinet/in.h>
#include <sys/socket.h>

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "cancel-test:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

static wf_driver firing_driver;
static int frame;

#if WF_GUARD_MODE == 2
/* Recovering a missing first clock changes already-published link keys.
 * Five objects plus a table unit exercise both inline and chunk links;
 * registration on a sixth object recovers the age through the production
 * clock/registration path, without a mock clock or public interface. */
static void recovered_guard_age(void) {
    wf_context contexts[3] = {0};
    wf_watch watches[3] = {0};
    wf_shared *objects[6];
    wf_watch_list table = {0};
    wf_watch_chunk *dead = NULL;
    wf_context *saved = wf_context_current;
    wf_test_guard_phase("ordered guard queue: missing clock recovery");
    for (unsigned unit = 0u; unit < 6u; ++unit) objects[unit] = wf__shared_new(1u);
    contexts[0].guard_started = 1u;
    contexts[1].guard_started = 10u;
    contexts[2].guard_started = 20u;
    for (unsigned left = 3u; left != 0u; --left) {
        unsigned i = left - 1u;
        wf_context_current = &contexts[i];
        wf__watch_begin(&watches[i]);
        for (unsigned unit = 0u; unit < 5u; ++unit)
            wf__watch_object(&watches[i], objects[unit]);
        wf__watch_unit(&watches[i], &table);
    }
    wf_spin_lock(&wf_watch_lock);
    for (unsigned unit = 0u; unit < 5u; ++unit)
        CHECK(objects[unit]->watching->watch == &watches[0]);
    /* Age 1 -> 0 preserves the existing order but arranges the missing
     * first-sample state. A later object registration with an earlier hold
     * must take a valid clock sample and repair every prior link itself. */
    contexts[0].guard_started = 0u;
    wf_spin_unlock(&wf_watch_lock);
    wf_context_current = &contexts[0];
    contexts[0].shared_holds = 1u;
    wf__watch_object(&watches[0], objects[5]);
    contexts[0].shared_holds = 0u;
    wf_spin_lock(&wf_watch_lock);
    CHECK(contexts[0].guard_started > 20u);
    CHECK(objects[5]->watched == 1u && objects[5]->watching->watch == &watches[0]);
    CHECK(objects[5]->watching->next == NULL);
    /* The independent expected age order is 10, 20, recovered; repaired
     * links retain their counts and consistent back pointers. Removing
     * the registration's repair call must fail these order assertions. */
    const unsigned expected[3] = {1u, 2u, 0u};
    for (unsigned unit = 0u; unit < 6u; ++unit) {
        wf_watch_link **back = unit == 5u ? &table.links : &objects[unit]->watching;
        CHECK((unit == 5u ? table.count : objects[unit]->watched) == 3u);
        for (unsigned slot = 0u; slot < 3u; ++slot) {
            wf_watch_link *link = *back;
            CHECK(link != NULL && link->watch == &watches[expected[slot]]);
            CHECK(link->back == back);
            back = &link->next;
        }
        CHECK(*back == NULL);
    }
    for (unsigned i = 0u; i < 3u; ++i)
        CHECK(wf_watch_wake_one_locked(&watches[i], &dead) == NULL);
    CHECK(table.count == 0u && table.links == NULL);
    for (unsigned unit = 0u; unit < 6u; ++unit)
        CHECK(objects[unit]->watched == 0u && objects[unit]->watching == NULL);
    wf_spin_unlock(&wf_watch_lock);
    wf_watch_give_back(dead);
    wf_context_current = saved;
    for (unsigned unit = 0u; unit < 6u; ++unit) {
        CHECK(wf__shared_release(objects[unit]));
        wf__shared_free(objects[unit]);
    }
}

/* Representation boundaries not observable through the public frame ABI:
 * equal ages use context identity, removing a successor through another
 * unit repairs the reservation anchor, and a new age beyond the saved
 * cutoff cannot receive the old notification. Stack-array addresses are
 * the independent identity oracle; no production comparator constructs
 * the expected order. All fixture publication/removal uses production
 * watch operations and the same object-then-watch lock order. */
static void ordered_guard_queue(void) {
    wf_context contexts[4] = {0};
    wf_watch watches[4] = {0};
    wf_context *saved = wf_context_current;
    wf_shared *ordinary = wf__shared_new(1u);
    wf_shared *other = wf__shared_new(1u);
    wf_watch_chunk *dead = NULL;
    wf_context *ready = NULL;
    wf_test_guard_phase("ordered guard queue: identity, cross-unit unlink, cutoff");
    for (unsigned i = 0u; i < 4u; ++i) {
        contexts[i].guard_started = i == 3u ? 40u : 10u;
        if (i != 0u) CHECK((uintptr_t)&contexts[i - 1u] < (uintptr_t)&contexts[i]);
    }
    /* Scrambled registration makes unordered prepend and reverse tie
     * comparators distinguishable from age/identity order. */
    const unsigned registration[4] = {2u, 0u, 3u, 1u};
    for (unsigned slot = 0u; slot < 4u; ++slot) {
        unsigned i = registration[slot];
        wf_context_current = &contexts[i];
        wf__watch_begin(&watches[i]);
        wf__watch_object(&watches[i], ordinary);
        if (i == 1u) wf__watch_object(&watches[i], other);
    }
    wf_spin_lock(&ordinary->lock);
    wf_spin_lock(&wf_watch_lock);
    CHECK(ordinary->watched == 4u && other->watched == 1u);
    wf_watch_link *link = ordinary->watching;
    for (unsigned i = 0u; i < 4u; ++i) {
        CHECK(link != NULL && link->watch == &watches[i]);
        link = link->next;
    }
    CHECK(link == NULL);
    CHECK(wf_guard_reserve_locked(ordinary, &watches[0], 0u, 20u, &dead) == NULL);
    CHECK(ordinary->reservation == &contexts[0]);
    CHECK(contexts[0].guard_anchor.next->watch == &watches[1]);
    wf_spin_unlock(&wf_watch_lock);
    wf_spin_unlock(&ordinary->lock);

    /* Notify the anchor's immediate successor through its other unit.
     * Detaching both links must repair the anchor even though its selected
     * watch no longer owns any real link on ordinary. */
    wf_spin_lock(&other->lock);
    wf_spin_lock(&wf_watch_lock);
    CHECK(wf_watch_wake_one_locked(&watches[1], &dead) == NULL);
    CHECK(other->watched == 0u && other->watching == NULL);
    CHECK(ordinary->watched == 2u);
    CHECK(contexts[0].guard_anchor.next->watch == &watches[2]);
    CHECK(contexts[0].guard_anchor.next->back == &contexts[0].guard_anchor.next);
    wf_spin_unlock(&wf_watch_lock);
    wf_spin_unlock(&other->lock);

    wf_context_current = &contexts[0];
    wf__watch_begin(&watches[0]);
    wf__watch_object(&watches[0], ordinary);
    wf_spin_lock(&ordinary->lock);
    CHECK(wf_guard_release_locked(ordinary, 0, &dead, &ready) == 0u);
    CHECK(ready == NULL && ordinary->reservation == &contexts[2]);
    CHECK(contexts[0].guard_anchor.back == NULL && contexts[0].shared_turn == NULL);
    wf_spin_unlock(&ordinary->lock);

    wf_context_current = &contexts[2];
    wf__watch_begin(&watches[2]);
    wf__watch_object(&watches[2], ordinary);
    wf_spin_lock(&ordinary->lock);
    CHECK(wf_guard_release_locked(ordinary, 0, &dead, &ready) == 0u);
    CHECK(ordinary->reservation == NULL && contexts[2].shared_turn == NULL);
    CHECK(contexts[2].guard_anchor.back == NULL);
    wf_spin_unlock(&ordinary->lock);
    wf_spin_lock(&wf_watch_lock);
    CHECK(watches[3].state == WF_GUARD_REGISTERED && ordinary->watched == 3u);
    CHECK(wf_watch_wake_one_locked(&watches[0], &dead) == NULL);
    CHECK(wf_watch_wake_one_locked(&watches[2], &dead) == NULL);
    CHECK(wf_watch_wake_one_locked(&watches[3], &dead) == NULL);
    CHECK(ordinary->watching == NULL && ordinary->watched == 0u && dead == NULL);
    wf_spin_unlock(&wf_watch_lock);
    wf_context_current = saved;
    CHECK(wf__shared_release(ordinary));
    wf__shared_free(ordinary);
    CHECK(wf__shared_release(other));
    wf__shared_free(other);
    recovered_guard_age();
}
#endif

static void *fire_on_other_driver(void *source) {
    wf_context firing = {0};
    wf_host_operation call = {0};
    uint8_t result = 0xff;
    firing.driver = &firing_driver;
    wf_driver_self = &firing_driver;
    wf_context_current = &firing;
    for (unsigned repeat = 0; repeat < 2; ++repeat) {
        CHECK(wf__body_cancel_fire_start(&result, source, &call) == 3);
        CHECK(wf__context_wait(&call, &frame) == 0);
        wf__body_cancel_fire_finish(&result, source, &call);
        CHECK(result == 0);
    }
    wf_context_current = NULL;
    wf_driver_self = NULL;
    return NULL;
}

static void fire_elsewhere(wf_cancel_handle *source) {
    pthread_t thread;
    CHECK(pthread_create(&thread, NULL, fire_on_other_driver, source) == 0);
    CHECK(pthread_join(thread, NULL) == 0);
}

static wf_host_operation *operation(void) {
    return wf__context_operation();
}

static void parked(int state) {
    CHECK(state == 2);
    CHECK(wf__context_wait(operation(), &frame) == 1);
    CHECK(wf_context_root.record == &operation()->record);
    CHECK(atomic_load(&wf_driver_root.host_waits) == 1u);
}

static void finish_park(void) {
    /* The test watchdog fails a lost wake/cancellation; no test timeout is
     * passed as the operation's deadline, so it cannot make a broken fire pass. */
    while (wf_context_root.record != NULL) {
        wf_driver_reap(&wf_driver_root);
        if (wf_context_root.record == NULL) break;
        uint64_t epoch = wf_completion_wake_epoch(wf_driver_root.runtime);
        if (!wf_driver_find_cancelled(&wf_driver_root)
            && !wf_context_harvest(&wf_driver_root) && wf_context_root.record != NULL)
            wf_bridge_park(epoch, wf_timer_wait_ms(&wf_driver_root));
    }
    CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    CHECK(wf_driver_root.cancel_waits == NULL);
    CHECK(wf_context_root.timer_slot == 0);
    CHECK(wf_driver_root.timer_count == 0);
    CHECK(atomic_load(&wf_driver_root.host_waits) == 0u);
}

static void stopped_result(const wf_read_result *result, unsigned char byte, unsigned reason) {
    CHECK(result->tag == 1);
    CHECK(result->err.error.tag == 1);
    CHECK(result->err.error.error.tag == reason);
    CHECK(byte == 0xcc);
}

typedef struct wake_race {
    wf_cancel_handle *source;
    void *ordinary;
    _Atomic unsigned arrived;
    _Atomic unsigned go;
} wake_race;

static void meet_wake_race(wake_race *race) {
    atomic_fetch_add(&race->arrived, 1u);
    while (!atomic_load(&race->go)) wf_prim_yield();
}

static void *race_fire(void *opaque) {
    wake_race *race = opaque;
    meet_wake_race(race);
    return fire_on_other_driver(race->source);
}

static void *race_write(void *opaque) {
    wake_race *race = opaque;
    wf_context writer = {0};
    writer.driver = &firing_driver;
    wf_driver_self = &firing_driver;
    wf_context_current = &writer;
    meet_wake_race(race);
    CHECK(wf__shared_acquire(race->ordinary, 1u, &frame) == 0);
    wf__shared_unlock(race->ordinary, 1u);
    wf_context_current = NULL;
    wf_driver_self = NULL;
    return NULL;
}

/* Each schedule uses the production start/wait/finish and guard protocol.
 * The watchdog, never a program deadline, detects a lost wake. */
static void guard_races(void) {
    wf_cancel_handle source, watch;
    wf_watch guard;
    wf_test_guard_phase("cancellation guard: fire before registration");
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    wf_cancel *view = wf__body_cancel_state(&watch);
    CHECK(view == source.state);
    fire_elsewhere(&source);
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    CHECK(view->visible_fired == 1u);
    wf__shared_unlock(view, 1u);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    CHECK(wf__shared_release(view));
    wf__shared_free(view);

    wf_test_guard_phase("cancellation guard: register before fire before park");
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    view = wf__body_cancel_state(&watch);
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    CHECK(view->visible_fired == 0u);
    wf__watch_begin(&guard);
    wf__watch_object(&guard, view);
    wf__watch_object(&guard, view);
    CHECK(guard.used == 1u && view->shared.watched == 1u);
    wf__shared_unlock(view, 1u);
    fire_elsewhere(&source);
    CHECK(wf__watch_park(&guard, &frame) == 0);
    CHECK(guard.used == 0u && view->shared.watched == 0u);
    CHECK(view->shared.watching == NULL);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    /* The early-woken statement retries at once, as emitted code does,
     * then completes by releasing the unit. */
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    CHECK(view->visible_fired == 1u);
    wf__shared_unlock(view, 1u);
    CHECK(view->shared.reservation == NULL);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    CHECK(wf__shared_release(view));
    wf__shared_free(view);

    wf_test_guard_phase("cancellation guard: two parked contexts on two drivers");
    wf_context other = {0};
    wf_watch other_guard;
    other.driver = &firing_driver;
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    view = wf__body_cancel_state(&watch);
    void *ordinary = wf__shared_new(1u);
    for (unsigned index = 0; index < 2; ++index) {
        wf_watch *current = index ? &other_guard : &guard;
        wf_context_current = index ? &other : &wf_context_root;
        wf_driver_self = index ? &firing_driver : &wf_driver_root;
        CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
        CHECK(wf__shared_acquire(ordinary, 1u, &frame) == 0);
        CHECK(view->visible_fired == 0u);
        wf__watch_begin(current);
        wf__watch_object(current, view);
        wf__watch_object(current, ordinary);
        wf__shared_unlock(ordinary, 1u);
        wf__shared_unlock(view, 1u);
        CHECK(wf__watch_park(current, &frame) == 1);
    }
    wf_context_current = &wf_context_root;
    wf_driver_self = &wf_driver_root;
    fire_elsewhere(&source);
    CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    CHECK(wf_run_take(&firing_driver) == &other);
    CHECK(wf_run_take(&firing_driver) == NULL);
    CHECK(guard.used == 0u && other_guard.used == 0u);
    CHECK(view->shared.watching == NULL && view->shared.watched == 0u);
    CHECK(((wf_shared *)ordinary)->watching == NULL);
    CHECK(((wf_shared *)ordinary)->watched == 0u);
    /* A later write on another dependency cannot enqueue either twice. */
    CHECK(wf__shared_acquire(ordinary, 1u, &frame) == 0);
    wf__shared_unlock(ordinary, 1u);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    CHECK(wf_run_take(&firing_driver) == NULL);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    CHECK(view->visible_fired == 1u && atomic_load(&view->fired) == 1u);
    wf__shared_unlock(view, 1u);
    CHECK(wf__shared_release(view));
    wf__shared_free(view);
    CHECK(wf__shared_release(ordinary));
    wf__shared_free(ordinary);

    wf_test_guard_phase("cancellation guard: concurrent state and firing wakes");
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    view = wf__body_cancel_state(&watch);
    ordinary = wf__shared_new(1u);
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    CHECK(wf__shared_acquire(ordinary, 1u, &frame) == 0);
    wf__watch_begin(&guard);
    wf__watch_object(&guard, view);
    wf__watch_object(&guard, ordinary);
    wf__shared_unlock(ordinary, 1u);
    wf__shared_unlock(view, 1u);
    CHECK(wf__watch_park(&guard, &frame) == 1);
    wake_race race = { .source = &source, .ordinary = ordinary };
    pthread_t fire_thread, write_thread;
    CHECK(pthread_create(&fire_thread, NULL, race_fire, &race) == 0);
    CHECK(pthread_create(&write_thread, NULL, race_write, &race) == 0);
    while (atomic_load(&race.arrived) != 2u) wf_prim_yield();
    atomic_store(&race.go, 1u);
    CHECK(pthread_join(fire_thread, NULL) == 0);
    CHECK(pthread_join(write_thread, NULL) == 0);
    CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    CHECK(guard.used == 0u && view->shared.watched == 0u);
    CHECK(view->shared.watching == NULL);
    CHECK(((wf_shared *)ordinary)->watched == 0u);
    CHECK(((wf_shared *)ordinary)->watching == NULL);
    /* The woken statement retries its targets, as emitted code does, and
     * releases every unit when the cancellation alternative completes. */
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    CHECK(wf__shared_acquire(ordinary, 1u, &frame) == 0);
    wf__shared_unlock(ordinary, 1u);
    wf__shared_unlock(view, 1u);
    CHECK(wf_context_root.shared_turn == NULL);
    CHECK(view->shared.reservation == NULL && ((wf_shared *)ordinary)->reservation == NULL);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    CHECK(wf__shared_release(view));
    wf__shared_free(view);
    CHECK(wf__shared_release(ordinary));
    wf__shared_free(ordinary);

    wf_test_guard_phase("cancellation fire: parked acquisition and retry");
    wf_context firing = {0};
    wf_host_operation call = {0};
    uint8_t result = 0xff;
    firing.driver = &firing_driver;
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    view = wf__body_cancel_state(&watch);
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    wf_context_current = &firing;
    wf_driver_self = &firing_driver;
    CHECK(wf__body_cancel_fire_start(&result, &source, &call) == 3);
    CHECK(wf__context_wait(&call, &frame) == 1);
    CHECK(view->visible_fired == 0u && atomic_load(&view->fired) == 0u);
    CHECK(result == 0xff && firing.record == NULL);
    wf_context_current = &wf_context_root;
    wf_driver_self = &wf_driver_root;
    wf__shared_unlock(view, 1u);
    CHECK(wf_run_take(&firing_driver) == &firing);
    /* A contender wins before the woken fire; its next wait must retry. */
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    wf_context_current = &firing;
    wf_driver_self = &firing_driver;
    CHECK(wf__context_wait(&call, &frame) == 1);
    CHECK(view->visible_fired == 0u && atomic_load(&view->fired) == 0u);
    wf_context_current = &wf_context_root;
    wf_driver_self = &wf_driver_root;
    wf__shared_unlock(view, 1u);
    CHECK(wf_run_take(&firing_driver) == &firing);
    wf_context_current = &firing;
    wf_driver_self = &firing_driver;
    CHECK(wf__context_wait(&call, &frame) == 0);
    wf__body_cancel_fire_finish(&result, &source, &call);
    CHECK(result == 0 && view->visible_fired == 1u && atomic_load(&view->fired) == 1u);
    CHECK(view->shared.waiting_tail == NULL);
    CHECK(atomic_load(&view->shared.holders) == 0u);
    CHECK(wf_run_take(&firing_driver) == NULL);
    wf_context_current = &wf_context_root;
    wf_driver_self = &wf_driver_root;
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    CHECK(wf__shared_release(view));
    wf__shared_free(view);

    wf_test_guard_phase("cancellation view: retained never state");
    CHECK(atomic_load(&wf_never_state.shared.handles) == 1u);
    wf__body_cancel_never(&watch);
    view = wf__body_cancel_state(&watch);
    void *same = wf__body_cancel_state(&watch);
    CHECK(view == same && watch.state == view);
    CHECK(atomic_load(&wf_never_state.shared.handles) == 4u);
    wf__body_close_cancel_watch(&watch);
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    CHECK(view->visible_fired == 0u && atomic_load(&view->fired) == 0u);
    wf__shared_unlock(view, 1u);
    CHECK(!wf__shared_release(same));
    CHECK(!wf__shared_release(view));
    CHECK(atomic_load(&wf_never_state.shared.handles) == 1u);
}

int main(void) {
    wf_test_guard_start(30);
#if WF_GUARD_MODE == 2
    const char *selected = getenv("CANCEL_TEST_CASE");
    if (selected != NULL) {
        CHECK(strcmp(selected, "ordered") == 0);
        ordered_guard_queue();
        wf_test_guard_finish();
        return 0;
    }
#endif
    wf_test_guard_phase("find-at-fire cancellation");
    wf__context_root_begin();
    firing_driver.runtime = &firing_driver.own_runtime;
    CHECK(wf_completion_runtime_init(firing_driver.runtime) == 0);
    atomic_store(&wf_drivers[1], &firing_driver);
    atomic_store(&wf_driver_count, 2u);

    int pair[2];
    CHECK(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);
    wf_value receive;
    wf_cancel_handle source, shared, watch, never;
    wf_descriptor_value(&receive, pair[0]);
    wf__body_cancel_source(&source);
    wf__body_cancel_share(&shared, &source);
    wf__body_cancel_watch(&watch, &source);
    wf__body_close_cancel_source(&source); /* watch and shared retain it */
    wf__body_cancel_never(&never);
    unsigned char byte = 0xcc;
    wf_view destination = {&byte, 1};
    wf_read_result result;

    parked(wf__body_receive_next_start(&result, &receive, &destination,
                                             0, 1, NULL, &watch, operation()));
    CHECK(wf_driver_root.cancel_waits == &wf_context_root);
    CHECK(wf_driver_root.timer_count == 0); /* no clock deadline heap work */
    uint64_t before = wf_completion_wake_epoch(wf_driver_root.runtime);
    fire_elsewhere(&shared);
    CHECK(wf_completion_wake_epoch(wf_driver_root.runtime) != before);
    /* A notification arriving before the native park cannot be lost. */
    CHECK(wf_completion_park_if_unchanged(wf_driver_root.runtime, before, 0)
          == WF_COMPLETION_PARK_EPOCH_CHANGED);
    finish_park();
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, NULL, &watch, operation());
    stopped_result(&result, byte, WF_IO_CANCELLED);
    fire_elsewhere(&shared);
    wf_driver_reap(&wf_driver_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);

    /* A watch already fired before submission answers without touching the
     * socket, even when a byte is ready, and remains valid after all sources
     * have closed. The never watch must then receive that very byte. */
    wf__body_close_cancel_source(&shared);
    CHECK(send(pair[1], "N", 1, 0) == 1);
    uint64_t publications = atomic_load(&wf_bridge_publications);
    CHECK(wf__body_receive_next_start(&result, &receive, &destination,
                  0, 1, NULL, &watch, operation()) == 1);
    CHECK(atomic_load(&wf_bridge_publications) == publications + 1u);
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, NULL, &watch, operation());
    stopped_result(&result, byte, WF_IO_CANCELLED);
    wf__body_close_cancel_watch(&watch);
    int state = wf__body_receive_next_start(&result, &receive, &destination,
                                                 0, 1, NULL, &never, operation());
    CHECK(wf_driver_root.cancel_waits == NULL);
    if (state == 2) {
        if (wf__context_wait(operation(), &frame)) finish_park();
    } else CHECK(state == 1);
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, NULL, &never, operation());
    CHECK(result.tag == 0 && result.ok.value == 1 && byte == 'N');

    /* Host completion of a real watch unlinks before its context can move;
     * firing that source afterwards cannot cancel a later operation. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    wf_deadline later = {0};
    later.tag = WF_OPTION_SOME;
    later.value.ticks = wf_file_monotonic_ns() + UINT64_C(60000000000);
    parked(wf__body_receive_next_start(&result, &receive, &destination,
                                             0, 1, &later, &watch, operation()));
    CHECK(wf_driver_root.timer_count == 1);
    CHECK(send(pair[1], "R", 1, 0) == 1);
    finish_park();
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, NULL, &watch, operation());
    CHECK(result.tag == 0 && byte == 'R');
    fire_elsewhere(&source);
    wf_driver_reap(&wf_driver_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    /* Fire after linking but before context_wait, with the driver's epoch
     * already advanced: the driver's post-snapshot recheck still finds it. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    byte = 0xcc;
    state = wf__body_receive_next_start(&result, &receive, &destination,
                                             0, 1, NULL, &watch, operation());
    fire_elsewhere(&source);
    parked(state);
    CHECK(wf_driver_find_cancelled(&wf_driver_root));
    finish_park();
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, NULL, &watch, operation());
    stopped_result(&result, byte, WF_IO_CANCELLED);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    /* Clock expiry removes the same watch even when the source never fires. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    later.value.ticks = 1;
    parked(wf__body_receive_next_start(&result, &receive, &destination,
                                             0, 1, &later, &watch, operation()));
    finish_park();
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, &later, &watch, operation());
    stopped_result(&result, byte, WF_IO_DEADLINE_PASSED);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    /* The other watched public operation returns its factory credit when
     * cancellation ends an accept, without manufacturing a connection. */
    int listener_fd = socket(AF_INET, SOCK_STREAM, 0);
    CHECK(listener_fd >= 0);
    struct sockaddr_in address;
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    CHECK(bind(listener_fd, (struct sockaddr *)&address, sizeof(address)) == 0);
    CHECK(listen(listener_fd, 1) == 0);
    wf_value listener, factory = {{8, 0, 0, 0}};
    wf_descriptor_value(&listener, listener_fd);
    wf_accept_result accepted;
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    parked(wf__body_tcp_accept_start(&accepted, &factory, &listener,
                                           NULL, &watch, operation()));
    CHECK(factory.words[0] == 7);
    fire_elsewhere(&source);
    finish_park();
    wf__body_tcp_accept_finish(&accepted, &factory, &listener, NULL, &watch, operation());
    CHECK(accepted.tag == 1 && accepted.err.error.tag == WF_IO_CANCELLED);
    CHECK(factory.words[0] == 8);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    CHECK(close(listener_fd) == 0);

    /* Empty completion between watched start and park removes registration
     * before the ordinary fairness pass can migrate the context. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    CHECK(wf__body_receive_next_start(&result, &receive, &destination,
                  0, 0, NULL, &watch, operation()) == 2);
    CHECK(wf__context_wait(operation(), &frame) == 0);
    CHECK(wf_driver_root.cancel_waits == NULL);
    CHECK(wf_context_root.timer_slot == 0);
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 0, NULL, &watch, operation());
    CHECK(result.tag == 0 && result.ok.value == 0);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    wf__body_close_cancel_watch(&never);

    /* A timer uses the same watched list but completes in its own driver.
     * No host timeout is allowed to stand in for the firing. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    wf_instant future = {wf_file_monotonic_ns() + UINT64_C(60000000000)};
    wf_sleep_result slept;
    parked(wf__body_sleep_until_start(&slept, &future, &watch, operation()));
    fire_elsewhere(&source);
    finish_park();
    wf__body_sleep_until_finish(&slept, &future, &watch, operation());
    CHECK(slept.tag == 1);
    CHECK(wf_file_monotonic_ns() < future.ticks);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    /* Every other watched route rejects a fired-before-wait operation with
     * its own result shape, leaving the transfer and handle credit intact. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    fire_elsewhere(&source);
    byte = 0xcc;
    CHECK(wf__body_read_next_start(&result, &factory, &receive, &destination,
                                   0, 1, NULL, &watch, operation()) == 1);
    wf__body_read_next_finish(&result, &factory, &receive, &destination,
                              0, 1, NULL, &watch, operation());
    stopped_result(&result, byte, WF_IO_CANCELLED);
    wf_write_result written;
    CHECK(wf__body_write_once_start(&written, &factory, &receive, &destination,
                                    0, 1, NULL, &watch, operation()) == 1);
    wf__body_write_once_finish(&written, &factory, &receive, &destination,
                               0, 1, NULL, &watch, operation());
    CHECK(written.tag == 1 && written.err.error.tag == WF_IO_CANCELLED);
    CHECK(wf__body_send_once_start(&written, &receive, &destination,
                                   0, 1, NULL, &watch, operation()) == 1);
    wf__body_send_once_finish(&written, &receive, &destination,
                              0, 1, NULL, &watch, operation());
    CHECK(written.tag == 1 && written.err.error.tag == WF_IO_CANCELLED);
    wf_connect_result connected;
    wf_value address_value = {{UINT64_C(0x0100007f), 0, 1, 0}};
    CHECK(wf__body_tcp_connect_start(&connected, &factory, &address_value,
                                     NULL, &watch, operation()) == 1);
    wf__body_tcp_connect_finish(&connected, &factory, &address_value,
                                NULL, &watch, operation());
    CHECK(connected.tag == 1 && connected.err.error.tag == WF_IO_CANCELLED);
    CHECK(factory.words[0] == 8);
    CHECK(wf__body_sleep_until_start(&slept, &future, &watch, operation()) == 1);
    wf__body_sleep_until_finish(&slept, &future, &watch, operation());
    CHECK(slept.tag == 1);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    guard_races();
#if WF_GUARD_MODE == 2
    ordered_guard_queue();
#endif
    CHECK(close(pair[0]) == 0 && close(pair[1]) == 0);
    atomic_store(&wf_driver_count, 1u);
    while (atomic_load(&wf_drivers_notifying) != 0u) wf_prim_yield();
    atomic_store(&wf_drivers[1], NULL);
    CHECK(wf_completion_runtime_destroy(firing_driver.runtime) == 0);
    wf_test_guard_finish();
    return 0;
}
