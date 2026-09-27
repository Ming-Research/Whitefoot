/* Practical HashMap comparison. The historical driver shares its independent
 * key-ID oracle through map-oracle.h. Retire this driver with the explicit
 * ecosystem targets; it is not a compiler or conformance dependency. */
#if !defined(_WIN32)
#define _POSIX_C_SOURCE 200809L
#endif
#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <time.h>
#endif

enum { INSERTED, REPLACED, REFUSED };
enum { HIT, MISS, REPLACE, CHURN, GROW, REHASH, SETUP, EDIT, POLICY, RESERVE_CHECK, RESERVE_OMITTED };
enum { WORDS = 32, SAMPLE_COUNT = 11, IMPLEMENTATIONS = 5 };
typedef struct { uint64_t ordered, sum, parity, count; } Digest;
typedef uint64_t (*Trace)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
typedef struct { uint64_t capacity, count; } Shape;
typedef struct {
    const char *name;
    bool wide;
    Trace ordinary, aligned;
    bool attribution_control;
} Variant;

#define DECLARE(NAME) extern uint64_t NAME(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t)
DECLARE(wf_map_cost_library_word_trace);
DECLARE(wf_map_cost_library_record_trace);
DECLARE(eco_rust_map_word_default);
DECLARE(eco_rust_map_record_default);
DECLARE(eco_rust_map_word_aligned);
DECLARE(eco_rust_map_record_aligned);
DECLARE(eco_cpp_map_word_default);
DECLARE(eco_cpp_map_record_default);
DECLARE(eco_cpp_map_word_aligned);
DECLARE(eco_cpp_map_record_aligned);
DECLARE(eco_absl_map_word_default);
DECLARE(eco_absl_map_record_default);
DECLARE(eco_absl_map_word_aligned);
DECLARE(eco_absl_map_record_aligned);
DECLARE(eco_c_map_word);
DECLARE(eco_c_map_record);
#undef DECLARE

static const Variant variants[] = {
    {"whitefoot-hash-map", false, wf_map_cost_library_word_trace, wf_map_cost_library_word_trace, false},
    {"rust-hash-map", false, eco_rust_map_word_default, eco_rust_map_word_aligned, false},
    {"cpp-unordered-map", false, eco_cpp_map_word_default, eco_cpp_map_word_aligned, false},
    {"absl-flat-hash-map", false, eco_absl_map_word_default, eco_absl_map_word_aligned, false},
    {"c-sparse-direct", false, eco_c_map_word, eco_c_map_word, true},
    {"whitefoot-hash-map", true, wf_map_cost_library_record_trace, wf_map_cost_library_record_trace, false},
    {"rust-hash-map", true, eco_rust_map_record_default, eco_rust_map_record_aligned, false},
    {"cpp-unordered-map", true, eco_cpp_map_record_default, eco_cpp_map_record_aligned, false},
    {"absl-flat-hash-map", true, eco_absl_map_record_default, eco_absl_map_record_aligned, false},
    {"c-sparse-direct", true, eco_c_map_record, eco_c_map_record, true},
};
static const Shape measured_shapes[] = {{3, 2}, {64, 56}, {4096, 3584}};
static const unsigned paths[] = {HIT, MISS, REPLACE, CHURN, EDIT, SETUP, GROW};
static const char *path_names[] = {
    "hit", "miss", "replace-old-value", "remove-churn", "reserve-more-entries",
    "excluded-same-capacity-rehash", "fill-free", "edit-first-word", "ceiling-policy", "reserve-check", "reserve-omitted"
};
static volatile uint64_t observed;

static void require(bool condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "map ecosystem: %s\n", message);
        exit(1);
    }
}

static uint64_t mix(uint64_t value) {
    value ^= value >> 30; value *= UINT64_C(0xbf58476d1ce4e5b9);
    value ^= value >> 27; value *= UINT64_C(0x94d049bb133111eb);
    return value ^ (value >> 31);
}
static uint64_t key_at(uint64_t index) { return index * 2 + 1; }
static void ordered(Digest *digest, uint64_t value) {
    digest->ordered = digest->ordered * UINT64_C(131) + value;
}
static void final_value(Digest *digest, uint64_t value) {
    digest->sum += value; digest->parity ^= mix(value); ++digest->count;
}
static uint64_t finish(Digest digest) {
    return mix(digest.ordered) ^ mix(digest.sum) ^ digest.parity
        ^ digest.count * UINT64_C(0x9e3779b97f4a7c15);
}

#include "map-oracle.h"

/* This witness models an application ceiling of three entries by identities,
 * independent of bucket counts, capacity rounding, or native growth policy. */
static uint64_t policy_oracle(bool wide, uint64_t seed) {
    Digest digest = {seed, 0, 0, 0};
    for (unsigned i = 0; i < 3; ++i) ordered(&digest, INSERTED);
    ordered(&digest, REPLACED);
    ordered(&digest, oracle_content(wide, 1, seed));
    ordered(&digest, REFUSED);
    ordered(&digest, oracle_content(wide, 7, seed + 20));
    final_value(&digest, oracle_content(wide, 1, seed + 10));
    final_value(&digest, oracle_content(wide, 3, seed + 1));
    final_value(&digest, oracle_content(wide, 5, seed + 2));
    return finish(digest);
}

static uint64_t reserve_oracle(bool wide, uint64_t count, uint64_t seed) {
    Digest digest = {seed, 0, 0, 0};
    for (uint64_t i = 0; i < count; ++i) ordered(&digest, INSERTED);
    ordered(&digest, true); // Reserve outcome.
    ordered(&digest, true); // Observed public capacity meets the requested floor.
    for (uint64_t i = 0; i < count; ++i) {
        ordered(&digest, true); ordered(&digest, seed + i);
        final_value(&digest, oracle_content(wide, key_at(i), seed + i));
    }
    return finish(digest);
}

#ifdef ACCOUNT_ONLY
typedef struct { uint64_t requests, releases, bytes, live, peak; } Ledger;
typedef union {
    struct { uint64_t bytes, magic; } data;
    max_align_t alignment;
} AllocationHeader;
static Ledger ledger;

void wf_ecosystem_note_alloc(uint64_t bytes) {
    require(UINT64_MAX - ledger.live >= bytes && UINT64_MAX - ledger.bytes >= bytes,
            "allocation accounting extent");
    ++ledger.requests; ledger.bytes += bytes; ledger.live += bytes;
    if (ledger.live > ledger.peak) ledger.peak = ledger.live;
}
void wf_ecosystem_note_dealloc(uint64_t bytes) {
    require(ledger.live >= bytes, "allocation release extent");
    ++ledger.releases; ledger.live -= bytes;
}
void wf_ecosystem_note_realloc(uint64_t old_bytes, uint64_t new_bytes) {
    wf_ecosystem_note_dealloc(old_bytes);
    wf_ecosystem_note_alloc(new_bytes);
}
void *wf_cost_allocate(uint64_t bytes) {
    require(bytes <= SIZE_MAX - sizeof(AllocationHeader), "allocation extent");
    AllocationHeader *header = malloc(sizeof *header + (size_t)bytes);
    require(header != NULL, "host allocation failure");
    header->data.bytes = bytes;
    header->data.magic = UINT64_C(0x6d617065636f6e6f);
    wf_ecosystem_note_alloc(bytes);
    return header + 1;
}
void wf_cost_release(void *pointer) {
    if (pointer == NULL) return;
    AllocationHeader *header = (AllocationHeader *)pointer - 1;
    require(header->data.magic == UINT64_C(0x6d617065636f6e6f), "allocation header signature");
    wf_ecosystem_note_dealloc(header->data.bytes);
    header->data.magic = 0;
    free(header);
}
static void clean_ledger(void) {
    require(ledger.live == 0 && ledger.requests == ledger.releases,
            "every allocation is reclaimed");
}
static void reset_ledger(void) { clean_ledger(); ledger = (Ledger){0}; }
#else
static void clean_ledger(void) {}
static void reset_ledger(void) {}
#endif

static Trace trace_for(const Variant *variant, unsigned series) {
    return series ? variant->aligned : variant->ordinary;
}
static const char *hasher_for(const Variant *variant, unsigned series) {
    if (series || variant->ordinary == variant->aligned) return "salted-mix64";
    if (strcmp(variant->name, "rust-hash-map") == 0) return "random-state";
    if (strcmp(variant->name, "cpp-unordered-map") == 0) return "std-hash-u64";
    return "absl-default-hash-u64";
}

static void checked_trace(const Variant *variant, unsigned series, Shape shape,
                          uint64_t rounds, uint64_t seed, unsigned path, bool collide) {
    uint64_t expected = path == POLICY ? policy_oracle(variant->wide, seed)
        : path == RESERVE_CHECK ? reserve_oracle(variant->wide, shape.count, seed)
        : oracle(variant->wide, shape.count, rounds, seed, path);
    reset_ledger();
    uint64_t actual = trace_for(variant, series)(shape.capacity, shape.count, rounds,
                                               seed, path, collide);
    if (actual != expected) {
        fprintf(stderr, "map ecosystem cell: %s payload=%u series=%u capacity=%" PRIu64
                " count=%" PRIu64 " rounds=%" PRIu64 " seed=%" PRIu64 " path=%s collide=%u\n",
                variant->name, variant->wide ? 256 : 8, series, shape.capacity,
                shape.count, rounds, seed, path_names[path], collide);
    }
    require(actual == expected, "independent key-ID/content/outcome oracle");
    clean_ledger(); observed ^= actual;
}

static void check(void) {
    const Shape shapes[] = {{0, 0}, {1, 0}, {1, 1}, {3, 2}, {3, 3},
                            {63, 55}, {64, 32}, {64, 56}, {64, 64}, {4096, 3584}};
    const uint64_t rounds[] = {0, 1, 3}, seeds[] = {0, 17, UINT64_MAX};
    size_t executions = 0;
    for (size_t v = 0; v < sizeof variants / sizeof variants[0]; ++v)
        for (unsigned series = 0; series < 2; ++series) {
            const Variant *variant = &variants[v];
            for (size_t s = 0; s < sizeof shapes / sizeof shapes[0]; ++s)
                for (size_t p = 0; p < sizeof paths / sizeof paths[0]; ++p)
                    for (size_t r = 0; r < sizeof rounds / sizeof rounds[0]; ++r)
                        for (size_t n = 0; n < sizeof seeds / sizeof seeds[0]; ++n)
                            for (unsigned collide = 0; collide < (series && shapes[s].count <= 64 ? 2u : 1u); ++collide) {
                                checked_trace(variant, series, shapes[s], rounds[r], seeds[n], paths[p], collide != 0);
                                ++executions;
                            }
            if (variant->attribution_control) continue;
            for (size_t n = 0; n < sizeof seeds / sizeof seeds[0]; ++n)
                for (unsigned collide = 0; collide < (series ? 2u : 1u); ++collide) {
                    checked_trace(variant, series, (Shape){3, 3}, 0, seeds[n], POLICY, collide != 0);
                    ++executions;
                }
            for (size_t s = 0; s < sizeof measured_shapes / sizeof measured_shapes[0]; ++s) {
                checked_trace(variant, series, measured_shapes[s], 1, 17, RESERVE_CHECK, false);
                ++executions;
            }
        }
    printf("map ecosystem: %zu oracle-checked traces passed\n", executions);
}

static void trace_size(Shape shape, unsigned path, uint64_t work,
                       uint64_t *rounds, uint64_t *traces) {
    uint64_t repeats = work / shape.count;
    if (repeats == 0) repeats = 1;
    *rounds = path == SETUP ? 0 : path == GROW ? 1 : repeats;
    *traces = path == SETUP || path == GROW ? repeats : 1;
}

#ifndef ACCOUNT_ONLY
static uint64_t nanoseconds(void) {
#if defined(_WIN32)
    LARGE_INTEGER count, frequency;
    QueryPerformanceCounter(&count); QueryPerformanceFrequency(&frequency);
    return (uint64_t)((long double)count.QuadPart * 1000000000.0L / frequency.QuadPart);
#else
    struct timespec value;
    require(clock_gettime(CLOCK_MONOTONIC, &value) == 0, "monotonic clock");
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
#endif
}

static uint64_t run_batch(Trace trace, Shape shape, unsigned path, uint64_t rounds,
                          uint64_t traces, uint64_t seed) {
    uint64_t checksum = 0;
    for (uint64_t i = 0; i < traces; ++i)
        checksum = checksum * UINT64_C(257)
            + trace(shape.capacity, shape.count, rounds, seed + i, path, 0);
    return checksum;
}
static uint64_t batch_oracle(bool wide, Shape shape, unsigned path, uint64_t rounds,
                             uint64_t traces, uint64_t seed) {
    uint64_t checksum = 0;
    for (uint64_t i = 0; i < traces; ++i)
        checksum = checksum * UINT64_C(257) + oracle(wide, shape.count, rounds, seed + i, path);
    return checksum;
}

static void measure(unsigned cohort, uint64_t work) {
    puts("contract,cohort,series,element_bytes,path,requested_capacity,count,hash,variant,sample,rounds,traces,elapsed_ns,checksum");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (size_t s = 0; s < sizeof measured_shapes / sizeof measured_shapes[0]; ++s)
            for (size_t p = 0; p < sizeof paths / sizeof paths[0]; ++p)
                for (unsigned series = 0; series < 2; ++series) {
                    Shape shape = measured_shapes[s];
                    unsigned path = paths[p];
                    uint64_t rounds, traces;
                    trace_size(shape, path, work, &rounds, &traces);
                    // Two complete, independently checked warmup batches per
                    // cell. No counters or allocation wrappers in this image.
                    for (unsigned warmup = 0; warmup < 2; ++warmup) {
                        uint64_t expected = batch_oracle(wide != 0, shape, path, rounds, traces, 71 + warmup);
                        for (unsigned offset = 0; offset < IMPLEMENTATIONS; ++offset) {
                            unsigned position = cohort ? IMPLEMENTATIONS - 1 - offset : offset;
                            const Variant *variant = &variants[wide * IMPLEMENTATIONS + position];
                            uint64_t actual = run_batch(trace_for(variant, series), shape, path, rounds, traces, 71 + warmup);
                            require(actual == expected, "warmup independent oracle"); observed ^= actual;
                        }
                    }
                    for (unsigned sample = 0; sample < SAMPLE_COUNT; ++sample) {
                        uint64_t seed = 101 + sample;
                        uint64_t expected = batch_oracle(wide != 0, shape, path, rounds, traces, seed);
                        for (unsigned offset = 0; offset < IMPLEMENTATIONS; ++offset) {
                            unsigned position = (sample + offset) % IMPLEMENTATIONS;
                            const Variant *variant = &variants[wide * IMPLEMENTATIONS + (cohort ? IMPLEMENTATIONS - 1 - position : position)];
                            uint64_t start = nanoseconds();
                            uint64_t checksum = run_batch(trace_for(variant, series), shape, path, rounds, traces, seed);
                            uint64_t elapsed = nanoseconds() - start;
                            require(checksum == expected, "timed independent oracle"); observed ^= checksum;
                            printf("normal,%u,%s,%u,%s,%" PRIu64 ",%" PRIu64 ",%s,%s,%u,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                                   cohort, series ? "aligned-hash" : "native-default", wide ? 256 : 8,
                                   path_names[path], shape.capacity, shape.count, hasher_for(variant, series),
                                   variant->name, sample, rounds, traces, elapsed, checksum);
                        }
                    }
                }
}
#else
static void account(uint64_t work) {
    puts("contract,series,element_bytes,path,requested_capacity,count,hash,variant,rounds,traces,requests,releases,requested_bytes,peak_bytes,live_bytes,checksum");
    for (size_t v = 0; v < sizeof variants / sizeof variants[0]; ++v)
        for (size_t s = 0; s < sizeof measured_shapes / sizeof measured_shapes[0]; ++s)
            for (size_t p = 0; p < sizeof paths / sizeof paths[0]; ++p)
                for (unsigned series = 0; series < 2; ++series) {
                    const Variant *variant = &variants[v];
                    Shape shape = measured_shapes[s];
                    unsigned path = paths[p];
                    uint64_t rounds, traces, checksum = 0, expected = 0;
                    trace_size(shape, path, work, &rounds, &traces);
                    for (uint64_t i = 0; i < traces; ++i)
                        expected = expected * UINT64_C(257) + oracle(variant->wide, shape.count, rounds, 101 + i, path);
                    reset_ledger();
                    for (uint64_t i = 0; i < traces; ++i)
                        checksum = checksum * UINT64_C(257)
                            + trace_for(variant, series)(shape.capacity, shape.count, rounds, 101 + i, path, 0);
                    require(checksum == expected, "accounting independent oracle"); clean_ledger();
                    printf("accounting,%s,%u,%s,%" PRIu64 ",%" PRIu64 ",%s,%s,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                           series ? "aligned-hash" : "native-default", variant->wide ? 256 : 8, path_names[path],
                           shape.capacity, shape.count, hasher_for(variant, series), variant->name, rounds, traces,
                           ledger.requests, ledger.releases, ledger.bytes, ledger.peak, ledger.live, checksum);
                }
}
#endif

static uint64_t parse_work(const char *text) {
    char *end = NULL;
    unsigned long long work = strtoull(text, &end, 10);
    require(end != text && *end == '\0' && work > 0 && work <= UINT64_C(16777216),
            "work must be 1..16777216 item-rounds");
    return (uint64_t)work;
}

int main(int argc, char **argv) {
    require(argc >= 2, "usage: check | negative-checksum | negative-leak | negative-reserve variant | account work | measure 0|1 work");
    if (strcmp(argv[1], "check") == 0) {
        require(argc == 2, "check takes no arguments"); check();
    } else if (strcmp(argv[1], "negative-checksum") == 0) {
        require(argc == 2, "negative-checksum takes no arguments");
        checked_trace(&variants[0], 1, (Shape){3, 2}, 1, 17, HIT, false);
        uint64_t actual = variants[0].aligned(3, 2, 1, 17, HIT, 0);
        require((actual ^ UINT64_C(1)) == oracle(false, 2, 1, 17, HIT), "independent key-ID/content/outcome oracle");
    } else if (strcmp(argv[1], "negative-reserve") == 0) {
        require(argc == 3 && strlen(argv[2]) == 1 && argv[2][0] >= '0' && argv[2][0] <= '9',
                "negative-reserve requires variant 0..9");
        const Variant *variant = &variants[argv[2][0] - '0'];
        require(!variant->attribution_control, "C attribution control has no reserve diagnostic path");
        checked_trace(variant, 1, (Shape){3, 2}, 1, 17, RESERVE_CHECK, false);
        uint64_t actual = variant->aligned(3, 2, 1, 17, RESERVE_OMITTED, 0);
        clean_ledger();
        require(actual == reserve_oracle(variant->wide, 2, 17), "reserve capacity floor");
    }
#ifdef ACCOUNT_ONLY
    else if (strcmp(argv[1], "negative-leak") == 0) {
        require(argc == 2, "negative-leak takes no arguments");
        checked_trace(&variants[0], 1, (Shape){3, 2}, 1, 17, HIT, false);
        wf_ecosystem_note_alloc(8); clean_ledger();
    } else if (strcmp(argv[1], "account") == 0) {
        require(argc == 3, "account requires a work count"); account(parse_work(argv[2]));
    }
#else
    else if (strcmp(argv[1], "measure") == 0) {
        require(argc == 4 && (strcmp(argv[2], "0") == 0 || strcmp(argv[2], "1") == 0),
                "measure requires cohort 0 or 1 and a work count");
        measure((unsigned)(argv[2][0] - '0'), parse_work(argv[3]));
    }
#endif
    else require(false, "unknown or unavailable mode");
    return 0;
}
