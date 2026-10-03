/* Reuse the frozen independent oracle and clocks without rewriting historical
 * evidence. The explicit ecosystem targets are this runner's only callers;
 * retire it with that comparison. Historical entry/allocator symbols stay
 * separate and are never called by the practical traces below. */
#define main ordered_historical_main
#define wf_cost_allocate ordered_historical_allocate
#define wf_cost_release ordered_historical_release
#include "ordered-costs.c"
#undef main
#undef wf_cost_allocate
#undef wf_cost_release

#include <errno.h>

#define DECLARE_NATIVE(name) \
    uint64_t name##_word_trace(uint64_t, uint64_t, uint64_t, uint64_t); \
    uint64_t name##_record_trace(uint64_t, uint64_t, uint64_t, uint64_t); \
    void name##_audit(void)
DECLARE_NATIVE(rust_ordered);
DECLARE_NATIVE(cpp_ordered);
DECLARE_NATIVE(absl_ordered);
enum { VARIANTS = 7 };
static const char *ecosystem_variants[] = {
    "whitefoot", "source-c", "direct-c", "avl-c", "rust-btree-map", "cpp-map", "absl-btree-map"
};
static const char *ecosystem_paths[] = {
    "build-cleanup", "hit-miss", "replace-edit-remove-insert", "range-16", "replace-only"
};

#if defined(ACCOUNT_ONLY)
/* Observed native allocators continue to use their ordinary system allocator.
 * Requested bytes exclude the bookkeeping header used only for WF/C nodes. */
void wf_ecosystem_note_alloc(uint64_t bytes) {
    require(bytes <= SIZE_MAX, "accounted allocation extent");
    ++accounting.requests;
    accounting.requested_bytes += (size_t)bytes;
    ++accounting.live_nodes;
    accounting.live_bytes += (size_t)bytes;
    if (accounting.live_nodes > accounting.peak_nodes) accounting.peak_nodes = accounting.live_nodes;
    if (accounting.live_bytes > accounting.peak_bytes) accounting.peak_bytes = accounting.live_bytes;
}
void wf_ecosystem_note_dealloc(uint64_t bytes) {
    require(accounting.live_nodes > 0 && accounting.live_bytes >= bytes, "accounted deallocation extent");
    --accounting.live_nodes;
    accounting.live_bytes -= (size_t)bytes;
}
void wf_ecosystem_note_realloc(uint64_t old_bytes, uint64_t new_bytes) {
    wf_ecosystem_note_dealloc(old_bytes);
    wf_ecosystem_note_alloc(new_bytes);
}
__attribute__((malloc, returns_nonnull)) void *wf_cost_allocate(uint64_t bytes) {
    require(bytes == required_node_bytes, "exact per-layout node allocation extent");
    require(bytes <= SIZE_MAX - sizeof(AllocationHeader), "allocation extent");
    AllocationHeader *header = malloc(sizeof *header + (size_t)bytes);
    require(header != NULL, "host allocation failure");
    header->value.bytes = (size_t)bytes;
    header->value.identity = UINT64_C(0x6f726465726564);
    wf_ecosystem_note_alloc(bytes);
    return header + 1;
}
void wf_cost_release(void *pointer) {
    if (pointer == NULL) return;
    AllocationHeader *header = (AllocationHeader *)pointer - 1;
    require(header->value.identity == UINT64_C(0x6f726465726564), "allocation identity");
    wf_ecosystem_note_dealloc(header->value.bytes);
    header->value.identity = 0;
    free(header);
}
#endif

static void ecosystem_reset(unsigned variant, bool wide) {
    reset_accounting(variant < 4 ? variant : 0, wide);
}
static uint64_t ecosystem_run(unsigned variant, bool wide, uint64_t count,
                              uint64_t rounds, uint64_t seed, uint64_t path) {
    uint64_t value;
    switch (variant) {
    case 0: value = wide ? wf_ordered_cost_record_trace(count, rounds, seed, path)
                        : wf_ordered_cost_word_trace(count, rounds, seed, path); break;
    case 1: value = wide ? record_source_trace(count, rounds, seed, path)
                        : word_source_trace(count, rounds, seed, path); break;
    case 2: value = wide ? record_direct_trace(count, rounds, seed, path)
                        : word_direct_trace(count, rounds, seed, path); break;
    case 3: value = wide ? record_avl_trace(count, rounds, seed, path)
                        : word_avl_trace(count, rounds, seed, path); break;
    case 4: value = wide ? rust_ordered_record_trace(count, rounds, seed, path)
                        : rust_ordered_word_trace(count, rounds, seed, path); break;
    case 5: value = wide ? cpp_ordered_record_trace(count, rounds, seed, path)
                        : cpp_ordered_word_trace(count, rounds, seed, path); break;
    default: value = wide ? absl_ordered_record_trace(count, rounds, seed, path)
                         : absl_ordered_word_trace(count, rounds, seed, path); break;
    }
    observed = value;
    return value;
}
static void ecosystem_checksum(uint64_t actual, uint64_t expected) {
    require(actual == expected, "ecosystem independent sorted sequence checksum");
}
static void ecosystem_cleanup(void) {
    require(accounting.live_nodes == 0 && accounting.live_bytes == 0, "ecosystem complete allocation cleanup");
#if !defined(ACCOUNT_ONLY)
    const Accounting empty = {0};
    require(memcmp(&accounting, &empty, sizeof empty) == 0, "ecosystem timed allocation counters disabled");
#endif
}
static void ecosystem_check(void) {
    const uint64_t counts[] = {0, 1, 2, 7, 8, 15, 16, 31, 63, 256, 4096};
    const uint64_t seeds[] = {0, 17, UINT64_MAX};
    size_t configurations = 0;
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned path = 0; path < 5; ++path)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n)
                for (size_t s = 0; s < sizeof seeds / sizeof seeds[0]; ++s) {
                    uint64_t rounds = path == 0 ? 0 : 2;
                    uint64_t expected = oracle(wide, counts[n], rounds, seeds[s], path);
                    Accounting matched = {0};
                    for (unsigned variant = 0; variant < VARIANTS; ++variant) {
                        ecosystem_reset(variant, wide);
                        uint64_t actual = ecosystem_run(variant, wide, counts[n], rounds, seeds[s], path);
                        if (actual != expected)
                            fprintf(stderr, "variant=%s wide=%u path=%u count=%" PRIu64 " seed=%" PRIu64 " actual=%" PRIu64 " expected=%" PRIu64 "\n",
                                ecosystem_variants[variant], wide, path, counts[n], seeds[s], actual, expected);
                        ecosystem_checksum(actual, expected);
                        ecosystem_cleanup();
                        if (variant == 0) matched = accounting;
                        if (variant == 1)
                            require(memcmp(&matched, &accounting, sizeof matched) == 0, "ecosystem WF/source C allocation agreement");
                    }
                    ++configurations;
                }
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned variant = 1; variant < 4; ++variant) {
            ecosystem_reset(variant, wide);
            if (variant == 1) { if (wide) record_source_audit(); else word_source_audit(); }
            else if (variant == 2) { if (wide) record_direct_audit(); else word_direct_audit(); }
            else { if (wide) record_avl_audit(); else word_avl_audit(); }
            ecosystem_cleanup();
        }
    void (*audits[])(void) = {rust_ordered_audit, cpp_ordered_audit, absl_ordered_audit};
    for (size_t i = 0; i < sizeof audits / sizeof audits[0]; ++i) {
        ecosystem_reset(4 + (unsigned)i, false);
        audits[i]();
        ecosystem_cleanup();
    }
    printf("ordered ecosystem: %zu configurations, %zu executions, 6 C tree audits and %zu native container audits passed\n",
        configurations, configurations * VARIANTS, sizeof audits / sizeof audits[0]);
}

static void ecosystem_samples(bool account, unsigned cohort, uint64_t scale) {
    const uint64_t counts[] = {8, 256, 4096};
    puts("contract,pair_bytes,path,count,variant,sample,cohort,rounds,traces,elapsed_ns,checksum,requests,requested_bytes,peak_allocations,peak_bytes");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned path = 0; path < 5; ++path)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n) {
                uint64_t count = counts[n];
                uint64_t rounds = path == 0 ? 0 : 8192 / count;
                if (!account) {
                    uint64_t expected = oracle(wide, count, rounds, 101, path);
                    for (unsigned offset = 0; offset < VARIANTS; ++offset) {
                        unsigned variant = cohort ? VARIANTS - 1 - offset : offset;
                        ecosystem_reset(variant, wide);
                        uint64_t actual = ecosystem_run(variant, wide, count, rounds, 101, path);
                        ecosystem_checksum(actual, expected);
                        ecosystem_cleanup();
                    }
                }
                for (unsigned sample = 0; sample < (account ? 1U : 6U); ++sample) {
                    uint64_t seed = 101 + sample;
                    uint64_t traces = account ? 1 : (path == 0 ? 4096 / count : 1) * scale;
                    uint64_t expected = 0;
                    for (uint64_t t = 0; t < traces; ++t)
                        expected = expected * 257 + oracle(wide, count, rounds, seed + t, path);
                    Accounting matched[2];
                    for (unsigned offset = 0; offset < VARIANTS; ++offset) {
                        unsigned position = (sample + offset) % VARIANTS;
                        unsigned variant = cohort ? VARIANTS - 1 - position : position;
                        ecosystem_reset(variant, wide);
                        uint64_t checksum = 0, before = account ? 0 : nanos();
                        for (uint64_t t = 0; t < traces; ++t)
                            checksum = checksum * 257 + ecosystem_run(variant, wide, count, rounds, seed + t, path);
                        uint64_t elapsed = account ? 0 : nanos() - before;
                        ecosystem_checksum(checksum, expected);
                        ecosystem_cleanup();
                        if (variant < 2) matched[variant] = accounting;
                        printf("%s,%u,%s,%" PRIu64 ",%s,%u,%u,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%zu,%zu,%zu,%zu\n",
                            account ? "account" : "normal", wide ? 264 : 16, ecosystem_paths[path], count,
                            ecosystem_variants[variant], sample, cohort, rounds, traces, elapsed, checksum,
                            accounting.requests, accounting.requested_bytes, accounting.peak_nodes, accounting.peak_bytes);
                    }
                    require(memcmp(&matched[0], &matched[1], sizeof matched[0]) == 0, "ecosystem WF/source C allocation agreement");
                }
            }
}

static uint64_t ecosystem_scale(const char *text) {
    char *end;
    errno = 0;
    unsigned long long value = strtoull(text, &end, 10);
    require(errno == 0 && end != text && *end == '\0' && value >= 1 && value <= 1024,
        "ecosystem scale must be in 1..1024");
    return (uint64_t)value;
}
int main(int argc, char **argv) {
    require(argc >= 2, "usage: ordered-ecosystem check|account|measure [cohort scale]|clock-resolution|check-fault-checksum|check-fault-cleanup");
    if (strcmp(argv[1], "check") == 0) {
        require(argc == 2, "check takes no arguments");
        ecosystem_check();
    } else if (strcmp(argv[1], "account") == 0) {
#if !defined(ACCOUNT_ONLY)
        require(false, "account requires the accounting executable");
#endif
        require(argc == 2, "account takes no arguments");
        ecosystem_samples(true, 0, 1);
    } else if (strcmp(argv[1], "measure") == 0) {
#if defined(ACCOUNT_ONLY)
        require(false, "measure requires the ordinary allocation executable");
#endif
        require(argc == 4 && (strcmp(argv[2], "0") == 0 || strcmp(argv[2], "1") == 0),
            "measure requires cohort 0|1 and scale 1..1024");
        ecosystem_samples(false, (unsigned)(argv[2][0] - '0'), ecosystem_scale(argv[3]));
    } else if (strcmp(argv[1], "clock-resolution") == 0) {
        require(argc == 2, "clock-resolution takes no arguments");
        clock_resolution();
    } else if (strcmp(argv[1], "check-fault-checksum") == 0 || strcmp(argv[1], "check-fault-cleanup") == 0) {
        require(argc == 2, "failure controls take no arguments");
        ecosystem_reset(0, false);
        uint64_t expected = oracle(false, 8, 0, 101, 0);
        uint64_t actual = ecosystem_run(0, false, 8, 0, 101, 0);
        if (strcmp(argv[1], "check-fault-checksum") == 0) actual ^= 1;
        else ++accounting.live_nodes;
        ecosystem_checksum(actual, expected);
        ecosystem_cleanup();
    } else require(false, "unknown ordered ecosystem command");
    return 0;
}
