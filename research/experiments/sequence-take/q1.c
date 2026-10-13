/* Q1 of research/investigations/sequence-take/DESIGN.md section 7: arms S0,
 * S1, S2, S3 and S4, one thread per CPU the process may run on, each thread
 * pinned to its own CPU. An operation is STEPS dependent steps of a linear
 * congruential sequence on the thread's own state (kept on the thread's own
 * 64-byte line and folded into the check value, so no step can be dropped),
 * followed by the arm's shared-memory part:
 *
 *   S0  nothing
 *   S1  one sequentially consistent fetch-and-add on the one shared line
 *   S2  one fetch-and-add on the thread's own line
 *   S3  take a test-and-set lock on the thread's own line, the work, the
 *       fetch-and-add on the shared line, release the lock
 *   S4  one load of a shared line that no thread writes after start, and a
 *       compare that never succeeds
 *
 * The harness follows the counter study's w5.c: a start gate carrying the
 * common deadline, batches of 1024 operations, one clock reading per batch,
 * totals added once, one output line per run.
 *
 *   q1 ARM MILLISECONDS STEPS      the timed run, ARM one of S0 S1 S2 S3 S4
 *   q1 calibrate TARGET_NS         one thread, S0: prints the STEPS that make
 *                                  one operation take about TARGET_NS
 */
#define _GNU_SOURCE
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define BATCH 1024
#define LCG_A 6364136223846793005u
#define LCG_C 1442695040888963407u
#define RAISE_LIMIT (1u << 20)

typedef struct {
    _Alignas(64) _Atomic uint64_t value;
    _Atomic uint64_t lock;
    uint64_t state;
} line_t;

typedef enum { S0, S1, S2, S3, S4 } arm_t;

static line_t shared_line;
static line_t stable_line;
static line_t *lines;

static _Alignas(64) _Atomic int gate_open;
static uint64_t gate_deadline;
static _Atomic uint64_t total_ops;
static _Atomic uint64_t total_check;

static arm_t arm;
static uint64_t steps;

typedef struct {
    uint64_t index;
    int cpu;
    pthread_t thread;
} worker_t;

static uint64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000u + (uint64_t)t.tv_nsec;
}

static inline uint64_t work(uint64_t state, uint64_t count) {
    for (uint64_t k = 0; k < count; k++) {
        state = state * LCG_A + LCG_C;
    }
    return state;
}

static void *run(void *argument) {
    worker_t *self = argument;
    cpu_set_t one;
    CPU_ZERO(&one);
    CPU_SET(self->cpu, &one);
    pthread_setaffinity_np(pthread_self(), sizeof one, &one);
    line_t *mine = &lines[self->index];
    uint64_t count = steps;
    mine->state = self->index + 1;
    while (!atomic_load_explicit(&gate_open, memory_order_acquire)) {
        sched_yield();
    }
    uint64_t deadline = gate_deadline;
    uint64_t state = mine->state;
    uint64_t check = 0;
    uint64_t done = 0;
    for (;;) {
        for (int turn = 0; turn < BATCH; turn++) {
            switch (arm) {
            case S0:
                state = work(state, count);
                mine->state = state;
                break;
            case S1:
                state = work(state, count);
                mine->state = state;
                check += atomic_fetch_add_explicit(&shared_line.value, 1, memory_order_seq_cst);
                break;
            case S2:
                state = work(state, count);
                mine->state = state;
                check += atomic_fetch_add_explicit(&mine->value, 1, memory_order_seq_cst);
                break;
            case S3:
                while (atomic_exchange_explicit(&mine->lock, 1, memory_order_acquire) != 0) {
                }
                state = work(state, count);
                mine->state = state;
                check += atomic_fetch_add_explicit(&shared_line.value, 1, memory_order_seq_cst);
                atomic_store_explicit(&mine->lock, 0, memory_order_release);
                break;
            case S4: {
                state = work(state, count);
                mine->state = state;
                uint64_t seen = atomic_load_explicit(&stable_line.value, memory_order_acquire);
                check += seen < (state & (RAISE_LIMIT - 1));
                break;
            }
            }
        }
        done += BATCH;
        if (now_ns() >= deadline) {
            break;
        }
    }
    atomic_fetch_add_explicit(&total_ops, done, memory_order_relaxed);
    atomic_fetch_xor_explicit(&total_check, state ^ check, memory_order_relaxed);
    return NULL;
}

static unsigned long number(const char *text) {
    char *end = NULL;
    unsigned long value = strtoul(text, &end, 10);
    if (end == text || *end != '\0' || value == 0) {
        fprintf(stderr, "usage: q1 ARM MILLISECONDS STEPS | q1 calibrate TARGET_NS\n");
        exit(2);
    }
    return value;
}

static int arm_of(const char *name) {
    static const char *names[] = {"S0", "S1", "S2", "S3", "S4"};
    for (int i = 0; i < 5; i++) {
        if (strcmp(name, names[i]) == 0) {
            return i;
        }
    }
    fprintf(stderr, "unknown arm %s\n", name);
    exit(2);
}

static int cpus_of(int *list) {
    cpu_set_t allowed;
    CPU_ZERO(&allowed);
    sched_getaffinity(0, sizeof allowed, &allowed);
    int n = 0;
    for (int c = 0; c < CPU_SETSIZE; c++) {
        if (CPU_ISSET(c, &allowed)) {
            list[n++] = c;
        }
    }
    return n;
}

static int timed(unsigned long millis, uint64_t *ops_out, uint64_t *ns_out, uint64_t *check_out, int *drivers_out) {
    int cpu_list[CPU_SETSIZE];
    int threads = cpus_of(cpu_list);
    *drivers_out = threads;
    lines = aligned_alloc(64, (size_t)threads * sizeof(line_t));
    if (lines == NULL) {
        return 2;
    }
    memset(lines, 0, (size_t)threads * sizeof(line_t));
    atomic_init(&shared_line.value, 0);
    atomic_init(&stable_line.value, RAISE_LIMIT);
    atomic_store(&gate_open, 0);
    atomic_store(&total_ops, 0);
    atomic_store(&total_check, 0);
    worker_t *workers = calloc((size_t)threads, sizeof *workers);
    if (workers == NULL) {
        return 2;
    }
    for (int i = 0; i < threads; i++) {
        workers[i].index = (uint64_t)i;
        workers[i].cpu = cpu_list[i];
        if (pthread_create(&workers[i].thread, NULL, run, &workers[i]) != 0) {
            return 2;
        }
    }
    uint64_t started = now_ns();
    gate_deadline = started + (uint64_t)millis * 1000000u;
    atomic_store_explicit(&gate_open, 1, memory_order_release);
    for (int i = 0; i < threads; i++) {
        pthread_join(workers[i].thread, NULL);
    }
    uint64_t ended = now_ns();
    *ops_out = atomic_load(&total_ops);
    *check_out = atomic_load(&total_check);
    *ns_out = ended - started;
    free(workers);
    free(lines);
    return 0;
}

static double ns_per_op(unsigned long millis, uint64_t count) {
    steps = count;
    arm = S0;
    uint64_t ops, ns, check;
    int drivers;
    if (timed(millis, &ops, &ns, &check, &drivers) != 0 || ops == 0) {
        exit(2);
    }
    if (check == 1) {
        fprintf(stderr, "impossible check\n");
    }
    return (double)ns / (double)ops;
}

static int calibrate(unsigned long target) {
    double base = ns_per_op(100, 1000);
    double per_step = (base - 0.0) / 1000.0;
    uint64_t guess = (uint64_t)((double)target / per_step + 0.5);
    if (guess == 0) {
        guess = 1;
    }
    double seen = ns_per_op(200, guess);
    uint64_t adjusted = (uint64_t)((double)guess * (double)target / seen + 0.5);
    if (adjusted == 0) {
        adjusted = 1;
    }
    double final = ns_per_op(300, adjusted);
    printf("target_ns=%lu steps=%llu ns_per_op=%.2f\n", target, (unsigned long long)adjusted, final);
    return 0;
}

int main(int argc, char **argv) {
    if (argc == 3 && strcmp(argv[1], "calibrate") == 0) {
        cpu_set_t allowed;
        CPU_ZERO(&allowed);
        sched_getaffinity(0, sizeof allowed, &allowed);
        if (CPU_COUNT(&allowed) != 1) {
            fprintf(stderr, "calibrate runs on exactly one CPU (use taskset -c 0)\n");
            return 2;
        }
        return calibrate(number(argv[2]));
    }
    if (argc != 4) {
        fprintf(stderr, "usage: q1 ARM MILLISECONDS STEPS | q1 calibrate TARGET_NS\n");
        return 2;
    }
    arm = (arm_t)arm_of(argv[1]);
    unsigned long millis = number(argv[2]);
    steps = number(argv[3]);
    uint64_t ops, ns, check;
    int drivers;
    int status = timed(millis, &ops, &ns, &check, &drivers);
    if (status != 0) {
        return status;
    }
    uint64_t micros = ns / 1000u;
    printf("witness=%s contexts=%d drivers=%d ops=%llu ns=%llu ops_per_s=", argv[1], drivers, drivers,
           (unsigned long long)ops, (unsigned long long)ns);
    if (micros > 0 && ops <= 18000000000000u) {
        printf("%llu", (unsigned long long)(ops * 1000000u / micros));
    } else {
        printf("?");
    }
    printf(" check=%llu\n", (unsigned long long)check);
    return 0;
}
