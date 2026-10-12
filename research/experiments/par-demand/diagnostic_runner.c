/* Investigation-only observation runner. Retire with par-demand diagnostics.
 * Oracle preparation/checking and all observations stay outside timed calls. */
#define _POSIX_C_SOURCE 200809L
#include "../../../tests/programs/compute/oracle.h"
#include <inttypes.h>
#include <stdint.h>
#include <string.h>
#include <time.h>
extern int wf__floor_run(int, char **);
extern void wf__par_diagnostic_observe(FILE *) __attribute__((weak));
extern void wf__par_diagnostic_snapshot(FILE *) __attribute__((weak));
extern void wf__par_diagnostic_trace(FILE *) __attribute__((weak));

static uint64_t now(clockid_t clock) {
    struct timespec value;
    if (clock_gettime(clock, &value)) wf_oracle_fail("diagnostic clock failed");
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
}
static void string(const char *value) {
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)value; *p; ++p) {
        if (*p == '"' || *p == '\\') printf("\\%c", *p);
        else if (*p < 32) printf("\\u%04x", *p);
        else putchar(*p);
    }
    putchar('"');
}
static void mappings(void) {
    FILE *file = fopen("/proc/self/maps", "r");
    if (!file) wf_oracle_fail("diagnostic requires Linux process mappings");
    char *line = NULL;
    size_t capacity = 0;
    unsigned count = 0;
    putchar('[');
    while (getline(&line, &capacity, file) >= 0) {
        line[strcspn(line, "\n")] = '\0';
        if (count++) putchar(',');
        string(line);
    }
    free(line);
    fclose(file);
    putchar(']');
    if (!count) wf_oracle_fail("empty diagnostic mappings");
}
static char *capture(void (*serialize)(FILE *)) {
    if (!serialize) return NULL;
    char *buffer = NULL;
    size_t size = 0;
    FILE *stream = open_memstream(&buffer, &size);
    if (!stream) wf_oracle_fail("diagnostic memory stream");
    serialize(stream);
    if (fclose(stream)) wf_oracle_fail("diagnostic memory capture");
    return buffer;
}
int wf__main_body(int argc, char **argv) {
    if (argc == 2 && !strcmp(argv[1], "verify")) {
        size_t compared = wf_oracle_verify();
        if (!compared) wf_oracle_fail("empty diagnostic oracle");
        printf("{\"verification\":\"pass\",\"compared\":%zu}\n", compared);
        return 0;
    }
    if (argc != 4 || strcmp(argv[1], "diagnostic"))
        wf_oracle_fail("usage: image diagnostic LABEL WIDTH");
    const char *workers = getenv("WF_WORKERS");
    if (!workers || strcmp(workers, argv[3])) wf_oracle_fail("diagnostic worker identity");
    char *end;
    unsigned long width = strtoul(argv[3], &end, 10);
    if (*end || (width != 1 && width != 2 && width != 4 && width != 8)) wf_oracle_fail("diagnostic width");
    if (!wf__par_diagnostic_observe) wf_oracle_fail("missing observation runtime");
    const char *trace_setting = getenv("WFD_STARTUP_TRACE");
    int want_trace = trace_setting && !strcmp(trace_setting, "1");
    if (trace_setting && !want_trace) wf_oracle_fail("invalid startup trace setting");
    if (want_trace && !wf__par_diagnostic_trace) wf_oracle_fail("missing startup trace runtime");
    uint64_t process_start = now(CLOCK_MONOTONIC), previous_end = 0;
    struct call {
        char *before, *after, *snapshot_before, *snapshot_after;
        uint64_t start, end, cpu, gap;
        size_t count;
    } calls[2];
    wf_oracle_prepare();
    for (unsigned sample = 0; sample < 2; ++sample) {
        struct call *call = &calls[sample];
        call->before = capture(wf__par_diagnostic_observe);
        call->snapshot_before = capture(wf__par_diagnostic_snapshot);
        uint64_t cpu_start = now(CLOCK_PROCESS_CPUTIME_ID);
        call->start = now(CLOCK_MONOTONIC);
        call->count = wf_oracle_call();
        call->end = now(CLOCK_MONOTONIC);
        call->cpu = now(CLOCK_PROCESS_CPUTIME_ID) - cpu_start;
        call->snapshot_after = capture(wf__par_diagnostic_snapshot);
        call->after = capture(wf__par_diagnostic_observe);
        call->gap = previous_end ? call->start - previous_end : 0;
        previous_end = call->end;
        if (!call->count || wf_oracle_check() != call->count) wf_oracle_fail("diagnostic comparison extent");
    }
    char *trace = want_trace ? capture(wf__par_diagnostic_trace) : NULL;
    fputs("{\"workload\":", stdout); string(wf_oracle_name);
    fputs(",\"label\":", stdout); string(argv[2]);
    printf(",\"width\":%s,\"instrumented\":%s,\"process_start_ns\":%" PRIu64 ",\"mappings\":",
           argv[3], wf__par_diagnostic_snapshot ? "true" : "false", process_start);
    mappings();
    fputs(",\"samples\":[", stdout);
    for (unsigned sample = 0; sample < 2; ++sample) {
        struct call *call = &calls[sample];
        if (sample) putchar(',');
        printf("{\"sample\":%u,\"before\":%s,\"snapshot_before\":%s", sample,
               call->before, call->snapshot_before ? call->snapshot_before : "null");
        printf(",\"start_ns\":%" PRIu64 ",\"end_ns\":%" PRIu64 ",\"gap_since_previous_call_ns\":%" PRIu64
               ",\"wall_ns\":%" PRIu64 ",\"cpu_ns\":%" PRIu64 ",\"count\":%zu,\"after\":",
               call->start, call->end, call->gap, call->end - call->start, call->cpu, call->count);
        fputs(call->after, stdout);
        printf(",\"snapshot_after\":%s", call->snapshot_after ? call->snapshot_after : "null");
        fputs(",\"oracle\":\"pass\"}", stdout);
        free(call->before); free(call->after); free(call->snapshot_before); free(call->snapshot_after);
    }
    printf("],\"startup_trace_requested\":%s,\"startup_trace\":", want_trace ? "true" : "false");
    fputs(trace ? trace : "null", stdout);
    free(trace);
    wf_oracle_finish();
    printf(",\"process_end_ns\":%" PRIu64 "}\n", now(CLOCK_MONOTONIC));
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
