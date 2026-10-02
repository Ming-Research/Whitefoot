/* Tests the shared-object runtime's waits (bridge.c) with contexts whose
 * frames are written here, in place of emitted coroutines, on one driver:
 *
 * - context P parks when it finds the object held, and asks for it again
 *   and again;
 * - two threads hold the object again and again, as statements in map
 *   blocks on other drivers would, so that P parks, is woken in vain and is
 *   then handed the object while they borrow it;
 * - context T, on P's own driver, takes the object again and again as a
 *   statement in a map block does, so that it meets the object handed to P
 *   while P waits in its own driver's queue and must borrow P's hold rather
 *   than wait for P;
 * - every statement counts its turn on a clock under the object.
 *
 * A statement of P is handed the object at most once, so no borrower takes
 * its turn away, and once P has resumed holding a handed object and taken
 * the object's lock, at most the one borrower under way takes the object
 * before P does [WAIT-2]. The counter under the object equals the
 * statements run. T never parks, so the driver is never idle while P waits
 * for the threads, which are no context.
 *
 * The threads must run beside the driver for P to find the object held, so
 * on one processor the test reports that it checked no hand-off.
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

/* P asks until it has been handed the object HANDED_ENOUGH times and its
 * hold has been borrowed LENT_ENOUGH times, which took up to 2.6 s on an
 * idle host, and stops after LIMIT_SECONDS. */
enum { HANDED_ENOUGH = 50, LENT_ENOUGH = 20, LIMIT_SECONDS = 30, THREADS = 2 };

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
static _Atomic int stop;
static _Atomic uint64_t handed, lent, handed_total;
static _Atomic int resumed;
static uint64_t resumed_at, worst_after_resume, worst_handed, p_statements;
static struct timespec started;

void wf__shared_seen(unsigned moment) {
    if (moment == WF_SHARED_HANDED) {
        atomic_fetch_add(&handed, 1);
        atomic_fetch_add(&handed_total, 1);
    } else if (moment == WF_SHARED_LENT) {
        atomic_fetch_add(&lent, 1);
    } else if (moment == WF_SHARED_RESUMED) {
        resumed_at = atomic_load(&clock_turns);
        atomic_store(&resumed, 1);
    }
}

static void fail(const char *what, unsigned long long a, unsigned long long b) {
    printf("shared-object-test: %s (%llu, %llu)\n", what, a, b);
    exit(1);
}

static int past_limit(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return now.tv_sec - started.tv_sec >= LIMIT_SECONDS;
}

static uint64_t *counter(void) { return (uint64_t *)((char *)object + WF_SHARED_STATE_OFFSET); }

/* One turn under the object, held for `spins` empty iterations: the
 * threads hold it longer than a statement spins before it parks, so that P
 * mostly parks and is woken in vain until it is handed the object. */
static void turn(int spins) {
    atomic_fetch_add(&clock_turns, 1);
    *counter() += 1;
    for (volatile int spin = 0; spin < spins; spin++) {
    }
}

/* P stops only between statements, so that a statement that never ends
 * fails the test rather than ending it, and fails as soon as one statement
 * has been handed the object twice or overtaken after it resumed. */
static void p_step(test_frame *f) {
    for (;;) {
        if (!f->begun) {
            if ((atomic_load(&handed_total) >= HANDED_ENOUGH && atomic_load(&lent) >= LENT_ENOUGH) || past_limit()) {
                p_statements = f->statements;
                atomic_store(&stop, 1);
                f->done = 1;
                return;
            }
            f->begun = 1;
            atomic_store(&handed, 0);
            atomic_store(&resumed, 0);
        }
        if (atomic_load(&handed) > 1)
            fail("a statement was handed the object more than once (times, bound)", atomic_load(&handed), 1);
        if (wf__shared_acquire(object, 1, f)) {
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
        wf__shared_unlock(object, 1);
        f->begun = 0;
        f->statements += 1;
    }
}

static void t_step(test_frame *f) {
    while (!atomic_load(&stop)) {
        wf__shared_take(object, 1);
        turn(100);
        wf__shared_unlock(object, 1);
        atomic_fetch_add(&taken, 1);
        if (wf__context_pass(f)) {
            return;
        }
    }
    f->done = 1;
}

static void *take_again(void *arg) {
    (void)arg;
    while (!atomic_load(&stop)) {
        wf__shared_take(object, 1);
        turn(20000);
        wf__shared_unlock(object, 1);
        atomic_fetch_add(&taken, 1);
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

static pthread_t threads[THREADS];

static void root_step(test_frame *f) {
    if (!f->begun) {
        f->begun = 1;
        object = wf__shared_new(sizeof(uint64_t));
        *counter() = 0;
        for (unsigned i = 0; i < THREADS; i++)
            pthread_create(&threads[i], NULL, take_again, NULL);
        launch(f, t_step);
        launch(f, p_step);
    }
    if (wf__context_join_wait(f->group, f)) {
        return;
    }
    for (unsigned i = 0; i < THREADS; i++)
        pthread_join(threads[i], NULL);
    uint64_t statements = p_statements + atomic_load(&taken);
    if (getenv("SHARED_OBJECT_TEST_VERBOSE"))
        printf("P %llu statements, takers %llu, handed %llu, lent %llu\n", (unsigned long long)p_statements,
               (unsigned long long)atomic_load(&taken), (unsigned long long)atomic_load(&handed_total),
               (unsigned long long)atomic_load(&lent));
    if (*counter() != statements)
        fail("a statement's turn under the object was lost (counted, statements)", *counter(), statements);
    if (atomic_load(&handed_total) < HANDED_ENOUGH || atomic_load(&lent) < LENT_ENOUGH)
        fail("too few statements were handed the object or borrowed it (handed, lent)", atomic_load(&handed_total),
             atomic_load(&lent));
    if (worst_handed > 1)
        fail("a statement was handed the object more than once (times, bound)", worst_handed, 1);
    if (worst_after_resume > 1)
        fail("a statement resumed holding the object was overtaken (overtaken, bound)", worst_after_resume, 1);
    if (wf__shared_release(object))
        wf__shared_free(object);
    f->done = 1;
}

/* The processors this process may run on. */
static long processors(void) {
#if defined(__linux__)
    cpu_set_t cpus;
    if (sched_getaffinity(0, sizeof cpus, &cpus) == 0)
        return CPU_COUNT(&cpus);
#endif
    return sysconf(_SC_NPROCESSORS_ONLN);
}

int main(void) {
    if (processors() < 2) {
        printf("shared-object-test: the process may run on one processor, so no hand-off is checked\n");
        return 0;
    }
    /* One driver, so that T runs on the driver whose queue holds P;
     * statements that wait on each other in a cycle fail the test here. */
    setenv("WF_DRIVERS", "1", 1);
    alarm(120);
    clock_gettime(CLOCK_MONOTONIC, &started);
    static test_frame root = {root_step, 0, 0, 0, {0, 0}};
    wf__context_root_begin();
    wf__context_root_run(&root);
    printf("shared-object-test: all checks passed\n");
    return 0;
}
