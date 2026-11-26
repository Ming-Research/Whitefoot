/* Linux paired regression process. Only call() is timed; preparation,
 * checking, final release, printing and shutdown are outside the interval.
 * The explicit slowdown control is used only to calibrate the instrument.
 * `layout` and WF_PERFORMANCE_POSITION serve the data-placement control
 * (data-placement.sh). */
#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE
#include "../programs/compute/oracle.h"
#include <inttypes.h>
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>

extern int wf__floor_run(int, char **);

static uint64_t clock_ns(clockid_t clock) {
    struct timespec value;
    if (clock_gettime(clock, &value)) wf_oracle_fail("performance clock failed");
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
}

static int known_width(const char *width) {
    return !strcmp(width, "1") || !strcmp(width, "2") || !strcmp(width, "4");
}

int wf__main_body(int argc, char **argv) {
    if (argc == 2 && !strcmp(argv[1], "verify")) {
        printf("%s oracle PASS: compared=%zu\n", wf_oracle_name, wf_oracle_verify());
        return 0;
    }
    if (argc == 3 && !strcmp(argv[1], "layout")) {
        /* The mapping calls of a measured process, for a trace: the same
         * preparation and six calls with their checks, then an exit that maps
         * and unmaps nothing more, so the trace ends before any teardown. */
        const char *workers = getenv("WF_WORKERS");
        if (!known_width(argv[2]) || !workers || strcmp(workers, argv[2]))
            wf_oracle_fail("invalid layout width or worker identity mismatch");
        wf_oracle_prepare();
        for (unsigned sample = 0; sample <= 5; ++sample) {
            size_t count = wf_oracle_call();
            if (!count || wf_oracle_check() != count) wf_oracle_fail("output comparison extent");
        }
        _Exit(0);
    }
    if (argc != 5 && argc != 6)
        wf_oracle_fail("usage: image verify | layout WIDTH | measure baseline|candidate WIDTH PASS [slow]");
    if (strcmp(argv[1], "measure") ||
        (strcmp(argv[2], "baseline") && strcmp(argv[2], "candidate")) ||
        !known_width(argv[3]) ||
        strlen(argv[4]) != 1 || argv[4][0] < '0' || argv[4][0] > '4' ||
        (argc == 6 && strcmp(argv[5], "slow")))
        wf_oracle_fail("invalid performance process identity");
    const char *workers = getenv("WF_WORKERS");
    if (!workers || strcmp(workers, argv[3])) wf_oracle_fail("worker identity mismatch");
    fprintf(stderr, "%s: %s; wall=CLOCK_MONOTONIC cpu=CLOCK_PROCESS_CPUTIME_ID\n",
            wf_oracle_name, wf_oracle_fixture);
    wf_oracle_prepare();
    for (unsigned sample = 0; sample <= 5; ++sample) {
        uint64_t cpu_start = clock_ns(CLOCK_PROCESS_CPUTIME_ID);
        uint64_t wall_start = clock_ns(CLOCK_MONOTONIC);
        size_t count = wf_oracle_call();
        if (argc == 6) {
            /* Deliberately repeat real WF work and its intermediate cleanup.
             * This is a visible instrument control, never a compiler option. */
            if (wf_oracle_check() != count) wf_oracle_fail("slow control output extent");
            if (wf_oracle_call() != count) wf_oracle_fail("slow control call extent");
        }
        uint64_t wall = clock_ns(CLOCK_MONOTONIC) - wall_start;
        uint64_t cpu = clock_ns(CLOCK_PROCESS_CPUTIME_ID) - cpu_start;
        if (!count || wf_oracle_check() != count) wf_oracle_fail("output comparison extent");
        printf("%s\t%s\t%s\t%s\t%u\t%" PRIu64 "\t%" PRIu64 "\t%zu\n",
               wf_oracle_name, argv[2], argv[3], argv[4], sample, wall, cpu, count);
    }
    wf_oracle_finish();
    return 0;
}

/* WF_PERFORMANCE_POSITION=k, 0 to 31, maps k times 2 MiB of inaccessible
 * memory before the entry thread exists, so every later mapping lies that much
 * lower. With address randomization off, the 32 positions walk the entry's
 * stack and heap through the C library's 64 MiB heap alignment. */
int main(int argc, char **argv) {
    const char *position = getenv("WF_PERFORMANCE_POSITION");
    if (position) {
        size_t length = strlen(position), k = 0;
        int valid = length >= 1 && length <= 2;
        for (size_t i = 0; valid && i < length; ++i) {
            valid = position[i] >= '0' && position[i] <= '9';
            k = k * 10 + (size_t)(position[i] - '0');
        }
        if (!valid || k > 31) wf_oracle_fail("WF_PERFORMANCE_POSITION must be 0 to 31");
        if (k && mmap(NULL, k << 21, PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0) ==
                     MAP_FAILED)
            wf_oracle_fail("WF_PERFORMANCE_POSITION mapping failed");
    }
    return wf__floor_run(argc, argv);
}
