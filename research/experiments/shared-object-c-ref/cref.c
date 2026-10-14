/* C reference programs for the one-object redesign
 * (research/investigations/shared-object-model/ONE-OBJECT.md, sections 3, 4, 12).
 *
 *   cref ARM CPULIST MILLISECONDS [READER_CPU]
 *
 * One thread per CPU in CPULIST (for example "0,2,4,6" or "0-15"), each pinned
 * with a thread affinity, a start gate carrying the common deadline, batches of
 * 1024 operations with one clock reading per batch, one output line.
 *
 * Arms: A1 A1t perthread-faa (A1t is the twin), A2 shared-faa,
 *   A4 shard-asym (READER_CPU given: with the membarrier reader),
 *   A5a..A5e layout, A6sl A6sf A6sx A6sc A6pl A6pf A6px A6pc atomic ops
 *   (s shared, p per thread; l load, f fetch_add, x exchange, c CAS loop),
 *   A7n A7s seq-structure without and with a global sequence, A8 timedwait.
 * A3 is w7.c, run by run.sh.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <linux/futex.h>
#include <linux/membarrier.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

#define BATCH 1024
#define MAXT 64
#define LINE 64

typedef struct {
    _Alignas(LINE) _Atomic uint64_t v;
} line_t;
_Static_assert(sizeof(line_t) == LINE, "line_t is one line");

typedef struct {
    int index;
    int cpu;
    pthread_t thread;
    uint64_t ops;
    uint64_t check;
    uint64_t *lat;
    uint64_t nlat;
} worker_t;

static _Alignas(LINE) _Atomic int gate_open;
static uint64_t gate_deadline;
static int nthreads;
static int cpu_list[MAXT];
static worker_t workers[MAXT];
static unsigned long millis;

#define COMPILER_BARRIER() __asm__ __volatile__("" ::: "memory")

static uint64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000u + (uint64_t)t.tv_nsec;
}

static inline void spin_pause(void) { __builtin_ia32_pause(); }

static void wait_for_gate(void) {
    while (!atomic_load_explicit(&gate_open, memory_order_acquire)) {
        sched_yield();
    }
}

static void die(const char *message) {
    fprintf(stderr, "cref: %s\n", message);
    exit(2);
}

#define STEP(s) ((s) = (s) * 6364136223846793005u + 1442695040888963407u)

#define BEGIN                                                                  \
    worker_t *self = arg;                                                      \
    uint64_t state = (uint64_t)self->index + 1;                                \
    uint64_t done = 0;                                                         \
    wait_for_gate();                                                           \
    uint64_t deadline = gate_deadline;
#define LOOP_OPEN                                                              \
    for (;;) {                                                                 \
        for (int turn = 0; turn < BATCH; turn++) {
#define LOOP_CLOSE                                                             \
        }                                                                      \
        done += BATCH;                                                         \
        if (now_ns() >= deadline) {                                            \
            break;                                                             \
        }                                                                      \
    }                                                                          \
    self->ops = done;                                                          \
    self->check = state;                                                       \
    return NULL;

/* ---- A1, A2, A6: atomic operations ---- */

static line_t *lines;
static line_t *shared_line;

static void *run_a1(void *arg) {
    BEGIN
    _Atomic uint64_t *p = &lines[self->index].v;
    LOOP_OPEN
        atomic_fetch_add(p, 1);
        STEP(state);
    LOOP_CLOSE
}

static void *run_a2(void *arg) {
    BEGIN
    _Atomic uint64_t *p = &shared_line->v;
    LOOP_OPEN
        atomic_fetch_add(p, 1);
        STEP(state);
    LOOP_CLOSE
}

#define A6FN(NAME, TARGET, OPBODY)                                             \
    static void *NAME(void *arg) {                                             \
        BEGIN                                                                  \
        _Atomic uint64_t *p = TARGET;                                          \
        LOOP_OPEN                                                              \
            OPBODY                                                             \
            STEP(state);                                                       \
        LOOP_CLOSE                                                             \
    }

#define OP_LOAD { uint64_t x = atomic_load(p); state ^= x; }
#define OP_FAA { atomic_fetch_add(p, 1); }
#define OP_XCHG { uint64_t x = atomic_exchange(p, state); state ^= x; }
#define OP_CAS { uint64_t o = atomic_load_explicit(p, memory_order_relaxed); \
                 while (!atomic_compare_exchange_weak(p, &o, o + 1)) { } }

A6FN(run_a6sl, &shared_line->v, OP_LOAD)
A6FN(run_a6sf, &shared_line->v, OP_FAA)
A6FN(run_a6sx, &shared_line->v, OP_XCHG)
A6FN(run_a6sc, &shared_line->v, OP_CAS)
A6FN(run_a6pl, &lines[self->index].v, OP_LOAD)
A6FN(run_a6pf, &lines[self->index].v, OP_FAA)
A6FN(run_a6px, &lines[self->index].v, OP_XCHG)
A6FN(run_a6pc, &lines[self->index].v, OP_CAS)

/* ---- A4: sharded counter, writer without read-modify-write ---- */

typedef struct {
    _Alignas(LINE) _Atomic uint32_t in_cs;
    uint64_t value;
} slot_t;
_Static_assert(sizeof(slot_t) == LINE, "slot_t is one line");

static slot_t *slots;
static _Alignas(LINE) _Atomic uint32_t reader_wants;

static void *run_a4(void *arg) {
    BEGIN
    slot_t *s = &slots[self->index];
    LOOP_OPEN
        for (;;) {
            atomic_store_explicit(&s->in_cs, 1, memory_order_relaxed);
            COMPILER_BARRIER();
            if (atomic_load_explicit(&reader_wants, memory_order_relaxed)) {
                atomic_store_explicit(&s->in_cs, 0, memory_order_release);
                while (atomic_load_explicit(&reader_wants, memory_order_acquire)) {
                    spin_pause();
                }
                continue;
            }
            break;
        }
        uint64_t seen = s->value;
        s->value = seen + 1;
        COMPILER_BARRIER();
        atomic_store_explicit(&s->in_cs, 0, memory_order_release);
        state ^= seen;
        STEP(state);
    LOOP_CLOSE
}

typedef struct {
    pthread_t thread;
    int cpu;
    int cpu_seen;
    int went_backwards;
    int failed;
    uint64_t reads;
    uint64_t capacity;
    uint64_t *durations;
} reader_t;

static void *a4_reader(void *arg) {
    reader_t *self = arg;
    self->cpu_seen = sched_getcpu();
    wait_for_gate();
    uint64_t deadline = gate_deadline;
    uint64_t last = 0;
    struct timespec next;
    clock_gettime(CLOCK_MONOTONIC, &next);
    for (;;) {
        next.tv_nsec += 1000000;
        if (next.tv_nsec >= 1000000000) {
            next.tv_nsec -= 1000000000;
            next.tv_sec += 1;
        }
        clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &next, NULL);
        uint64_t started = now_ns();
        if (started >= deadline) {
            break;
        }
        uint64_t due = (uint64_t)next.tv_sec * 1000000000u + (uint64_t)next.tv_nsec;
        if (started > due + 1000000u) {
            next.tv_sec = (time_t)(started / 1000000000u);
            next.tv_nsec = (long)(started % 1000000000u);
        }
        atomic_store_explicit(&reader_wants, 1, memory_order_relaxed);
        if (syscall(SYS_membarrier, MEMBARRIER_CMD_PRIVATE_EXPEDITED, 0, 0) != 0) {
            fprintf(stderr, "cref: membarrier failed: %s\n", strerror(errno));
            self->failed = 1;
            atomic_store_explicit(&reader_wants, 0, memory_order_release);
            break;
        }
        for (int i = 0; i < nthreads; i++) {
            while (atomic_load_explicit(&slots[i].in_cs, memory_order_acquire)) {
                spin_pause();
            }
        }
        uint64_t sum = 0;
        for (int i = 0; i < nthreads; i++) {
            sum += slots[i].value;
        }
        atomic_store_explicit(&reader_wants, 0, memory_order_release);
        uint64_t took = now_ns() - started;
        if (sum < last) {
            self->went_backwards = 1;
        }
        last = sum;
        if (self->reads < self->capacity) {
            self->durations[self->reads] = took;
        }
        self->reads += 1;
    }
    return NULL;
}

/* ---- A5: layout of parts in an object ---- */

typedef struct {
    _Atomic uint32_t lock;
    volatile uint64_t value;
} part_t;
_Static_assert(sizeof(part_t) == 16, "part_t is 16 bytes");

static part_t *parts[MAXT];

static void *run_a5(void *arg) {
    BEGIN
    part_t *p = parts[self->index];
    LOOP_OPEN
        while (atomic_exchange_explicit(&p->lock, 1, memory_order_acquire)) {
            spin_pause();
        }
        uint64_t seen = p->value;
        p->value = seen + 1;
        atomic_store_explicit(&p->lock, 0, memory_order_release);
        state ^= seen;
        STEP(state);
    LOOP_CLOSE
}

/* ---- A7: lock a table entry, optional global sequence, append to a ring ---- */

#define KEYS 100000
#define RING (1u << 20)

typedef struct {
    _Alignas(LINE) _Atomic uint32_t lock;
    uint64_t value;
} entry_t;
_Static_assert(sizeof(entry_t) == LINE, "entry_t is one line");

typedef struct {
    uint64_t n, key, value, pad[3];
} rec_t;
_Static_assert(sizeof(rec_t) == 48, "rec_t is 48 bytes");

static entry_t *table;
static uint8_t *rings[MAXT];
static _Alignas(LINE) _Atomic uint64_t global_seq;

#define A7FN(NAME, SEQ)                                                        \
    static void *NAME(void *arg) {                                             \
        BEGIN                                                                  \
        uint8_t *ring = rings[self->index];                                    \
        uint64_t pos = 0;                                                      \
        LOOP_OPEN                                                              \
            STEP(state);                                                       \
            uint64_t key = (state >> 33) % KEYS;                               \
            entry_t *e = &table[key];                                          \
            while (atomic_exchange_explicit(&e->lock, 1, memory_order_acquire)) { \
                spin_pause();                                                  \
            }                                                                  \
            e->value = state;                                                  \
            uint64_t n = 0;                                                    \
            if (SEQ) {                                                         \
                n = atomic_fetch_add(&global_seq, 1);                          \
            }                                                                  \
            rec_t r = {n, key, state, {0, 0, 0}};                              \
            memcpy(ring + pos, &r, sizeof r);                                  \
            pos += sizeof r;                                                   \
            if (pos + sizeof r > RING) {                                       \
                pos = 0;                                                       \
            }                                                                  \
            __asm__ __volatile__("" : : "r"(ring) : "memory");                 \
            atomic_store_explicit(&e->lock, 0, memory_order_release);          \
        LOOP_CLOSE                                                             \
    }
A7FN(run_a7n, 0)
A7FN(run_a7s, 1)

/* ---- A8: deadline wait ---- */

static volatile uint32_t futex_word;
#define LAT_CAP 20000

static void *run_a8(void *arg) {
    worker_t *self = arg;
    wait_for_gate();
    uint64_t deadline = gate_deadline;
    uint64_t n = 0;
    while (now_ns() < deadline) {
        uint64_t due = now_ns() + 1000000u;
        struct timespec ts;
        ts.tv_sec = (time_t)(due / 1000000000u);
        ts.tv_nsec = (long)(due % 1000000000u);
        syscall(SYS_futex, &futex_word, FUTEX_WAIT_BITSET | FUTEX_PRIVATE_FLAG, 0u, &ts, NULL,
                FUTEX_BITSET_MATCH_ANY);
        uint64_t t = now_ns();
        uint64_t late = t > due ? t - due : 0;
        if (n < LAT_CAP) {
            self->lat[n] = late;
        }
        n++;
    }
    self->ops = n;
    self->nlat = n < LAT_CAP ? n : LAT_CAP;
    return NULL;
}

/* ---- main ---- */

static int ascending(const void *a, const void *b) {
    uint64_t x = *(const uint64_t *)a;
    uint64_t y = *(const uint64_t *)b;
    return (x > y) - (x < y);
}

static void stats(uint64_t *v, uint64_t n, uint64_t *med, uint64_t *p99, uint64_t *max) {
    *med = *p99 = *max = 0;
    if (n == 0) {
        return;
    }
    qsort(v, n, sizeof *v, ascending);
    *med = n % 2 ? v[n / 2] : (v[n / 2 - 1] + v[n / 2]) / 2;
    *p99 = v[(n * 99 + 99) / 100 - 1];
    *max = v[n - 1];
}

static void parse_cpus(const char *text) {
    char *copy = strdup(text);
    for (char *tok = strtok(copy, ","); tok != NULL; tok = strtok(NULL, ",")) {
        int lo, hi;
        if (sscanf(tok, "%d-%d", &lo, &hi) == 2) {
        } else if (sscanf(tok, "%d", &lo) == 1) {
            hi = lo;
        } else {
            die("bad CPU list");
        }
        for (int c = lo; c <= hi; c++) {
            if (nthreads >= MAXT || c >= CPU_SETSIZE) {
                die("too many CPUs");
            }
            cpu_list[nthreads++] = c;
        }
    }
    free(copy);
}

static void *alloc_lines(size_t n) {
    void *p = aligned_alloc(LINE, n * LINE);
    if (p == NULL) {
        die("out of memory");
    }
    memset(p, 0, n * LINE);
    return p;
}

int main(int argc, char **argv) {
    if (argc != 4 && argc != 5) {
        die("usage: cref ARM CPULIST MILLISECONDS [READER_CPU]");
    }
    const char *arm = argv[1];
    parse_cpus(argv[2]);
    millis = strtoul(argv[3], NULL, 10);
    if (millis == 0) {
        die("bad milliseconds");
    }
    int reader_cpu = argc == 5 ? atoi(argv[4]) : -1;
    for (int i = 0; i < nthreads; i++) {
        if (cpu_list[i] == reader_cpu) {
            die("the reader's CPU lies in the writers' list");
        }
    }

    void *(*fn)(void *) = NULL;
    long stride = 0;
    int check_a1 = 0, check_a2 = 0, check_a4 = 0, check_a5 = 0;
    lines = alloc_lines(MAXT);
    shared_line = alloc_lines(1);

    if (!strcmp(arm, "A1") || !strcmp(arm, "A1t")) { fn = run_a1; check_a1 = 1; }
    else if (!strcmp(arm, "A2")) { fn = run_a2; check_a2 = 1; }
    else if (!strcmp(arm, "A6sl")) fn = run_a6sl;
    else if (!strcmp(arm, "A6sf")) fn = run_a6sf;
    else if (!strcmp(arm, "A6sx")) fn = run_a6sx;
    else if (!strcmp(arm, "A6sc")) fn = run_a6sc;
    else if (!strcmp(arm, "A6pl")) fn = run_a6pl;
    else if (!strcmp(arm, "A6pf")) fn = run_a6pf;
    else if (!strcmp(arm, "A6px")) fn = run_a6px;
    else if (!strcmp(arm, "A6pc")) fn = run_a6pc;
    else if (!strcmp(arm, "A4") || !strcmp(arm, "A4r")) {
        fn = run_a4;
        check_a4 = 1;
        slots = alloc_lines(MAXT);
    } else if (!strncmp(arm, "A5", 2) && strlen(arm) == 3 && arm[2] >= 'a' && arm[2] <= 'e') {
        fn = run_a5;
        check_a5 = 1;
        char v = arm[2];
        if (v == 'a') {
            for (int i = 0; i < nthreads; i++) {
                parts[i] = malloc(16);
                if (parts[i] == NULL) die("out of memory");
                memset(parts[i], 0, 16);
            }
        } else if (v == 'b') {
            for (int i = 0; i < nthreads; i++) {
                parts[i] = aligned_alloc(64, 64);
                if (parts[i] == NULL) die("out of memory");
                memset(parts[i], 0, 64);
            }
        } else {
            size_t gap = v == 'c' ? 16 : v == 'd' ? 64 : 128;
            size_t bytes = ((gap * (size_t)nthreads + 127) / 128) * 128;
            char *base = aligned_alloc(128, bytes);
            if (base == NULL) die("out of memory");
            memset(base, 0, bytes);
            for (int i = 0; i < nthreads; i++) {
                parts[i] = (part_t *)(base + gap * (size_t)i);
            }
        }
        stride = nthreads > 1 ? (long)((char *)parts[1] - (char *)parts[0]) : 0;
    } else if (!strcmp(arm, "A7n") || !strcmp(arm, "A7s")) {
        fn = !strcmp(arm, "A7n") ? run_a7n : run_a7s;
        table = alloc_lines(KEYS);
        for (int i = 0; i < nthreads; i++) {
            rings[i] = aligned_alloc(64, RING);
            if (rings[i] == NULL) die("out of memory");
            memset(rings[i], 0, RING);
        }
    } else if (!strcmp(arm, "A8")) {
        fn = run_a8;
        for (int i = 0; i < nthreads; i++) {
            workers[i].lat = calloc(LAT_CAP, sizeof(uint64_t));
            if (workers[i].lat == NULL) die("out of memory");
        }
    } else {
        die("unknown arm");
    }
    if (reader_cpu >= 0 && !check_a4) {
        die("only A4r takes a reader CPU");
    }

    reader_t reader;
    memset(&reader, 0, sizeof reader);
    reader.cpu = reader_cpu;
    if (reader_cpu >= 0) {
        if (syscall(SYS_membarrier, MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED, 0, 0) != 0) {
            fprintf(stderr, "cref: membarrier registration failed: %s\n", strerror(errno));
            return 2;
        }
    }

    for (int i = 0; i < nthreads; i++) {
        workers[i].index = i;
        workers[i].cpu = cpu_list[i];
        cpu_set_t one;
        CPU_ZERO(&one);
        CPU_SET(cpu_list[i], &one);
        pthread_attr_t attr;
        pthread_attr_init(&attr);
        if (pthread_attr_setaffinity_np(&attr, sizeof one, &one) != 0 ||
            pthread_create(&workers[i].thread, &attr, fn, &workers[i]) != 0) {
            die("cannot start a worker");
        }
        pthread_attr_destroy(&attr);
    }
    if (reader_cpu >= 0) {
        reader.capacity = millis + 1000u;
        reader.durations = calloc(reader.capacity, sizeof *reader.durations);
        if (reader.durations == NULL) die("out of memory");
        cpu_set_t one;
        CPU_ZERO(&one);
        CPU_SET(reader_cpu, &one);
        pthread_attr_t attr;
        pthread_attr_init(&attr);
        if (pthread_attr_setaffinity_np(&attr, sizeof one, &one) != 0 ||
            pthread_create(&reader.thread, &attr, a4_reader, &reader) != 0) {
            die("cannot start the reader");
        }
        pthread_attr_destroy(&attr);
    }

    uint64_t started = now_ns();
    gate_deadline = started + (uint64_t)millis * 1000000u;
    atomic_store_explicit(&gate_open, 1, memory_order_release);
    for (int i = 0; i < nthreads; i++) {
        pthread_join(workers[i].thread, NULL);
    }
    uint64_t ended = now_ns();
    if (reader_cpu >= 0) {
        pthread_join(reader.thread, NULL);
    }

    uint64_t ops = 0, check = 0;
    for (int i = 0; i < nthreads; i++) {
        ops += workers[i].ops;
        check ^= workers[i].check;
    }
    uint64_t elapsed = ended - started;
    int failed = 0;

    uint64_t sum = 0;
    if (check_a1) {
        for (int i = 0; i < nthreads; i++) sum += atomic_load(&lines[i].v);
        if (sum != ops) failed = 1;
    } else if (check_a2) {
        sum = atomic_load(&shared_line->v);
        if (sum != ops) failed = 1;
    } else if (check_a4) {
        for (int i = 0; i < nthreads; i++) sum += slots[i].value;
        if (sum != ops) failed = 1;
    } else if (check_a5) {
        for (int i = 0; i < nthreads; i++) sum += parts[i]->value;
        if (sum != ops) failed = 1;
    }
    if (failed) {
        fprintf(stderr, "cref: %s counted %llu where the writers did %llu\n", arm,
                (unsigned long long)sum, (unsigned long long)ops);
    }
    if (reader_cpu >= 0) {
        if (reader.went_backwards) { fprintf(stderr, "cref: a read went backwards\n"); failed = 1; }
        if (reader.failed) failed = 1;
        if (reader.reads == 0) { fprintf(stderr, "cref: the reader made no read\n"); failed = 1; }
        if (reader.cpu_seen != reader_cpu) { fprintf(stderr, "cref: the reader ran on CPU %d\n", reader.cpu_seen); failed = 1; }
    }

    printf("arm=%s cpus=%s threads=%d ms=%lu ops=%llu ns=%llu ops_per_s=%.0f check=%llu",
           arm, argv[2], nthreads, millis, (unsigned long long)ops, (unsigned long long)elapsed,
           elapsed ? (double)ops * 1e9 / (double)elapsed : 0.0, (unsigned long long)check);
    if (check_a5) {
        printf(" stride=%ld", stride);
    }
    if (reader_cpu >= 0) {
        uint64_t kept = reader.reads < reader.capacity ? reader.reads : reader.capacity;
        uint64_t med, p99, max;
        stats(reader.durations, kept, &med, &p99, &max);
        printf(" reads=%llu read_med_ns=%llu read_p99_ns=%llu read_max_ns=%llu",
               (unsigned long long)reader.reads, (unsigned long long)med,
               (unsigned long long)p99, (unsigned long long)max);
    }
    if (!strcmp(arm, "A8")) {
        uint64_t total = 0;
        for (int i = 0; i < nthreads; i++) total += workers[i].nlat;
        uint64_t *all = malloc((total ? total : 1) * sizeof *all);
        uint64_t k = 0;
        for (int i = 0; i < nthreads; i++) {
            memcpy(all + k, workers[i].lat, workers[i].nlat * sizeof *all);
            k += workers[i].nlat;
        }
        uint64_t med, p99, max;
        stats(all, total, &med, &p99, &max);
        printf(" waits=%llu lat_med_ns=%llu lat_p99_ns=%llu lat_max_ns=%llu",
               (unsigned long long)total, (unsigned long long)med,
               (unsigned long long)p99, (unsigned long long)max);
    }
    printf("\n");
    return failed;
}
