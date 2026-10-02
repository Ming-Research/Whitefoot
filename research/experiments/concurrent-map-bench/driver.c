/* Serves concurrent-map-bench: the one driver every native implementation is
 * linked with. It runs the measurement of
 * research/investigations/concurrent-map/DESIGN.md#the-measurement for one
 * implementation, one size and one key choice, and prints one CSV row per
 * cell and per check:
 *
 *   impl,flags,size,dist,mix,threads,cpus,mops,ops,seconds,check
 *
 *   bench-<prefix> --size N --dist uniform|zipf|one --threads 1,2,4
 *       [--mixes read,mostly-read,balanced,update,churn,grow]
 *       [--warmup-ms 200] [--duration-ms 1000] [--cpus 0,1,2,3]
 *       [--zipf FILE]
 */
#define _GNU_SOURCE
#include <errno.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include "cmap.h"
#include "workload.h"

#define MAX_THREADS 256
#define BATCH 256

enum { DIST_UNIFORM, DIST_ZIPF, DIST_ONE };
enum { MODE_RUN, MODE_FILL };

/* A mix: an operation is a get when its roll is below get, else an update
 * below update, else an insert below insert, else a remove. */
typedef struct {
    const char *name;
    unsigned id;
    uint32_t get, update, insert;
} mix_t;

static const mix_t MIXES[] = {
    {"read", 0, 100, 100, 100},
    {"mostly-read", 1, 95, 100, 100},
    {"balanced", 2, 50, 100, 100},
    {"update", 3, 0, 100, 100},
    {"churn", 4, 50, 50, 75},
    {"grow", 5, 0, 0, 0},
};
#define MIX_COUNT (sizeof MIXES / sizeof MIXES[0])
#define MIX_CHURN 4
#define MIX_GROW 5

typedef struct {
    _Alignas(64) _Atomic uint64_t published;
    uint64_t get_misses, update_misses, updates, inserted, removed, sink;
    struct timespec finished;
    unsigned index;
    pthread_t thread;
} slot_t;

static slot_t slots[MAX_THREADS];

static struct {
    cm_map *map;
    const mix_t *mix;
    unsigned threads;
    uint64_t range; /* indices are drawn from [0, range) */
    uint64_t fill;  /* MODE_FILL inserts indices [0, fill) */
    int dist;
    int mode;
    const uint32_t *zipf;
    _Atomic unsigned ready;
    _Atomic int go;
    _Atomic int stop;
} cell;

static int cpus[MAX_THREADS];
static unsigned ncpus;

static void die(const char *what) {
    fprintf(stderr, "bench-%s: %s\n", CM(name)(), what);
    exit(2);
}

static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (double)t.tv_sec + (double)t.tv_nsec * 1e-9;
}

static void sleep_ms(unsigned ms) {
    struct timespec t = {ms / 1000, (long)(ms % 1000) * 1000000L};
    while (nanosleep(&t, &t) != 0 && errno == EINTR) {
    }
}

static void pin(unsigned t) {
    cpu_set_t set;
    CPU_ZERO(&set);
    CPU_SET(cpus[t % ncpus], &set);
    if (sched_setaffinity(0, sizeof set, &set) != 0)
        die("cannot pin a thread to its CPU");
}

static void run_mix(slot_t *s, unsigned t) {
    cm_map *map = cell.map;
    const mix_t *m = cell.mix;
    const uint32_t *zipf = cell.zipf ? cell.zipf + (size_t)t * WL_ZIPF_LENGTH : NULL;
    const int dist = cell.dist;
    const uint64_t range = cell.range;
    uint64_t state = wl_seed(m->id, cell.threads, t);
    uint64_t pos = 0, ops = 0;
    uint64_t gm = 0, um = 0, up = 0, ins = 0, rem = 0, sink = 0;
    for (;;) {
        for (int j = 0; j < BATCH; j++) {
            uint64_t r = wl_next(&state);
            uint32_t roll = wl_roll(r);
            uint64_t index = dist == DIST_UNIFORM ? wl_pick(r, range)
                             : dist == DIST_ZIPF  ? zipf[pos++ & (WL_ZIPF_LENGTH - 1)]
                                                  : 0;
            uint64_t key = wl_key(index);
            if (roll < m->get) {
                uint64_t v;
                if (CM(get)(map, key, &v))
                    sink += v;
                else
                    gm++;
            } else if (roll < m->update) {
                if (CM(update)(map, key))
                    up++;
                else
                    um++;
            } else if (roll < m->insert) {
                ins += (uint64_t)CM(insert)(map, key, index);
            } else {
                rem += (uint64_t)CM(remove)(map, key);
            }
        }
        ops += BATCH;
        atomic_store_explicit(&s->published, ops, memory_order_relaxed);
        if (atomic_load_explicit(&cell.stop, memory_order_relaxed))
            break;
    }
    s->get_misses += gm;
    s->update_misses += um;
    s->updates += up;
    s->inserted += ins;
    s->removed += rem;
    s->sink += sink;
}

static void *worker(void *arg) {
    slot_t *s = arg;
    unsigned t = s->index;
    pin(t);
    CM(enter)(cell.map);
    atomic_fetch_add(&cell.ready, 1);
    while (!atomic_load_explicit(&cell.go, memory_order_acquire)) {
    }
    if (cell.mode == MODE_FILL) {
        uint64_t ins = 0;
        for (uint64_t i = t; i < cell.fill; i += cell.threads)
            ins += (uint64_t)CM(insert)(cell.map, wl_key(i), i);
        s->inserted += ins;
        clock_gettime(CLOCK_MONOTONIC, &s->finished);
    } else {
        run_mix(s, t);
    }
    CM(leave)(cell.map);
    return NULL;
}

typedef struct {
    double mops, seconds;
    uint64_t ops, get_misses, update_misses, updates, inserted, removed;
} result_t;

static void start(unsigned threads) {
    cell.threads = threads;
    atomic_store(&cell.ready, 0);
    atomic_store(&cell.go, 0);
    atomic_store(&cell.stop, 0);
    for (unsigned t = 0; t < threads; t++) {
        slot_t *s = &slots[t];
        memset(s, 0, sizeof *s);
        s->index = t;
        if (pthread_create(&s->thread, NULL, worker, s) != 0)
            die("cannot start a thread");
    }
    while (atomic_load(&cell.ready) < threads)
        sched_yield();
}

static void finish(unsigned threads, result_t *r) {
    for (unsigned t = 0; t < threads; t++) {
        slot_t *s = &slots[t];
        pthread_join(s->thread, NULL);
        r->get_misses += s->get_misses;
        r->update_misses += s->update_misses;
        r->updates += s->updates;
        r->inserted += s->inserted;
        r->removed += s->removed;
    }
}

static uint64_t published(unsigned threads) {
    uint64_t sum = 0;
    for (unsigned t = 0; t < threads; t++)
        sum += atomic_load_explicit(&slots[t].published, memory_order_relaxed);
    return sum;
}

/* Runs one timed cell: warm, then count the operations of a fixed span. */
static result_t run_cell(unsigned threads, unsigned warmup_ms, unsigned duration_ms) {
    result_t r = {0};
    cell.mode = MODE_RUN;
    start(threads);
    atomic_store_explicit(&cell.go, 1, memory_order_release);
    sleep_ms(warmup_ms);
    double t0 = now();
    uint64_t a = published(threads);
    sleep_ms(duration_ms);
    double t1 = now();
    uint64_t b = published(threads);
    atomic_store(&cell.stop, 1);
    finish(threads, &r);
    r.ops = b - a;
    r.seconds = t1 - t0;
    r.mops = (double)r.ops / r.seconds / 1e6;
    return r;
}

/* Inserts indices [0, count) from threads in parallel, timed from the start
 * to the last thread's end. */
static result_t run_fill(unsigned threads, uint64_t count) {
    result_t r = {0};
    cell.mode = MODE_FILL;
    cell.fill = count;
    start(threads);
    double t0 = now();
    atomic_store_explicit(&cell.go, 1, memory_order_release);
    finish(threads, &r);
    double last = t0;
    for (unsigned t = 0; t < threads; t++) {
        double f = (double)slots[t].finished.tv_sec + (double)slots[t].finished.tv_nsec * 1e-9;
        if (f > last)
            last = f;
    }
    r.ops = count;
    r.seconds = last - t0;
    r.mops = (double)count / r.seconds / 1e6;
    return r;
}

static long resident_bytes(void) {
    long pages = 0, resident = 0;
    FILE *f = fopen("/proc/self/statm", "r");
    if (f == NULL)
        return -1;
    if (fscanf(f, "%ld %ld", &pages, &resident) != 2)
        resident = -1;
    fclose(f);
    return resident < 0 ? -1 : resident * sysconf(_SC_PAGESIZE);
}

static const char *DIST_NAMES[] = {"uniform", "zipf", "one"};
static char flag_text[128];
static uint64_t size_n;
static int dist_kind;

static const char *cpu_text(unsigned threads) {
    static char text[MAX_THREADS * 5];
    size_t at = 0;
    text[0] = 0;
    for (unsigned t = 0; t < threads && at + 6 < sizeof text; t++)
        at += (size_t)snprintf(text + at, sizeof text - at, "%s%d", t ? "+" : "", cpus[t % ncpus]);
    return text;
}

static void row(const char *mix, unsigned threads, const result_t *r, const char *check) {
    printf("%s,%s,%llu,%s,%s,%u,%s,%.3f,%llu,%.4f,%s\n", CM(name)(), flag_text,
           (unsigned long long)size_n, DIST_NAMES[dist_kind], mix, threads,
           threads ? cpu_text(threads) : "-", r ? r->mops : 0.0,
           r ? (unsigned long long)r->ops : 0ull, r ? r->seconds : 0.0, check);
    fflush(stdout);
}

static void parse_list(const char *text, int *out, unsigned *count, unsigned limit) {
    *count = 0;
    char *copy = strdup(text), *save = NULL;
    for (char *p = strtok_r(copy, ",", &save); p; p = strtok_r(NULL, ",", &save)) {
        if (*count == limit)
            die("too many list entries");
        out[(*count)++] = atoi(p);
    }
    free(copy);
}

static uint32_t *load_zipf(const char *path, uint64_t n, unsigned threads) {
    FILE *f = fopen(path, "rb");
    if (f == NULL)
        die("cannot open the Zipf file");
    struct wl_zipf_header h;
    if (fread(&h, sizeof h, 1, f) != 1 || memcmp(h.magic, "WFZIPF1", 8) != 0)
        die("the Zipf file has no header");
    if (h.n != n || h.length != WL_ZIPF_LENGTH || h.streams < threads)
        die("the Zipf file does not match the size or the thread count");
    size_t count = (size_t)threads * WL_ZIPF_LENGTH;
    uint32_t *ranks = malloc(count * sizeof *ranks);
    if (ranks == NULL || fread(ranks, sizeof *ranks, count, f) != count)
        die("cannot read the Zipf ranks");
    fclose(f);
    return ranks;
}

int main(int argc, char **argv) {
    int thread_list[MAX_THREADS];
    unsigned nthreads = 0;
    const char *mix_text = "read,mostly-read,balanced,update,churn,grow";
    const char *zipf_path = NULL;
    unsigned warmup_ms = 200, duration_ms = 1000;
    size_n = 0;
    dist_kind = -1;
    ncpus = 0;
    for (int i = 1; i + 1 < argc; i += 2) {
        const char *k = argv[i], *v = argv[i + 1];
        if (!strcmp(k, "--size"))
            size_n = strtoull(v, NULL, 10);
        else if (!strcmp(k, "--dist"))
            dist_kind = !strcmp(v, "uniform") ? DIST_UNIFORM : !strcmp(v, "zipf") ? DIST_ZIPF
                        : !strcmp(v, "one")   ? DIST_ONE
                                              : -2;
        else if (!strcmp(k, "--threads"))
            parse_list(v, thread_list, &nthreads, MAX_THREADS);
        else if (!strcmp(k, "--mixes"))
            mix_text = v;
        else if (!strcmp(k, "--warmup-ms"))
            warmup_ms = (unsigned)atoi(v);
        else if (!strcmp(k, "--duration-ms"))
            duration_ms = (unsigned)atoi(v);
        else if (!strcmp(k, "--cpus"))
            parse_list(v, cpus, &ncpus, MAX_THREADS);
        else if (!strcmp(k, "--zipf"))
            zipf_path = v;
        else
            die("unknown option");
    }
    if (size_n == 0 || size_n > (1ull << 31) || dist_kind < 0 || nthreads == 0)
        die("--size, --dist and --threads are required");
    if (ncpus == 0) {
        long n = sysconf(_SC_NPROCESSORS_ONLN);
        for (long c = 0; c < n && c < MAX_THREADS; c++)
            cpus[ncpus++] = (int)c;
    }
    int flags = CM(flags)();
    if (flags & CM_ONE_THREAD) {
        thread_list[0] = 1;
        nthreads = 1;
    }
    unsigned max_threads = 0;
    for (unsigned t = 0; t < nthreads; t++) {
        if (thread_list[t] < 1 || thread_list[t] > MAX_THREADS)
            die("a thread count is out of range");
        if ((unsigned)thread_list[t] > max_threads)
            max_threads = (unsigned)thread_list[t];
    }
    snprintf(flag_text, sizeof flag_text, "%s%s%s%s%s",
             flags & CM_OPTIMISTIC_UPDATE ? "optimistic-update+" : "",
             flags & CM_ATOMIC_ADD_UPDATE ? "atomic-add-update+" : "",
             flags & CM_NO_STORAGE ? "no-storage+" : "", flags & CM_ONE_THREAD ? "one-thread+" : "",
             flags ? "" : "-");
    size_t fl = strlen(flag_text);
    if (fl > 1 && flag_text[fl - 1] == '+')
        flag_text[fl - 1] = 0;

    int wanted[MIX_COUNT] = {0};
    {
        char *copy = strdup(mix_text), *save = NULL;
        for (char *p = strtok_r(copy, ",", &save); p; p = strtok_r(NULL, ",", &save)) {
            unsigned m = 0;
            while (m < MIX_COUNT && strcmp(MIXES[m].name, p) != 0)
                m++;
            if (m == MIX_COUNT)
                die("unknown mix");
            wanted[m] = 1;
        }
        free(copy);
    }
    /* The key choices each mix runs under (DESIGN.md, "Key choice"). */
    for (unsigned m = 0; m < MIX_COUNT; m++) {
        if (dist_kind == DIST_ONE && m != 3)
            wanted[m] = 0;
        if (dist_kind != DIST_UNIFORM && (m == MIX_CHURN || m == MIX_GROW))
            wanted[m] = 0;
    }
    cell.dist = dist_kind;
    cell.zipf = dist_kind == DIST_ZIPF ? load_zipf(zipf_path ? zipf_path : "", size_n, max_threads) : NULL;
    int checked = !(flags & CM_NO_STORAGE);
    char check[160];

    /* Prefill, then every mix that keeps the key set, at every count. */
    long rss0 = resident_bytes();
    cell.map = CM(create)(size_n);
    if (cell.map == NULL)
        die("cannot create the map");
    result_t fill = run_fill(max_threads, size_n);
    long rss1 = resident_bytes();
    snprintf(check, sizeof check, "bytes-per-key=%.1f",
             rss0 >= 0 && rss1 >= 0 ? (double)(rss1 - rss0) / (double)size_n : -1.0);
    row("prefill", max_threads, &fill, check);
    uint64_t updates = 0;
    for (unsigned m = 0; m < MIX_CHURN; m++) {
        if (!wanted[m])
            continue;
        cell.mix = &MIXES[m];
        cell.range = size_n;
        for (unsigned t = 0; t < nthreads; t++) {
            result_t r = run_cell((unsigned)thread_list[t], warmup_ms, duration_ms);
            updates += r.updates;
            if (!checked)
                snprintf(check, sizeof check, "none");
            else if (r.get_misses || r.update_misses)
                snprintf(check, sizeof check, "fail:misses=%llu",
                         (unsigned long long)(r.get_misses + r.update_misses));
            else
                snprintf(check, sizeof check, "pass");
            row(MIXES[m].name, (unsigned)thread_list[t], &r, check);
        }
    }
    /* Every key is present and every counted update is in its value. */
    if (checked) {
        CM(enter)(cell.map);
        uint64_t sum = 0, missing = 0;
        for (uint64_t i = 0; i < size_n; i++) {
            uint64_t v;
            if (CM(get)(cell.map, wl_key(i), &v))
                sum += v;
            else
                missing++;
        }
        CM(leave)(cell.map);
        uint64_t expected = size_n * (size_n - 1) / 2 + updates;
        if (missing)
            snprintf(check, sizeof check, "fail:missing=%llu", (unsigned long long)missing);
        else if (sum != expected)
            snprintf(check, sizeof check, "fail:sum-off-by=%lld", (long long)(sum - expected));
        else
            snprintf(check, sizeof check, "pass");
        row("check-values", 0, NULL, check);
    }
    /* Churn over 2N keys, then count what is left. */
    if (wanted[MIX_CHURN]) {
        uint64_t inserted = 0, removed = 0;
        cell.mix = &MIXES[MIX_CHURN];
        cell.range = 2 * size_n;
        for (unsigned t = 0; t < nthreads; t++) {
            result_t r = run_cell((unsigned)thread_list[t], warmup_ms, duration_ms);
            inserted += r.inserted;
            removed += r.removed;
            row("churn", (unsigned)thread_list[t], &r, checked ? "see-check-churn" : "none");
        }
        if (checked) {
            CM(enter)(cell.map);
            uint64_t live = 0;
            for (uint64_t i = 0; i < 2 * size_n; i++) {
                uint64_t v;
                live += (uint64_t)CM(get)(cell.map, wl_key(i), &v);
            }
            CM(leave)(cell.map);
            uint64_t expected = size_n + inserted - removed;
            if (live != expected)
                snprintf(check, sizeof check, "fail:live=%llu,expected=%llu",
                         (unsigned long long)live, (unsigned long long)expected);
            else
                snprintf(check, sizeof check, "pass");
            row("check-churn", 0, NULL, check);
        }
    }
    CM(destroy)(cell.map);
    /* Grow a new map from empty at every count. */
    if (wanted[MIX_GROW]) {
        for (unsigned t = 0; t < nthreads; t++) {
            unsigned threads = (unsigned)thread_list[t];
            cell.map = CM(create)(0);
            if (cell.map == NULL)
                die("cannot create the map");
            result_t r = run_fill(threads, size_n);
            if (!checked) {
                snprintf(check, sizeof check, "none");
            } else {
                CM(enter)(cell.map);
                uint64_t found = 0;
                for (uint64_t i = 0; i < size_n; i++) {
                    uint64_t v;
                    found += (uint64_t)(CM(get)(cell.map, wl_key(i), &v) && v == i);
                }
                CM(leave)(cell.map);
                if (found != size_n || r.inserted != size_n)
                    snprintf(check, sizeof check, "fail:found=%llu,inserted=%llu",
                             (unsigned long long)found, (unsigned long long)r.inserted);
                else
                    snprintf(check, sizeof check, "pass");
            }
            row("grow", threads, &r, check);
            CM(destroy)(cell.map);
        }
    }
    return 0;
}
