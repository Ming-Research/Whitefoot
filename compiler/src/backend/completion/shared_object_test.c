/* Tests the shared-object runtime's waits (bridge.c) with contexts whose
 * frames are written here, in place of emitted coroutines, on one driver,
 * and two threads that stand for statements holding a keyed table's entries
 * on other drivers. The test steps them through each hand-off in a fixed
 * order, so every cycle reaches the moments it checks whatever the host's
 * speed, load or lock fairness:
 *
 * - thread H holds the object until context P has parked behind it, then
 *   unlocks it and takes it again while context T keeps the driver, so P is
 *   woken in vain, until an unlock hands P the object;
 * - T, on P's own driver, then takes the object as a statement holding a
 *   table's entries does while P waits in that driver's queue, and must
 *   borrow P's hold rather than wait for P;
 * - thread B borrows the hold next and keeps it until P has resumed and
 *   claimed it, and gives it back only once H asks for the object, so a
 *   borrow that ignored the claim would take the object before P;
 * - every statement counts its turn on a clock under the object.
 *
 * An unlock hands P's statement the object after exactly WF_SHARED_HANDOFF
 * unlocks have woken it in vain. A statement of P is handed the object at
 * most once, so no borrower takes its turn away, and once P has resumed
 * holding a handed object, at most the one borrower under way takes the
 * object before P does [WAIT-2]. The counter under the object equals the
 * statements run. Every wait here yields its processor now and then, so the
 * test also runs on one.
 *
 * Then guards that read a keyed table (keyed_table.c): context W's
 * statement holds a second object and then entry "k" of a table, and its
 * guard is true once that entry is Some. While it is None, W registers a
 * watch on both, releases them and parks. The first time, thread G writes
 * the object between W's release and W's park, so the park answers at once;
 * then context X writes another entry, which wakes W to find its guard
 * still false, and then "k", which W takes. A statement that finds no watch
 * on its unit never enters the wake, a release that follows W's own false
 * guard wakes no one, and a wake takes W's watch off every unit it was on.
 *
 * Finally, on the same driver, R writes an object watched by an aged W and
 * asks for it again before yielding: it must park until W's next acquisition,
 * even though W's guard is still false. Repeat with two watchers and with
 * R first taking an exempt statement holding another object or table entry.
 * The final nonexempt acquire detects leaked hold counts as well as missing
 * turns. A supplied clock also checks young and exactly-at-threshold wakes,
 * age retained over retries, and age reset for the next statement. No wall
 * clock selects any of these interleavings.
 *
 * Prints the first failure and exits 1, or exits 0.
 */
#if defined(__linux__)
#define _GNU_SOURCE
#endif
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE 1
#endif

#include "bridge.h"

#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

/* P asks until it has been handed the object HANDED_ENOUGH times, and every
 * mutant of the hand-off fails in the first cycle; the run stops after
 * LIMIT_SECONDS, and fails when no cycle advances for STALL_SECONDS. An
 * unlock hands P the object after VAIN_WAKES unlocks have woken it in vain,
 * the count bridge.c's WF_SHARED_HANDOFF sets. */
enum { HANDED_ENOUGH = 50, LIMIT_SECONDS = 30, STALL_SECONDS = 10, VAIN_WAKES = 2 };

/* The step of a hand-off cycle, which says what T and B do next. */
enum { CYCLE_HOLD, CYCLE_T_BORROWS, CYCLE_B_BORROWS, CYCLE_RESUME };
static const char *const cycle_steps[] = {"H holds", "T borrows", "B borrows", "P resumes"};

/* A frame: the step its resumption runs, which returns when the frame
 * suspends or is done. */
typedef struct test_frame {
    void (*step)(struct test_frame *frame);
    int done;
    int begun;
    uint64_t statements;
    uint64_t group[2];
} test_frame;

void wf__coro_resume(void *frame) { ((test_frame *)frame)->step(frame); }
void wf__coro_destroy(void *frame) { (void)frame; }
int wf__coro_done(void *frame) { return ((test_frame *)frame)->done; }

static void *object;
static _Atomic uint64_t clock_turns, taken;
static _Atomic int stop, phase, gate, gate_seen, h_holds, h_asking;
static _Atomic uint64_t handed, woken, lent, handed_total, resumes, p_suspends, p_ended;
static _Atomic int resumed;
static uint64_t resumed_at, worst_after_resume, worst_handed, p_statements;
static struct timespec started;

/* The runtime's default threshold, independent of elapsed host time. Keep
 * the supplied clock nonzero, since the platform uses zero for failure. */
enum { TURN_AGE_NS = 1000000 };
static _Atomic uint64_t guard_now = 1;
uint64_t wf__guard_clock_ns(void) { return atomic_load(&guard_now); }

static void fail(const char *what, unsigned long long a, unsigned long long b) {
    printf("shared-object-test: %s (%llu, %llu)\n", what, a, b);
    exit(1);
}

void wf__shared_seen(unsigned moment) {
    if (moment == WF_SHARED_HANDED) {
        if (atomic_load(&woken) != VAIN_WAKES)
            fail("an unlock handed a statement the object after other vain wakes (woken, expected)",
                 atomic_load(&woken), VAIN_WAKES);
        atomic_fetch_add(&handed, 1);
        atomic_fetch_add(&handed_total, 1);
    } else if (moment == WF_SHARED_WOKEN) {
        atomic_fetch_add(&woken, 1);
    } else if (moment == WF_SHARED_LENT) {
        atomic_fetch_add(&lent, 1);
    } else if (moment == WF_SHARED_RESUMED) {
        resumed_at = atomic_load(&clock_turns);
        atomic_store(&resumed, 1);
        atomic_fetch_add(&resumes, 1);
    }
}

static double seconds_since(const struct timespec *then) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (double)(now.tv_sec - then->tv_sec) + (double)(now.tv_nsec - then->tv_nsec) / 1e9;
}

static uint64_t *counter(void) { return (uint64_t *)((char *)object + WF_SHARED_STATE_OFFSET); }

/* One turn under the object, held for `spins` empty iterations. */
static void turn(int spins) {
    atomic_fetch_add(&clock_turns, 1);
    *counter() += 1;
    for (volatile int spin = 0; spin < spins; spin++) {
    }
}

/* One more look at a condition another thread or context changes, giving
 * up the processor now and then for it. */
static void relax(unsigned *spins) {
    if (++*spins % 64 == 0)
        sched_yield();
}

/* P stops only between statements, so that a statement that never ends
 * fails the test rather than ending it, and fails as soon as one statement
 * has been handed the object twice or overtaken after it resumed. It begins
 * a statement once H holds the object for the next cycle, passing the
 * driver until then. */
static void p_step(test_frame *f) {
    unsigned spins = 0;
    for (;;) {
        if (!f->begun) {
            if (atomic_load(&handed_total) >= HANDED_ENOUGH || seconds_since(&started) >= LIMIT_SECONDS) {
                p_statements = f->statements;
                atomic_store(&stop, 1);
                f->done = 1;
                return;
            }
            if (atomic_load(&phase) != CYCLE_HOLD || !atomic_load(&h_holds)) {
                relax(&spins);
                if (wf__context_pass(f))
                    return;
                continue;
            }
            f->begun = 1;
            atomic_store(&handed, 0);
            atomic_store(&woken, 0);
            atomic_store(&resumed, 0);
        }
        if (atomic_load(&handed) > 1)
            fail("a statement was handed the object more than once (times, bound)", atomic_load(&handed), 1);
        if (wf__shared_acquire(object, 1, f)) {
            atomic_fetch_add(&p_suspends, 1);
            return;
        }
        uint64_t now = atomic_load(&clock_turns);
        if (atomic_load(&handed) > worst_handed)
            worst_handed = atomic_load(&handed);
        if (atomic_load(&resumed) && now - resumed_at > worst_after_resume)
            worst_after_resume = now - resumed_at;
        if (worst_after_resume > 1)
            fail("a statement resumed holding the object was overtaken (overtaken, bound)", worst_after_resume, 1);
        turn(100);
        atomic_fetch_add(&p_ended, 1);
        wf__shared_unlock(object, 1);
        f->begun = 0;
        f->statements += 1;
    }
}

/* Takes the object as a statement holding a table's entries, which must
 * borrow the hold an unlock handed P. */
static void borrow(const char *who) {
    uint64_t before = atomic_load(&lent);
    wf__shared_take(object, 1);
    if (atomic_load(&lent) == before)
        fail(who, atomic_load(&handed_total), atomic_load(&lent));
}

/* T keeps the driver, without passing it to P, while H unlocks and takes
 * the object again, which H does only once T has seen the gate closed, and
 * while B has yet to borrow; it borrows once a cycle when H has handed P the
 * object, and passes the driver otherwise. */
static void t_step(test_frame *f) {
    unsigned spins = 0;
    while (!atomic_load(&stop)) {
        if (atomic_load(&gate)) {
            atomic_store(&gate_seen, 1);
            while (atomic_load(&gate))
                relax(&spins);
            atomic_store(&gate_seen, 0);
            continue;
        }
        int now = atomic_load(&phase);
        if (now == CYCLE_T_BORROWS) {
            borrow("T took the object handed to P without borrowing it (handed, lent)");
            turn(100);
            wf__shared_unlock(object, 1);
            atomic_fetch_add(&taken, 1);
            atomic_store(&phase, CYCLE_B_BORROWS);
        } else if (now == CYCLE_B_BORROWS || (now == CYCLE_HOLD && !atomic_load(&h_holds))) {
            relax(&spins);
        } else if (wf__context_pass(f)) {
            return;
        }
    }
    f->done = 1;
}

/* H's hold that P parks behind, counted as a statement when it ends. */
static void h_take(void) {
    wf__shared_take(object, 1);
    turn(0);
    atomic_store(&h_holds, 1);
}

static void h_unlock(void) {
    atomic_store(&h_holds, 0);
    wf__shared_unlock(object, 1);
    atomic_fetch_add(&taken, 1);
}

static void *h_thread(void *arg) {
    (void)arg;
    unsigned spins = 0;
    uint64_t seen = atomic_load(&p_suspends);
    h_take();
    for (;;) {
        while (atomic_load(&p_suspends) == seen) {
            if (atomic_load(&stop)) {
                h_unlock();
                return NULL;
            }
            relax(&spins);
        }
        uint64_t before = atomic_load(&handed_total);
        atomic_store(&gate, 1);
        while (!atomic_load(&gate_seen))
            relax(&spins);
        h_unlock();
        seen = atomic_load(&p_suspends);
        if (atomic_load(&handed_total) == before) {
            /* P was woken, or had passed the driver; take the object again
             * before it runs. */
            h_take();
            atomic_store(&gate, 0);
            continue;
        }
        uint64_t resumed_before = atomic_load(&resumes);
        atomic_store(&phase, CYCLE_T_BORROWS);
        atomic_store(&gate, 0);
        while (atomic_load(&resumes) == resumed_before) {
            if (atomic_load(&stop))
                return NULL;
            relax(&spins);
        }
        /* P has claimed the hold B borrows: one statement asking for the
         * object now, which takes it only after P's statement. */
        uint64_t ended = atomic_load(&p_ended);
        atomic_store(&h_asking, 1);
        wf__shared_take(object, 1);
        atomic_store(&h_asking, 0);
        if (atomic_load(&p_ended) == ended)
            fail("a statement resumed holding the object was overtaken (overtaken, bound)", 2, 1);
        turn(100);
        wf__shared_unlock(object, 1);
        atomic_fetch_add(&taken, 1);
        seen = atomic_load(&p_suspends);
        h_take();
        atomic_store(&phase, CYCLE_HOLD);
    }
}

static void *b_thread(void *arg) {
    (void)arg;
    unsigned spins = 0;
    for (;;) {
        while (atomic_load(&phase) != CYCLE_B_BORROWS) {
            if (atomic_load(&stop))
                return NULL;
            relax(&spins);
        }
        uint64_t resumed_before = atomic_load(&resumes);
        borrow("B took the object handed to P without borrowing it (handed, lent)");
        atomic_store(&phase, CYCLE_RESUME);
        while (atomic_load(&resumes) == resumed_before || !atomic_load(&h_asking)) {
            if (atomic_load(&stop))
                fail("the test stopped while a borrow was under way (handed, resumes)", atomic_load(&handed_total),
                     atomic_load(&resumes));
            relax(&spins);
        }
        for (int i = 0; i < 64; i++)
            relax(&spins);
        turn(100);
        wf__shared_unlock(object, 1);
        atomic_fetch_add(&taken, 1);
    }
}

/* Fails the test when no cycle has advanced for STALL_SECONDS, naming
 * where it stopped, in place of waiting for the alarm. */
static void *watchdog(void *arg) {
    (void)arg;
    uint64_t last = UINT64_MAX;
    struct timespec since;
    clock_gettime(CLOCK_MONOTONIC, &since);
    while (!atomic_load(&stop)) {
        uint64_t progress = atomic_load(&p_suspends) + atomic_load(&taken) + atomic_load(&resumes);
        if (progress != last) {
            last = progress;
            clock_gettime(CLOCK_MONOTONIC, &since);
        } else if (seconds_since(&since) >= STALL_SECONDS) {
            printf("shared-object-test: no cycle advanced for %d s (step: %s, gate %d, H holds %d, handed %llu, "
                   "resumed %llu)\n",
                   STALL_SECONDS, cycle_steps[atomic_load(&phase)], atomic_load(&gate), atomic_load(&h_holds),
                   (unsigned long long)atomic_load(&handed_total), (unsigned long long)atomic_load(&resumes));
            exit(1);
        }
        usleep(10000);
    }
    return NULL;
}

static void *start(void *(step)) {
    test_frame *f = wf__context_frame_allocate(sizeof(test_frame));
    *f = (test_frame){*(void (**)(test_frame *))step, 0, 0, 0, {0, 0}};
    return f;
}

static void launch(test_frame *root, void (*step)(test_frame *)) {
    void (**argument)(test_frame *) = wf__context_prepare(sizeof(step));
    *argument = step;
    wf__context_launch(root->group, argument, start);
}

static pthread_t threads[3];

/* The guard phase. W evaluates its guard at most this often. */
enum { GUARD_EVALUATIONS = 8 };

typedef struct guard_frame {
    test_frame base;
    _Alignas(8) unsigned char watch[WF_WATCH_SIZE];
    _Alignas(8) unsigned char entry[WF_TABLE_ENTRY_SIZE];
} guard_frame;

static void *guard_table, *guard_map_object, *guard_object;
static _Atomic uint64_t watch_written, watch_early;
static _Atomic int w_released, g_wrote;
static uint64_t w_evaluations, w_parks, w_seen;
static int root_phase;
static pthread_t guard_writer;
static struct timespec guard_started;

void wf__watch_seen(unsigned moment) {
    if (moment == WF_WATCH_WRITTEN)
        atomic_fetch_add(&watch_written, 1);
    else if (moment == WF_WATCH_EARLY)
        atomic_fetch_add(&watch_early, 1);
}

static uint64_t *state_of(void *shared) { return (uint64_t *)((char *)shared + WF_SHARED_STATE_OFFSET); }

/* One statement that writes the guarded table's entry under key: Some with
 * value, or None for zero. */
static void put_entry(const char *key, uint64_t value) {
    _Alignas(8) unsigned char entry[WF_TABLE_ENTRY_SIZE];
    uint64_t *slot = wf__table_lock_entry(guard_table, (const unsigned char *)key, strlen(key), 0,
                                          (struct wf_table_entry *)entry);
    slot[0] = value != 0;
    slot[1] = value;
    wf__table_unlock_entry((struct wf_table_entry *)entry, value != 0);
}

/* One statement of a plain thread that writes the guarded object. */
static void *write_object(void *arg) {
    (void)arg;
    wf__shared_take(guard_object, 1);
    *state_of(guard_object) += 1;
    wf__shared_unlock(guard_object, 1);
    return NULL;
}

static void *g_thread(void *arg) {
    unsigned spins = 0;
    while (!atomic_load(&w_released))
        relax(&spins);
    /* The early wake must carry an aged turn through park's immediate
     * answer, just as the later tests carry one through a suspended park. */
    atomic_store(&guard_now, TURN_AGE_NS + 2u);
    write_object(arg);
    atomic_store(&g_wrote, 1);
    return NULL;
}

static void w_step(test_frame *frame) {
    guard_frame *f = (guard_frame *)frame;
    struct wf_table_entry *entry = (struct wf_table_entry *)f->entry;
    unsigned spins = 0;
    for (;;) {
        if (wf__shared_acquire(guard_object, 1, frame))
            return;
        uint64_t *slot = wf__table_lock_entry(guard_table, (const unsigned char *)"k", 1, 0, entry);
        if (++w_evaluations > GUARD_EVALUATIONS)
            fail("a guard was evaluated more often than writes woke it (evaluations, bound)", w_evaluations,
                 GUARD_EVALUATIONS);
        if (slot[0] != 0) {
            w_seen = slot[1];
            slot[0] = 0;
            wf__table_unlock_entry(entry, 0);
            wf__shared_unlock(guard_object, 1);
            frame->done = 1;
            return;
        }
        wf__watch_begin(f->watch);
        wf__watch_object(f->watch, guard_object);
        wf__watch_table(f->watch, guard_table);
        wf__table_unlock_entry(entry, 0);
        wf__shared_unlock(guard_object, 1);
        if (w_evaluations == 1) {
            /* G writes the object now, before W parks. */
            atomic_store(&w_released, 1);
            while (!atomic_load(&g_wrote))
                relax(&spins);
        }
        if (wf__watch_park(f->watch, frame)) {
            w_parks += 1;
            return;
        }
    }
}

/* X writes another entry once W has parked, and "k" once W has parked
 * again, passing the driver to W in between; a W that a write did not wake
 * never parks again, which fails the test after STALL_SECONDS. */
static void x_step(test_frame *frame) {
    unsigned spins = 0;
    while (frame->begun < 2) {
        if (w_parks <= (uint64_t)frame->begun) {
            if (seconds_since(&guard_started) >= STALL_SECONDS)
                fail("a guard's context never parked again after a write (parks, writes)", w_parks,
                     (unsigned long long)frame->begun);
            relax(&spins);
            if (wf__context_pass(frame))
                return;
            continue;
        }
        put_entry(frame->begun == 0 ? "j" : "k", frame->begun == 0 ? 5 : 99);
        frame->begun += 1;
    }
    frame->done = 1;
}

static void *start_guard(void *step) {
    guard_frame *f = wf__context_frame_allocate(sizeof(guard_frame));
    memset(f, 0, sizeof(*f));
    f->base.step = *(void (**)(test_frame *))step;
    return f;
}

static void launch_guard(test_frame *root, void (*step)(test_frame *)) {
    void (**argument)(test_frame *) = wf__context_prepare(sizeof(step));
    *argument = step;
    wf__context_launch(root->group, argument, start_guard);
}

static void guard_phase_begin(test_frame *root) {
    clock_gettime(CLOCK_MONOTONIC, &guard_started);
    guard_map_object = wf__shared_map_new(2 * sizeof(uint64_t), sizeof(uint64_t), 0);
    guard_table = *(void **)((char *)guard_map_object + WF_SHARED_STATE_OFFSET);
    guard_object = wf__shared_new(sizeof(uint64_t));
    *state_of(guard_object) = 0;
    put_entry("j0", 1);
    if (atomic_load(&watch_written) != 0)
        fail("a write to a table no guard watched entered the wake (wakes)", atomic_load(&watch_written), 0);
    pthread_create(&guard_writer, NULL, g_thread, NULL);
    launch_guard(root, w_step);
    launch(root, x_step);
}

static void guard_phase_end(void) {
    pthread_join(guard_writer, NULL);
    if (w_seen != 99 || w_evaluations != 4 || w_parks != 2)
        fail("a guard over a table and an object missed a write or woke without one (evaluations, parks)",
             w_evaluations, w_parks);
    if (atomic_load(&watch_early) != 1)
        fail("a park did not answer at once for a write that came before it (early answers)",
             atomic_load(&watch_early), 1);
    /* W's guard is true and its watch is on no unit, so writes to the object
     * and the table find none to wake. */
    uint64_t written = atomic_load(&watch_written);
    pthread_t writer;
    pthread_create(&writer, NULL, write_object, NULL);
    pthread_join(writer, NULL);
    put_entry("j1", 1);
    if (atomic_load(&watch_written) != written)
        fail("a write found a watch left on a unit after its guard was true (wakes)",
             atomic_load(&watch_written) - written, 0);
    /* The entry's tag is the slot's first word, 0 for None. */
    uint64_t kept = wf__keyed_table_count(guard_table, 0, sizeof(uint64_t), 0);
    if (kept != 3 || *state_of(guard_object) != 2)
        fail("the guard phase's writes were not kept (entries, object)", kept, *state_of(guard_object));
    while (wf__keyed_table_drain(guard_table) != NULL) {
    }
    wf__keyed_table_free(guard_table);
    if (wf__shared_release(guard_map_object))
        wf__shared_free(guard_map_object);
    if (wf__shared_release(guard_object))
        wf__shared_free(guard_object);
}

static void object_phase_end(void) {
    for (unsigned i = 0; i < 3; i++)
        pthread_join(threads[i], NULL);
    uint64_t statements = p_statements + atomic_load(&taken);
    if (getenv("SHARED_OBJECT_TEST_VERBOSE"))
        printf("P %llu statements, others %llu, handed %llu, lent %llu, %.2f s\n", (unsigned long long)p_statements,
               (unsigned long long)atomic_load(&taken), (unsigned long long)atomic_load(&handed_total),
               (unsigned long long)atomic_load(&lent), seconds_since(&started));
    if (*counter() != statements)
        fail("a statement's turn under the object was lost (counted, statements)", *counter(), statements);
    if (atomic_load(&handed_total) < HANDED_ENOUGH)
        fail("too few statements were handed the object (handed, wanted)", atomic_load(&handed_total), HANDED_ENOUGH);
    if (atomic_load(&lent) < 2 * HANDED_ENOUGH)
        fail("too few handed holds were borrowed (lent, wanted)", atomic_load(&lent), 2 * HANDED_ENOUGH);
    if (worst_handed > 1)
        fail("a statement was handed the object more than once (times, bound)", worst_handed, 1);
    if (worst_after_resume > 1)
        fail("a statement resumed holding the object was overtaken (overtaken, bound)", worst_after_resume, 1);
    if (wf__shared_release(object))
        wf__shared_free(object);
}

enum { TURN_PLAIN, TURN_OBJECT, TURN_TABLE, TURN_CASES };
static unsigned turn_case, turn_waiters, turn_parks, turn_attempts, turn_finished;
static unsigned turn_r_parked, turn_exempt;
static void *turn_object, *turn_other, *turn_map, *turn_table;

/* All guard targets and frames stay live until the group joins, as in an
 * emitted atomic retry. State 0 and 1 both make W's guard false. */
static void turn_w_step(test_frame *f) {
    guard_frame *g = (guard_frame *)f;
    struct wf_table_entry *entry = (struct wf_table_entry *)g->entry;
    uint32_t present = 0;
    if (turn_case == TURN_TABLE) {
        uint64_t *slot = wf__table_lock_entry(turn_table, (const unsigned char *)"k", 1, 0, entry);
        present = slot[0] != 0;
        /* The object's wake must give this mixed watch a turn, consumed
         * by take as well as acquire, even if the table was written next. */
        wf__shared_take(turn_object, 1);
    } else if (wf__shared_acquire(turn_object, 1, f)) {
        return;
    }
    if (f->statements == 0) {
        if (*state_of(turn_object) != 0)
            fail("R wrote before W registered (state, expected)", *state_of(turn_object), 0);
    } else {
        if (!turn_r_parked)
            fail("W retried without R yielding its driver (parked, expected)", turn_r_parked, 1);
        if (f->statements == 1) {
            if (*state_of(turn_object) != 1)
                fail("R was admitted before W's attempt (state, expected)", *state_of(turn_object), 1);
            turn_attempts += 1;
        } else {
            if (*state_of(turn_object) != 2)
                fail("W missed R's second write (state, expected)", *state_of(turn_object), 2);
            turn_finished += 1;
            wf__shared_unlock(turn_object, 1);
            if (turn_case == TURN_TABLE)
                wf__table_unlock_entry(entry, present);
            f->done = 1;
            return;
        }
    }
    f->statements += 1;
    turn_parks += 1;
    int parked;
    if (turn_case == TURN_TABLE) {
        wf__watch_begin(g->watch);
        wf__watch_object(g->watch, turn_object);
        wf__watch_table(g->watch, turn_table);
        wf__shared_unlock(turn_object, 1);
        wf__table_unlock_entry(entry, present);
        parked = wf__watch_park(g->watch, f);
    } else {
        parked = wf__shared_watch(turn_object, 1, f);
    }
    if (!parked)
        fail("a guard with no intervening writer did not park (parks, case)", turn_parks, turn_case);
}

static void turn_r_step(test_frame *f) {
    if (!f->begun) {
        if (turn_parks != turn_waiters)
            fail("W had not parked before R ran (parks, watchers)", turn_parks, turn_waiters);
        if (wf__shared_acquire(turn_object, 1, f))
            return;
        atomic_store(&guard_now, TURN_AGE_NS + 2u);
        *state_of(turn_object) = 1;
        wf__shared_unlock(turn_object, 1);

        /* W is ready on this very driver and has not attempted the object.
         * A statement already holding an earlier lock must still take O. */
        if (turn_case == TURN_OBJECT) {
            if (wf__shared_acquire(turn_other, 1, f))
                fail("an uncontended earlier object suspended R (case)", turn_case, 0);
            if (wf__shared_acquire(turn_object, 1, f))
                fail("a holder of another object waited for guard turns (case)", turn_case, 0);
            wf__shared_unlock(turn_object, 1);
            /* Taking and releasing O must leave the earlier hold counted. */
            if (wf__shared_acquire(turn_object, 1, f))
                fail("releasing O lost R's earlier hold (case)", turn_case, 0);
            wf__shared_unlock(turn_object, 1);
            wf__shared_unlock(turn_other, 1);
            turn_exempt += 1;
        } else if (turn_case == TURN_TABLE) {
            _Alignas(8) unsigned char entry[WF_TABLE_ENTRY_SIZE];
            uint64_t *slot = wf__table_lock_entry(turn_table, (const unsigned char *)"k", 1, 0,
                                                  (struct wf_table_entry *)entry);
            wf__shared_take(turn_object, 1);
            wf__shared_unlock(turn_object, 1);
            slot[0] = 1;
            slot[1] = 7;
            wf__table_unlock_entry((struct wf_table_entry *)entry, 1);
            turn_exempt += 1;
        }

        f->begun = 1;
        /* Less than 64 immediate waits since resume: a return of 1 here
         * is an object-queue park, not the periodic cooperative yield. */
        if (!wf__shared_acquire(turn_object, 1, f))
            fail("R overtook a woken guard instead of parking (attempts, watchers)", turn_attempts, turn_waiters);
        turn_r_parked = 1;
        return;
    }
    if (wf__shared_acquire(turn_object, 1, f))
        return;
    if (turn_attempts != turn_waiters || turn_parks != 2 * turn_waiters)
        fail("R took O before every false-guard attempt ended its turn (attempts, watchers)",
             turn_attempts, turn_waiters);
    *state_of(turn_object) = 2;
    wf__shared_unlock(turn_object, 1);
    f->done = 1;
}

static void turn_phase_begin(test_frame *root) {
    atomic_store(&guard_now, 1u);
    turn_waiters = turn_case == TURN_PLAIN ? 1 : 2;
    turn_parks = turn_attempts = turn_finished = turn_r_parked = turn_exempt = 0;
    turn_object = wf__shared_new(sizeof(uint64_t));
    turn_other = wf__shared_new(sizeof(uint64_t));
    *state_of(turn_object) = 0;
    if (turn_case == TURN_TABLE) {
        turn_map = wf__shared_map_new(2 * sizeof(uint64_t), sizeof(uint64_t), 0);
        turn_table = *(void **)((char *)turn_map + WF_SHARED_STATE_OFFSET);
    }
    for (unsigned i = 0; i < turn_waiters; i++)
        launch_guard(root, turn_w_step);
    launch(root, turn_r_step);
}

static void turn_phase_end(void) {
    if (turn_finished != turn_waiters || turn_exempt != (turn_case != TURN_PLAIN))
        fail("the turn case did not complete its watchers and exemption (finished, exempt)",
             turn_finished, turn_exempt);
    if (turn_case == TURN_TABLE) {
        while (wf__keyed_table_drain(turn_table) != NULL) {
        }
        wf__keyed_table_free(turn_table);
        if (wf__shared_release(turn_map))
            wf__shared_free(turn_map);
    }
    if (wf__shared_release(turn_object))
        wf__shared_free(turn_object);
    if (wf__shared_release(turn_other))
        wf__shared_free(turn_other);
}

/* Four writes at supplied ages T-1, T, T+1 and (in a new statement) 1.
 * W retries the same statement after the first two; only the third grants
 * a turn. Thus resetting age at wake, watch_begin, park, acquisition, or a
 * false-guard release all fail the third write's refusal of R. */
static unsigned age_parks, age_retries, age_r_parked;
static void *age_object;

static void age_w_step(test_frame *f) {
    guard_frame *g = (guard_frame *)f;
    if (wf__shared_acquire(age_object, 1, f))
        return;
    if (*state_of(age_object) != f->statements)
        fail("age watcher missed a write (state, expected)", *state_of(age_object), f->statements);
    if (f->statements != 0) {
        age_retries += 1;
        if (f->statements == 3) {
            if (!age_r_parked)
                fail("an aged watcher retried without refusing R (parked)", age_r_parked, 1);
            /* Complete this statement, then execute it again in the same
             * context and frame. The next watch must start a fresh age. */
            wf__shared_unlock(age_object, 1);
            if (wf__shared_acquire(age_object, 1, f))
                fail("a consumed turn blocked its next statement (retries)", age_retries, 3);
        } else if (f->statements == 4) {
            wf__shared_unlock(age_object, 1);
            f->done = 1;
            return;
        }
    }
    f->statements += 1;
    age_parks += 1;
    /* Use the general watch path as well as the shared_watch path exercised
     * by TURN_PLAIN: watch_begin must not restart an execution's age. */
    wf__watch_begin(g->watch);
    wf__watch_object(g->watch, age_object);
    wf__shared_unlock(age_object, 1);
    if (!wf__watch_park(g->watch, f))
        fail("age watcher did not park (parks)", age_parks, 0);
}

static void age_r_step(test_frame *f) {
    if (f->begun == 3) {
        /* Resume the acquire refused by the aged turn. */
        if (wf__shared_acquire(age_object, 1, f))
            return;
        if (age_retries != 3 || age_parks != 4)
            fail("R passed an outstanding aged turn (retries, parks)", age_retries, age_parks);
    } else {
        if (age_parks != (unsigned)f->begun + 1u)
            fail("age watcher had not parked before writer (parks, round)", age_parks, f->begun);
        if (wf__shared_acquire(age_object, 1, f))
            return;
    }
    atomic_store(&guard_now, TURN_AGE_NS + (unsigned)f->begun);
    *state_of(age_object) = (uint64_t)f->begun + 1u;
    wf__shared_unlock(age_object, 1);
    int parked = wf__shared_acquire(age_object, 1, f);
    if (f->begun == 2) {
        if (!parked)
            fail("R overtook a watcher older than T (retries)", age_retries, 2);
        age_r_parked = 1;
        f->begun = 3;
        return;
    }
    if (parked)
        fail("a young, boundary, or fresh-statement watch blocked R (round)", f->begun, 0);
    if (age_retries != (unsigned)f->begun)
        fail("W ran before the permitted newcomer (retries, round)", age_retries, f->begun);
    wf__shared_unlock(age_object, 1);
    if (++f->begun == 4) {
        f->done = 1;
        return;
    }
    /* Only now let the queued W retry; no host time selects this order. */
    while (!wf__context_pass(f)) {
    }
}

static void age_phase_begin(test_frame *root) {
    atomic_store(&guard_now, 1u);
    age_object = wf__shared_new(sizeof(uint64_t));
    *state_of(age_object) = 0;
    launch_guard(root, age_w_step);
    launch(root, age_r_step);
}

/* The object phase's contexts and threads, then the guard and turn phases, each
 * joined before its checks; a resumed root starts here again. */
static void root_step(test_frame *f) {
    if (root_phase == 0) {
        root_phase = 1;
        object = wf__shared_new(sizeof(uint64_t));
        *counter() = 0;
        pthread_create(&threads[0], NULL, h_thread, NULL);
        pthread_create(&threads[1], NULL, b_thread, NULL);
        pthread_create(&threads[2], NULL, watchdog, NULL);
        launch(f, t_step);
        launch(f, p_step);
    }
    if (wf__context_join_wait(f->group, f)) {
        return;
    }
    if (root_phase == 4) {
        if (age_retries != 4 || age_parks != 4 || !age_r_parked)
            fail("age cases did not finish (retries, parks)", age_retries, age_parks);
        if (wf__shared_release(age_object))
            wf__shared_free(age_object);
        f->done = 1;
        return;
    }
    if (root_phase == 1) {
        object_phase_end();
        root_phase = 2;
        guard_phase_begin(f);
        if (wf__context_join_wait(f->group, f)) {
            return;
        }
    }
    if (root_phase == 2) {
        guard_phase_end();
        root_phase = 3;
        turn_phase_begin(f);
        if (wf__context_join_wait(f->group, f))
            return;
    }
    for (;;) {
        turn_phase_end();
        if (++turn_case == TURN_CASES)
            break;
        turn_phase_begin(f);
        if (wf__context_join_wait(f->group, f))
            return;
    }
    root_phase = 4;
    age_phase_begin(f);
    if (!wf__context_join_wait(f->group, f))
        fail("newly launched age cases did not suspend their join (phase)", root_phase, 0);
}

int main(void) {
    /* One driver, so that T runs on the driver whose queue holds P;
     * statements that wait on each other in a cycle stall the test here. */
    setenv("WF_DRIVERS", "1", 1);
    alarm(120);
    clock_gettime(CLOCK_MONOTONIC, &started);
    static test_frame root = {root_step, 0, 0, 0, {0, 0}};
    wf__context_root_begin();
    wf__context_root_run(&root);
    printf("shared-object-test: all checks passed\n");
    return 0;
}
