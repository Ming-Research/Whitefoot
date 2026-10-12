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
 * even though W's guard is still false. Repeat with several watchers and with
 * R first taking an exempt statement holding another object or table entry.
 * The final nonexempt acquire detects leaked hold counts as well as missing
 * reservations. Only the oldest wakes initially; with four watchers, each
 * false retry must pass through the next oldest, including interior queue
 * positions, before R runs. Re-registering an older false retry must not
 * select it again in the same sweep or hide the younger successors.
 * Cancellation is a guard alternative: firing the watched cancellation
 * state after selection makes the reserved retry complete, freeing the
 * object for another statement and for the reserved context's exit.
 * A supplied clock also checks young and exactly-at-threshold wakes,
 * age retained over retries, and age reset for the next statement. No wall
 * clock selects any of these interleavings.
 *
 * Then three watchers register wake tests over a two-word state: the
 * oldest that the first word is 2, the other two equal copies of a test that
 * it is 1, the youngest's guard also needing the second word to be 5. A
 * release that wrote nothing evaluates no test; a write of 1 evaluates the
 * two distinct tests once each and wakes only the middle watcher, never the
 * older one whose test is false; its read-only success passes the
 * notification to the youngest, whose false retry passes it to none; a later
 * write of the second word wakes the youngest, whose write of 2 wakes the
 * oldest. Below the threshold the writer's next statement is admitted at
 * once; past it, the writer waits until the notification has passed through
 * every watch whose test held.
 *
 * Then a write while another context owns the notification and has not
 * retried, which makes true the test of an older watch behind the sweep's
 * cursor, or of a watch registered after the notifying write's cutoff: the
 * owner's release must sweep again and wake it, with no later write.
 *
 * Then a statement's first type group asks for an object an aged watch's
 * notification reserves: it parks holding none of its members until that
 * watch's attempt has completed. A group's release gives up a member no
 * target may write without entering the wake. Last, the rescan above, when
 * the notification's owner holds the object as a borrower of a hold an
 * unlock handed a parked context.
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
static _Atomic uint64_t watch_written, watch_early, watch_ready, watch_tested;
static _Atomic int w_released, g_wrote;
static uint64_t w_evaluations, w_parks, w_seen;
static int root_phase;
static const char *selected_case;
static pthread_t guard_writer;
static struct timespec guard_started;

void wf__watch_seen(unsigned moment) {
    if (moment == WF_WATCH_WRITTEN)
        atomic_fetch_add(&watch_written, 1);
    else if (moment == WF_WATCH_EARLY)
        atomic_fetch_add(&watch_early, 1);
    else if (moment == WF_WATCH_READY)
        atomic_fetch_add(&watch_ready, 1);
    else if (moment == WF_WATCH_TESTED)
        atomic_fetch_add(&watch_tested, 1);
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
        wf__watch_object(f->watch, guard_object, NULL);
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
static unsigned turn_r_parked, turn_exempt, turn_registered;
static uint64_t turn_first_wakes;
static void *turn_object, *turn_other, *turn_map, *turn_table;

/* All guard targets and frames stay live until the group joins, as in an
 * emitted atomic retry. State 0 and 1 both make W's guard false. */
static void turn_w_step(test_frame *f) {
    guard_frame *g = (guard_frame *)f;
    struct wf_table_entry *entry = (struct wf_table_entry *)g->entry;
    uint32_t present = 0;
    if (f->statements == 0 && !f->begun) {
        /* Registration order gives distinct original ages, independently
         * of context allocation addresses. A head-inserting list without
         * ordered selection fails oldest-first; an ordered queue with a
         * stale successor after false re-registration fails on later passes. */
        f->begun = (int)++turn_registered;
        atomic_store(&guard_now, (uint64_t)f->begun);
    }
    if (turn_case == TURN_TABLE) {
        uint64_t *slot = wf__table_lock_entry(turn_table, (const unsigned char *)"k", 1, 0, entry);
        present = slot[0] != 0;
        /* The object's wake must preserve this mixed watch's reservation
         * through take and until release, even if the table was written next. */
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
            if ((unsigned)f->begun != turn_attempts + 1u)
                fail("a younger guard retried before the oldest (watcher, expected)",
                     (unsigned)f->begun, turn_attempts + 1u);
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
        wf__watch_object(g->watch, turn_object, NULL);
        wf__watch_table(g->watch, turn_table);
        wf__shared_unlock(turn_object, 1);
        wf__table_unlock_entry(entry, present);
        parked = wf__watch_park(g->watch, f);
    } else {
        parked = wf__shared_watch(turn_object, 1, f, NULL);
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
        atomic_store(&guard_now, TURN_AGE_NS + turn_waiters + 1u);
        *state_of(turn_object) = 1;
        uint64_t before = atomic_load(&watch_ready);
        wf__shared_unlock(turn_object, 1);
        turn_first_wakes = atomic_load(&watch_ready) - before;
        if (turn_first_wakes != 1u)
            fail("an aged write did not wake only the oldest guard (woken, expected)", turn_first_wakes, 1);

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
        fail("a false guard did not pass its reservation before R ran (attempts, watchers)",
             turn_attempts, turn_waiters);
    *state_of(turn_object) = 2;
    wf__shared_unlock(turn_object, 1);
    f->done = 1;
}

static void turn_phase_begin(test_frame *root) {
    atomic_store(&guard_now, 1u);
    turn_waiters = turn_case == TURN_PLAIN ? 1 : (turn_case == TURN_OBJECT ? 4 : 2);
    turn_parks = turn_attempts = turn_finished = turn_r_parked = turn_exempt = turn_registered = 0;
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
    wf__watch_object(g->watch, age_object, NULL);
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

/* Cancellation does not abort an atomic statement. Its guard explicitly
 * observes cancellation as an alternative, completes the reserved retry,
 * and exits normally. All phase variables below are accessed on the one
 * driver only; the injected clock and wake observer remain atomic because
 * the earlier phases also use native writer threads. */
static void *cancel_object, *cancel_state;
static unsigned cancel_registered, cancel_parks, cancel_finished, cancel_r_parked;

static void cancel_w_step(test_frame *f) {
    guard_frame *g = (guard_frame *)f;
    if (!f->begun) {
        f->begun = (int)++cancel_registered;
        atomic_store(&guard_now, (uint64_t)f->begun);
    }
    if (wf__shared_acquire(cancel_state, 1u, f))
        return;
    /* Holding the earlier cancellation unit must not create a cycle with
     * the reserved ordinary unit. This is the emitted mixed-target path. */
    wf__shared_take(cancel_object, 1u);
    if (*(unsigned char *)((char *)cancel_state + WF_SHARED_STATE_OFFSET)) {
        if (!cancel_r_parked)
            fail("a cancelled reservation ran before the newcomer was refused (parked)", cancel_r_parked, 1);
        cancel_finished += 1u;
        wf__shared_unlock(cancel_object, 1u);
        wf__shared_unlock(cancel_state, 1u);
        f->done = 1;
        return;
    }
    if (f->statements != 0)
        fail("a cancellation watcher retried without observing the firing (attempts)", f->statements, 0);
    wf__watch_begin(g->watch);
    wf__watch_object(g->watch, cancel_object, NULL);
    wf__watch_object(g->watch, cancel_state, NULL);
    wf__shared_unlock(cancel_object, 1u);
    wf__shared_unlock(cancel_state, 1u);
    f->statements = 1;
    cancel_parks += 1u;
    if (!wf__watch_park(g->watch, f))
        fail("a cancellation guard did not initially park (parks)", cancel_parks, 0);
}

static void cancel_r_step(test_frame *f) {
    if (!f->begun) {
        if (cancel_parks != 2u)
            fail("cancellation writer ran before both watches parked (parks)", cancel_parks, 2);
        if (wf__shared_acquire(cancel_object, 1u, f))
            fail("the initial cancellation writer suspended (parks)", cancel_parks, 0);
        atomic_store(&guard_now, TURN_AGE_NS + 3u);
        uint64_t before = atomic_load(&watch_ready);
        *state_of(cancel_object) = 1u;
        wf__shared_unlock(cancel_object, 1u);
        if (atomic_load(&watch_ready) - before != 1u)
            fail("cancellation setup did not reserve only its oldest watch (woken)",
                 atomic_load(&watch_ready) - before, 1);
        /* The selected watch is already detached and ready. Firing wakes
         * the other mixed watch through its cancellation link, and must
         * not strand or overwrite the ordinary object's reservation. */
        if (wf__shared_acquire(cancel_state, 1u, f))
            fail("the cancellation firing suspended behind a guard reservation (parks)", cancel_parks, 0);
        wf__cancel_fire_held(cancel_state);
        f->begun = 1;
        if (!wf__shared_acquire(cancel_object, 1u, f))
            fail("a newcomer overtook the cancelled reserved retry (finished)", cancel_finished, 0);
        cancel_r_parked = 1u;
        return;
    }
    if (wf__shared_acquire(cancel_object, 1u, f))
        return;
    if (cancel_finished == 0u)
        fail("a reserved cancellation exit did not precede the newcomer (finished)", cancel_finished, 1);
    wf__shared_unlock(cancel_object, 1u);
    f->done = 1;
}

static void cancel_phase_begin(test_frame *root) {
    atomic_store(&guard_now, 1u);
    cancel_registered = cancel_parks = cancel_finished = cancel_r_parked = 0;
    cancel_object = wf__shared_new(sizeof(uint64_t));
    *state_of(cancel_object) = 0;
    cancel_state = wf__cancel_new();
    launch_guard(root, cancel_w_step);
    launch_guard(root, cancel_w_step);
    launch(root, cancel_r_step);
}

static void cancel_fresh_step(test_frame *f) {
    /* Both cancelled guarded statements and their contexts have exited.
     * A reservation still naming either dead context refuses this acquire;
     * a mutant omitting successful-release cleanup fails or stalls here. */
    if (wf__shared_acquire(cancel_object, 1u, f))
        fail("a cancelled statement left a reservation after context exit (finished)", cancel_finished, 2);
    wf__shared_unlock(cancel_object, 1u);
    f->done = 1;
}

/* A reserved retry can register on two objects, release B first, and be
 * notified by B before releasing its reservation on A. The native writer's
 * acknowledgement fixes that order independently of scheduling. A second
 * B-only watcher proves that joining the old A reservation does not lose
 * the rest of B's notification. The broken double-reserve implementation
 * aborts here; skipping the already-reserved watch or the remaining list
 * stalls or fails the early-wake / completion observations. */
static void *cross_a, *cross_b;
static _Atomic unsigned cross_released, cross_written;
static unsigned cross_parked, cross_finished;
static pthread_t cross_writer;

static void *cross_write_thread(void *ignored) {
    (void)ignored;
    unsigned spins = 0u;
    while (!atomic_load(&cross_released)) relax(&spins);
    wf__shared_take(cross_b, 1u);
    *state_of(cross_b) = 2u;
    uint64_t before = atomic_load(&watch_ready);
    wf__shared_unlock(cross_b, 1u);
    if (atomic_load(&watch_ready) - before != 2u)
        fail("cross-unit write lost one of its watch notifications (woken, expected)",
             atomic_load(&watch_ready) - before, 2u);
    atomic_store(&cross_written, 1u);
    return NULL;
}

static void cross_w_step(test_frame *f) {
    guard_frame *g = (guard_frame *)f;
    for (;;) {
        if (wf__shared_acquire(cross_a, 1u, f)) return;
        /* The real lowering takes same-type targets in identity order;
         * this fixture orders the two objects by identity at setup. */
        wf__shared_take(cross_b, 1u);
        if (*state_of(cross_b) == 2u) {
            wf__shared_unlock(cross_b, 1u);
            wf__shared_unlock(cross_a, 1u);
            ++cross_finished;
            f->done = 1;
            return;
        }
        wf__watch_begin(g->watch);
        wf__watch_object(g->watch, cross_a, NULL);
        wf__watch_object(g->watch, cross_b, NULL);
        wf__shared_unlock(cross_b, 1u);
        if (f->statements++ != 0u) {
            unsigned spins = 0u;
            uint64_t early = atomic_load(&watch_early);
            atomic_store(&cross_released, 1u);
            while (!atomic_load(&cross_written)) relax(&spins);
            wf__shared_unlock(cross_a, 1u);
            if (wf__watch_park(g->watch, f))
                fail("cross-unit reserved retry lost its early notification (early, expected)",
                     atomic_load(&watch_early) - early, 1u);
            if (atomic_load(&watch_early) - early != 1u)
                fail("cross-unit retry did not observe its early wake (early, expected)",
                     atomic_load(&watch_early) - early, 1u);
            continue;
        }
        wf__shared_unlock(cross_a, 1u);
        ++cross_parked;
        if (!wf__watch_park(g->watch, f))
            fail("cross-unit initial guard did not park (parked)", cross_parked, 1u);
        return;
    }
}

static void cross_b_step(test_frame *f) {
    if (wf__shared_acquire(cross_b, 1u, f)) return;
    if (f->statements != 0u) {
        if (*state_of(cross_b) != 2u)
            fail("remaining cross-unit watcher missed the write (state, expected)", *state_of(cross_b), 2u);
        wf__shared_unlock(cross_b, 1u);
        ++cross_finished;
        f->done = 1;
        return;
    }
    atomic_store(&guard_now, 2u);
    ++f->statements;
    ++cross_parked;
    if (!wf__shared_watch(cross_b, 1u, f, NULL))
        fail("B-only guard did not initially park (parked)", cross_parked, 2u);
}

static void cross_r_step(test_frame *f) {
    if (cross_parked != 2u)
        fail("cross-unit writer preceded registration (parked, expected)", cross_parked, 2u);
    if (wf__shared_acquire(cross_a, 1u, f)) return;
    atomic_store(&guard_now, TURN_AGE_NS + 3u);
    *state_of(cross_a) = 1u;
    uint64_t before = atomic_load(&watch_ready);
    wf__shared_unlock(cross_a, 1u);
    if (atomic_load(&watch_ready) - before != 1u)
        fail("cross-unit setup did not select its A waiter (woken, expected)",
             atomic_load(&watch_ready) - before, 1u);
    f->done = 1;
}

static void cross_phase_begin(test_frame *root) {
    atomic_store(&guard_now, 1u);
    atomic_store(&cross_released, 0u);
    atomic_store(&cross_written, 0u);
    cross_parked = cross_finished = 0u;
    cross_a = wf__shared_new(sizeof(uint64_t));
    cross_b = wf__shared_new(sizeof(uint64_t));
    if ((uintptr_t)cross_a > (uintptr_t)cross_b) {
        void *swap = cross_a;
        cross_a = cross_b;
        cross_b = swap;
    }
    *state_of(cross_a) = *state_of(cross_b) = 0u;
    pthread_create(&cross_writer, NULL, cross_write_thread, NULL);
    launch_guard(root, cross_w_step);
    launch(root, cross_b_step);
    launch(root, cross_r_step);
}

static void cross_fresh_step(test_frame *f) {
    if (wf__shared_acquire(cross_a, 1u, f))
        fail("cross-unit exit left A reserved", cross_finished, 2u);
    wf__shared_unlock(cross_a, 1u);
    if (wf__shared_acquire(cross_b, 1u, f))
        fail("cross-unit exit left B reserved", cross_finished, 2u);
    wf__shared_unlock(cross_b, 1u);
    f->done = 1;
}

/* Wake tests: watchers 1, 2 and 3 register at supplied times 1, 2 and 3
 * with the tests word0 == 2, word0 == 1 and an equal copy of word0 == 1;
 * watcher 3's guard also needs word1 == 5. R writes at time 10, below the
 * threshold, or past it in the aged case. */
enum { WAKE_WATCHERS = 3, WAKE_PASSES = 1000 };
static void *wake_object;
static wf_wake_test wake_two, wake_one, wake_one_copy;
static int wake_aged;
static unsigned wake_registered, wake_parks, wake_done;
static unsigned wake_evaluations[WAKE_WATCHERS + 1], wake_order[WAKE_WATCHERS];

static void wake_equal(wf_wake_test *test, uint64_t value) {
    memset(test, 0, sizeof(*test));
    test->length = 1u;
    test->program[0] = 1u;
    test->atoms[0].offset = 0u;
    test->atoms[0].value = value;
    test->atoms[0].width = 8u;
    test->atoms[0].comparison = WF_WAKE_EQUAL;
}

static void wake_w_step(test_frame *f) {
    if (!f->begun) {
        f->begun = (int)++wake_registered;
        atomic_store(&guard_now, (uint64_t)f->begun);
    }
    unsigned id = (unsigned)f->begun;
    if (wf__shared_acquire(wake_object, 1u, f))
        return;
    uint64_t *state = state_of(wake_object);
    int holds = id == 1u ? state[0] == 2u : (state[0] == 1u && (id == 2u || state[1] == 5u));
    wake_evaluations[id] += 1u;
    if (holds) {
        wake_order[wake_done++] = id;
        if (id == 3u) {
            state[0] = 2u;
            wf__shared_unlock(wake_object, 1u);
        } else {
            wf__shared_unlock_unwritten(wake_object, 1u);
        }
        f->done = 1;
        return;
    }
    wake_parks += 1u;
    const wf_wake_test *test = id == 1u ? &wake_two : (id == 2u ? &wake_one : &wake_one_copy);
    if (!wf__shared_watch(wake_object, 1u, f, test))
        fail("a watch no write followed did not park (watcher, parks)", id, wake_parks);
}

static void wake_r_step(test_frame *f) {
    uint64_t *state = state_of(wake_object);
    if (f->begun == 0) {
        if (wake_parks != WAKE_WATCHERS)
            fail("the watchers had not parked before R ran (parks, watchers)", wake_parks, WAKE_WATCHERS);
        atomic_store(&guard_now, wake_aged ? TURN_AGE_NS + 10u : 10u);
        if (wf__shared_acquire(wake_object, 1u, f))
            fail("an uncontended statement suspended (aged)", (unsigned long long)wake_aged, 0);
        uint64_t written = atomic_load(&watch_written);
        uint64_t ready = atomic_load(&watch_ready);
        uint64_t tested = atomic_load(&watch_tested);
        wf__shared_unlock_unwritten(wake_object, 1u);
        if (atomic_load(&watch_written) != written || atomic_load(&watch_ready) != ready
            || atomic_load(&watch_tested) != tested)
            fail("a release that wrote nothing entered the wake (woken, evaluations)",
                 atomic_load(&watch_ready) - ready, atomic_load(&watch_tested) - tested);
        if (wf__shared_acquire(wake_object, 1u, f))
            fail("an uncontended statement suspended (aged)", (unsigned long long)wake_aged, 0);
        state[0] = 1u;
        ready = atomic_load(&watch_ready);
        tested = atomic_load(&watch_tested);
        wf__shared_unlock(wake_object, 1u);
        if (atomic_load(&watch_ready) - ready != 1u)
            fail("a write woke other than the one oldest watch whose test holds (woken, expected)",
                 atomic_load(&watch_ready) - ready, 1);
        if (atomic_load(&watch_tested) - tested != 2u)
            fail("a write did not evaluate each distinct test once (evaluations, distinct tests)",
                 atomic_load(&watch_tested) - tested, 2);
        f->begun = 1;
        int parked = wf__shared_acquire(wake_object, 1u, f);
        if (wake_aged) {
            if (!parked)
                fail("a newcomer overtook an aged watch whose test holds (done, evaluations)", wake_done,
                     wake_evaluations[2]);
            return;
        }
        if (parked)
            fail("a young watch's notification refused a newcomer (done, parks)", wake_done, wake_parks);
        ready = atomic_load(&watch_ready);
        wf__shared_unlock_unwritten(wake_object, 1u);
        if (atomic_load(&watch_ready) != ready)
            fail("a newcomer's release passed on a notification it did not own (woken, expected)",
                 atomic_load(&watch_ready) - ready, 0);
    }
    if (f->begun == 1) {
        if (wake_aged) {
            /* Admitted only once the aged notification has ended. */
            if (wf__shared_acquire(wake_object, 1u, f))
                return;
        } else {
            while (wake_done != 1u || wake_parks != WAKE_WATCHERS + 1u) {
                if (++f->statements > WAKE_PASSES)
                    fail("the young notification stalled (done, parks)", wake_done, wake_parks);
                if (wf__context_pass(f))
                    return;
            }
            if (wf__shared_acquire(wake_object, 1u, f))
                fail("a newcomer waited behind young watches (done, parks)", wake_done, wake_parks);
        }
        /* Watcher 2's read-only success passed the notification to watcher
         * 3, whose false retry passed it to none; watcher 1 never ran. */
        if (wake_done != 1u || wake_order[0] != 2u || wake_parks != WAKE_WATCHERS + 1u)
            fail("the notification did not pass from the woken watch to the next whose test holds (done, parks)",
                 wake_done, wake_parks);
        if (wake_evaluations[1] != 1u || wake_evaluations[2] != 2u || wake_evaluations[3] != 2u)
            fail("a watch whose test was false retried (oldest's evaluations, youngest's)",
                 wake_evaluations[1], wake_evaluations[3]);
        state[1] = 5u;
        uint64_t ready = atomic_load(&watch_ready);
        wf__shared_unlock(wake_object, 1u);
        if (atomic_load(&watch_ready) - ready != 1u)
            fail("the second write woke other than the youngest watch (woken, expected)",
                 atomic_load(&watch_ready) - ready, 1);
        f->begun = 2;
    }
    f->done = 1;
}

static void wake_phase_begin(test_frame *root, int aged) {
    atomic_store(&guard_now, 1u);
    wake_aged = aged;
    wake_registered = wake_parks = wake_done = 0u;
    memset(wake_evaluations, 0, sizeof(wake_evaluations));
    memset(wake_order, 0, sizeof(wake_order));
    wake_equal(&wake_two, 2u);
    wake_equal(&wake_one, 1u);
    wake_equal(&wake_one_copy, 1u);
    wake_object = wf__shared_new(2u * sizeof(uint64_t));
    state_of(wake_object)[0] = state_of(wake_object)[1] = 0u;
    for (unsigned i = 0u; i < WAKE_WATCHERS; i++)
        launch_guard(root, wake_w_step);
    launch(root, wake_r_step);
}

static void wake_phase_end(void) {
    if (wake_done != WAKE_WATCHERS || wake_order[0] != 2u || wake_order[1] != 3u || wake_order[2] != 1u)
        fail("the watches did not complete in the order their tests allow (done, first)", wake_done,
             wake_order[0]);
    if (wake_evaluations[1] != 2u || wake_evaluations[2] != 2u || wake_evaluations[3] != 3u)
        fail("a watch retried more often than a write let its test hold (oldest's evaluations, youngest's)",
             wake_evaluations[1], wake_evaluations[3]);
    if (state_of(wake_object)[0] != 2u || state_of(wake_object)[1] != 5u)
        fail("the wake case's writes were not kept (word0, word1)", state_of(wake_object)[0],
             state_of(wake_object)[1]);
    if (wf__shared_release(wake_object))
        wf__shared_free(wake_object);
}

/* A write while another context owns the object's notification (bridge.c's
 * lost-wake guard): such a write cannot pass the notification on, so it marks
 * a rescan when a watch whose test it made true lies where the owner's pass
 * will not reach it, and the owner's release sweeps again.
 *
 * Behind: watchers 1 (time 1, guard word0 == 1 and word1 == 5) and 2 (time 2,
 * guard word0 == 1) both test word0 == 1. R's write of word0 = 1 at time 10
 * notifies 1, whose false retry re-registers it behind the cursor and passes
 * the notification to 2; before 2 runs, R writes word1 = 5, which makes 1's
 * guard true. 2's read-only success finds no watch after it, so only the
 * rescan wakes 1.
 *
 * Late: watcher 1 (time 1) tests word0 == 1, and R's write at time 10
 * notifies it; before 1 runs, watcher 2 begins at time 20, past that write's
 * cutoff, registers a test word1 == 5, and Z writes word1 = 5. 1's read-only
 * success passes the notification to no watch inside the cutoff, so only the
 * rescan wakes 2.
 *
 * No write follows the owner's release in either case, so a watch the rescan
 * misses is never woken and R's wait for it fails. */
enum { RESCAN_PASSES = 64 * 32 };
static void *rescan_object;
static wf_wake_test rescan_x_first, rescan_x_second, rescan_y;
static int rescan_late, rescan_first_written;
static unsigned rescan_ids, rescan_done;
static unsigned rescan_evaluations[3], rescan_parks[3], rescan_order[2];
static uint64_t rescan_ready;

static void rescan_wait_bound(test_frame *f, const char *what, unsigned long long a,
                              unsigned long long b) {
    if (++f->statements > RESCAN_PASSES)
        fail(what, a, b);
}

static void rescan_w_step(test_frame *f) {
    if (!f->begun) {
        f->begun = (int)++rescan_ids;
        if (!rescan_late)
            atomic_store(&guard_now, (uint64_t)f->begun);
    }
    unsigned id = (unsigned)f->begun;
    /* The late watcher's statement begins only after R's first write. */
    while (rescan_late && id == 2u && !rescan_first_written) {
        rescan_wait_bound(f, "the late watcher never saw the first write (parks, late)", rescan_parks[1],
                          (unsigned long long)rescan_late);
        if (wf__context_pass(f))
            return;
    }
    if (wf__shared_acquire(rescan_object, 1u, f))
        return;
    uint64_t *state = state_of(rescan_object);
    int holds = rescan_late ? (id == 1u ? state[0] == 1u : state[1] == 5u)
                            : (state[0] == 1u && (id == 2u || state[1] == 5u));
    rescan_evaluations[id] += 1u;
    if (holds) {
        rescan_order[rescan_done++] = id;
        wf__shared_unlock_unwritten(rescan_object, 1u);
        f->done = 1;
        return;
    }
    rescan_parks[id] += 1u;
    const wf_wake_test *test =
        rescan_late && id == 2u ? &rescan_y : (id == 1u ? &rescan_x_first : &rescan_x_second);
    if (!wf__shared_watch(rescan_object, 1u, f, test))
        fail("a rescan watch's park answered at once (watcher, parks)", id, rescan_parks[id]);
}

/* The late case's second writer, which runs after watcher 2 has registered
 * and before watcher 1, the notification's owner, retries. */
static void rescan_z_step(test_frame *f) {
    while (rescan_parks[2] == 0u) {
        rescan_wait_bound(f, "the late watcher never registered (parks, evaluations)", rescan_parks[2],
                          rescan_evaluations[2]);
        if (wf__context_pass(f))
            return;
    }
    if (rescan_evaluations[1] != 1u)
        fail("the notification's owner retried before the second write (evaluations, expected)",
             rescan_evaluations[1], 1);
    if (wf__shared_acquire(rescan_object, 1u, f))
        fail("a newcomer waited behind a young notification (late, parks)", 1, rescan_parks[2]);
    state_of(rescan_object)[1] = 5u;
    uint64_t ready = atomic_load(&watch_ready);
    wf__shared_unlock(rescan_object, 1u);
    if (atomic_load(&watch_ready) != ready)
        fail("a write while another context owned the notification woke a watch itself (woken, expected)",
             atomic_load(&watch_ready) - ready, 0);
    f->done = 1;
}

static void rescan_r_step(test_frame *f) {
    uint64_t *state = state_of(rescan_object);
    if (f->begun == 0) {
        unsigned expected = rescan_late ? 1u : 2u;
        if (rescan_parks[1] + rescan_parks[2] != expected)
            fail("the rescan watchers had not parked before R ran (parks, expected)",
                 rescan_parks[1] + rescan_parks[2], expected);
        atomic_store(&guard_now, 10u);
        if (wf__shared_acquire(rescan_object, 1u, f))
            fail("an uncontended statement suspended (late)", (unsigned long long)rescan_late, 0);
        state[0] = 1u;
        rescan_ready = atomic_load(&watch_ready);
        wf__shared_unlock(rescan_object, 1u);
        if (atomic_load(&watch_ready) - rescan_ready != 1u)
            fail("the first write woke other than the oldest watch whose test holds (woken, expected)",
                 atomic_load(&watch_ready) - rescan_ready, 1);
        if (rescan_late)
            atomic_store(&guard_now, 20u);
        rescan_first_written = 1;
        f->begun = 1;
    }
    if (f->begun == 1 && !rescan_late) {
        /* Watcher 1's false retry re-registers it behind the cursor and
         * passes the notification to watcher 2, which has not retried. */
        while (rescan_parks[1] != 2u) {
            rescan_wait_bound(f, "the oldest watch's false retry did not re-register (parks, evaluations)",
                              rescan_parks[1], rescan_evaluations[1]);
            if (wf__context_pass(f))
                return;
        }
        if (rescan_evaluations[2] != 1u || atomic_load(&watch_ready) - rescan_ready != 2u)
            fail("watcher 2 did not own the notification unretried at the second write (evaluations, woken)",
                 rescan_evaluations[2], atomic_load(&watch_ready) - rescan_ready);
        if (wf__shared_acquire(rescan_object, 1u, f))
            fail("a newcomer waited behind a young notification (late, parks)", 0, rescan_parks[1]);
        state[1] = 5u;
        uint64_t ready = atomic_load(&watch_ready);
        wf__shared_unlock(rescan_object, 1u);
        if (atomic_load(&watch_ready) != ready)
            fail("a write while another context owned the notification woke a watch itself (woken, expected)",
                 atomic_load(&watch_ready) - ready, 0);
        f->begun = 2;
    }
    while (rescan_done != 2u) {
        rescan_wait_bound(f,
                          rescan_late
                              ? "a watch registered past the write's cutoff, whose test a later write made true, was never woken (done, its evaluations)"
                              : "an older watch behind the sweep cursor, whose test a later write made true, was never woken (done, its evaluations)",
                          rescan_done, rescan_evaluations[rescan_late ? 2 : 1]);
        if (wf__context_pass(f))
            return;
    }
    f->done = 1;
}

static void rescan_phase_begin(test_frame *root, int late) {
    atomic_store(&guard_now, 1u);
    rescan_late = late;
    rescan_first_written = 0;
    rescan_ids = rescan_done = 0u;
    memset(rescan_evaluations, 0, sizeof(rescan_evaluations));
    memset(rescan_parks, 0, sizeof(rescan_parks));
    memset(rescan_order, 0, sizeof(rescan_order));
    wake_equal(&rescan_x_first, 1u);
    wake_equal(&rescan_x_second, 1u);
    wake_equal(&rescan_y, 5u);
    rescan_y.atoms[0].offset = (uint32_t)sizeof(uint64_t);
    rescan_object = wf__shared_new(2u * sizeof(uint64_t));
    state_of(rescan_object)[0] = state_of(rescan_object)[1] = 0u;
    launch_guard(root, rescan_w_step);
    launch_guard(root, rescan_w_step);
    if (late)
        launch(root, rescan_z_step);
    launch(root, rescan_r_step);
}

static void rescan_phase_end(void) {
    /* Behind: watcher 2 completes, then the rescan wakes watcher 1, which
     * read its guard three times. Late: watcher 1, then watcher 2, twice
     * each. */
    unsigned first = rescan_late ? 1u : 2u, second = rescan_late ? 2u : 1u;
    if (rescan_done != 2u || rescan_order[0] != first || rescan_order[1] != second)
        fail("the rescan watchers did not complete owner first (done, first)", rescan_done, rescan_order[0]);
    if (rescan_evaluations[first] != 2u || rescan_evaluations[second] != (rescan_late ? 2u : 3u))
        fail("a rescan watcher retried other than its writes allow (owner's evaluations, other's)",
             rescan_evaluations[first], rescan_evaluations[second]);
    if (state_of(rescan_object)[0] != 1u || state_of(rescan_object)[1] != 5u)
        fail("the rescan case's writes were not kept (word0, word1)", state_of(rescan_object)[0],
             state_of(rescan_object)[1]);
    if (wf__shared_release(rescan_object))
        wf__shared_free(rescan_object);
}

/* A statement's first type group under a guard's reservation: watcher W
 * registers on X at time 1, testing word0 == 1; R writes word0 = 1 past the
 * threshold, so W's notification reserves X, and at once asks for the group
 * {X, Y}, X first in identity order. The group's acquire must refuse X and
 * park holding nothing, Y included, which W, holding X, then takes and gives
 * back; once W's attempt has completed, R's resumed acquire takes both. A
 * group take that ignores the reservation, as one that took every member as
 * an earlier object's holder did, takes X at once. */
/* A group record's target (bridge.h): its object, its table hold, and
 * whether the statement may write the object. */
typedef struct group_target { void *object; void *hold; uint32_t wrote; } group_target;
static void *group_x, *group_y;
static wf_wake_test group_test;
static group_target group_record[2];
static unsigned group_w_evaluations, group_w_done, group_r_parks, group_r_done;

static void group_w_step(test_frame *f) {
    if (wf__shared_acquire(group_x, 1u, f))
        return;
    group_w_evaluations += 1u;
    if (state_of(group_x)[0] != 1u) {
        if (!wf__shared_watch(group_x, 1u, f, &group_test))
            fail("the group case's watch parked at once (evaluations, expected)", group_w_evaluations, 1);
        return;
    }
    if (group_r_parks != 1u)
        fail("the reserved watch ran before the group asked for its object (parks, expected)", group_r_parks, 1);
    /* Y is free while R's group is parked. A context holding Y would be
     * exempt from X's reservation and take X at once, which the check above
     * refuses first, so this only confirms that no path parks holding Y. */
    if (wf__shared_acquire(group_y, 1u, f))
        fail("a parked first group held a later member (parks, done)", group_r_parks, group_w_done);
    wf__shared_unlock_unwritten(group_y, 1u);
    group_w_done = 1u;
    wf__shared_unlock_unwritten(group_x, 1u);
    f->done = 1;
}

static void group_r_step(test_frame *f) {
    if (f->begun == 0) {
        if (group_w_evaluations != 1u)
            fail("the group case's watch had not parked before R ran (evaluations, expected)",
                 group_w_evaluations, 1);
        atomic_store(&guard_now, TURN_AGE_NS + 10u);
        if (wf__shared_acquire(group_x, 1u, f))
            fail("an uncontended statement suspended (group, parks)", 0, group_r_parks);
        state_of(group_x)[0] = 1u;
        wf__shared_unlock(group_x, 1u);
        f->begun = 1;
    }
    group_record[0] = (group_target){group_y, NULL, 1u};
    group_record[1] = (group_target){group_x, NULL, 0u};
    if (wf__atomic_group_acquire(group_record, 2u, f)) {
        group_r_parks += 1u;
        if (group_w_done)
            fail("a first group parked after the reserved attempt had completed (parks, done)", group_r_parks,
                 group_w_done);
        return;
    }
    if (group_r_parks == 0u || !group_w_done)
        fail("a first group took an object a live reservation keeps (parks, reserved attempt done)",
             group_r_parks, group_w_done);
    if (group_record[0].object != group_x || group_record[1].object != group_y)
        fail("the group was not taken in identity order (first is X, second is Y)",
             group_record[0].object == group_x, group_record[1].object == group_y);
    state_of(group_y)[0] = 2u;
    wf__atomic_group_release(group_record, 2u, 0u, 0u, 0u);
    group_r_done = 1u;
    f->done = 1;
}

static void group_phase_begin(test_frame *root) {
    atomic_store(&guard_now, 1u);
    group_w_evaluations = group_w_done = group_r_parks = group_r_done = 0u;
    wake_equal(&group_test, 1u);
    void *a = wf__shared_new(sizeof(uint64_t)), *b = wf__shared_new(sizeof(uint64_t));
    group_x = (uintptr_t)a < (uintptr_t)b ? a : b;
    group_y = group_x == a ? b : a;
    state_of(group_x)[0] = state_of(group_y)[0] = 0u;
    launch_guard(root, group_w_step);
    launch(root, group_r_step);
}

static void group_phase_end(void) {
    if (!group_w_done || !group_r_done || group_r_parks != 1u || group_w_evaluations != 2u)
        fail("the group case did not finish as expected (parks, watch evaluations)", group_r_parks,
             group_w_evaluations);
    if (state_of(group_x)[0] != 1u || state_of(group_y)[0] != 2u)
        fail("the group case's writes were not kept (X, Y)", state_of(group_x)[0], state_of(group_y)[0]);
    if (wf__shared_release(group_x))
        wf__shared_free(group_x);
    if (wf__shared_release(group_y))
        wf__shared_free(group_y);
}

/* A group's release gives up each object as its record says: watcher W
 * registers on X, testing word0 == 1, which already holds, while its guard
 * also needs word1 == 5. R takes the group {X, Y} marked written for
 * neither and releases it, which must enter no wake; then marked written
 * for X only, writing word1 = 5, whose release must notify W. A release
 * that gave up every member as written notifies W the first time. */
static void *release_x, *release_y;
static wf_wake_test release_test;
static unsigned release_w_evaluations, release_w_done;

static void release_w_step(test_frame *f) {
    if (wf__shared_acquire(release_x, 1u, f))
        return;
    release_w_evaluations += 1u;
    if (state_of(release_x)[1] != 5u) {
        if (!wf__shared_watch(release_x, 1u, f, &release_test))
            fail("the release case's watch parked at once (evaluations, expected)", release_w_evaluations, 1);
        return;
    }
    release_w_done = 1u;
    wf__shared_unlock_unwritten(release_x, 1u);
    f->done = 1;
}

static void release_r_step(test_frame *f) {
    group_target record[2];
    if (f->begun == 0) {
        if (release_w_evaluations != 1u)
            fail("the release case's watch had not parked before R ran (evaluations, expected)",
                 release_w_evaluations, 1);
        record[0] = (group_target){release_y, NULL, 0u};
        record[1] = (group_target){release_x, NULL, 0u};
        if (wf__atomic_group_acquire(record, 2u, f))
            fail("an uncontended group parked (written, evaluations)", 0, release_w_evaluations);
        uint64_t written = atomic_load(&watch_written), ready = atomic_load(&watch_ready);
        wf__atomic_group_release(record, 2u, 0u, 0u, 0u);
        if (atomic_load(&watch_written) != written || atomic_load(&watch_ready) != ready)
            fail("a group release marked unwritten entered the wake (woken, writes seen)",
                 atomic_load(&watch_ready) - ready, atomic_load(&watch_written) - written);
        record[0] = (group_target){release_y, NULL, 0u};
        record[1] = (group_target){release_x, NULL, 1u};
        if (wf__atomic_group_acquire(record, 2u, f))
            fail("an uncontended group parked (written, evaluations)", 1, release_w_evaluations);
        state_of(release_x)[1] = 5u;
        ready = atomic_load(&watch_ready);
        wf__atomic_group_release(record, 2u, 0u, 0u, 0u);
        if (atomic_load(&watch_ready) - ready != 1u)
            fail("a group release marked written for its watched member woke other than its watch (woken, expected)",
                 atomic_load(&watch_ready) - ready, 1);
        f->begun = 1;
    }
    while (!release_w_done) {
        if (++f->statements > RESCAN_PASSES)
            fail("the release case's watch never completed (evaluations, done)", release_w_evaluations,
                 release_w_done);
        if (wf__context_pass(f))
            return;
    }
    f->done = 1;
}

static void release_phase_begin(test_frame *root) {
    atomic_store(&guard_now, 1u);
    release_w_evaluations = release_w_done = 0u;
    wake_equal(&release_test, 1u);
    /* X takes the lower identity, so the record's order {Y, X} must be
     * sorted and each write flag must move with its target. */
    void *a = wf__shared_new(2u * sizeof(uint64_t)), *b = wf__shared_new(2u * sizeof(uint64_t));
    release_x = (uintptr_t)a < (uintptr_t)b ? a : b;
    release_y = release_x == a ? b : a;
    state_of(release_x)[0] = 1u;
    state_of(release_x)[1] = 0u;
    state_of(release_y)[0] = 0u;
    launch_guard(root, release_w_step);
    launch(root, release_r_step);
}

static void release_phase_end(void) {
    if (!release_w_done || release_w_evaluations != 2u)
        fail("the release case did not finish as expected (done, evaluations)", release_w_done,
             release_w_evaluations);
    if (wf__shared_release(release_x))
        wf__shared_free(release_x);
    if (wf__shared_release(release_y))
        wf__shared_free(release_y);
}

/* The rescan of a borrowed hold's release (bridge.c's give-back path):
 * watchers A (time 1, guard word0 == 1 and word1 == 5) and O (time 2, a
 * statement holding an earlier object E, guard word0 == 1) both test
 * word0 == 1. R writes word0 = 1, which notifies A; A's false retry passes
 * the notification to O. Before O retries, thread H holds X while P parks
 * behind it and wakes P in vain twice, P's driver held by a context while H
 * unlocks and takes X again; P parks a third time. O's retry takes E and,
 * while it keeps the driver, H writes word1 = 5 and unlocks, which hands X
 * to P and, O owning the notification, marks O's rescan for A, an older
 * watch whose test holds. O then borrows the hold handed to P, as a
 * statement holding earlier locks does, reads its guard true and gives the
 * hold back without writing: the give-back's rescan alone wakes A. */
static void *borrowed_x, *borrowed_e;
static wf_wake_test borrowed_a_test, borrowed_o_test;
static pthread_t borrowed_holder;
static _Atomic int borrowed_gate, borrowed_gate_seen, borrowed_h_holds, borrowed_handoff_ready, borrowed_go,
    borrowed_h_done;
static _Atomic uint64_t borrowed_p_suspends;
static unsigned borrowed_ids, borrowed_a_parks, borrowed_a_done, borrowed_o_done, borrowed_p_done;
static struct timespec borrowed_started;

static void borrowed_bound(const char *what, unsigned long long a, unsigned long long b) {
    if (seconds_since(&borrowed_started) >= STALL_SECONDS)
        fail(what, a, b);
}

/* One look at the gate from the driver: while H holds it open, the running
 * context keeps the driver so that P cannot run; otherwise it passes. */
static int borrowed_pass(test_frame *f) {
    unsigned spins = 0u;
    if (atomic_load(&borrowed_gate)) {
        atomic_store(&borrowed_gate_seen, 1);
        while (atomic_load(&borrowed_gate))
            relax(&spins);
        atomic_store(&borrowed_gate_seen, 0);
        return 0;
    }
    return wf__context_pass(f);
}

static void *borrowed_h_thread(void *arg) {
    (void)arg;
    unsigned spins = 0u;
    wf__shared_take(borrowed_x, 1u);
    atomic_store(&borrowed_h_holds, 1);
    for (uint64_t wake = 1u; wake <= VAIN_WAKES; ++wake) {
        while (atomic_load(&borrowed_p_suspends) != wake) {
            borrowed_bound("P did not park behind H (suspends, wake)", atomic_load(&borrowed_p_suspends), wake);
            relax(&spins);
        }
        atomic_store(&borrowed_gate, 1);
        while (!atomic_load(&borrowed_gate_seen))
            relax(&spins);
        wf__shared_unlock_unwritten(borrowed_x, 1u);
        wf__shared_take(borrowed_x, 1u);
        atomic_store(&borrowed_gate, 0);
    }
    while (atomic_load(&borrowed_p_suspends) != VAIN_WAKES + 1u) {
        borrowed_bound("P did not park a third time (suspends, expected)", atomic_load(&borrowed_p_suspends),
                       VAIN_WAKES + 1u);
        relax(&spins);
    }
    atomic_store(&borrowed_handoff_ready, 1);
    while (!atomic_load(&borrowed_go)) {
        borrowed_bound("O's retry never asked for the hand-off (ready, go)", 1, 0);
        relax(&spins);
    }
    state_of(borrowed_x)[1] = 5u;
    wf__shared_unlock(borrowed_x, 1u);
    atomic_store(&borrowed_h_done, 1);
    return NULL;
}

static void borrowed_a_step(test_frame *f) {
    if (!f->begun) {
        f->begun = (int)++borrowed_ids;
        atomic_store(&guard_now, 1u);
    }
    if (wf__shared_acquire(borrowed_x, 1u, f))
        return;
    uint64_t *state = state_of(borrowed_x);
    if (state[0] == 1u && state[1] == 5u) {
        borrowed_a_done = 1u;
        wf__shared_unlock_unwritten(borrowed_x, 1u);
        f->done = 1;
        return;
    }
    borrowed_a_parks += 1u;
    if (!wf__shared_watch(borrowed_x, 1u, f, &borrowed_a_test))
        fail("A's park answered at once (parks, expected)", borrowed_a_parks, 0);
}

static void borrowed_o_step(test_frame *f) {
    guard_frame *g = (guard_frame *)f;
    if (!f->begun) {
        f->begun = (int)++borrowed_ids;
        atomic_store(&guard_now, 2u);
    }
    if (f->statements != 0u) {
        /* The notified retry waits, holding nothing, until P waits with
         * two vain wakes behind H. */
        while (!atomic_load(&borrowed_handoff_ready)) {
            borrowed_bound("the hand-off to P was never set up (P's suspends, A's parks)",
                           atomic_load(&borrowed_p_suspends), borrowed_a_parks);
            if (borrowed_pass(f))
                return;
        }
    }
    if (wf__shared_acquire(borrowed_e, 1u, f))
        return;
    if (f->statements != 0u) {
        unsigned spins = 0u;
        atomic_store(&borrowed_go, 1);
        while (!atomic_load(&borrowed_h_done))
            relax(&spins);
        uint64_t lent_before = atomic_load(&lent);
        wf__shared_take(borrowed_x, 1u);
        if (atomic_load(&lent) - lent_before != 1u)
            fail("O's retry did not borrow the hold handed to P (lent, expected)", atomic_load(&lent) - lent_before, 1);
        if (state_of(borrowed_x)[0] != 1u)
            fail("O's guard read false on its notified retry (word0, expected)", state_of(borrowed_x)[0], 1);
        uint64_t ready = atomic_load(&watch_ready);
        wf__shared_unlock_unwritten(borrowed_x, 1u);
        if (atomic_load(&watch_ready) - ready != 1u || borrowed_a_done)
            fail("a borrowed hold's release did not sweep again for an older watch a write during its notification made true (woken, expected)",
                 atomic_load(&watch_ready) - ready, 1);
        wf__shared_unlock_unwritten(borrowed_e, 1u);
        borrowed_o_done = 1u;
        f->done = 1;
        return;
    }
    wf__shared_take(borrowed_x, 1u);
    if (state_of(borrowed_x)[0] == 1u)
        fail("O's first guard read true (word0, expected)", state_of(borrowed_x)[0], 0);
    f->statements = 1u;
    wf__watch_begin(g->watch);
    wf__watch_object(g->watch, borrowed_x, &borrowed_o_test);
    wf__shared_unlock_unwritten(borrowed_x, 1u);
    wf__shared_unlock_unwritten(borrowed_e, 1u);
    if (!wf__watch_park(g->watch, f))
        fail("O's first park answered at once (A's parks, expected)", borrowed_a_parks, 1);
}

/* P asks for X once H holds it; each park it reports is a real one, made
 * on a fresh resumption, before its pass count can hand the driver on. */
static void borrowed_p_step(test_frame *f) {
    unsigned looks = 0u;
    if (!f->begun) {
        while (!atomic_load(&borrowed_h_holds) || looks >= 32u) {
            borrowed_bound("H never held X for P (A's parks, looks)", borrowed_a_parks, looks);
            looks += 1u;
            if (borrowed_pass(f))
                return;
        }
        f->begun = 1;
    }
    if (wf__shared_acquire(borrowed_x, 1u, f)) {
        atomic_fetch_add(&borrowed_p_suspends, 1);
        return;
    }
    if (!borrowed_o_done)
        fail("P took X before O's borrowed retry ended (suspends, O done)", atomic_load(&borrowed_p_suspends),
             borrowed_o_done);
    borrowed_p_done = 1u;
    wf__shared_unlock_unwritten(borrowed_x, 1u);
    f->done = 1;
}

static void borrowed_r_step(test_frame *f) {
    if (f->begun == 0) {
        if (borrowed_a_parks != 1u)
            fail("A had not parked before R ran (parks, expected)", borrowed_a_parks, 1);
        atomic_store(&guard_now, 10u);
        if (wf__shared_acquire(borrowed_x, 1u, f))
            fail("an uncontended statement suspended (borrowed, parks)", 0, borrowed_a_parks);
        state_of(borrowed_x)[0] = 1u;
        wf__shared_unlock(borrowed_x, 1u);
        f->begun = 1;
    }
    if (f->begun == 1) {
        /* A's false retry passes the notification to O. */
        while (borrowed_a_parks != 2u) {
            borrowed_bound("A's false retry did not re-register (parks, expected)", borrowed_a_parks, 2);
            if (borrowed_pass(f))
                return;
        }
        pthread_create(&borrowed_holder, NULL, borrowed_h_thread, NULL);
        f->begun = 2;
    }
    while (!borrowed_a_done || !borrowed_o_done || !borrowed_p_done) {
        borrowed_bound("the borrowed rescan case did not finish (A done, O done)", borrowed_a_done,
                       borrowed_o_done);
        if (borrowed_pass(f))
            return;
    }
    f->done = 1;
}

static void borrowed_phase_begin(test_frame *root) {
    clock_gettime(CLOCK_MONOTONIC, &borrowed_started);
    atomic_store(&guard_now, 1u);
    atomic_store(&woken, 0);
    atomic_store(&borrowed_gate, 0);
    atomic_store(&borrowed_gate_seen, 0);
    atomic_store(&borrowed_h_holds, 0);
    atomic_store(&borrowed_handoff_ready, 0);
    atomic_store(&borrowed_go, 0);
    atomic_store(&borrowed_h_done, 0);
    atomic_store(&borrowed_p_suspends, 0);
    borrowed_ids = borrowed_a_parks = borrowed_a_done = borrowed_o_done = borrowed_p_done = 0u;
    wake_equal(&borrowed_a_test, 1u);
    wake_equal(&borrowed_o_test, 1u);
    borrowed_e = wf__shared_new(sizeof(uint64_t));
    borrowed_x = wf__shared_new(2u * sizeof(uint64_t));
    state_of(borrowed_e)[0] = 0u;
    state_of(borrowed_x)[0] = state_of(borrowed_x)[1] = 0u;
    launch_guard(root, borrowed_a_step);
    launch_guard(root, borrowed_o_step);
    launch(root, borrowed_p_step);
    launch(root, borrowed_r_step);
}

static void borrowed_phase_end(void) {
    pthread_join(borrowed_holder, NULL);
    if (!borrowed_a_done || !borrowed_o_done || !borrowed_p_done || borrowed_a_parks != 2u
        || atomic_load(&borrowed_p_suspends) != VAIN_WAKES + 1u)
        fail("the borrowed rescan case did not finish as expected (A's parks, P's suspends)", borrowed_a_parks,
             atomic_load(&borrowed_p_suspends));
    if (wf__shared_release(borrowed_x))
        wf__shared_free(borrowed_x);
    if (wf__shared_release(borrowed_e))
        wf__shared_free(borrowed_e);
}

/* The object phase's contexts and threads, then the guard and turn phases, each
 * joined before its checks; a resumed root starts here again. */
static void root_step(test_frame *f) {
    if (root_phase == 0) {
        /* SHARED_OBJECT_TEST_CASE runs one observation alone: handoff
         * catches a false retry that passes its notification to no one, a
         * waking of every watch, a youngest-first selector, and a
         * reservation dropped at acquire; age catches an unconditional
         * reservation or any age reset on false retry; cancel catches a
         * successful retry that leaves its reservation after release/exit;
         * newcomer catches a newcomer barging past the one selected watch;
         * wake and wake-aged catch a write that ignores wake tests, skips
         * the oldest watch whose test holds, evaluates equal tests apart or
         * reserves for a young watch, below and past the threshold; rescan
         * and rescan-late catch a write during another context's
         * notification that loses the wake of an older watch behind the
         * sweep's cursor or of one registered past its cutoff; group-turn
         * catches a statement's first type group taking an object a live
         * reservation keeps;
         * group-release catches a group release that gives up a member no
         * target may write as written; rescan-borrowed catches a borrowed
         * hold's release that drops the rescan a write during its
         * notification marked. */
        if (selected_case != NULL) {
            if (!strcmp(selected_case, "handoff") || !strcmp(selected_case, "newcomer")) {
                root_phase = 3;
                turn_case = !strcmp(selected_case, "handoff") ? TURN_OBJECT : TURN_PLAIN;
                turn_phase_begin(f);
            } else if (!strcmp(selected_case, "age")) {
                root_phase = 4;
                age_phase_begin(f);
            } else if (!strcmp(selected_case, "cancel")) {
                root_phase = 5;
                cancel_phase_begin(f);
            } else if (!strcmp(selected_case, "cross-unit")) {
                root_phase = 7;
                cross_phase_begin(f);
            } else if (!strcmp(selected_case, "wake") || !strcmp(selected_case, "wake-aged")) {
                root_phase = !strcmp(selected_case, "wake") ? 9 : 10;
                wake_phase_begin(f, root_phase == 10);
            } else if (!strcmp(selected_case, "rescan") || !strcmp(selected_case, "rescan-late")) {
                root_phase = !strcmp(selected_case, "rescan") ? 11 : 12;
                rescan_phase_begin(f, root_phase == 12);
            } else if (!strcmp(selected_case, "group-turn")) {
                root_phase = 13;
                group_phase_begin(f);
            } else if (!strcmp(selected_case, "group-release")) {
                root_phase = 14;
                release_phase_begin(f);
            } else if (!strcmp(selected_case, "rescan-borrowed")) {
                root_phase = 15;
                borrowed_phase_begin(f);
            } else {
                fail("unknown SHARED_OBJECT_TEST_CASE", 0, 0);
            }
        } else {
            root_phase = 1;
            object = wf__shared_new(sizeof(uint64_t));
            *counter() = 0;
            pthread_create(&threads[0], NULL, h_thread, NULL);
            pthread_create(&threads[1], NULL, b_thread, NULL);
            pthread_create(&threads[2], NULL, watchdog, NULL);
            launch(f, t_step);
            launch(f, p_step);
        }
    }
    if (wf__context_join_wait(f->group, f)) {
        return;
    }
    if (root_phase == 4) {
        if (age_retries != 4 || age_parks != 4 || !age_r_parked)
            fail("age cases did not finish (retries, parks)", age_retries, age_parks);
        if (wf__shared_release(age_object))
            wf__shared_free(age_object);
        if (selected_case != NULL) {
            f->done = 1;
            return;
        }
        root_phase = 5;
        cancel_phase_begin(f);
        if (wf__context_join_wait(f->group, f))
            return;
    }
    if (root_phase == 5) {
        if (cancel_finished != 2u || !cancel_r_parked)
            fail("cancelled reserved watches did not exit (finished, refused)", cancel_finished, cancel_r_parked);
        root_phase = 6;
        launch(f, cancel_fresh_step);
        if (wf__context_join_wait(f->group, f))
            return;
    }
    if (root_phase == 6) {
        if (wf__shared_release(cancel_object))
            wf__shared_free(cancel_object);
        if (wf__shared_release(cancel_state))
            wf__shared_free(cancel_state);
        if (selected_case != NULL) {
            f->done = 1;
            return;
        }
        root_phase = 7;
        cross_phase_begin(f);
        if (wf__context_join_wait(f->group, f)) return;
    }
    if (root_phase == 7) {
        pthread_join(cross_writer, NULL);
        if (cross_finished != 2u)
            fail("cross-unit watchers did not complete (finished, expected)", cross_finished, 2u);
        root_phase = 8;
        launch(f, cross_fresh_step);
        if (wf__context_join_wait(f->group, f)) return;
    }
    if (root_phase == 8) {
        if (wf__shared_release(cross_a)) wf__shared_free(cross_a);
        if (wf__shared_release(cross_b)) wf__shared_free(cross_b);
        if (selected_case != NULL) {
            f->done = 1;
            return;
        }
        root_phase = 9;
        wake_phase_begin(f, 0);
        if (wf__context_join_wait(f->group, f)) return;
    }
    if (root_phase == 9) {
        wake_phase_end();
        if (selected_case != NULL) {
            f->done = 1;
            return;
        }
        root_phase = 10;
        wake_phase_begin(f, 1);
        if (wf__context_join_wait(f->group, f)) return;
    }
    if (root_phase == 10) {
        wake_phase_end();
        if (selected_case != NULL) {
            f->done = 1;
            return;
        }
        root_phase = 11;
        rescan_phase_begin(f, 0);
        if (wf__context_join_wait(f->group, f)) return;
    }
    if (root_phase == 11) {
        rescan_phase_end();
        if (selected_case != NULL) {
            f->done = 1;
            return;
        }
        root_phase = 12;
        rescan_phase_begin(f, 1);
        if (wf__context_join_wait(f->group, f)) return;
    }
    if (root_phase == 12) {
        rescan_phase_end();
        if (selected_case != NULL) {
            f->done = 1;
            return;
        }
        root_phase = 13;
        group_phase_begin(f);
        if (wf__context_join_wait(f->group, f)) return;
    }
    if (root_phase == 13) {
        group_phase_end();
        if (selected_case != NULL) {
            f->done = 1;
            return;
        }
        root_phase = 14;
        release_phase_begin(f);
        if (wf__context_join_wait(f->group, f)) return;
    }
    if (root_phase == 14) {
        release_phase_end();
        if (selected_case != NULL) {
            f->done = 1;
            return;
        }
        root_phase = 15;
        borrowed_phase_begin(f);
        if (wf__context_join_wait(f->group, f)) return;
    }
    if (root_phase == 15) {
        borrowed_phase_end();
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
        if (selected_case != NULL) {
            f->done = 1;
            return;
        }
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
    selected_case = getenv("SHARED_OBJECT_TEST_CASE");
    alarm(120);
    clock_gettime(CLOCK_MONOTONIC, &started);
    static test_frame root = {root_step, 0, 0, 0, {0, 0}};
    wf__context_root_begin();
    wf__context_root_run(&root);
    printf("shared-object-test: all checks passed\n");
    return 0;
}
