/* The formal blocked oracle has no performance API. Adapt its independent
 * one-pass reference and complete-result comparison without copying them. */
#include "../../../tests/programs/compute/blocked_oracle.c"
#include "../../../tests/programs/compute/oracle.h"
extern void ENTRY(const uint64_t *, uint64_t, uint64_t, uint64_t, uint64_t **, uint64_t *, void **);
extern void RELEASE(void *);
const char *const wf_oracle_name = KERNEL_NAME;
const char *const wf_oracle_fixture = "count=1048593 block=64 buckets=256 distribution=0";
static uint64_t *input, *expected, *actual;
static void *held;
static uint64_t length;
static const size_t count = 1048593, buckets = 256;
size_t wf_oracle_verify(void) { return verify_matrix(ENTRY, RELEASE); }
void wf_oracle_prepare(void) {
    input = words_new(count);
    for (size_t i = 0; i < count; ++i) input[i] = key_at(i, 0);
    expected = oracle(input, count, buckets);
}
size_t wf_oracle_call(void) {
    ENTRY(input, count, 64, buckets, &actual, &length, &held);
    return (size_t)length;
}
size_t wf_oracle_check(void) {
    size_t n = output_count(count, buckets);
    if (length != n) wf_oracle_fail("timed length mismatch");
    size_t compared = compare(expected, actual, n);
    for (size_t i = 0; i < count; ++i) if (input[i] != key_at(i, 0)) fail("input modified");
    RELEASE(held); held = NULL; actual = NULL;
    return compared;
}
void wf_oracle_finish(void) { free(input); free(expected); }
