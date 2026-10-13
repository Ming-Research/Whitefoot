/* Oracle of the minimal pair for runner.c's prepare/call/check protocol. Call 0
 * is the warm call (WFC_WARM_REPS repetitions); every later call is a timed one
 * (WFC_REPS). Expected values come from pair_reference, computed once per cell. */
#include "oracle.h"
#include "pair_reference.h"
#include <string.h>
const char *const wf_oracle_name = "pair";
const char *const wf_oracle_fixture = "two mix_words calls over disjoint ranges; WFC_REPS, WFC_EXTENT";
extern uint64_t wf_bench_pair(uint64_t, uint64_t, uint64_t);
#define SEED UINT64_C(17)
static uint64_t reps, warm_reps, extent, expected, expected_warm, observed, calls_made;
static int skip_check;
static uint64_t need(const char *name) {
    const char *text = getenv(name);
    if (!text || !*text) {
        if (skip_check) return 0;
        fprintf(stderr, "pair oracle needs %s\n", name);
        exit(1);
    }
    return strtoull(text, 0, 10);
}
size_t wf_oracle_verify(void) {
    static const uint64_t cases[][2] = {{3, 17}, {2, 0}, {1, 1}, {2, 300}};
    for (size_t i = 0; i < sizeof cases / sizeof cases[0]; ++i)
        if (wf_bench_pair(cases[i][0], cases[i][1], SEED) != pair_reference(cases[i][0], cases[i][1], SEED))
            wf_oracle_fail("pair oracle mismatch");
    return sizeof cases / sizeof cases[0];
}
void wf_oracle_prepare(void) {
    skip_check = getenv("WFC_SKIP_CHECK") != NULL;
    reps = need("WFC_REPS"); warm_reps = need("WFC_WARM_REPS"); extent = need("WFC_EXTENT");
    expected = need("WFC_EXPECTED"); expected_warm = need("WFC_EXPECTED_WARM");
}
size_t wf_oracle_call(void) {
    observed = wf_bench_pair(calls_made == 0 ? warm_reps : reps, extent, SEED);
    ++calls_made;
    return 1;
}
size_t wf_oracle_check(void) {
    if (!skip_check && observed != (calls_made == 1 ? expected_warm : expected)) wf_oracle_fail("timed pair result mismatch");
    return 1;
}
void wf_oracle_finish(void) { }
