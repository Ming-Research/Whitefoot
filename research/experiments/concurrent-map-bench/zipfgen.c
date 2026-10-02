/* Serves concurrent-map-bench: writes the Zipf rank buffers every driver
 * reads, so that every language draws the same ranks.
 *
 *   zipfgen N STREAMS OUT
 *
 * The ranks follow the Zipfian generator of the Yahoo! Cloud Serving
 * Benchmark (Gray et al., "Quickly generating billion-record synthetic
 * databases") with theta 0.99; rank 0 is the most frequent.
 */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "workload.h"

#define THETA 0.99

int main(int argc, char **argv) {
    if (argc != 4) {
        fprintf(stderr, "usage: zipfgen N STREAMS OUT\n");
        return 2;
    }
    uint64_t n = strtoull(argv[1], NULL, 10);
    uint64_t streams = strtoull(argv[2], NULL, 10);
    if (n < 2 || n > 0xffffffffull || streams == 0 || streams > 4096) {
        fprintf(stderr, "zipfgen: N must lie in [2, 2^32) and STREAMS in [1, 4096]\n");
        return 2;
    }
    double zetan = 0.0;
    for (uint64_t i = 1; i <= n; i++)
        zetan += 1.0 / pow((double)i, THETA);
    double zeta2 = 1.0 + 1.0 / pow(2.0, THETA);
    double alpha = 1.0 / (1.0 - THETA);
    double eta = (1.0 - pow(2.0 / (double)n, 1.0 - THETA)) / (1.0 - zeta2 / zetan);
    double half = 1.0 + pow(0.5, THETA);

    FILE *out = fopen(argv[3], "wb");
    if (out == NULL) {
        fprintf(stderr, "zipfgen: cannot open %s\n", argv[3]);
        return 2;
    }
    struct wl_zipf_header h;
    memset(&h, 0, sizeof h);
    memcpy(h.magic, "WFZIPF1", 8);
    h.n = n;
    h.streams = streams;
    h.length = WL_ZIPF_LENGTH;
    h.theta = THETA;
    fwrite(&h, sizeof h, 1, out);
    uint32_t *buffer = malloc(WL_ZIPF_LENGTH * sizeof *buffer);
    for (uint64_t s = 0; s < streams; s++) {
        uint64_t state = wl_mix64(0x5A1F000000000000ull ^ s);
        for (uint32_t i = 0; i < WL_ZIPF_LENGTH; i++) {
            double u = (double)(wl_next(&state) >> 11) * 0x1.0p-53;
            double uz = u * zetan;
            uint64_t rank;
            if (uz < 1.0)
                rank = 0;
            else if (uz < half)
                rank = 1;
            else
                rank = (uint64_t)((double)n * pow(eta * u - eta + 1.0, alpha));
            if (rank >= n)
                rank = n - 1;
            buffer[i] = (uint32_t)rank;
        }
        if (fwrite(buffer, sizeof *buffer, WL_ZIPF_LENGTH, out) != WL_ZIPF_LENGTH) {
            fprintf(stderr, "zipfgen: cannot write %s\n", argv[3]);
            return 2;
        }
    }
    free(buffer);
    return fclose(out) == 0 ? 0 : 2;
}
