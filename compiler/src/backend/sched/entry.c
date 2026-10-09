/* Process settings and ordinary-call entry; no stack reservation. */
#include "entry.h"
#include "prim.h"
#include <stdio.h>
#include <stdlib.h>
#if defined(WF_PAR_DEMAND)
#include <string.h>
#endif

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
/* How much work one chunk must be worth before the splitter can afford
 * another, in the same unit the emitted weight operand counts: estimated IR
 * instructions of one iteration of the chunk body. `wf__par_split_budget` in
 * core.c applies it as span / ceil(work unit / weight), so this is a floor on
 * chunk size and never a ceiling on chunk count -- WF_PAR_SPLIT_OVERSUBSCRIBE
 * caps that -- and the two are combined with a minimum, so a range too small
 * to be worth splitting stays bounded by this term whatever the pool wants.
 *
 * It closes the over-split hazard: splitting a range that is not worth
 * splitting is a real regression, not a wash, and an allowance derived from
 * the lane count alone produces one on every small loop. 1,200,000 was
 * measured directly for that crossing at `f7127c03`, by sweeping the width of
 * a counted loop of estimated weight 150 over a fixed total amount of work and
 * comparing the split against the same program's sequential build:
 *
 *     width     2 000   8 000  32 000  128 000  512 000
 *     split    0.0831  0.0269  0.0116   0.0084   0.0072
 *     plain    0.0205  0.0196  0.0201   0.0201   0.0210
 *     ratio     4.1x    1.37x   0.58x    0.42x    0.34x
 *                loss    loss     win      win      win
 *
 * The crossing is near a width of 16,000, so a whole range first pays for two
 * chunks at about 16,000 x 150 = 2.4e6 instruction-equivalents and one chunk
 * is worth publishing at about half that. That measurement is a crossing and
 * not a plateau: it says where splitting starts to pay, not what a block of
 * kernels prefers once every chunk already pays for itself. The guard below is
 * what lets the compute scoreboard build a runtime at another value without
 * editing this line, through the bundle's WF_RUNTIME_CONTROL_FLAGS.
 *
 * That scoreboard swept 1,200,000 / 600,000 / 300,000 / 150,000 on the
 * four-CPU Linux development host, one plain `compare PASSES=5 CALLS=5` per
 * value with the control run three times, aimed at the Mandelbrot kernel,
 * whose 16 chunks over skewed input are what the work term affords rather than
 * what the oversubscription term wants. The chunk counts moved as the rule
 * predicts -- mandelbrot 16/16/16, 32/32/32, 32/64/64 and 32/64/128 at
 * W=2/4/8 -- and none of the candidates met acceptance: mandelbrot's W=4 wall
 * ratio read 1.057, 1.067 and 1.017 against controls of 1.037, 1.015 and
 * 1.030, none below the control band, and finer grain cost the W=2 row
 * (1.020/1.023/1.031 at 16 chunks against 1.040/1.025/1.063 at 32). The reason
 * the sweep could not select is on the record too: quadrature emits no split
 * call at all, so its rows are identical work in all six runs, and its W=4
 * ratio spread 13.0 percent across them, wider than the effect being cut for.
 * So this value is unchanged by measurement rather than by default
 * (research/investigations/compute-runtime/RESULTS.md, the split work unit
 * swept against the Mandelbrot grain). A host whose spread is narrower, or one
 * with eight lanes where the map kernels are not already at the
 * oversubscription cap, can reopen it.
 *
 * It was swept a second time, against a raised cap so that the cap could not be
 * what stopped the count, and again nothing was selected: 300,000 with the cap
 * at 64, 75,000 with it at 256 and 20,000 with it at 1,024, each measured
 * against the shipped runtime inside one set of passes by the scoreboard's A/B
 * twin rather than against a separate control run. The chunk counts moved as
 * the rule predicts and the readings are not separable from the twin's own null
 * arm: quadrature emits no split call, so its two images do identical work, and
 * its W=4 pair read anywhere from 0.936 to 1.086 across those runs. The reason
 * is stated beside WF_PAR_SPLIT_OVERSUBSCRIBE in core.c
 * (research/investigations/compute-runtime/RESULTS.md, the oversubscription cap
 * measured with the A/B twin).
 *
 * The third measurement moved it. The two sweeps above ran on four-CPU hosts,
 * where a map of this size is at or near the 16-per-lane cap at every value,
 * and the hosted twins at 300,000 and 150,000 (runs 34630112178, 34628390507
 * and 34644579271) read within the instrument's one-percent lean at W=4 on
 * every runner whose lanes ran evenly. An eight-CPU Apple M1 Pro is where the
 * work term was binding: it afforded mandelbrot 16 chunks at W=8 as at W=4, so
 * eight lanes bought nothing over four. In the grain twin's own table that
 * runtime reads 5,441 us at W=8 against 5,506 at W=4 while tbb reads 3,440 at
 * W=8; the window twin's table, taken on the same host, reads the same shape
 * more flatly still at 5,412 against 5,411. The twin built at 150,000 with the
 * cap at 32 read mandelbrot W=8 0.692 [0.65-0.75] with five of five pairs
 * lower and records W=8 0.965 with four of five, at a W=4 cost in CPU alone
 * (paired cpu 1.189 on mandelbrot, 1.177 on fir, wall within 1.5 percent).
 * 150,000 is therefore the value, and the cap stays at 16 per lane, which
 * halves the W=4 chunk count the twin paid for. What that leaves, for these
 * sizes: records and fir are cap-bound at every width through 32 -- 64 chunks
 * at W=4, 128 at W=8, 256 at W=16, 512 at W=32 -- and mandelbrot is cap-bound
 * at W=4 and W=8 at 64 and 128, then work-term-bound at 128 from W=16 up,
 * where its 143 affordable chunks fall below the cap
 * (research/investigations/compute-runtime/RESULTS.md, the idle-window
 * sections and the M1 Pro grain twin).
 *
 * WF_SPLIT_WORK overrides it per process for diagnosis; the scoreboard's
 * harness unsets that variable so a recorded row can never be taken under one.
 * This selects how finely an admitted program is actualized in parallel; no
 * acceptance path reads it. */
#ifndef WF_PAR_SPLIT_WORK_UNIT
#define WF_PAR_SPLIT_WORK_UNIT 150000ul
#endif
static unsigned long split_work = WF_PAR_SPLIT_WORK_UNIT;
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
    wf__sched_setting("WF_SPLIT_WORK", 1000000000ul, &split_work);
    wf__sched_setting("WF_SCHED_REPORT", 2ul, &report_wanted);
    unsigned long ceiling = wf__sched_helper_ceiling();
    if (ceiling) wf__sched_setting("WF_IO_HELPERS", ceiling, &helpers);
    if (report_wanted == 2) atexit(print_report);
}
int wf__sched_lanes(void) {
    wf__sched_once(&initialized, initialize);
    return lanes;
}
uint64_t wf__sched_split_work(void) { return split_work; }
void wf__runtime_start(void) { (void)wf__sched_lanes(); }

#if defined(WF_PAR_DEMAND)
/* This setting is consulted once when a --par-demand pool starts (or its
 * first accessor attaches). Ordinary --par never interprets it. */
static unsigned demand_initialized;
static int demand_requests = 1;
static void initialize_demand(void) {
    char text[WF_PRIM_SETTING_BYTES];
    int state = wf_prim_setting_text("WF_PAR_DEMAND", text, sizeof(text));
    if (state == 0 || (state == 1 && (text[0] == '\0' || strcmp(text, "on") == 0))) return;
    if (state == 1 && strcmp(text, "off-never-request") == 0) {
        demand_requests = 0;
        return;
    }
    fputs("whitefoot scheduler: WF_PAR_DEMAND must be on or off-never-request\n", stderr);
    fflush(stderr);
    _Exit(1);
}
int wf__sched_demand_requests(void) {
    wf__sched_once(&demand_initialized, initialize_demand);
    return demand_requests;
}
#endif
