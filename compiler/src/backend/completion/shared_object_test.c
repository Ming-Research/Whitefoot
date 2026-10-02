/* Tests the shared-object runtime's waits (bridge.c) with contexts whose
 * frames are written here, in place of emitted coroutines, on one driver,
 * and two threads that stand for statements in map blocks on other drivers.
 * The test steps them through each hand-off in a fixed order, so every
 * cycle reaches the moments it checks whatever the host's speed, load or
 * lock fairness:
 *
 * - thread H holds the object until context P has parked behind it, then
 *   unlocks it and takes it again while context T keeps the driver, so P is
 *   woken in vain, until an unlock hands P the object;
 * - T, on P's own driver, then takes the object as a statement in a map
 *   block does while P waits in that driver's queue, and must borrow P's
 *   hold rather than wait for P;
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

/* Takes the object as a statement in a map block, which must borrow the
 * hold an unlock handed P. */
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

static void root_step(test_frame *f) {
    if (!f->begun) {
        f->begun = 1;
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
    f->done = 1;
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
