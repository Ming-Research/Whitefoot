/* Timing process of the call-grain-args experiment. Same prepare/call/check
 * boundary as tests/performance/runner.c: only the WF call is timed, with wall
 * and process CPU clocks, and the work-hand-out count of the call is printed so
 * that an arm that never handed work out can be told from one that did.
 * Sample 0 is the warm call and is kept but never enters a ratio. */
#define _POSIX_C_SOURCE 200809L
#include "oracle.h"
#include <inttypes.h>
#include <stdint.h>
#include <string.h>
#include <time.h>

extern int wf__floor_run(int, char **);
extern unsigned long wf__par_grants(void);

static uint64_t clock_ns(clockid_t clock) {
    struct timespec value;
    if (clock_gettime(clock, &value)) wf_oracle_fail("performance clock failed");
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
}

int wf__main_body(int argc, char **argv) {
    if (argc == 2 && !strcmp(argv[1], "verify")) {
        printf("%s oracle PASS: compared=%zu\n", wf_oracle_name, wf_oracle_verify());
        return 0;
    }
    if (argc != 5 || strcmp(argv[1], "measure"))
        wf_oracle_fail("usage: image verify | image measure ARM WIDTH ROUND");
    const char *workers = getenv("WF_WORKERS");
    if (!workers || strcmp(workers, argv[3])) wf_oracle_fail("worker identity mismatch");
    const char *samples_text = getenv("WFC_SAMPLES");
    unsigned samples = samples_text ? (unsigned)atoi(samples_text) : 5;
    if (samples < 1 || samples > 50) wf_oracle_fail("WFC_SAMPLES must be 1 to 50");
    wf_oracle_prepare();
    for (unsigned sample = 0; sample <= samples; ++sample) {
        unsigned long grants_before = wf__par_grants();
        uint64_t cpu_start = clock_ns(CLOCK_PROCESS_CPUTIME_ID);
        uint64_t wall_start = clock_ns(CLOCK_MONOTONIC);
        size_t count = wf_oracle_call();
        uint64_t wall = clock_ns(CLOCK_MONOTONIC) - wall_start;
        uint64_t cpu = clock_ns(CLOCK_PROCESS_CPUTIME_ID) - cpu_start;
        unsigned long grants = wf__par_grants() - grants_before;
        if (!count || wf_oracle_check() != count) wf_oracle_fail("output comparison extent");
        printf("%s\t%s\t%s\t%s\t%u\t%" PRIu64 "\t%" PRIu64 "\t%zu\t%lu\n",
               wf_oracle_name, argv[2], argv[3], argv[4], sample, wall, cpu, count, grants);
    }
    wf_oracle_finish();
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
