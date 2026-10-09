/* tests/performance's prepare/call/check timing boundary, with experiment
 * identities and width 8. Kept here so no research mode enters the gate. */
#define _POSIX_C_SOURCE 200809L
#include "../../../tests/programs/compute/oracle.h"
#include <inttypes.h>
#include <stdint.h>
#include <string.h>
#include <time.h>
extern int wf__floor_run(int, char **);
static uint64_t clock_ns(clockid_t clock) {
    struct timespec value;
    if (clock_gettime(clock, &value)) wf_oracle_fail("clock failed");
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
}
int wf__main_body(int argc, char **argv) {
    if (argc == 2 && !strcmp(argv[1], "verify")) {
        printf("%s oracle PASS: compared=%zu\n", wf_oracle_name, wf_oracle_verify());
        return 0;
    }
    if (argc != 6 || strcmp(argv[1], "measure")) wf_oracle_fail("usage: image measure ARM WIDTH ROUND ATTEMPT");
    const char *workers = getenv("WF_WORKERS");
    if (!workers || strcmp(workers, argv[3])) wf_oracle_fail("worker identity mismatch");
    wf_oracle_prepare();
    for (unsigned sample = 0; sample < 2; ++sample) {
        uint64_t cpu_start = clock_ns(CLOCK_PROCESS_CPUTIME_ID);
        uint64_t wall_start = clock_ns(CLOCK_MONOTONIC);
        size_t count = wf_oracle_call();
        uint64_t wall = clock_ns(CLOCK_MONOTONIC) - wall_start;
        uint64_t cpu = clock_ns(CLOCK_PROCESS_CPUTIME_ID) - cpu_start;
        if (!count || wf_oracle_check() != count) wf_oracle_fail("comparison extent");
        printf("%s\t%s\t%s\t%s\t%s\t%u\t%" PRIu64 "\t%" PRIu64 "\t%zu\n",
               wf_oracle_name, argv[2], argv[3], argv[4], argv[5], sample, wall, cpu, count);
    }
    wf_oracle_finish();
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
