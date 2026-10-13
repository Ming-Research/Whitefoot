/* Held startup schedules over the real native scheduler. Test hooks delay or
 * observe lanes; requests, publication, epoch changes and wakes stay real. */
#define WF_SCHED_STARTUP_TEST 1
#include "core.c"
#include "../runtime_test_guard.h"
#include <string.h>

enum startup_mode { LATE_SPINNING, LATE_PARKED, REGISTERED_FIRST, HELPER_REGISTRATION };
static enum startup_mode mode;
static const char *mode_name;
static unsigned helpers;
static unsigned helpers_released;
static unsigned helper_offers;
static unsigned helper_results;
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
    if (mode == REGISTERED_FIRST || mode == HELPER_REGISTRATION) return 0;
    return UINT64_C(1000);
}
static void helper_offered(void *frame) {
    (void)frame;
    __atomic_add_fetch(&helper_results, 1u, __ATOMIC_RELEASE);
}
static void wf_sched_startup_test_helper_ready(unsigned lane) {
    void *frame = NULL;
    check(lane > 0 && lane <= helpers, "invalid helper lane");
    check(__atomic_load_n(&wf__par_lanes[lane].request_word, __ATOMIC_ACQUIRE)
              == &wf__par_demand_word, "helper did not register its own TLS word");
    check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) != 0,
          "helper registration did not self-request");
    if (mode != REGISTERED_FIRST && mode != HELPER_REGISTRATION) return;
    if (mode == HELPER_REGISTRATION) {
        /* No helper has scanned: only registration can have asked this lane.
         * Exercise its first request-gated offer while the others are held. */
        check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) != 0,
              "helper first request-gated offer was suppressed");
        frame = wf__par_acquire_lane(8);
        check(frame != NULL, "helper first offer refused");
        wf__par_publish(frame, helper_offered);
        __atomic_add_fetch(&helper_offers, 1u, __ATOMIC_RELEASE);
    }
    while (!__atomic_load_n(&helpers_released, __ATOMIC_ACQUIRE)) wf_prim_yield();
    if (frame != NULL) {
        wf__par_join(frame);
        wf__par_release(frame);
    }
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
    if (mode == HELPER_REGISTRATION)
        while (__atomic_load_n(&helper_offers, __ATOMIC_ACQUIRE) < helpers) wf_prim_yield();
    unsigned *hold = mode == LATE_SPINNING ? &entered_count
        : (mode == REGISTERED_FIRST || mode == HELPER_REGISTRATION) ? NULL : &parked_count;
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
    else if (strcmp(mode_name, "helper-registration") == 0) mode = HELPER_REGISTRATION;
    else {
        fputs("usage: sched-startup late-spinning|late-parked|registered-first|helper-registration\n", stderr);
        return 2;
    }
    wf__runtime_start();
    int lanes = wf__sched_lanes();
    helpers = lanes >= 2 ? (unsigned)lanes - 1u : 0u;
    check(helpers >= 2, "needs WF_WORKERS of at least 3");

    wf_test_guard_phase("startup exposure pool start and registration");
    check(wf__par_attach() != NULL, "the pool did not start");
    check(wf__par_self == &wf__par_lanes[0], "this thread is not the registered owner");
    check(wf__sched_pool_running() == helpers, "partial startup; this case needs every helper");
    if (mode == REGISTERED_FIRST || mode == HELPER_REGISTRATION) __atomic_store_n(&helpers_released, 1u, __ATOMIC_RELEASE);

    /* The schedule each mode claims, read where the owner registered. The
     * counts record first arrivals, so a spurious return from a helper's
     * sleep cannot make a held schedule look unheld. */
    if (mode == LATE_SPINNING) {
        check(entered_at_registration == helpers, "a helper had not entered its window");
        check(parked_at_registration == 0 && idle_at_registration == 0,
              "a helper parked before registration; the schedule was not held");
    }
    if (mode == LATE_PARKED)
        check(parked_at_registration == helpers, "a helper was not parked at registration");
    if (mode == REGISTERED_FIRST || mode == HELPER_REGISTRATION)
        check(entered_at_registration == 0 && parked_at_registration == 0,
              "a helper ran before registration");

    wf_test_guard_phase("startup exposure request-gated root");
    unsigned polls;
    int offered_once = request_gated_root(&polls);
    check(offered_once, "the owner was never asked: no publication, no steal");
    if (mode == HELPER_REGISTRATION) {
        while (__atomic_load_n(&helper_results, __ATOMIC_ACQUIRE) < helpers) wf_prim_yield();
        check(__atomic_load_n(&helper_offers, __ATOMIC_ACQUIRE) == helpers,
              "some helper did not expose its first work");
    }
    printf("startup exposure: PASS %s lanes=%u polls=%u stolen_by=%d\n", mode_name,
           helpers + 1u, polls, __atomic_load_n(&ran_on_lane, __ATOMIC_ACQUIRE));
    wf_test_guard_finish();
    return 0;
}
