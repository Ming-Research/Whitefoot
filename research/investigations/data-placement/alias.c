/* Times one stencil-shaped loop over two 32 MiB arrays mapped at chosen
 * addresses, for research/investigations/data-placement: the regression
 * stencil's access pattern (four reads of one array around a cell, one write
 * to the same cell of the other, rows 8192 bytes apart) with the arrays'
 * distance as the only variable. Each array starts 16 bytes into its mapping,
 * as a block of that size from glibc does.
 *
 * usage: alias LOW DISTANCE [LOW DISTANCE ...], hexadecimal; one array is
 * mapped at LOW and the other at LOW + DISTANCE. Prints LOW, DISTANCE and the
 * fastest and the median of five runs in nanoseconds. */
#define _GNU_SOURCE
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <time.h>

enum { WIDTH = 1024, HEIGHT = 4096, STEPS = 16, RUNS = 5 };
#define CELLS ((size_t)WIDTH * HEIGHT)
#define BLOCK (CELLS * sizeof(double) + 4096)

static uint64_t now(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC, &t)) exit(3);
    return (uint64_t)t.tv_sec * UINT64_C(1000000000) + (uint64_t)t.tv_nsec;
}

static double *map_at(uintptr_t address) {
    void *p = mmap((void *)address, BLOCK, PROT_READ | PROT_WRITE,
                   MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE, -1, 0);
    if (p != (void *)address) {
        fprintf(stderr, "cannot map %#" PRIxPTR "\n", address);
        exit(3);
    }
    return (double *)((char *)p + 16);
}

__attribute__((noinline)) static void step(const double *restrict a, double *restrict b) {
    for (size_t y = 1; y + 1 < HEIGHT; ++y)
        for (size_t x = 1; x + 1 < WIDTH; ++x) {
            size_t at = y * WIDTH + x;
            b[at] = ((a[at - WIDTH] + a[at + WIDTH]) + (a[at - 1] + a[at + 1])) * 0.25;
        }
}

int main(int argc, char **argv) {
    if (argc < 3 || argc % 2 != 1) {
        fprintf(stderr, "usage: alias LOW DISTANCE [LOW DISTANCE ...]\n");
        return 2;
    }
    for (int i = 1; i + 1 < argc; i += 2) {
        uintptr_t low = (uintptr_t)strtoull(argv[i], NULL, 16);
        uintptr_t distance = (uintptr_t)strtoull(argv[i + 1], NULL, 16);
        if (distance < BLOCK) {
            fprintf(stderr, "distance %#" PRIxPTR " overlaps\n", distance);
            return 2;
        }
        double *b = map_at(low), *a = map_at(low + distance);
        for (size_t c = 0; c < CELLS; ++c) a[c] = b[c] = (double)(c % 1021);
        uint64_t times[RUNS];
        for (int run = 0; run < RUNS; ++run) {
            double *from = a, *to = b;
            uint64_t start = now();
            for (int s = 0; s < STEPS; ++s) {
                step(from, to);
                double *swap = from;
                from = to;
                to = swap;
            }
            times[run] = now() - start;
        }
        for (int m = 1; m < RUNS; ++m)
            for (int n = m; n > 0 && times[n - 1] > times[n]; --n) {
                uint64_t swap = times[n];
                times[n] = times[n - 1];
                times[n - 1] = swap;
            }
        printf("%#" PRIxPTR "\t%#" PRIxPTR "\t%" PRIu64 "\t%" PRIu64 "\n",
               low, distance, times[0], times[RUNS / 2]);
        fflush(stdout);
        munmap((char *)a - 16, BLOCK);
        munmap((char *)b - 16, BLOCK);
    }
    return 0;
}
