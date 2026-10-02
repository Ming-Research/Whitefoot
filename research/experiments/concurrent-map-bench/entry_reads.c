/* Serves concurrent-map-bench's `make entry-reads`: one key of a map of
 * entries read by every thread, each read holding the entry exclusively
 * (wf_cmap_lock_entry) or beside the other reads (wf_cmap_read_entry), as a
 * keyed statement whose block only reads would. A read copies the entry's
 * `WORDS` words and writes `elements` three-byte RESP bulk strings from
 * them, as firn's LRANGE writes a reply inside its statement; `outside`
 * rounds of a multiply-add chain between reads stand for the work a server
 * does around a statement. Usage: entry-reads ELEMENTS OUTSIDE. Each thread
 * count runs both modes, interleaved, five times for 0.4 s; it prints the
 * median rate and range of each and their ratio. */
#define _GNU_SOURCE
#include <sched.h>
#include <stdlib.h>
#define WF_CMAP_TAKE(bytes) aligned_alloc(16, ((size_t)(bytes) + 15) / 16 * 16)
#define WF_CMAP_GIVE(block, bytes) free(block)
#define WF_CMAP_YIELD() sched_yield()
#define WF_CMAP_EXHAUSTED() abort()
#include "concurrent_map.c"
#include <pthread.h>
#include <stdio.h>
#include <time.h>
enum { WORDS = 100 };
static wf_cmap *map;
static int shared_mode;
static _Atomic int go, stop;
static unsigned char key[8] = "hotkey!!";
static unsigned elements, outside;
/* Writes `elements` RESP bulk strings of three bytes each, as LRANGE answers
 * a list of redis-benchmark's values, from the entry's words. */
static uint64_t format(const uint64_t *s, unsigned char *out) {
    uint64_t at = 0;
    for (unsigned i = 0; i < elements; i++) {
        uint64_t w = s[i % WORDS];
        out[at++] = '$';
        uint64_t len = 3 + (w & 0);
        unsigned char digits[20]; int d = 0;
        do { digits[d++] = (unsigned char)('0' + len % 10); len /= 10; } while (len);
        while (d) out[at++] = digits[--d];
        out[at++] = '\r'; out[at++] = '\n';
        memcpy(out + at, (const unsigned char *)&s[i % WORDS], 3); at += 3;
        out[at++] = '\r'; out[at++] = '\n';
    }
    return at;
}
static void *worker(void *arg) {
    unsigned idx = (unsigned)(uintptr_t)arg;
    wf_cmap_user *u = wf_cmap_user_at(map, idx);
    uint64_t sink[WORDS], n = 0, bytes = 0, lcg = idx;
    static _Thread_local unsigned char out[16384];
    while (!atomic_load(&go)) {}
    while (!atomic_load_explicit(&stop, memory_order_relaxed)) {
        wf_cmap_entry e;
        if (shared_mode) {
            const uint64_t *s = wf_cmap_read_entry(u, key, 8, 0, &e);
            memcpy(sink, s, sizeof sink);
            bytes += format(s, out);
            wf_cmap_unread_entry(u, &e, 0);
        } else {
            uint64_t *s = wf_cmap_lock_entry(u, key, 8, 0, &e);
            memcpy(sink, s, sizeof sink);
            bytes += format(s, out);
            wf_cmap_unlock_entry(u, &e, 0, 1);
        }
        n++;
        /* Work between statements, as a server parses and sends. */
        for (unsigned k = 0; k < outside; k++) { lcg = lcg * 6364136223846793005ULL + 1; __asm__ volatile("" : "+r"(lcg)); }
    }
    return (void *)(uintptr_t)(n + (sink[3] & 0) + (bytes & 0) + (out[5] & 0) + (lcg & 0));
}
static double run(unsigned threads) {
    map = wf_cmap_create_entries(WORDS * 8, 8, 0);
    wf_cmap_entry e;
    uint64_t *s = wf_cmap_lock_entry(wf_cmap_user_at(map, 0), key, 8, 0, &e);
    for (int i = 0; i < WORDS; i++) s[i] = i;
    wf_cmap_unlock_entry(wf_cmap_user_at(map, 0), &e, 0, 1);
    pthread_t t[4];
    atomic_store(&go, 0); atomic_store(&stop, 0);
    for (unsigned i = 0; i < threads; i++) pthread_create(&t[i], NULL, worker, (void *)(uintptr_t)i);
    atomic_store(&go, 1);
    struct timespec d = {0, 400000000}; nanosleep(&d, NULL);
    atomic_store(&stop, 1);
    uint64_t total = 0;
    for (unsigned i = 0; i < threads; i++) { void *r; pthread_join(t[i], &r); total += (uint64_t)(uintptr_t)r; }
    wf_cmap_destroy(map);
    return total / 0.4e6;
}
static int cmp(const void *a, const void *b) { double x = *(const double *)a, y = *(const double *)b; return (x > y) - (x < y); }
int main(int argc, char **argv) {
    elements = argc > 1 ? (unsigned)atoi(argv[1]) : 0;
    outside = argc > 2 ? (unsigned)atoi(argv[2]) : 0;
    enum { REPS = 5 };
    for (unsigned threads = 1; threads <= 4; threads *= 2) {
        double r[2][REPS];
        for (int k = 0; k < REPS; k++)
            for (shared_mode = 0; shared_mode <= 1; shared_mode++) r[shared_mode][k] = run(threads);
        qsort(r[0], REPS, sizeof(double), cmp); qsort(r[1], REPS, sizeof(double), cmp);
        printf("elements %3u outside %4u threads %u: exclusive %.2f [%.2f-%.2f] shared %.2f [%.2f-%.2f] M/s, shared/exclusive %.2f\n",
               elements, outside, threads, r[0][REPS / 2], r[0][0], r[0][REPS - 1], r[1][REPS / 2], r[1][0], r[1][REPS - 1], r[1][REPS / 2] / r[0][REPS / 2]);
    }
    return 0;
}
