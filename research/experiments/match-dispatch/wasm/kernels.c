/* E0's kernels as a WASI program, for running the same work on
 * Silverfir-nano's interpreter. Each kernel follows vm.c's bytecode step for
 * step and prints the same checksum, which the runner compares with vm.c's.
 * The floor kernel has no counterpart: compiled C would remove its stores.
 *
 *   kernels <loop|fib|sieve|mandel|poly>
 */
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static uint64_t k_loop(int64_t n) {
    uint64_t sum = 0, t;
    int64_t i = 0;
    do {
        t = sum >> 3;
        t ^= (uint64_t)i;
        sum += t;
        i += 1;
    } while (i < n);
    return sum;
}

static uint64_t fib(int64_t n) {
    if (n < 2)
        return (uint64_t)n;
    uint64_t a = fib(n - 1);
    return a + fib(n - 2);
}

#define SIEVE_N 8192
static uint8_t mem[SIEVE_N];
static uint64_t k_sieve(int64_t reps) {
    uint64_t count = 0;
    for (int64_t rep = 0; rep < reps; rep++) {
        for (int64_t i = 0; i < SIEVE_N; i++)
            mem[i] = 1;
        for (int64_t i = 2; i < SIEVE_N; i++) {
            if (mem[i] == 0)
                continue;
            count += 1;
            for (int64_t j = i + i; j < SIEVE_N; j += i)
                mem[j] = 0;
        }
    }
    return count;
}

static uint64_t k_mandel(int64_t size) {
    const double x0 = -2.0, y0 = -1.5, dx = 3.0 / (double)size, dy = 3.0 / (double)size;
    uint64_t total = 0;
    for (int64_t y = 0; y < size; y++) {
        double ci = (double)y * dy;
        ci = ci + y0;
        for (int64_t x = 0; x < size; x++) {
            double cr = (double)x * dx;
            cr = cr + x0;
            double zr = 0.0, zi = 0.0;
            int64_t it = 0;
            do {
                double zr2 = zr * zr, zi2 = zi * zi, t = zr2 + zi2;
                if (t > 4.0)
                    break;
                t = zr * zi;
                t = t * 2.0;
                zi = t + ci;
                t = zr2 - zi2;
                zr = t + cr;
                it += 1;
            } while (it < 100);
            total += (uint64_t)it;
        }
    }
    return total;
}

/* Volatile so that the compiler cannot evaluate the whole kernel at
 * compile time, which it otherwise does. */
static volatile uint64_t poly_init[8] = { 13, 7932, 15851, 23770, 31689, 39608, 47527, 55446 };
static volatile int64_t poly_reps = 100000;
static uint64_t k_poly(int64_t reps_unused) {
    (void)reps_unused;
    int64_t reps = poly_reps;
    uint64_t r[8];
    for (int i = 0; i < 8; i++)
        r[i] = poly_init[i];
    uint64_t r0 = r[0], r1 = r[1], r2 = r[2], r3 = r[3], r4 = r[4], r5 = r[5], r6 = r[6], r7 = r[7];
    int64_t rep = 0;
    do {
#include "poly_body.h"
        rep += 1;
    } while (rep < reps);
    return r0 ^ r1 ^ r2 ^ r3 ^ r4 ^ r5 ^ r6 ^ r7;
}

int main(int argc, char **argv) {
    if (argc < 2)
        return 2;
    const char *k = argv[1];
    uint64_t v;
    if (!strcmp(k, "loop")) v = k_loop(200000000);
    else if (!strcmp(k, "fib")) v = fib(35);
    else if (!strcmp(k, "sieve")) v = k_sieve(4000);
    else if (!strcmp(k, "mandel")) v = k_mandel(600);
    else if (!strcmp(k, "poly")) v = k_poly(100000);
    else if (!strcmp(k, "nop")) v = 0;
    else return 2;
    printf("%s %llu\n", k, (unsigned long long)v);
    return 0;
}
