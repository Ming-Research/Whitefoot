/* Common object for every FIR construction/layout control; no LTO.
 * Separate named leaves call the exact same selected kernel entrance. */
#include <stdint.h>
extern void *wf_diagnostic_filter(const double *, uint64_t, const double *, uint64_t,
                                  uint64_t, uint64_t, uint64_t);
extern int wf__par_pool_active(void);
extern uint64_t wf_release_samples(void *);
int wf__par_demand_mode(void) { return 1; }
#define PARAMETERS const double *input, uint64_t input_len, const double *taps, uint64_t tap_len, uint64_t first, uint64_t end, uint64_t last_tap
#define ARGUMENTS input, input_len, taps, tap_len, first, end, last_tap
__attribute__((noinline)) void *wf_diagnostic_fir_inactive(PARAMETERS) {
    return wf_diagnostic_filter(ARGUMENTS);
}
__attribute__((noinline)) void *wf_diagnostic_fir_active(PARAMETERS) {
    return wf_diagnostic_filter(ARGUMENTS);
}
__attribute__((noinline)) void *wf_diagnostic_fir_direct(PARAMETERS) {
    return wf_diagnostic_fir_inactive(ARGUMENTS);
}
__attribute__((noinline)) void *wf_diagnostic_fir_query(PARAMETERS) {
    if (wf__par_pool_active()) return wf_diagnostic_fir_active(ARGUMENTS);
    return wf_diagnostic_fir_inactive(ARGUMENTS);
}
#undef PARAMETERS
#undef ARGUMENTS
void wf_bench_fir_release(void *cell) { (void)wf_release_samples(cell); }
