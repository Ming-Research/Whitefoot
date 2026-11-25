/* Native current-stack execution, refusal, startup failure and completion-tail
 * lifetime tests. Includes the delivered core; hooks only hold native threads
 * at otherwise unobservable race windows, never replace the deque operations. */
#include "entry.h"
#include "prim.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static unsigned worker_limit = WF_SCHED_MAX_THREADS;
static int owner_allowed = 1;
static unsigned hold_tail;
static unsigned tail_entered;
static unsigned tail_finished;
static void *tail_frame;
static _Thread_local int delayed_tail;
static void *waiting_frame;
static unsigned waiting_phase;
static unsigned first_released;
static unsigned second_released;
static unsigned task_entered;
static unsigned second_waits;
static unsigned tail_notified;
static unsigned second_rewaited;
static unsigned second_joined;
static unsigned coordinator_done;
static unsigned held_thief;
static unsigned release_thief;
static unsigned arm_thief;
static unsigned thief_returned;
static _Thread_local int delayed_thief;
static int hold_startup;
static unsigned startup_arrivals;
static unsigned startup_released;
static unsigned startup_checks;
static _Thread_local int starting_owner;

/* Release helpers gradually as the creator checks readiness. Delaying floor
 * attachment makes an early return observable even on an otherwise idle host. */
void wf__floor_attach_thread(void) {
    if (hold_startup) {
        unsigned arrival = __atomic_add_fetch(&startup_arrivals, 1u, __ATOMIC_RELAXED);
        while (__atomic_load_n(&startup_released, __ATOMIC_ACQUIRE) < arrival) wf_prim_yield();
    }
}
static void core_yield(void) {
    if (starting_owner) {
        startup_checks += 1;
        __atomic_store_n(&startup_released, startup_checks, __ATOMIC_RELEASE);
    }
    wf_prim_yield();
}
static int wf_sched_test_allow_owner_wait(void) { return owner_allowed; }
static int wf_sched_test_allow_worker(unsigned index) { return index < worker_limit; }
static void wf_sched_test_before_steal_read(void) {
    unsigned expected = 1;
    if (__atomic_compare_exchange_n(&arm_thief, &expected, 0, 0,
            __ATOMIC_ACQ_REL, __ATOMIC_RELAXED)) {
        __atomic_store_n(&held_thief, 1, __ATOMIC_RELEASE);
        while (!__atomic_load_n(&release_thief, __ATOMIC_ACQUIRE)) wf_prim_yield();
        delayed_thief = 1;
    }
}
static void wf_sched_test_after_steal(int claimed) {
    if (delayed_thief) {
        if (claimed) abort();
        delayed_thief = 0;
        __atomic_store_n(&thief_returned, 1, __ATOMIC_RELEASE);
    }
}
static void wf_sched_test_before_done(void *frame) {
    /* Capture this generation before DONE permits the owner to rearm the
     * same address. A delayed older tail must not capture a later test. */
    delayed_tail = frame == __atomic_load_n(&tail_frame, __ATOMIC_ACQUIRE)
        && __atomic_load_n(&hold_tail, __ATOMIC_ACQUIRE);
}
static void wf_sched_test_after_done(void) {
    if (delayed_tail) {
        __atomic_store_n(&tail_entered, 1, __ATOMIC_RELEASE);
        while (__atomic_load_n(&hold_tail, __ATOMIC_ACQUIRE)) wf_prim_yield();
    }
}
static void wf_sched_test_signal_locked(void) {
    /* The owner cannot observe this marker until the actual notification
     * releases its wait lock, even if the OS permits spurious returns. */
    if (delayed_tail && __atomic_load_n(&waiting_phase, __ATOMIC_ACQUIRE) == 2)
        __atomic_store_n(&tail_notified, 1u, __ATOMIC_RELEASE);
}
static void wf_sched_test_after_notify(void) {
    if (delayed_tail) {
        delayed_tail = 0;
        __atomic_add_fetch(&tail_finished, 1u, __ATOMIC_RELEASE);
    }
}
static void wf_sched_test_before_wait(void *frame) {
    if (frame != __atomic_load_n(&waiting_frame, __ATOMIC_ACQUIRE)) return;
    unsigned phase = __atomic_load_n(&waiting_phase, __ATOMIC_ACQUIRE);
    if (phase == 1) __atomic_store_n(&first_released, 1u, __ATOMIC_RELEASE);
    if (phase == 2) {
        __atomic_add_fetch(&second_waits, 1u, __ATOMIC_RELEASE);
        if (__atomic_load_n(&tail_notified, __ATOMIC_ACQUIRE))
            __atomic_store_n(&second_rewaited, 1u, __ATOMIC_RELEASE);
    }
}
#define wf_prim_yield core_yield
#include "core.c"
#undef wf_prim_yield
#include "../runtime_test_guard.h"

static void check(int value, const char *why) {
    if (!value) { fprintf(stderr, "compute smoke: %s\n", why); abort(); }
}
static uint64_t sum(unsigned depth);
typedef struct { unsigned depth; uint64_t result; } task_frame;
static void task(void *opaque) {
    task_frame *frame = opaque;
    frame->result = sum(frame->depth);
}
static uint64_t sum(unsigned depth) {
    if (!depth) return 1;
    task_frame *frame = wf__par_acquire_lane(sizeof(*frame));
    if (!frame) return sum(depth - 1) + sum(depth - 1);
    frame->depth = depth - 1;
    wf__par_publish(frame, task);
    uint64_t right = sum(depth - 1);
    wf__par_join(frame);
    uint64_t left = frame->result;
    wf__par_release(frame);
    return left + right;
}
static void stamp(void *opaque) { *(uint64_t *)opaque = 42; }

typedef struct { unsigned generation; uint64_t result; } held_frame;
static void held_task(void *opaque) {
    held_frame *frame = opaque;
    unsigned generation = frame->generation;
    unsigned *released = generation == 1 ? &first_released : &second_released;
    __atomic_store_n(&task_entered, generation, __ATOMIC_RELEASE);
    while (!__atomic_load_n(released, __ATOMIC_ACQUIRE)) wf_prim_yield();
    frame->result = 42;
}
static void late_notification(void *owner) {
    /* The first callback completes only after the owner enters its wait.
     * Wake that owner while the original notification remains held. */
    while (!__atomic_load_n(&tail_entered, __ATOMIC_ACQUIRE)) wf_prim_yield();
    wf__par_signal(owner);
    while (!__atomic_load_n(&second_waits, __ATOMIC_ACQUIRE)) wf_prim_yield();
    __atomic_store_n(&hold_tail, 0u, __ATOMIC_RELEASE);
    while (!__atomic_load_n(&tail_finished, __ATOMIC_ACQUIRE)) wf_prim_yield();
    while (!__atomic_load_n(&second_rewaited, __ATOMIC_ACQUIRE)) wf_prim_yield();
    check(!__atomic_load_n(&second_joined, __ATOMIC_ACQUIRE), "late signal completed a pending task");
    __atomic_store_n(&second_released, 1u, __ATOMIC_RELEASE);
    __atomic_store_n(&coordinator_done, 1u, __ATOMIC_RELEASE);
}
static void check_registered_wait_reuse(void) {
    static wf_prim_thread coordinator;
    held_frame *frame = wf__par_acquire_lane(sizeof(*frame));
    check(frame != NULL, "registered-wait frame refused");
    __atomic_store_n(&tail_entered, 0u, __ATOMIC_RELAXED);
    __atomic_store_n(&tail_finished, 0u, __ATOMIC_RELAXED);
    __atomic_store_n(&tail_frame, frame, __ATOMIC_RELEASE);
    __atomic_store_n(&waiting_frame, frame, __ATOMIC_RELEASE);
    __atomic_store_n(&waiting_phase, 1u, __ATOMIC_RELEASE);
    __atomic_store_n(&hold_tail, 1u, __ATOMIC_RELEASE);
    frame->generation = 1;
    check(wf_prim_thread_start(&coordinator, late_notification, wf__par_self, 0) == 0,
          "late-notification coordinator creation failed");
    wf__par_publish(frame, held_task);
    while (__atomic_load_n(&task_entered, __ATOMIC_ACQUIRE) != 1) wf_prim_yield();
    wf__par_join(frame);
    check(frame->result == 42, "registered DONE did not publish the payload");
    wf__par_release(frame);
    held_frame *reused = wf__par_acquire_lane(sizeof(*reused));
    check(reused == frame, "registered-wait test did not reuse its slot");
    reused->generation = 2;
    reused->result = 99;
    __atomic_store_n(&waiting_phase, 2u, __ATOMIC_RELEASE);
    wf__par_publish(reused, held_task);
    while (__atomic_load_n(&task_entered, __ATOMIC_ACQUIRE) != 2) wf_prim_yield();
    wf__par_join(reused);
    __atomic_store_n(&second_joined, 1u, __ATOMIC_RELEASE);
    check(reused->result == 42, "late notification bypassed the second completion");
    wf__par_release(reused);
    while (!__atomic_load_n(&coordinator_done, __ATOMIC_ACQUIRE)) wf_prim_yield();
    __atomic_store_n(&waiting_frame, NULL, __ATOMIC_RELEASE);
    __atomic_store_n(&waiting_phase, 0u, __ATOMIC_RELEASE);
}

int main(int argc, char **argv) {
    wf_test_guard_start(60);
    wf_test_guard_phase("scheduler startup");
    int protocol = argc < 2 || !strcmp(argv[1], "normal") || !strcmp(argv[1], "partial");
    if (argc > 1 && strcmp(argv[1], "owner-fail") == 0) owner_allowed = 0;
    if (argc > 1 && strcmp(argv[1], "worker-fail") == 0) worker_limit = 1;
    if (argc > 1 && strcmp(argv[1], "partial") == 0) worker_limit = 2;
    if (argc > 1 && strcmp(argv[1], "partial-three") == 0) worker_limit = 3;
    if (argc > 1 && strcmp(argv[1], "startup-delayed") == 0) hold_startup = 1;
    if (argc > 1 && strcmp(argv[1], "startup-partial") == 0) {
        hold_startup = 1;
        worker_limit = 2;
    }
    wf__runtime_start();
    if (!hold_startup) {
        int requested_pool = wf__sched_lanes() >= 2;
        check(wf__par_pool_active() == requested_pool,
              "world selection changed the requested-width policy");
        if (!requested_pool) {
            check(!wf__par_attached && wf__par_self == NULL
                      && wf__sched_pool_running() == 0,
                  "one-worker world selection started or attached a pool");
        } else if (!owner_allowed || worker_limit == 1) {
            check(wf__par_self == NULL && wf__sched_pool_running() == 0,
                  "refused startup registered a compute lane");
            check(wf__par_pool_active(),
                  "refused startup changed the selected world");
        } else {
            check(wf__par_self == &wf__par_lanes[0],
                  "world selection did not attach the owner before its first offer");
            check(__atomic_load_n(&wf__par_lanes[0].request_word, __ATOMIC_ACQUIRE)
                      == &wf__par_demand_word,
                  "world selection did not register the owner request word");
            check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) != 0,
                  "world selection did not request the owner's first offer");
        }
    }
    if (hold_startup) {
        starting_owner = 1;
        void *frame = wf__par_acquire_lane(8);
        starting_owner = 0;
        check(frame != NULL, "delayed startup refused the owner");
        check(startup_checks >= wf__sched_pool_running(), "startup did not await every helper");
        check(__atomic_load_n(&wf__par_ready, __ATOMIC_ACQUIRE)
                  == wf__sched_pool_running(), "startup returned before helpers were ready");
        wf__par_release(frame);
    }
    wf_test_guard_phase("nested scheduler joins");
    if (protocol) {
        for (unsigned round = 0; round < 8; ++round)
            check(sum(12) == 4096, "nested tasks lost or duplicated results");
    } else {
        check(sum(2) == 4, "startup path did not execute the ordinary result");
    }
    unsigned workers = wf__sched_pool_running();
    if (!owner_allowed || worker_limit == 1 || !wf__par_pool_active()) {
        check(workers == 0, "failed startup left an active pool");
        check(wf__par_acquire_lane(8) == NULL, "failed startup granted a lane");
        puts("compute smoke: PASS sequential/refused startup");
        wf_test_guard_finish();
        return 0;
    }
    unsigned expected_workers = worker_limit < 4 ? worker_limit - 1 : 3;
    check(workers == expected_workers, "unexpected actual pool width");
    if (!protocol) {
        printf("compute smoke: PASS startup workers=%u\n", workers);
        wf_test_guard_finish();
        return 0;
    }
    wf_test_guard_phase("scheduler frame capacity/reverse joins");
    check(wf__par_acquire_lane(UINT64_MAX) == NULL, "oversized frame was truncated");
    void *frames[WF_SCHED_LANE_SLOTS];
    for (unsigned i = 0; i < WF_SCHED_LANE_SLOTS; ++i) {
        frames[i] = wf__par_acquire_lane(8);
        check(frames[i] != NULL, "slot capacity lost");
        wf__par_publish(frames[i], stamp);
    }
    check(wf__par_acquire_lane(8) == NULL, "full lane did not refuse");
    for (unsigned i = WF_SCHED_LANE_SLOTS; i > 0; --i) {
        wf__par_join(frames[i-1]);
        check(*(uint64_t *)frames[i-1] == 42, "slot result corrupted");
        wf__par_release(frames[i-1]);
    }
    /* Hold a publisher after DONE, then reuse the same frame while its tail
     * is still outstanding. Only permanent synchronization metadata is read. */
    wf_test_guard_phase("scheduler completion-tail reuse");
    uint64_t *frame = wf__par_acquire_lane(8);
    __atomic_store_n(&tail_frame, frame, __ATOMIC_RELEASE);
    __atomic_store_n(&hold_tail, 1, __ATOMIC_RELEASE);
    wf__par_publish(frame, stamp);
    while (!__atomic_load_n(&tail_entered, __ATOMIC_ACQUIRE)) wf_prim_yield();
    wf__par_join(frame);
    check(*frame == 42, "DONE did not publish payload");
    wf__par_release(frame);
    uint64_t *reused = wf__par_acquire_lane(8);
    check(reused == frame, "test did not reuse retired frame");
    *reused = 99;
    __atomic_store_n(&hold_tail, 0, __ATOMIC_RELEASE);
    wf__par_publish(reused, stamp);
    wf__par_join(reused);
    check(*reused == 42, "completion tail corrupted next generation");
    wf__par_release(reused);
    while (!__atomic_load_n(&tail_finished, __ATOMIC_ACQUIRE)) wf_prim_yield();
    /* This case holds one helper's notification while a second helper runs
     * the reused task. Partial startup with one helper is checked above. */
    if (workers > 1) {
        wf_test_guard_phase("scheduler registered-wait reuse");
        check_registered_wait_reuse();
    }
    /* Delay a thief before its ring-cell read across several complete wraps.
     * Its stale CAS must lose, and reading the reused cell must stay atomic. */
    wf_test_guard_phase("scheduler stale thief across eight wraps");
    __atomic_store_n(&arm_thief, 1, __ATOMIC_RELEASE);
    frame = wf__par_acquire_lane(8);
    wf__par_publish(frame, stamp);
    while (!__atomic_load_n(&held_thief, __ATOMIC_ACQUIRE)) wf_prim_yield();
    wf__par_join(frame);
    wf__par_release(frame);
    for (unsigned i = 0; i < WF_SCHED_LANE_SLOTS * 8; ++i) {
        frame = wf__par_acquire_lane(8);
        wf__par_publish(frame, stamp);
        wf__par_join(frame);
        check(*frame == 42, "ring-wrap result corrupted");
        wf__par_release(frame);
    }
    __atomic_store_n(&release_thief, 1, __ATOMIC_RELEASE);
    while (!__atomic_load_n(&thief_returned, __ATOMIC_ACQUIRE)) wf_prim_yield();
    check(sum(12) == 4096, "post-wrap nested work failed");
    printf("compute smoke: PASS workers=%u slots=%u grants=%lu\n", workers, WF_SCHED_LANE_SLOTS, wf__par_grants());
    wf_test_guard_finish();
    return 0;
}
