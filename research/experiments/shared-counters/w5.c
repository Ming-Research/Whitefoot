/* Calibration arms W5 and W5p of README.md: threads that increment one
 * 64-byte-aligned 64-bit counter with a sequentially consistent
 * atomic_fetch_add (W5), or each its own counter on a line of its own (W5p,
 * built with -DPADDED=1), for a number of milliseconds. The structure follows
 * the Whitefoot harness (harness.wf): a start gate that carries the common
 * deadline, batches of 1024 operations with one local linear congruential
 * step each, one clock reading per batch, one add of the thread's count to
 * the totals, and one line of output in the harness's format.
 *
 *   w5 THREADS MILLISECONDS
 */
#define _GNU_SOURCE
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#ifndef PADDED
#define PADDED 0
#endif

#define BATCH 1024

typedef struct {
    _Alignas(64) _Atomic uint64_t value;
} line_t;

static line_t shared_line;
static line_t *lines;

static _Alignas(64) _Atomic int gate_open;
static uint64_t gate_deadline;
static _Atomic uint64_t total_ops;
static _Atomic uint64_t total_check;

typedef struct {
    uint64_t index;
    pthread_t thread;
} worker_t;

static uint64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000u + (uint64_t)t.tv_nsec;
}

static void *run(void *argument) {
    worker_t *self = argument;
    while (!atomic_load_explicit(&gate_open, memory_order_acquire)) {
        sched_yield();
    }
    uint64_t deadline = gate_deadline;
    line_t *mine = PADDED ? &lines[self->index] : &shared_line;
    uint64_t state = self->index;
    uint64_t done = 0;
    for (;;) {
        for (int turn = 0; turn < BATCH; turn++) {
            uint64_t seen = atomic_fetch_add_explicit(&mine->value, 1, memory_order_seq_cst);
            state ^= seen;
            state = state * 6364136223846793005u + 1442695040888963407u;
        }
        done += BATCH;
        if (now_ns() >= deadline) {
            break;
        }
    }
    atomic_fetch_add_explicit(&total_ops, done, memory_order_relaxed);
    atomic_fetch_xor_explicit(&total_check, state, memory_order_relaxed);
    return NULL;
}

static unsigned long number(const char *text) {
    char *end = NULL;
    unsigned long value = strtoul(text, &end, 10);
    if (end == text || *end != '\0' || value == 0) {
        fprintf(stderr, "usage: w5 THREADS MILLISECONDS\n");
        exit(2);
    }
    return value;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: w5 THREADS MILLISECONDS\n");
        return 2;
    }
    unsigned long threads = number(argv[1]);
    unsigned long millis = number(argv[2]);
    cpu_set_t allowed;
    CPU_ZERO(&allowed);
    sched_getaffinity(0, sizeof allowed, &allowed);
    int drivers = CPU_COUNT(&allowed);

    if (PADDED && posix_memalign((void **)&lines, 64, threads * sizeof(line_t)) != 0) {
        return 2;
    }
    for (unsigned long i = 0; PADDED && i < threads; i++) {
        atomic_init(&lines[i].value, 0);
    }
    atomic_init(&shared_line.value, 0);
    worker_t *workers = calloc(threads, sizeof *workers);
    if (workers == NULL) {
        return 2;
    }
    for (unsigned long i = 0; i < threads; i++) {
        workers[i].index = i;
        if (pthread_create(&workers[i].thread, NULL, run, &workers[i]) != 0) {
            return 2;
        }
    }
    uint64_t started = now_ns();
    gate_deadline = started + (uint64_t)millis * 1000000u;
    atomic_store_explicit(&gate_open, 1, memory_order_release);
    for (unsigned long i = 0; i < threads; i++) {
        pthread_join(workers[i].thread, NULL);
    }
    uint64_t ended = now_ns();

    uint64_t ops = atomic_load(&total_ops);
    uint64_t check = atomic_load(&total_check);
    uint64_t elapsed = ended - started;
    uint64_t micros = elapsed / 1000u;
    printf("witness=%s contexts=%lu drivers=%d ops=%llu ns=%llu ops_per_s=",
           PADDED ? "w5p" : "w5", threads, drivers,
           (unsigned long long)ops, (unsigned long long)elapsed);
    if (micros > 0 && ops <= 18000000000000u) {
        printf("%llu", (unsigned long long)(ops * 1000000u / micros));
    } else {
        printf("?");
    }
    printf(" check=%llu reads=0 read_ns=0 read_max_ns=0\n", (unsigned long long)check);
    return 0;
}
