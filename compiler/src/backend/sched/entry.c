/* Process settings and ordinary-call entry; no stack reservation. */
#include "entry.h"
#include "prim.h"
#include <stdio.h>
#include <stdlib.h>

int wf__sched_setting(
    const char *name,
    unsigned long ceiling,
    unsigned long *value
) {
    char text[WF_PRIM_SETTING_BYTES];
    int state = wf_prim_setting_text(name, text, sizeof(text));
    char *end = NULL;
    long written;
    if (state == 0 || (state == 1 && text[0] == '\0')) {
        return 0;
    }
    if (state == 1) {
        written = strtol(text, &end, 10);
        if (end != text && *end == '\0' && written >= 0
            && (unsigned long)written <= ceiling) {
            *value = (unsigned long)written;
            return 1;
        }
    }
    (void)fprintf(
        stderr,
        "whitefoot scheduler: %s must be an integer from 0 through %lu\n",
        name,
        ceiling
    );
    /* Invalid process settings are diagnosed before user code. Do not run
     * exit callbacks that could reenter this incomplete once-initializer. */
    (void)fflush(stderr);
    _Exit(1);
}

void wf__sched_once(unsigned *state, void (*body)(void)) {
    unsigned expected = 0;
    if (__atomic_load_n(state, __ATOMIC_ACQUIRE) == 2) return;
    if (__atomic_compare_exchange_n(state, &expected, 1, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        body();
        __atomic_store_n(state, 2, __ATOMIC_RELEASE);
    } else {
        while (__atomic_load_n(state, __ATOMIC_ACQUIRE) != 2) wf_prim_yield();
    }
}
__attribute__((weak)) size_t wf__floor_stack_bytes(void) { return 1024u * 1024u; }
__attribute__((weak)) unsigned long wf__sched_helper_ceiling(void) { return 0; }
static unsigned initialized;
static int lanes;
static unsigned long report_wanted;
int wf__sched_report(char *buffer, size_t capacity) {
    if (!WF_SCHED_STATS || !report_wanted || !buffer || !capacity) return 0;
    int written = snprintf(buffer, capacity,
        "compute: threads=%d workers_started=%u steals=%lu slots_per_lane=%u",
        lanes ? lanes : 1, wf__sched_pool_running(), wf__par_grants(), WF_SCHED_LANE_SLOTS);
    return written > 0 && (size_t)written < capacity;
}
static void print_report(void) {
    char buffer[256];
    if (report_wanted == 2 && wf__sched_report(buffer, sizeof(buffer)))
        fprintf(stderr, "%s\n", buffer);
}
static void initialize(void) {
    unsigned long requested = wf_prim_online_cpus();
    unsigned long helpers;
    if (requested > WF_SCHED_MAX_THREADS) requested = WF_SCHED_MAX_THREADS;
    wf__sched_setting("WF_WORKERS", WF_SCHED_MAX_THREADS, &requested);
    lanes = requested >= 2 ? (int)requested : 0;
    wf__sched_setting("WF_SCHED_REPORT", 2ul, &report_wanted);
    unsigned long ceiling = wf__sched_helper_ceiling();
    if (ceiling) wf__sched_setting("WF_IO_HELPERS", ceiling, &helpers);
    if (report_wanted == 2) atexit(print_report);
}
int wf__sched_lanes(void) {
    wf__sched_once(&initialized, initialize);
    return lanes;
}
void wf__runtime_start(void) { (void)wf__sched_lanes(); }
