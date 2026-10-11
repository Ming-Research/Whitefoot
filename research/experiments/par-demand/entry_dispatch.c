/* Research-only query/conditional-transfer witness; no workload is selected.
 * The actual scheduler is linked in a separate object without LTO. */
#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

extern int wf__floor_run(int, char **);
extern int wf__par_pool_active(void);
int wf__par_demand_mode(void) { return 1; }
static volatile uint64_t sink;
extern uint64_t wf_entry_active(uint64_t);
extern uint64_t wf_entry_inactive(uint64_t);
__attribute__((noinline)) uint64_t wf_entry_direct(uint64_t x) {
    return wf_entry_inactive(x);
}
__attribute__((noinline)) uint64_t wf_entry_dispatch(uint64_t x) {
    if (wf__par_pool_active()) return wf_entry_active(x);
    return wf_entry_inactive(x);
}
static void fail(const char *message) { fprintf(stderr, "%s\n", message); exit(1); }
static uint64_t now(clockid_t clock) {
    struct timespec t;
    if (clock_gettime(clock, &t)) fail("entry clock failed");
    return (uint64_t)t.tv_sec * UINT64_C(1000000000) + (uint64_t)t.tv_nsec;
}
struct sample { uint64_t wall, cpu; };
static struct sample batch(uint64_t n, int dispatch) {
    uint64_t total = 0, cpu = now(CLOCK_PROCESS_CPUTIME_ID), wall = now(CLOCK_MONOTONIC);
    for (uint64_t i = 0; i < n; ++i)
        total += dispatch ? wf_entry_dispatch(i) : wf_entry_direct(i);
    struct sample result = {now(CLOCK_MONOTONIC) - wall, now(CLOCK_PROCESS_CPUTIME_ID) - cpu};
    sink = total;
    if (total != n * (n + 1) / 2) fail("entry result mismatch");
    return result;
}
int wf__main_body(int argc, char **argv) {
    if (argc != 4) fail("usage: entry-dispatch WIDTH CALLS ORDER");
    const char *workers = getenv("WF_WORKERS");
    if (!workers || strcmp(workers, argv[1])) fail("entry worker identity mismatch");
    char *end;
    unsigned long width = strtoul(argv[1], &end, 10);
    if (*end || (width != 1 && width != 2 && width != 4 && width != 8)) fail("entry width");
    uint64_t calls = strtoull(argv[2], &end, 10);
    if (*end || calls < 1024 || calls > 1048576) fail("entry calls");
    int order = !strcmp(argv[3], "1");
    if (!order && strcmp(argv[3], "0")) fail("entry order");
    struct timespec resolution;
    if (clock_getres(CLOCK_MONOTONIC, &resolution)) fail("entry clock resolution");
    uint64_t cpu = now(CLOCK_PROCESS_CPUTIME_ID), wall = now(CLOCK_MONOTONIC);
    int active = wf__par_pool_active();
    uint64_t first_wall = now(CLOCK_MONOTONIC) - wall, first_cpu = now(CLOCK_PROCESS_CPUTIME_ID) - cpu;
    if (active != (width > 1)) fail("entry pool state mismatch");
    uint64_t warm = 0;
    for (uint64_t i = 0; i < 1024; ++i)
        warm += wf_entry_direct(i) + wf_entry_dispatch(i);
    sink = warm;
    if (warm != UINT64_C(1024) * 1025) fail("entry warm result mismatch");
    struct sample direct, dispatch;
    if (order) { dispatch = batch(calls, 1); direct = batch(calls, 0); }
    else { direct = batch(calls, 0); dispatch = batch(calls, 1); }
    printf("{\"width\":%lu,\"calls\":%" PRIu64 ",\"order\":%d,\"active\":%d,"
           "\"resolution_ns\":%" PRIu64 ",\"first_query\":{\"wall_ns\":%" PRIu64 ",\"cpu_ns\":%" PRIu64 "},"
           "\"direct\":{\"wall_ns\":%" PRIu64 ",\"cpu_ns\":%" PRIu64 ",\"pool_queries\":0},"
           "\"dispatch\":{\"wall_ns\":%" PRIu64 ",\"cpu_ns\":%" PRIu64 ",\"pool_queries\":%" PRIu64 "}}\n",
           width, calls, order, active, (uint64_t)resolution.tv_sec * UINT64_C(1000000000) + (uint64_t)resolution.tv_nsec,
           first_wall, first_cpu, direct.wall, direct.cpu, dispatch.wall, dispatch.cpu, calls);
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
