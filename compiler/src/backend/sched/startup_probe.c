/* Startup exposure: an owner that registers its request word after the
 * helpers have stopped scanning must still be asked for work, so that a
 * request-gated root offers it and a helper steals it.
 *
 * The schedule is held, not timed. Each helper scans every round for its
 * first spin bound (1,025 scans), then, inside the idle window, only when
 * the publish epoch moves, and once more before it parks; a failed scan asks
 * one victim chosen by the lane's fixed seed. Nothing moves the epoch before
 * the owner first publishes, so a helper that has entered its window has
 * exactly one scan left, its pre-park scan, and the seed replay in
 * research/investigations/par-demand/DESIGN.md ("Twin and startup diagnostic
 * results") shows that scan never chooses lane 0 at four or eight lanes.
 *
 *   late-spinning     The owner registers once every helper has entered a
 *                     window too long to close during the case: no helper
 *                     is parked, none is still scanning every round.
 *   late-parked       The owner registers once every helper has made its
 *                     pre-park scan and parked (a 1,000 us window).
 *   registered-first  Control: zero window, and helpers are held before
 *                     their first scan until the owner has registered, so
 *                     the ordinary idle requests reach it.
 *   requests-off      The late-parked schedule under
 *                     WF_PAR_DEMAND=off-never-request: the owner must never
 *                     be asked and must offer nothing.
 *
 * The root polls its word until it is asked or until every helper has
 * parked; parked helpers scan again only after a publication wakes one, so a
 * word still zero then can never become nonzero. That stop is a state, not a
 * count or a clock, so the verdict is the same on every host. A missing
 * steal after a publication is caught by the guard's phase deadline. */
#define WF_PAR_DEMAND 1
#define WF_PAR_DEMAND_MODE() 1
#define WF_SCHED_STARTUP_TEST 1
#include "core.c"
#include "../runtime_test_guard.h"
#include <string.h>

enum startup_mode { LATE_SPINNING, LATE_PARKED, REGISTERED_FIRST, REQUESTS_OFF };
static enum startup_mode mode;
static const char *mode_name;
static unsigned helpers;
static unsigned helpers_released;
static unsigned entered[WF_PAR_MAX_LANES];
static unsigned entered_count;
static unsigned parked[WF_PAR_MAX_LANES];
static unsigned parked_count;
static unsigned entered_at_registration;
static unsigned parked_at_registration;
static unsigned long long idle_at_registration;
static int ran_on_lane = -1;

static void check(int value, const char *why) {
    if (!value) {
        fprintf(stderr, "startup exposure %s lanes=%u: %s\n", mode_name, helpers + 1, why);
        fflush(stderr);
        abort();
    }
}

static uint64_t wf_sched_startup_test_window_us(uint64_t selected) {
    (void)selected;
    /* Ten seconds outlasts any startup stagger, so no helper parks before the
     * owner registers; a runtime that never asks the owner ends the case when
     * the windows close, inside the guard's 30-second phase. */
    if (mode == LATE_SPINNING) return UINT64_C(10000000);
    if (mode == REGISTERED_FIRST) return 0;
    return UINT64_C(1000);
}
static void wf_sched_startup_test_helper_ready(unsigned lane) {
    (void)lane;
    if (mode != REGISTERED_FIRST) return;
    while (!__atomic_load_n(&helpers_released, __ATOMIC_ACQUIRE)) wf_prim_yield();
}
static void wf_sched_startup_test_window_entered(unsigned lane) {
    if (lane == 0 || lane >= WF_PAR_MAX_LANES) return;
    if (__atomic_exchange_n(&entered[lane], 1u, __ATOMIC_ACQ_REL) == 0u)
        __atomic_add_fetch(&entered_count, 1u, __ATOMIC_RELEASE);
}
static void wf_sched_startup_test_before_park(unsigned lane) {
    if (lane == 0 || lane >= WF_PAR_MAX_LANES) return;
    if (__atomic_exchange_n(&parked[lane], 1u, __ATOMIC_ACQ_REL) == 0u)
        __atomic_add_fetch(&parked_count, 1u, __ATOMIC_RELEASE);
}
static void wf_sched_startup_test_before_register(void) {
    unsigned *hold = mode == LATE_SPINNING ? &entered_count
        : mode == REGISTERED_FIRST ? NULL : &parked_count;
    if (hold != NULL)
        while (__atomic_load_n(hold, __ATOMIC_ACQUIRE) < helpers) wf_prim_yield();
    entered_at_registration = __atomic_load_n(&entered_count, __ATOMIC_ACQUIRE);
    parked_at_registration = __atomic_load_n(&parked_count, __ATOMIC_ACQUIRE);
    idle_at_registration = __atomic_load_n(&wf__par_idle, __ATOMIC_SEQ_CST);
}

typedef struct { uint64_t value; } offer_frame;
static void offered(void *opaque) {
    ((offer_frame *)opaque)->value = 42;
    __atomic_store_n(&ran_on_lane, (int)(wf__par_self - wf__par_lanes), __ATOMIC_RELEASE);
}

/* The emitted poll is this same relaxed load of the owner's word. */
static int asked(void) {
    return __atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) != 0;
}

/* Returns 1 after one offer stolen by a helper, 0 if the owner was never
 * asked before every helper parked. */
static int request_gated_root(unsigned *polls) {
    *polls = 0;
    for (;;) {
        *polls += 1;
        if (asked()) break;
        if (__atomic_load_n(&parked_count, __ATOMIC_ACQUIRE) >= helpers && !asked())
            return 0;
        wf_prim_yield();
    }
    offer_frame *frame = wf__par_acquire_lane(sizeof(*frame));
    check(frame != NULL, "the owner's lane refused its first offer");
    frame->value = 0;
    wf__par_publish(frame, offered);
    /* Join only after a helper has taken it, so the owner cannot pop its own
     * offer back and hide a missing steal. */
    while (__atomic_load_n(&ran_on_lane, __ATOMIC_ACQUIRE) < 0) wf_prim_yield();
    wf__par_join(frame);
    check(frame->value == 42, "the stolen offer's result was lost");
    wf__par_release(frame);
    check(__atomic_load_n(&ran_on_lane, __ATOMIC_ACQUIRE) != 0, "the owner ran its own offer");
    check(wf__par_grants() >= 1, "no steal was counted");
    return 1;
}

int main(int argc, char **argv) {
    wf_test_guard_start(60);
    wf_test_guard_phase("startup exposure arguments");
    mode_name = argc == 2 ? argv[1] : "";
    if (strcmp(mode_name, "late-spinning") == 0) mode = LATE_SPINNING;
    else if (strcmp(mode_name, "late-parked") == 0) mode = LATE_PARKED;
    else if (strcmp(mode_name, "registered-first") == 0) mode = REGISTERED_FIRST;
    else if (strcmp(mode_name, "requests-off") == 0) mode = REQUESTS_OFF;
    else {
        fputs("usage: sched-startup late-spinning|late-parked|registered-first|requests-off\n", stderr);
        return 2;
    }
    wf__runtime_start();
    int lanes = wf__sched_lanes();
    helpers = lanes >= 2 ? (unsigned)lanes - 1u : 0u;
    check(helpers >= 2, "needs WF_WORKERS of at least 3");
    check((mode == REQUESTS_OFF) == !wf__sched_demand_requests(),
          "requests-off runs exactly under WF_PAR_DEMAND=off-never-request");

    wf_test_guard_phase("startup exposure pool start and registration");
    check(wf__par_pool_active(), "the pool did not start");
    check(wf__par_self == &wf__par_lanes[0], "this thread is not the registered owner");
    check(wf__sched_pool_running() == helpers, "partial startup; this case needs every helper");
    if (mode == REGISTERED_FIRST) __atomic_store_n(&helpers_released, 1u, __ATOMIC_RELEASE);

    /* The schedule each mode claims, read where the owner registered. The
     * counts record first arrivals, so a spurious return from a helper's
     * sleep cannot make a held schedule look unheld. */
    if (mode == LATE_SPINNING) {
        check(entered_at_registration == helpers, "a helper had not entered its window");
        check(parked_at_registration == 0 && idle_at_registration == 0,
              "a helper parked before registration; the schedule was not held");
    }
    if (mode == LATE_PARKED || mode == REQUESTS_OFF)
        check(parked_at_registration == helpers, "a helper was not parked at registration");
    if (mode == REGISTERED_FIRST)
        check(entered_at_registration == 0 && parked_at_registration == 0,
              "a helper ran before registration");

    wf_test_guard_phase("startup exposure request-gated root");
    unsigned polls;
    int offered_once = request_gated_root(&polls);
    if (mode == REQUESTS_OFF) {
        check(!offered_once && !asked(), "a pool with requests off asked its owner");
        check(wf__par_grants() == 0, "a pool with requests off ran a steal");
    } else {
        check(offered_once, "the owner was never asked: no publication, no steal");
    }
    printf("startup exposure: PASS %s lanes=%u polls=%u stolen_by=%d\n", mode_name,
           helpers + 1u, polls, __atomic_load_n(&ran_on_lane, __ATOMIC_ACQUIRE));
    wf_test_guard_finish();
    return 0;
}
