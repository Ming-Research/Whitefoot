/* Arm W7 of README.md, the C model of the planned sharded state
 * (research/investigations/shared-counters/DESIGN.md, sections 8.5, 8.6 and
 * 12): one allocation holding a header on a cache line of its own, then one
 * shard per line, each shard a first-come (ticket) lock beside a 64-bit
 * count. THREADS writer threads stand for the drivers: writer i is the
 * thread whose map-user number is i, and it adds 1 to shard `i mod len` in a
 * loop, taking and releasing that shard's lock around each add. The number of
 * shards `len` is twice the CPUs the process may run on, the number of
 * threads that may run contexts on a multi-driver target (section 12); the
 * shards above the writers' numbers stand for the spare threads and are only
 * locked by the reader.
 *
 * With READER_CPU given, one more thread, pinned to that CPU, which must lie
 * outside the process's CPU list, reads the total once per millisecond as a
 * whole target does: it takes every shard's lock in index order, sums the
 * counts, releases every lock, and times that whole read.
 *
 * The structure otherwise follows w5.c and the Whitefoot harness: a start
 * gate carrying the common deadline, batches of 1024 operations with one
 * local linear congruential step each, one clock reading per batch, the
 * thread's count added to the totals once, and one output line in the
 * harness's format with the reader's statistics appended.
 *
 *   w7 THREADS MILLISECONDS [READER_CPU]
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
#define LINE 64

/* A shard: its ticket lock and its count on one cache line. `next` is the
 * ticket the next taker draws, `serving` the ticket now admitted; `value` is
 * read and written only under the lock. */
typedef struct {
    _Alignas(LINE) _Atomic uint32_t next;
    _Atomic uint32_t serving;
    uint64_t value;
} shard_t;
_Static_assert(sizeof(shard_t) == LINE, "a shard fills exactly one cache line");

/* The header line: the handle count and the number of shards. A shard update
 * only reads `len`; nothing in this program writes the header after it is
 * made, as the planned shard path writes neither the handle count nor a
 * holder count (DESIGN.md section 8.5). */
typedef struct {
    _Alignas(LINE) uint64_t handles;
    uint64_t len;
} header_t;
_Static_assert(sizeof(header_t) == LINE, "the header fills exactly one cache line");

static header_t *object;

static _Alignas(LINE) _Atomic int gate_open;
static uint64_t gate_deadline;
static _Atomic uint64_t total_ops;
static _Atomic uint64_t total_check;

/* The running thread's map-user number, as the runtime keeps it. */
static _Thread_local uint64_t user_number;

typedef struct {
    uint64_t index;
    pthread_t thread;
} worker_t;

typedef struct {
    pthread_t thread;
    int cpu;
    int cpu_seen;
    int went_backwards;
    uint64_t reads;
    uint64_t read_ns;
    uint64_t read_max_ns;
    uint64_t capacity;
    uint64_t *durations;
} reader_t;

static uint64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000u + (uint64_t)t.tv_nsec;
}

static inline void spin_pause(void) {
#if defined(__x86_64__) || defined(__i386__)
    __builtin_ia32_pause();
#elif defined(__aarch64__)
    __asm__ __volatile__("yield");
#endif
}

static inline shard_t *first_shard(header_t *header) {
    return (shard_t *)(header + 1);
}

/* The shard a shard target selects: `user mod len`. `user` is below `len`
 * whenever the threads are the target's threads, so the division is skipped
 * on that path, as generated code can. */
static inline shard_t *shard_of(header_t *header, uint64_t user) {
    uint64_t len = header->len;
    return first_shard(header) + (user < len ? user : user % len);
}

static inline uint32_t lock_shard(shard_t *shard) {
    uint32_t ticket = atomic_fetch_add_explicit(&shard->next, 1, memory_order_acquire);
    while (atomic_load_explicit(&shard->serving, memory_order_acquire) != ticket) {
        spin_pause();
    }
    return ticket;
}

static inline void unlock_shard(shard_t *shard, uint32_t ticket) {
    atomic_store_explicit(&shard->serving, ticket + 1, memory_order_release);
}

static void wait_for_gate(void) {
    while (!atomic_load_explicit(&gate_open, memory_order_acquire)) {
        sched_yield();
    }
}

static void *write_loop(void *argument) {
    worker_t *self = argument;
    user_number = self->index;
    header_t *handle = object;
    wait_for_gate();
    uint64_t deadline = gate_deadline;
    uint64_t state = self->index;
    uint64_t done = 0;
    for (;;) {
        for (int turn = 0; turn < BATCH; turn++) {
            shard_t *mine = shard_of(handle, user_number);
            uint32_t ticket = lock_shard(mine);
            uint64_t seen = mine->value;
            mine->value = seen + 1;
            unlock_shard(mine, ticket);
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

static void *read_loop(void *argument) {
    reader_t *self = argument;
    header_t *handle = object;
    uint64_t len = handle->len;
    uint32_t *tickets = calloc(len, sizeof *tickets);
    if (tickets == NULL) {
        fprintf(stderr, "w7: reader out of memory\n");
        exit(2);
    }
    self->cpu_seen = sched_getcpu();
    wait_for_gate();
    uint64_t deadline = gate_deadline;
    uint64_t last = 0;
    struct timespec next;
    clock_gettime(CLOCK_MONOTONIC, &next);
    for (;;) {
        /* The next whole read starts one millisecond after the previous
         * one was due; a reader more than a period late starts its period
         * again from now instead of reading in a burst. */
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
        shard_t *shards = first_shard(handle);
        for (uint64_t i = 0; i < len; i++) {
            tickets[i] = lock_shard(&shards[i]);
        }
        uint64_t sum = 0;
        for (uint64_t i = 0; i < len; i++) {
            sum += shards[i].value;
        }
        for (uint64_t i = 0; i < len; i++) {
            unlock_shard(&shards[i], tickets[i]);
        }
        uint64_t took = now_ns() - started;
        if (sum < last) {
            self->went_backwards = 1;
        }
        last = sum;
        if (self->reads < self->capacity) {
            self->durations[self->reads] = took;
        }
        self->reads += 1;
        self->read_ns += took;
        if (took > self->read_max_ns) {
            self->read_max_ns = took;
        }
    }
    free(tickets);
    return NULL;
}

static int ascending(const void *a, const void *b) {
    uint64_t x = *(const uint64_t *)a;
    uint64_t y = *(const uint64_t *)b;
    return (x > y) - (x < y);
}

static unsigned long number(const char *text, int zero_allowed) {
    char *end = NULL;
    unsigned long value = strtoul(text, &end, 10);
    if (end == text || *end != '\0' || (value == 0 && !zero_allowed)) {
        fprintf(stderr, "usage: w7 THREADS MILLISECONDS [READER_CPU]\n");
        exit(2);
    }
    return value;
}

int main(int argc, char **argv) {
    if (argc != 3 && argc != 4) {
        fprintf(stderr, "usage: w7 THREADS MILLISECONDS [READER_CPU]\n");
        return 2;
    }
    unsigned long threads = number(argv[1], 0);
    unsigned long millis = number(argv[2], 0);
    int with_reader = argc == 4;
    reader_t reader;
    memset(&reader, 0, sizeof reader);
    unsigned long reader_cpu = with_reader ? number(argv[3], 1) : 0;
    if (reader_cpu >= CPU_SETSIZE) {
        fprintf(stderr, "w7: the reader's CPU %lu is beyond %d\n", reader_cpu, CPU_SETSIZE);
        return 2;
    }
    reader.cpu = with_reader ? (int)reader_cpu : -1;

    cpu_set_t allowed;
    CPU_ZERO(&allowed);
    sched_getaffinity(0, sizeof allowed, &allowed);
    int drivers = CPU_COUNT(&allowed);
    uint64_t len = 2u * (uint64_t)drivers;
    if (threads > len) {
        fprintf(stderr, "w7: %lu writers exceed the %llu shards of %d CPUs\n",
                threads, (unsigned long long)len, drivers);
        return 2;
    }
    if (with_reader && CPU_ISSET(reader.cpu, &allowed)) {
        fprintf(stderr, "w7: the reader's CPU %d must lie outside the writers' CPU list\n", reader.cpu);
        return 2;
    }

    size_t bytes = sizeof(header_t) + (size_t)len * sizeof(shard_t);
    object = aligned_alloc(LINE, bytes);
    if (object == NULL) {
        return 2;
    }
    memset(object, 0, bytes);
    object->handles = 1;
    object->len = len;
    for (uint64_t i = 0; i < len; i++) {
        atomic_init(&first_shard(object)[i].next, 0);
        atomic_init(&first_shard(object)[i].serving, 0);
        first_shard(object)[i].value = 0;
    }

    worker_t *workers = calloc(threads, sizeof *workers);
    if (workers == NULL) {
        return 2;
    }
    for (unsigned long i = 0; i < threads; i++) {
        workers[i].index = i;
        if (pthread_create(&workers[i].thread, NULL, write_loop, &workers[i]) != 0) {
            return 2;
        }
    }
    if (with_reader) {
        reader.capacity = millis + 1000u;
        reader.durations = calloc(reader.capacity, sizeof *reader.durations);
        if (reader.durations == NULL) {
            return 2;
        }
        cpu_set_t one;
        CPU_ZERO(&one);
        CPU_SET(reader.cpu, &one);
        pthread_attr_t attributes;
        pthread_attr_init(&attributes);
        if (pthread_attr_setaffinity_np(&attributes, sizeof one, &one) != 0 ||
            pthread_create(&reader.thread, &attributes, read_loop, &reader) != 0) {
            fprintf(stderr, "w7: cannot start the reader on CPU %d\n", reader.cpu);
            return 2;
        }
        pthread_attr_destroy(&attributes);
    }
    uint64_t started = now_ns();
    gate_deadline = started + (uint64_t)millis * 1000000u;
    atomic_store_explicit(&gate_open, 1, memory_order_release);
    for (unsigned long i = 0; i < threads; i++) {
        pthread_join(workers[i].thread, NULL);
    }
    uint64_t ended = now_ns();
    if (with_reader) {
        pthread_join(reader.thread, NULL);
    }

    uint64_t ops = atomic_load(&total_ops);
    uint64_t check = atomic_load(&total_check);
    uint64_t elapsed = ended - started;
    uint64_t micros = elapsed / 1000u;

    /* Every update is one add under its shard's lock, so the shards' counts
     * sum to the writers' operations exactly. */
    uint64_t sum = 0;
    for (uint64_t i = 0; i < len; i++) {
        sum += first_shard(object)[i].value;
    }
    int failed = 0;
    if (sum != ops) {
        fprintf(stderr, "w7: the shards hold %llu where the writers did %llu\n",
                (unsigned long long)sum, (unsigned long long)ops);
        failed = 1;
    }
    if (with_reader && reader.went_backwards) {
        fprintf(stderr, "w7: a whole read saw a smaller total than the read before it\n");
        failed = 1;
    }
    if (with_reader && reader.reads == 0) {
        fprintf(stderr, "w7: the reader made no whole read\n");
        failed = 1;
    }
    if (with_reader && reader.cpu_seen != reader.cpu) {
        fprintf(stderr, "w7: the reader ran on CPU %d, not %d\n", reader.cpu_seen, reader.cpu);
        failed = 1;
    }

    uint64_t median = 0;
    uint64_t p99 = 0;
    uint64_t kept = reader.reads < reader.capacity ? reader.reads : reader.capacity;
    if (kept > 0) {
        qsort(reader.durations, kept, sizeof *reader.durations, ascending);
        median = kept % 2 ? reader.durations[kept / 2]
                          : (reader.durations[kept / 2 - 1] + reader.durations[kept / 2]) / 2;
        p99 = reader.durations[(kept * 99 + 99) / 100 - 1];
    }

    printf("witness=w7 contexts=%lu drivers=%d shards=%llu ops=%llu ns=%llu ops_per_s=",
           threads, drivers, (unsigned long long)len,
           (unsigned long long)ops, (unsigned long long)elapsed);
    if (micros > 0 && ops <= 18000000000000u) {
        printf("%llu", (unsigned long long)(ops * 1000000u / micros));
    } else {
        printf("?");
    }
    printf(" check=%llu reads=%llu read_ns=%llu read_max_ns=%llu read_med_ns=%llu read_p99_ns=%llu reader_cpu=%d\n",
           (unsigned long long)check, (unsigned long long)reader.reads,
           (unsigned long long)reader.read_ns, (unsigned long long)reader.read_max_ns,
           (unsigned long long)median, (unsigned long long)p99, reader.cpu);
    return failed;
}
