/* Host primitive integration and deterministic scheduler policy boundaries.
 * The included cases share one production-core image; their fresh processes
 * keep startup/CPU settings isolated. Retire when the policy has no consumer. */
#include "prim.h"
#include <stdio.h>
#include <string.h>
static unsigned probe_cpus, probe_levels;
static int probe_override;
static unsigned probe_online_cpus(void) {
    return probe_override ? probe_cpus : wf_prim_online_cpus();
}
static unsigned probe_cpu_levels(void) {
    return probe_override ? probe_levels : wf_prim_cpu_levels();
}
#define wf_prim_online_cpus probe_online_cpus
#define wf_prim_cpu_levels probe_cpu_levels
#include "core.c"
#undef wf_prim_online_cpus
#undef wf_prim_cpu_levels
#include "../runtime_test_guard.h"
#define WF_SCHED_POLICY_COLLECTION
#include "wake_probe.c"
#include "recursion_budget_probe.c"
#undef WF_SCHED_POLICY_COLLECTION

static int probe_idle_policy(const char *mode) {
    int expected_open = 0;
    unsigned levels = wf_prim_cpu_levels();
    unsigned again = wf_prim_cpu_levels();
    unsigned cpus = wf_prim_online_cpus();
    if (!levels || levels != again) {
        fputs("cpu levels probe: invalid or unstable host result\n", stderr);
        return 1;
    }
    if (strcmp(mode, "host") != 0) {
        probe_override = 1;
        probe_cpus = 4;
        probe_levels = 1;
        if (!strcmp(mode, "fit")) expected_open = 1;
        else if (!strcmp(mode, "oversubscribed")) probe_cpus = 2;
        else if (!strcmp(mode, "unknown-count")) probe_cpus = 0;
        else if (!strcmp(mode, "mixed")) {
            probe_levels = 2;
#if defined(WF_PAR_IDLE_WINDOW_ON_ASYMMETRIC)
            expected_open = 1;
#endif
        } else return 2;
        if (wf__sched_lanes() != 4) {
            fputs("controlled CPU policy requires WF_WORKERS=4\n", stderr);
            return 2;
        }
    } else {
        int lanes = wf__sched_lanes();
        expected_open = lanes >= 2 && cpus && (unsigned)lanes <= cpus;
#if !defined(WF_PAR_IDLE_WINDOW_ON_ASYMMETRIC)
        expected_open = expected_open && levels == 1;
#endif
    }
    /* Only these two CPU query calls are interposed. Real startup and the
     * delivered policy assign the window. The host case is integration, not
     * an independent oracle for the machine's reported CPU topology. */
    (void)wf__par_attach();
    uint64_t expected = expected_open ? (uint64_t)WF_PAR_IDLE_WINDOW_US : 0;
    if (wf__par_idle_window_us != expected) {
        fprintf(stderr, "cpu policy %s: actual=%llu expected=%llu\n", mode,
                (unsigned long long)wf__par_idle_window_us,
                (unsigned long long)expected);
        return 1;
    }
    printf("cpu levels probe: PASS mode=%s levels=%u cpus=%u window_us=%llu\n",
           mode, levels, cpus, (unsigned long long)wf__par_idle_window_us);
    return 0;
}

int main(int argc, char **argv) {
    wf_test_guard_start(60);
    const char *mode = argc > 1 ? argv[1] : "host";
    wf_test_guard_phase(mode);
    int result;
    if (!strcmp(mode, "wake")) result = wf_probe_wake(argc - 1, argv + 1);
    else if (!strcmp(mode, "budget")) result = wf_probe_recursion_budget(argc - 1, argv + 1);
    else result = probe_idle_policy(mode);
    wf_test_guard_finish();
    return result;
}
