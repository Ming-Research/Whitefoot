/* Vector append attribution only; retire with its superseding experiment. */
#if !defined(__APPLE__)
#define _POSIX_C_SOURCE 200809L
#endif
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#if defined(__APPLE__)
#include <malloc/malloc.h>
#endif

typedef struct { unsigned char *allocation; size_t len, cap; } Vector;
void layout_append(Vector *, size_t, unsigned, size_t, size_t, uint64_t);
static void require(int ok, const char *message) {
    if (!ok) { fprintf(stderr, "append layout: %s\n", message); exit(1); }
}

#ifdef LAYOUT_HELPER
/* Compiled separately without LTO: every arm calls this one runtime body. */
__attribute__((noinline))
void layout_append(Vector *v, size_t width, unsigned route, size_t extra,
                   size_t offset, uint64_t seed) {
    size_t old_bytes = v->len * width, new_cap = v->cap * 2;
    unsigned char *next;
    if (route) {
        next = realloc(v->allocation, new_cap * width + extra);
        require(next != NULL, "realloc");
    } else {
        next = malloc(new_cap * width + extra);
        require(next != NULL, "malloc");
        memmove(next + offset, v->allocation + offset, old_bytes);
        free(v->allocation);
    }
    uint64_t *slot = (uint64_t *)(next + offset + old_bytes);
    for (size_t j = 0; j < width / 8; ++j) slot[j] = seed ^ j;
    v->allocation = next;
    v->cap = new_cap;
    v->len += 1;
}
#else
static const size_t extras[] = {0, 16, 16}, offsets[] = {0, 0, 16};
static uint64_t nanos(void) {
    struct timespec t;
#if defined(__APPLE__)
    require(clock_gettime(CLOCK_MONOTONIC_RAW, &t) == 0, "clock read");
#else
    require(clock_gettime(CLOCK_MONOTONIC, &t) == 0, "clock read");
#endif
    return (uint64_t)t.tv_sec * UINT64_C(1000000000) + (uint64_t)t.tv_nsec;
}
static void clock_check(int quantized) {
    uint64_t previous = nanos(), minimum = UINT64_MAX;
    if (quantized) previous -= previous % 1000;
    for (unsigned i = 0; i < 200000; ++i) {
        uint64_t current = nanos();
        if (quantized) current -= current % 1000;
        require(current >= previous, "clock nondecreasing");
        if (current > previous && current - previous < minimum)
            minimum = current - previous;
        previous = current;
    }
    fprintf(stderr, "clock minimum_nonzero_ns=%" PRIu64 " quantized=%d\n", minimum, quantized);
    require(minimum <= 100, "clock precision");
}
static Vector prepare(size_t width, size_t extra, size_t offset, uint64_t seed) {
    Vector v = {malloc(4096 * width + extra), 4096, 4096};
    require(v.allocation != NULL, "setup malloc");
    uint64_t *p = (uint64_t *)(v.allocation + offset);
    for (size_t j = 0; j < 4096 * width / 8; ++j) p[j] = seed + j;
    return v;
}
static void verify(Vector *v, size_t width, size_t offset, uint64_t seed) {
    require(v->allocation != NULL && v->len == 4097, "post length/pointer");
    require(v->cap == 8192, "post capacity");
    const uint64_t *p = (const uint64_t *)(v->allocation + offset);
    size_t words = 4096 * width / 8;
    for (size_t j = 0; j < words; ++j)
        require(p[j] - seed == j, "old payload");
    for (size_t j = 0; j < width / 8; ++j)
        require((p[words + j] ^ j) == seed, "appended payload");
    free(v->allocation);
    v->allocation = NULL; v->len = v->cap = 0;
}
static void check(const char *fault) {
    clock_check(strcmp(fault, "clock") == 0);
    for (size_t width = 8; width <= 256; width *= 32)
        for (unsigned arm = 0; arm < 6; ++arm) {
            size_t layout = arm % 3, offset = offsets[layout];
            Vector v = prepare(width, extras[layout], offset, 73);
            layout_append(&v, width, arm / 3, extras[layout], offset, 73);
            if (strcmp(fault, "payload") == 0) v.allocation[offset + 4096 * width - 1] ^= 1;
            if (strcmp(fault, "capacity") == 0) v.cap -= 1;
            verify(&v, width, offset, 73);
        }
    puts("all six layouts and both payload widths passed");
}
static size_t usable(void *p) {
#if defined(__APPLE__)
    return malloc_size(p);
#else
    (void)p; return 0; /* Not observed on this platform. */
#endif
}
static void observe(void) {
    puts("element_bytes,route,extra,offset,old_request,new_request,old_usable,new_usable,moved");
    for (size_t width = 8; width <= 256; width *= 32)
        for (unsigned arm = 0; arm < 6; ++arm) {
            size_t layout = arm % 3, extra = extras[layout], offset = offsets[layout];
            Vector v = prepare(width, extra, offset, 91);
            uintptr_t before = (uintptr_t)v.allocation;
            size_t old_usable = usable(v.allocation);
            layout_append(&v, width, arm / 3, extra, offset, 91);
            printf("%zu,%u,%zu,%zu,%zu,%zu,%zu,%zu,%d\n", width, arm / 3, extra, offset,
                   4096 * width + extra, 8192 * width + extra, old_usable,
                   usable(v.allocation), before != (uintptr_t)v.allocation);
            verify(&v, width, offset, 91);
        }
}
static void measure(void) {
    clock_check(0);
    puts("cohort,element_bytes,route,extra,offset,sample,contexts,cycles,operations,elapsed_ns,qualified");
    int qualified = 1;
    for (unsigned cohort = 0; cohort < 2; ++cohort)
        for (size_t width = 8; width <= 256; width *= 32) {
            const uint64_t unit = 4096 * width + 256;
            size_t contexts = 1048576 / unit;
            if (contexts < 1) contexts = 1;
            if (contexts > 1024) contexts = 1024;
            const uint64_t work = width == 8 ? UINT64_C(67108864) : UINT64_C(8589934592);
            const uint64_t cycles = (work + contexts * unit - 1) / (contexts * unit);
            Vector vectors[1024];
            for (unsigned sample = 0; sample < 7; ++sample)
                for (unsigned position = 0; position < 6; ++position) {
                    unsigned arm = (sample + (cohort ? 5 - position : position)) % 6;
                    size_t layout = arm % 3, extra = extras[layout], offset = offsets[layout];
                    uint64_t elapsed = 0;
                    for (uint64_t cycle = 0; cycle < cycles; ++cycle) {
                        uint64_t seed = 1009 * cycle + sample;
                        for (size_t j = 0; j < contexts; ++j)
                            vectors[j] = prepare(width, extra, offset, seed + j);
                        uint64_t start = nanos();
                        for (size_t j = 0; j < contexts; ++j)
                            layout_append(&vectors[j], width, arm / 3, extra, offset, seed + j);
                        uint64_t end = nanos();
                        require(end >= start, "sample clock nondecreasing");
                        elapsed += end - start;
                        for (size_t j = 0; j < contexts; ++j)
                            verify(&vectors[j], width, offset, seed + j);
                    }
                    int ok = elapsed >= 1000000;
                    qualified &= ok;
                    printf("%u,%zu,%u,%zu,%zu,%u,%zu,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%d\n",
                           cohort, width, arm / 3, extra, offset, sample, contexts, cycles,
                           contexts * cycles, elapsed, ok);
                    fflush(stdout);
                }
        }
    require(qualified, "unresolved samples below 1 ms");
}
int main(int argc, char **argv) {
    require(argc == 2, "usage: check|fail-payload|fail-capacity|fail-clock|observe|measure");
    if (strcmp(argv[1], "measure") == 0) measure();
    else if (strcmp(argv[1], "observe") == 0) observe();
    else if (strcmp(argv[1], "check") == 0) check("");
    else if (strcmp(argv[1], "fail-payload") == 0) check("payload");
    else if (strcmp(argv[1], "fail-capacity") == 0) check("capacity");
    else if (strcmp(argv[1], "fail-clock") == 0) check("clock");
    else require(0, "unknown mode");
    return 0;
}
#endif
