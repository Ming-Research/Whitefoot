/* Prints the expected results of pair.wf: "EXPECTED EXPECTED_WARM". Run once per
 * cell by run.py; the timed processes receive the numbers in their environment. */
#include "pair_reference.h"
#include <inttypes.h>
#include <stdio.h>
int main(int argc, char **argv) {
    if (argc != 5) { fputs("usage: pair_reference REPS WARM_REPS EXTENT SEED\n", stderr); return 2; }
    uint64_t reps = strtoull(argv[1], 0, 10), warm = strtoull(argv[2], 0, 10);
    uint64_t extent = strtoull(argv[3], 0, 10), seed = strtoull(argv[4], 0, 10);
    printf("%" PRIu64 " %" PRIu64 "\n", pair_reference(reps, extent, seed), pair_reference(warm, extent, seed));
    return 0;
}
