/* The C reference for the fixed cost of an uncontended atomic statement
 * (research/investigations/driver-handoff/DESIGN.md, "Plan for the 14900K
 * measurements", item 3): one thread does the per-operation work of
 * shared-take-overhead's `own` program, a counter read into a local number,
 * the counter incremented and one step of the same linear congruential
 * sequence, in batches of 1024 until MILLIS have passed, either with the
 * counter plain or with a spin lock taken and released around each
 * operation, as an uncontended lock costs at least.
 *
 *   c_reference plain|locked MILLIS
 *
 * Prints the `own` program's line format, witness=c_plain or c_locked
 * contexts=1 drivers=1 ops=N ns=T ops_per_s=R check=C wrong=W, with wrong=1
 * when the counter ended at another value than the operation count. */
#define _POSIX_C_SOURCE 200809L
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static uint64_t now_ns(void) {
    struct timespec at;
    clock_gettime(CLOCK_MONOTONIC, &at);
    return (uint64_t)at.tv_sec * UINT64_C(1000000000) + (uint64_t)at.tv_nsec;
}

static uint64_t step(uint64_t state) {
    return state * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
}

/* The counter is volatile so that the loop reads and writes it in memory on
 * every operation, as the atomic statement does its object's state. */
static volatile uint64_t counter;
static atomic_flag lock = ATOMIC_FLAG_INIT;

int main(int argc, char **argv) {
    if (argc != 3 || (strcmp(argv[1], "plain") != 0 && strcmp(argv[1], "locked") != 0)) {
        fprintf(stderr, "usage: c_reference plain|locked MILLIS\n");
        return 2;
    }
    int locked = strcmp(argv[1], "locked") == 0;
    uint64_t millis = strtoull(argv[2], NULL, 10);
    uint64_t start = now_ns(), deadline = start + millis * UINT64_C(1000000);
    uint64_t state = 0, done = 0, at;
    do {
        for (unsigned turn = 0; turn < 1024u; ++turn) {
            if (locked) {
                while (atomic_flag_test_and_set_explicit(&lock, memory_order_acquire)) {
                }
            }
            state ^= counter;
            counter = counter + 1u;
            if (locked) atomic_flag_clear_explicit(&lock, memory_order_release);
            state = step(state);
        }
        done += 1024u;
        at = now_ns();
    } while (at < deadline);
    uint64_t ns = at - start;
    printf("witness=c_%s contexts=1 drivers=1 ops=%llu ns=%llu ops_per_s=%.0f check=%llu wrong=%d\n",
           locked ? "locked" : "plain", (unsigned long long)done, (unsigned long long)ns,
           (double)done / ((double)ns / 1e9), (unsigned long long)state, counter != done);
    return 0;
}
