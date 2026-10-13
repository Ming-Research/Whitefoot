/* Cancellation runtime probe: includes the private bridge so a deterministic
 * schedule can observe registration, park, notification-before-sleep, and
 * exactly one ready-queue insertion; and, since it owns that translation
 * unit, the private wake-test reader on states it writes. The other pthread stands for another
 * driver's firing context, as in shared_object_test.c. Socketpairs carry
 * real receives; the silent peer remains open throughout cancellation.
 * No test-specific branch is compiled into the runtime. */
#if defined(__APPLE__)
#ifndef _DARWIN_C_SOURCE
#define _DARWIN_C_SOURCE 1
#endif
#else
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#endif
/* These races observe broadcast wake deduplication, rather than age-based
 * selection. Keep them below the age threshold regardless of host speed;
 * shared_object_test.c supplies the clock and separately covers an aged,
 * reserved guard completing through its cancellation alternative. A write
 * below the threshold wakes only the oldest watch whose wake test holds, and
 * its retry passes the notification on, so the two-context firing is
 * followed through both retries. */
#ifndef WF_GUARD_TURN_AGE_NS
#define WF_GUARD_TURN_AGE_NS UINT64_MAX
#endif
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
    /* The handwritten wait has now unwound, as the driver loop requires. */
    wf_context_adopt_wait(&wf_driver_root, &wf_context_root);
    CHECK(wf_context_root.record == &operation()->record);
    CHECK(atomic_load(&wf_driver_root.host_waits) == 1u);
}

static void finish_park(void) {
    if (wf_context_root.record != NULL && atomic_load(&wf_driver_root.host_waits) == 0u)
        wf_context_adopt_wait(&wf_driver_root, &wf_context_root);
    wf_context_current = NULL;
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
    wf_context_current = &wf_context_root;
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

/* The wake-test reader (bridge.h's wf_wake_test), which this translation
 * unit reaches because it includes the bridge: on states written here, each
 * form answers its guard's value, on states chosen so that a reader that
 * ignored the sign, read another width or more than the low bit, or combined
 * atoms otherwise would answer the opposite. Bytes the atom does not read
 * hold 0xa5, which a wider read sees. */
enum { READER_STATE_BYTES = 32 };
static _Alignas(16) unsigned char reader_object[WF_SHARED_STATE_OFFSET + READER_STATE_BYTES];

static void reader_put(uint32_t offset, const void *value, size_t bytes) {
    memcpy(reader_object + WF_SHARED_STATE_OFFSET + offset, value, bytes);
}

static wf_wake_test reader_atom(uint32_t offset, uint8_t width, uint8_t flags, uint8_t comparison,
                                uint64_t value) {
    wf_wake_test test;
    memset(&test, 0, sizeof(test));
    test.length = 1u;
    test.program[0] = 1u;
    test.atoms[0].offset = offset;
    test.atoms[0].value = value;
    test.atoms[0].width = width;
    test.atoms[0].flags = flags;
    test.atoms[0].comparison = comparison;
    return test;
}

static int reader_holds(const wf_wake_test *test) {
    CHECK(wf_wake_used(test, READER_STATE_BYTES) >= 1u);
    return wf_wake_holds(test, (const wf_shared *)(const void *)reader_object);
}

static void wake_reader(void) {
    wf_test_guard_phase("wake-test reader");
    memset(reader_object, 0xa5, sizeof(reader_object));
    /* A signed 4-byte field below -2: a zero-extending load makes -5 large
     * and positive, an unsigned comparison puts 3 below -2, and an 8-byte
     * read of 3 sees the 0xa5 bytes above it as a negative number. */
    wf_wake_test test = reader_atom(0u, 4u, WF_WAKE_SIGNED, WF_WAKE_LESS, (uint64_t)(int64_t)-2);
    int32_t level = -5;
    reader_put(0u, &level, sizeof(level));
    CHECK(reader_holds(&test));
    level = 3;
    reader_put(0u, &level, sizeof(level));
    CHECK(!reader_holds(&test));
    level = -2;
    reader_put(0u, &level, sizeof(level));
    CHECK(!reader_holds(&test));
    /* A signed byte at least 0: unsigned, -1 is 255; a 2-byte read of 1
     * sees 0xa501, negative. */
    test = reader_atom(8u, 1u, WF_WAKE_SIGNED, WF_WAKE_GREATER_EQUAL, 0u);
    int8_t tiny = -1;
    reader_put(8u, &tiny, sizeof(tiny));
    CHECK(!reader_holds(&test));
    tiny = 1;
    reader_put(8u, &tiny, sizeof(tiny));
    CHECK(reader_holds(&test));
    /* A signed 2-byte field equal to -1, whose operand is sign-extended:
     * zero-extended, the field is 0xffff and differs. */
    test = reader_atom(12u, 2u, WF_WAKE_SIGNED, WF_WAKE_EQUAL, UINT64_MAX);
    int16_t half = -1;
    reader_put(12u, &half, sizeof(half));
    CHECK(reader_holds(&test));
    half = 1;
    reader_put(12u, &half, sizeof(half));
    CHECK(!reader_holds(&test));
    /* Unsigned narrow fields: a byte equal to 7, which a 2-byte read sees
     * as 0xa507; a 2-byte field below 0x8000, which a signed read of 0xffff
     * would find below it and a 4-byte read of 0x7fff would not; a 4-byte
     * field above 0x7fffffff, which a signed read of 0x80000000 would not
     * find above it and an 8-byte read of 5 would. */
    memset(reader_object, 0xa5, sizeof(reader_object));
    test = reader_atom(16u, 1u, 0u, WF_WAKE_EQUAL, 7u);
    uint8_t byte = 7u;
    reader_put(16u, &byte, sizeof(byte));
    CHECK(reader_holds(&test));
    byte = 8u;
    reader_put(16u, &byte, sizeof(byte));
    CHECK(!reader_holds(&test));
    test = reader_atom(20u, 2u, 0u, WF_WAKE_LESS, 0x8000u);
    uint16_t word = 0xffffu;
    reader_put(20u, &word, sizeof(word));
    CHECK(!reader_holds(&test));
    word = 0x7fffu;
    reader_put(20u, &word, sizeof(word));
    CHECK(reader_holds(&test));
    test = reader_atom(24u, 4u, 0u, WF_WAKE_GREATER, 0x7fffffffu);
    uint32_t wide = 0x80000000u;
    reader_put(24u, &wide, sizeof(wide));
    CHECK(reader_holds(&test));
    wide = 5u;
    reader_put(24u, &wide, sizeof(wide));
    CHECK(!reader_holds(&test));
    /* A Bool or two-variant tag, one byte read as its low bit: "not
     * false" holds for 1 and not for 2, and equality with 1 holds for
     * 0xff, which a whole-byte reader would answer the other way. */
    test = reader_atom(28u, 1u, WF_WAKE_BIT, WF_WAKE_NOT_EQUAL, 0u);
    byte = 1u;
    reader_put(28u, &byte, sizeof(byte));
    CHECK(reader_holds(&test));
    byte = 2u;
    reader_put(28u, &byte, sizeof(byte));
    CHECK(!reader_holds(&test));
    byte = 0u;
    reader_put(28u, &byte, sizeof(byte));
    CHECK(!reader_holds(&test));
    test = reader_atom(28u, 1u, WF_WAKE_BIT, WF_WAKE_EQUAL, 1u);
    byte = 0xffu;
    reader_put(28u, &byte, sizeof(byte));
    CHECK(reader_holds(&test));
    byte = 0xfeu;
    reader_put(28u, &byte, sizeof(byte));
    CHECK(!reader_holds(&test));
    /* AND and OR of atoms a: word 0 == 1, b: word 1 == 2, c: word 2 == 3,
     * as a and b, a or b, (a and b) or c, and a or (b and c), on all eight
     * states; a reader that swapped the codes or the stack's operands fails
     * one. */
    wf_wake_test both = reader_atom(0u, 8u, 0u, WF_WAKE_EQUAL, 1u);
    both.atoms[1] = both.atoms[0];
    both.atoms[1].offset = 8u;
    both.atoms[1].value = 2u;
    both.atoms[2] = both.atoms[0];
    both.atoms[2].offset = 16u;
    both.atoms[2].value = 3u;
    wf_wake_test either = both, left = both, right = both;
    both.length = 3u;
    both.program[1] = 2u;
    both.program[2] = WF_WAKE_AND;
    either.length = 3u;
    either.program[1] = 2u;
    either.program[2] = WF_WAKE_OR;
    left.length = 5u;
    left.program[1] = 2u;
    left.program[2] = WF_WAKE_AND;
    left.program[3] = 3u;
    left.program[4] = WF_WAKE_OR;
    right.length = 5u;
    right.program[1] = 2u;
    right.program[2] = 3u;
    right.program[3] = WF_WAKE_AND;
    right.program[4] = WF_WAKE_OR;
    for (unsigned bits = 0u; bits < 8u; ++bits) {
        int a = (bits & 1u) != 0u, b = (bits & 2u) != 0u, c = (bits & 4u) != 0u;
        uint64_t words[3] = {a ? 1u : 0u, b ? 2u : 0u, c ? 3u : 0u};
        reader_put(0u, words, sizeof(words));
        CHECK(reader_holds(&both) == (a && b));
        CHECK(reader_holds(&either) == (a || b));
        CHECK(reader_holds(&left) == ((a && b) || c));
        CHECK(reader_holds(&right) == (a || (b && c)));
    }
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
    wf__watch_object(&guard, view, NULL);
    wf__watch_object(&guard, view, NULL);
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
    CHECK(wf_shared_reservation(&view->shared) == NULL);
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
    /* Fixed keys make the root's watch the older one whatever the clock. */
    wf_context_root.guard_started = 1u;
    other.guard_started = 2u;
    for (unsigned index = 0; index < 2; ++index) {
        wf_watch *current = index ? &other_guard : &guard;
        wf_context_current = index ? &other : &wf_context_root;
        wf_driver_self = index ? &firing_driver : &wf_driver_root;
        CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
        CHECK(wf__shared_acquire(ordinary, 1u, &frame) == 0);
        CHECK(view->visible_fired == 0u);
        wf__watch_begin(current);
        wf__watch_object(current, view, NULL);
        wf__watch_object(current, ordinary, NULL);
        wf__shared_unlock(ordinary, 1u);
        wf__shared_unlock(view, 1u);
        CHECK(wf__watch_park(current, &frame) == 1);
        wf_context_commit_wake(wf_context_current);
    }
    wf_context_current = &wf_context_root;
    wf_driver_self = &wf_driver_root;
    fire_elsewhere(&source);
    CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    /* The older watch holds the firing's notification; its retry, which
     * writes neither unit, passes it to the other, whose retry ends it. */
    CHECK(wf_run_take(&firing_driver) == NULL);
    CHECK(guard.used == 0u && other_guard.used == 2u);
    CHECK(wf_shared_reservation(&view->shared) == &wf_context_root);
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    CHECK(wf__shared_acquire(ordinary, 1u, &frame) == 0);
    CHECK(view->visible_fired == 1u);
    wf__shared_unlock_unwritten(ordinary, 1u);
    wf__shared_unlock_unwritten(view, 1u);
    CHECK(wf_shared_reservation(&view->shared) == &other);
    CHECK(wf_run_take(&firing_driver) == &other);
    CHECK(wf_run_take(&firing_driver) == NULL);
    wf_context_current = &other;
    wf_driver_self = &firing_driver;
    CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
    CHECK(wf__shared_acquire(ordinary, 1u, &frame) == 0);
    wf__shared_unlock_unwritten(ordinary, 1u);
    wf__shared_unlock_unwritten(view, 1u);
    CHECK(wf_shared_reservation(&view->shared) == NULL && other.shared_turn == NULL);
    wf_context_current = &wf_context_root;
    wf_driver_self = &wf_driver_root;
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
    wf__watch_object(&guard, view, NULL);
    wf__watch_object(&guard, ordinary, NULL);
    wf__shared_unlock(ordinary, 1u);
    wf__shared_unlock(view, 1u);
    CHECK(wf__watch_park(&guard, &frame) == 1);
    wf_context_commit_wake(wf_context_current);
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
    CHECK(wf_shared_reservation(&view->shared) == NULL && wf_shared_reservation((wf_shared *)ordinary) == NULL);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    CHECK(wf__shared_release(view));
    wf__shared_free(view);
    CHECK(wf__shared_release(ordinary));
    wf__shared_free(ordinary);

    wf_test_guard_phase("cancellation guard: a false wake test on the other unit");
    {
        /* The ordinary unit's test, that its word is 1, stays false: a write
         * that leaves it false wakes nothing, and the firing still wakes the
         * watch through the cancellation state, which has no test. */
        wf_wake_test word_is_one;
        wf_context writer = {0};
        memset(&word_is_one, 0, sizeof(word_is_one));
        word_is_one.length = 1u;
        word_is_one.program[0] = 1u;
        word_is_one.atoms[0].value = 1u;
        word_is_one.atoms[0].width = 8u;
        word_is_one.atoms[0].comparison = WF_WAKE_EQUAL;
        writer.driver = &firing_driver;
        wf__body_cancel_source(&source);
        wf__body_cancel_watch(&watch, &source);
        view = wf__body_cancel_state(&watch);
        ordinary = wf__shared_new(sizeof(uint64_t));
        *(uint64_t *)((char *)ordinary + WF_SHARED_STATE_OFFSET) = 0u;
        CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
        CHECK(wf__shared_acquire(ordinary, 1u, &frame) == 0);
        wf__watch_begin(&guard);
        wf__watch_object(&guard, view, NULL);
        wf__watch_object(&guard, ordinary, &word_is_one);
        wf__shared_unlock_unwritten(ordinary, 1u);
        wf__shared_unlock_unwritten(view, 1u);
        CHECK(wf__watch_park(&guard, &frame) == 1);
        wf_context_commit_wake(wf_context_current);
        wf_context_current = &writer;
        wf_driver_self = &firing_driver;
        CHECK(wf__shared_acquire(ordinary, 1u, &frame) == 0);
        *(uint64_t *)((char *)ordinary + WF_SHARED_STATE_OFFSET) = 2u;
        wf__shared_unlock(ordinary, 1u);
        wf_context_current = &wf_context_root;
        wf_driver_self = &wf_driver_root;
        CHECK(wf_run_take(&wf_driver_root) == NULL);
        CHECK(guard.used == 2u && wf_shared_reservation((wf_shared *)ordinary) == NULL);
        fire_elsewhere(&source);
        CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
        CHECK(wf_run_take(&wf_driver_root) == NULL);
        CHECK(guard.used == 0u && view->shared.watched == 0u);
        CHECK(((wf_shared *)ordinary)->watched == 0u);
        CHECK(wf__shared_acquire(view, 1u, &frame) == 0);
        CHECK(wf__shared_acquire(ordinary, 1u, &frame) == 0);
        CHECK(view->visible_fired == 1u);
        wf__shared_unlock_unwritten(ordinary, 1u);
        wf__shared_unlock_unwritten(view, 1u);
        CHECK(wf_shared_reservation(&view->shared) == NULL && wf_context_root.shared_turn == NULL);
        wf__body_close_cancel_watch(&watch);
        wf__body_close_cancel_source(&source);
        CHECK(wf__shared_release(view));
        wf__shared_free(view);
        CHECK(wf__shared_release(ordinary));
        wf__shared_free(ordinary);
    }

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
    wf_context_commit_wake(wf_context_current);
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
    wf_context_commit_wake(wf_context_current);
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
    wake_reader();
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

    /* Fire before post-unwind registration, with the driver's epoch already
     * advanced: adoption and the post-snapshot recheck still find it. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    byte = 0xcc;
    state = wf__body_receive_next_start(&result, &receive, &destination,
                                             0, 1, NULL, &watch, operation());
    fire_elsewhere(&source);
    parked(state);
    wf_context_current = NULL;
    CHECK(wf_driver_find_cancelled(&wf_driver_root));
    wf_context_current = &wf_context_root;
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
    CHECK(close(pair[0]) == 0 && close(pair[1]) == 0);
    atomic_store(&wf_driver_count, 1u);
    while (atomic_load(&wf_drivers_notifying) != 0u) wf_prim_yield();
    atomic_store(&wf_drivers[1], NULL);
    CHECK(wf_completion_runtime_destroy(firing_driver.runtime) == 0);
    wf_test_guard_finish();
    return 0;
}
