/* Deterministic protocol probe over the delivered implementation. No timing,
 * random scheduling or retries select the result. Includes core for its
 * failed-scan entry and lane state; ordinary acquire/publish/join stay real. */
#define WF_PAR_DEMAND 1
#define WF_PAR_DEMAND_MODE() 1
#include "core.c"
#include <assert.h>
#include <string.h>
static unsigned executed;
static void run(void *frame) { (void)frame; ++executed; }
int main(void) {
    int enabled = strcmp(getenv("WF_PAR_DEMAND"), "off-never-request") != 0;
    wf__sched_once(&wf__par_demand_initialized, wf__par_enable_demand);
    wf__par_prepare(&wf__par_lanes[0], 0);
    wf__par_prepare(&wf__par_lanes[1], 1);
    __atomic_store_n(&wf__par_lane_count, 2, __ATOMIC_RELAXED);
    struct wf__par_lane *idle = &wf__par_lanes[0];
    struct wf__par_lane *victim = &wf__par_lanes[1];
    wf__par_self = idle;
    assert(wf__par_find(idle) == NULL);
    assert(atomic_load(&victim->request) == (uint64_t)enabled);
    assert(atomic_load(&idle->request) == 0);
    wf__par_self = victim;
    assert(wf__par_demand_requested() == (uint64_t)enabled);
    if (enabled) {
        atomic_store(&victim->request, 7);
        wf__par_request(victim);
        assert(atomic_load(&victim->request) == 7); /* write-if-zero */
    }
    void *frame = wf__par_acquire_lane(8);
    assert(frame);
    wf__par_publish(frame, run);
    assert(wf__par_demand_requested() == 0);
    wf__par_join(frame);
    wf__par_release(frame);
    assert(executed == 1);
    /* A new miss can request again after publication cleared the old one. */
    assert(wf__par_find(idle) == NULL);
    assert(wf__par_demand_requested() == (uint64_t)enabled);
    puts("demand posting/clearing PASS");
    return 0;
}
