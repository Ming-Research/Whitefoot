/* Deterministic protocol probe over the delivered implementation. No timing,
 * random scheduling or retries select the result. Includes core for its
 * failed-scan entry and lane state; ordinary acquire/publish/join stay real.
 * One thread plays both lanes, so the idle lane registers a word of its own
 * and the victim registers this thread's word, the one the emitted poll and
 * the accessor read. */
#define WF_PAR_DEMAND 1
#define WF_PAR_DEMAND_MODE() 1
#include "core.c"
#include <assert.h>
#include <string.h>
static unsigned executed;
static uint64_t idle_word;
static void run(void *frame) { (void)frame; ++executed; }
static void *register_and_exit(void *lane) {
    wf__par_register_word(lane);
    assert(((struct wf__par_lane *)lane)->request_word == &wf__par_demand_word);
    return NULL;
}
int main(void) {
    int enabled = strcmp(getenv("WF_PAR_DEMAND"), "off-never-request") != 0;
    wf__sched_once(&wf__par_demand_initialized, wf__par_enable_demand);
    wf__par_prepare(&wf__par_lanes[0], 0);
    wf__par_prepare(&wf__par_lanes[1], 1);
    __atomic_store_n(&wf__par_lane_count, 2, __ATOMIC_RELAXED);
    struct wf__par_lane *idle = &wf__par_lanes[0];
    struct wf__par_lane *victim = &wf__par_lanes[1];
    wf__par_self = idle;
    /* An unregistered owner cannot be asked: the victim has no word yet. */
    assert(wf__par_find(idle) == NULL);
    assert(wf__par_demand_requested() == 0);
    idle->request_word = &idle_word;
    wf__par_register_word(victim);
    assert(victim->request_word == &wf__par_demand_word);
    assert(wf__par_find(idle) == NULL);
    assert(wf__par_demand_requested() == (uint64_t)enabled);
    assert(idle_word == 0);
    wf__par_self = victim;
    if (enabled) {
        wf__par_demand_word = 7;
        wf__par_request(victim);
        assert(wf__par_demand_requested() == 7); /* write-if-zero */
    }
    void *frame = wf__par_acquire_lane(8);
    assert(frame);
    wf__par_publish(frame, run);
    assert(wf__par_demand_requested() == 0);
    wf__par_join(frame);
    wf__par_release(frame);
    assert(executed == 1);
    /* A new miss can request again after publication cleared the old one. */
    wf__par_self = idle;
    assert(wf__par_find(idle) == NULL);
    assert(wf__par_demand_requested() == (uint64_t)enabled);
    /* A lane owner that exits withdraws its word: lane 0 can belong to the
     * program's own thread, which exits while workers still scan. */
    pthread_t owner;
    assert(pthread_create(&owner, NULL, register_and_exit, victim) == 0);
    assert(pthread_join(owner, NULL) == 0);
    assert(__atomic_load_n(&victim->request_word, __ATOMIC_SEQ_CST) == NULL);
    assert(victim->request_users == 0);
    wf__par_self = idle;
    assert(wf__par_find(idle) == NULL); /* asks nobody, writes no freed word */
    puts("demand posting/clearing PASS");
    return 0;
}
