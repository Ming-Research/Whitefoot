/* Independent unsigned arithmetic / final-store oracle. No WF code is timed
 * in verification; the timed runner calls the exported WF batch once. */
#include "../../../tests/programs/compute/oracle.h"
#include <stdint.h>
#include <string.h>
#ifndef MICRO
#error "select MICRO"
#endif
const char *const wf_oracle_name = MICRO;
const char *const wf_oracle_fixture = "runtime batch/extent; see manifest.json";
extern uint64_t wf_bench_micro(uint64_t, uint64_t, uint64_t);
static uint64_t repetitions, extent, observed, expected;
static uint64_t rotl(uint64_t x, unsigned shift) { return (x << shift) | (x >> (64 - shift)); }
static uint64_t work(uint64_t n, uint64_t seed) {
    for (uint64_t i = 0; i < n; ++i) seed = rotl(seed, 7) + i;
    return seed;
}
static uint64_t oracle(uint64_t reps, uint64_t n, uint64_t seed) {
    if (!strcmp(MICRO, "small_constant")) {
        /* Every call writes cells 0, 1 and 2 with its own salt, so only the
         * last call's values remain; the other cells stay zero. */
        if (reps == 0) return 0;
        uint64_t last = reps - 1 + seed;
        return (0 + last) + (1 + last) + (2 + last);
    }
    if (!strcmp(MICRO, "small_split")) {
        uint64_t cells[4096] = {0};
        /* Each of the 572 walker starts repeats independently. Only its final
         * write matters; compute those at most 572 batches, not 200M calls. */
        uint64_t period = (4000 + 6) / 7;
        uint64_t start = reps > period ? reps - period : 0;
        for (uint64_t i = start; i < reps; ++i) {
            uint64_t lo = (i % period) * 7;
            for (uint64_t j = 0; j < n; ++j) cells[lo + j] = lo + j + i + seed;
        }
        uint64_t sum = 0;
        for (size_t i = 0; i < 4096; ++i) sum += cells[i];
        return sum;
    }
    if (!strcmp(MICRO, "recursion")) {
        uint64_t a = 0, b = 1;
        for (uint64_t i = 0; i < n; ++i) { uint64_t next = a + b; a = b; b = next; }
        return seed + reps * a;
    }
    if (!strcmp(MICRO, "spine")) {
        /* spine(n, v) is its tip v + n plus a side leaf rotl(v + k, 7) * 3
         * for each k below n. The rotation keeps the compiled spine from
         * folding into a closed form, which a linear leaf allowed. */
        uint64_t sum = seed;
        for (uint64_t i = 0; i < reps; ++i) {
            uint64_t value = i + n;
            for (uint64_t k = 0; k < n; ++k) value += rotl(i + k, 7) * 3;
            sum += value;
        }
        return sum;
    }
    uint64_t sum = seed;
    if (!strcmp(MICRO, "hot_helper")) {
        for (uint64_t i = 0; i < reps; ++i) {
            uint64_t count = sum & 31;
            sum += work(count, sum) ^ work(count, sum + 1);
        }
        return sum;
    }
    uint64_t terms = 0;
    for (uint64_t i = 0; i < n; ++i) terms += rotl(i, 13) * UINT64_C(6364136223846793005);
    return seed + reps * 2 * terms + reps * (reps - 1) / 2;
}
size_t wf_oracle_verify(void) {
    uint64_t reps = 1200, n = 3;
    uint64_t actual = wf_bench_micro(reps, n, 17);
    if (actual != oracle(reps, n, 17)) wf_oracle_fail("micro oracle mismatch");
    return 1;
}
void wf_oracle_prepare(void) {
    const char *r = getenv("WFD_REPETITIONS"), *n = getenv("WFD_EXTENT");
    if (!r || !n) wf_oracle_fail("micro requires its manifest settings");
    repetitions = strtoull(r, NULL, 10); extent = strtoull(n, NULL, 10);
    expected = oracle(repetitions, extent, 17);
}
size_t wf_oracle_call(void) { observed = wf_bench_micro(repetitions, extent, 17); return 1; }
size_t wf_oracle_check(void) {
    if (observed != expected) wf_oracle_fail("timed micro result mismatch");
    return 1;
}
void wf_oracle_finish(void) { }
