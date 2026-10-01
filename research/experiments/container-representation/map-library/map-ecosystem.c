/* Practical HashMap comparison. The historical driver shares its independent
 * key-ID oracle through map-oracle.h. Retire this driver with the explicit
 * ecosystem targets; it is not a compiler or conformance dependency. */
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#endif
#if !defined(_WIN32)
#define _POSIX_C_SOURCE 200809L
#endif
#include <inttypes.h>
#include <math.h>
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
enum { WORDS = 32, SAMPLE_COUNT = 11, LOOKUP_SAMPLE_COUNT = 9,
       IMPLEMENTATIONS = 5, LOOKUP_IMPLEMENTATIONS = 4 };
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
static const Shape occupancy_shapes[] = {{4096, 3584}, {5120, 3584}, {6144, 3584}, {8192, 3584}};
static const unsigned occupancy_paths[] = {HIT, MISS, REPLACE, EDIT};
#define ELEMENTS(ARRAY) (sizeof(ARRAY) / sizeof((ARRAY)[0]))
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

/* The lookup owner lives in the caller's aligned storage. The Whitefoot
 * emitted ABI uses a 72-byte result; native peers assert the 128-byte bound. */
typedef union { max_align_t alignment; _Alignas(16) unsigned char bytes[128]; } LookupStorage;
_Static_assert(_Alignof(LookupStorage) >= 16 && sizeof(LookupStorage) >= 128,
               "lookup owner storage ABI");
typedef void (*LookupPrepare)(void *, uint64_t, uint64_t, uint64_t);
typedef uint64_t (*LookupQuery)(const void *, uint64_t, uint64_t);
typedef uint64_t (*LookupGeometry)(const void *);
typedef uint64_t (*LookupFinish)(void *);
typedef struct {
    const char *name;
    const char *capacity_kind;
    bool wide;
    LookupPrepare prepare;
    LookupQuery query;
    LookupGeometry geometry;
    LookupFinish finish;
} LookupApi;
#define LOOKUP_DECLARE(PREFIX, WIDTH) \
    extern void PREFIX##_lookup_##WIDTH##_prepare(void *, uint64_t, uint64_t, uint64_t); \
    extern uint64_t PREFIX##_lookup_##WIDTH##_query(const void *, uint64_t, uint64_t); \
    extern uint64_t PREFIX##_lookup_##WIDTH##_geometry(const void *); \
    extern uint64_t PREFIX##_lookup_##WIDTH##_finish(void *)
LOOKUP_DECLARE(wf_map_cost, word);
LOOKUP_DECLARE(wf_map_cost, record);
LOOKUP_DECLARE(eco_rust_map, word);
LOOKUP_DECLARE(eco_rust_map, record);
LOOKUP_DECLARE(eco_cpp_map, word);
LOOKUP_DECLARE(eco_cpp_map, record);
LOOKUP_DECLARE(eco_c_map, word);
LOOKUP_DECLARE(eco_c_map, record);
#undef LOOKUP_DECLARE
#define LOOKUP_ENTRY(NAME, KIND, WIDE, PREFIX, WIDTH) \
    {NAME, KIND, WIDE, PREFIX##_lookup_##WIDTH##_prepare, \
     PREFIX##_lookup_##WIDTH##_query, PREFIX##_lookup_##WIDTH##_geometry, \
     PREFIX##_lookup_##WIDTH##_finish}
static const LookupApi lookup_apis[] = {
    LOOKUP_ENTRY("whitefoot-hash-map", "physical-slots", false, wf_map_cost, word),
    LOOKUP_ENTRY("rust-hash-map", "usable-entries", false, eco_rust_map, word),
    LOOKUP_ENTRY("cpp-unordered-map", "chaining-buckets", false, eco_cpp_map, word),
    LOOKUP_ENTRY("c-linear-probe-attribution", "physical-slots", false, eco_c_map, word),
    LOOKUP_ENTRY("whitefoot-hash-map", "physical-slots", true, wf_map_cost, record),
    LOOKUP_ENTRY("rust-hash-map", "usable-entries", true, eco_rust_map, record),
    LOOKUP_ENTRY("cpp-unordered-map", "chaining-buckets", true, eco_cpp_map, record),
    LOOKUP_ENTRY("c-linear-probe-attribution", "physical-slots", true, eco_c_map, record),
};
#undef LOOKUP_ENTRY

static uint64_t lookup_query_oracle(uint64_t count, uint64_t rounds, uint64_t seed, bool miss) {
    Digest digest = {seed, 0, 0, 0};
    for (uint64_t round = 0; round < rounds; ++round)
        for (uint64_t index = 0; index < count; ++index) {
            ordered(&digest, !miss);
            if (!miss) ordered(&digest, seed + index);
        }
    return finish(digest);
}

static uint64_t lookup_cleanup_oracle(bool wide, uint64_t count, uint64_t seed) {
    Digest digest = {seed, 0, 0, 0};
    for (uint64_t index = 0; index < count; ++index)
        final_value(&digest, oracle_content(wide, key_at(index), seed + index));
    return finish(digest);
}

static uint64_t lookup_expected_geometry(const LookupApi *api, uint64_t slots) {
    return strcmp(api->capacity_kind, "usable-entries") == 0 ? slots - slots / 8 : slots;
}

static uint64_t lookup_rounds(uint64_t count, uint64_t work) {
    return (work + count - 1) / count;
}

typedef enum {
    LOOKUP_FAULT_NONE, LOOKUP_FAULT_POPULATION, LOOKUP_FAULT_CAPACITY,
    LOOKUP_FAULT_FILLED_ALLOCATION, LOOKUP_FAULT_CHECKSUM, LOOKUP_FAULT_CLEANUP,
    LOOKUP_FAULT_ALLOCATION, LOOKUP_FAULT_LEAK
} LookupFault;

static void lookup_check_one(const LookupApi *api, uint64_t slots, uint64_t seed, bool miss,
                             uint64_t rounds, LookupFault fault) {
    uint64_t count = slots / 2;
    LookupStorage storage = {0};
    reset_ledger();
    api->prepare(storage.bytes, slots, count, seed);
    uint64_t encoded = api->geometry(storage.bytes);
    if (fault == LOOKUP_FAULT_POPULATION) encoded ^= UINT64_C(1);
    if (fault == LOOKUP_FAULT_CAPACITY) encoded ^= UINT64_C(1) << 32;
    require((uint32_t)encoded == count, "lookup filled population");
    require(encoded >> 32 == lookup_expected_geometry(api, slots), "lookup filled backing geometry");
#ifdef ACCOUNT_ONLY
    Ledger filled = ledger;
    if (fault == LOOKUP_FAULT_FILLED_ALLOCATION) filled.live = 0;
    require(filled.live > 0, "lookup filled allocation");
#endif
    uint64_t actual = api->query(storage.bytes, rounds, miss);
    if (fault == LOOKUP_FAULT_CHECKSUM) actual ^= UINT64_C(1);
    require(actual == lookup_query_oracle(count, rounds, seed, miss),
            "lookup independent query oracle");
#ifdef ACCOUNT_ONLY
    if (fault == LOOKUP_FAULT_ALLOCATION) wf_ecosystem_note_alloc(8);
    require(memcmp(&ledger, &filled, sizeof ledger) == 0, "lookup query has no allocation");
#endif
    uint64_t cleanup = api->finish(storage.bytes);
    if (fault == LOOKUP_FAULT_CLEANUP) cleanup ^= UINT64_C(1);
    require(cleanup == lookup_cleanup_oracle(api->wide, count, seed),
            "lookup independent full-content cleanup oracle");
#ifdef ACCOUNT_ONLY
    if (fault == LOOKUP_FAULT_LEAK) wf_ecosystem_note_alloc(8);
#endif
    clean_ledger();
    observed ^= actual ^ cleanup;
}

static void lookup_check(void) {
    const uint64_t slots[] = {64, 4096};
    const uint64_t seeds[] = {17, 101, UINT64_MAX};
    size_t checked = 0;
    for (size_t v = 0; v < ELEMENTS(lookup_apis); ++v)
        for (size_t s = 0; s < ELEMENTS(slots); ++s)
            for (size_t n = 0; n < ELEMENTS(seeds); ++n)
                for (unsigned miss = 0; miss < 2; ++miss) {
                    lookup_check_one(&lookup_apis[v], slots[s], seeds[n], miss != 0, 2,
                                     LOOKUP_FAULT_NONE);
                    ++checked;
                }
    printf("map lookup: %zu isolated owner/query/cleanup cases passed\n", checked);
}

static void lookup_negative(const char *failure) {
    LookupFault fault = LOOKUP_FAULT_NONE;
    if (strcmp(failure, "population") == 0) fault = LOOKUP_FAULT_POPULATION;
    else if (strcmp(failure, "capacity") == 0) fault = LOOKUP_FAULT_CAPACITY;
    else if (strcmp(failure, "checksum") == 0) fault = LOOKUP_FAULT_CHECKSUM;
    else if (strcmp(failure, "cleanup") == 0) fault = LOOKUP_FAULT_CLEANUP;
#ifdef ACCOUNT_ONLY
    else if (strcmp(failure, "allocation") == 0) fault = LOOKUP_FAULT_ALLOCATION;
    else if (strcmp(failure, "leak") == 0) fault = LOOKUP_FAULT_LEAK;
    else if (strcmp(failure, "filled-allocation") == 0) fault = LOOKUP_FAULT_FILLED_ALLOCATION;
#endif
    else require(false, "unknown lookup negative control");
    lookup_check_one(&lookup_apis[0], 64, 17, false, 2, fault);
    require(false, "lookup negative control was not refused");
}


/* Isolated reserve: logical entry headroom, ordinary library growth policies. */
typedef uint64_t (*ReserveStep)(void *, uint64_t);
typedef uint64_t (*ReserveInsert)(void *, uint64_t, uint64_t);
typedef double (*ReserveLoad)(const void *);
typedef struct {
    const char *name;
    unsigned peer, series;
    bool wide;
    LookupPrepare prepare;
    ReserveStep step;
    LookupGeometry geometry;
    ReserveLoad load_factor;
    ReserveInsert insert;
    LookupFinish finish;
} ReserveApi;
#define RESERVE_DECLARE(P) \
    extern void P##_prepare(void *, uint64_t, uint64_t, uint64_t); \
    extern uint64_t P##_step(void *, uint64_t); \
    extern uint64_t P##_geometry(const void *); \
    extern uint64_t P##_insert(void *, uint64_t, uint64_t); \
    extern uint64_t P##_finish(void *)
#define RESERVE_WF(W) \
    extern void wf_map_cost_reserve_##W##_prepare(void *, uint64_t, uint64_t, uint64_t); \
    extern uint64_t wf_map_cost_reserve_##W##_step(void *, uint64_t); \
    extern uint64_t wf_map_cost_reserve_##W##_raw(void *, uint64_t); \
    extern uint64_t wf_map_cost_reserve_##W##_insert(void *, uint64_t, uint64_t)
RESERVE_WF(word); RESERVE_WF(record);
#define RESERVE_PEERS(W,S) \
    RESERVE_DECLARE(eco_rust_map_reserve_##W##_##S); \
    RESERVE_DECLARE(eco_cpp_map_reserve_##W##_##S); \
    extern double eco_cpp_map_reserve_##W##_##S##_load_factor(const void *)
RESERVE_PEERS(word,default); RESERVE_PEERS(word,aligned);
RESERVE_PEERS(record,default); RESERVE_PEERS(record,aligned);
#undef RESERVE_PEERS
#undef RESERVE_WF
#undef RESERVE_DECLARE
#define RESERVE_WF_ENTRY(W,B,S) {"whitefoot-hash-map",0,S,B,wf_map_cost_reserve_##W##_prepare,wf_map_cost_reserve_##W##_step,wf_map_cost_lookup_##W##_geometry,NULL,wf_map_cost_reserve_##W##_insert,wf_map_cost_lookup_##W##_finish}
#define RESERVE_PEER_ENTRY(N,P,W,B,S,H,L) {N,P,S,B,H##_prepare,H##_step,H##_geometry,L,H##_insert,H##_finish}
#define RESERVE_ENTRIES(W,B,S,H) \
    RESERVE_WF_ENTRY(W,B,S), \
    RESERVE_PEER_ENTRY("rust-hash-map",1,W,B,S,eco_rust_map_reserve_##W##_##H,NULL), \
    RESERVE_PEER_ENTRY("cpp-unordered-map",2,W,B,S,eco_cpp_map_reserve_##W##_##H,eco_cpp_map_reserve_##W##_##H##_load_factor)
static const ReserveApi reserve_apis[] = {
    RESERVE_ENTRIES(word,false,0,default), RESERVE_ENTRIES(word,false,1,aligned),
    RESERVE_ENTRIES(record,true,0,default), RESERVE_ENTRIES(record,true,1,aligned)
};
#undef RESERVE_ENTRIES
#undef RESERVE_PEER_ENTRY
#undef RESERVE_WF_ENTRY
static uint64_t reserve_slots(uint64_t total) { return 4 * (total - 1) / 3 + 1; }
static uint64_t reserve_usable(const ReserveApi *api, const void *storage, uint64_t cap) {
    if (api->peer == 0) return cap - cap / 4;
    if (api->peer == 1) return cap;
    return (uint64_t)((double)cap * api->load_factor(storage));
}
#ifdef ACCOUNT_ONLY
static uint64_t reserve_backing_bytes(const ReserveApi *api, uint64_t cap) {
    if (api->peer == 0) return cap * (api->wide ? 272 : 24);
    if (api->peer == 2) return cap * sizeof(void *);
    require(cap % 7 == 0, "reserve Rust usable-to-slot geometry");
    uint64_t slots = cap / 7 * 8;
    /* Pinned rustc 1.98.1 / hashbrown 0.17.1 group selection. Its aarch64 NEON
     * group is uint8x8_t; x86 SSE2 uses 16. Refuse unsupported accounting targets
     * rather than infer this independent byte expectation from the ledger. */
#if defined(__aarch64__)
    const uint64_t group_width = 8;
#elif (defined(__x86_64__) || defined(__i386__)) && defined(__SSE2__)
    const uint64_t group_width = 16;
#else
    require(false, "reserve Rust layout needs target qualification");
    const uint64_t group_width = 0;
#endif
    return slots * ((api->wide ? 264 : 16) + 1) + group_width;
}
#endif
/* Faults mutate observations except payload corruption, which changes the actual
 * last word of an inserted record. Raw-slot reserve deliberately uses the public
 * WF API with the wrong application adaptation to expose the headroom gap. */
enum { RESERVE_OK, RESERVE_OMIT, RESERVE_RAW, RESERVE_PAYLOAD, RESERVE_LENGTH,
       RESERVE_CAPACITY, RESERVE_LEAK, RESERVE_ALLOCATION };
static void reserve_case(const ReserveApi *api, uint64_t initial, uint64_t seed,
                         bool grow, bool fill, unsigned fault, bool print) {
    uint64_t count = initial / 2, target = grow ? initial * 2 : initial;
    LookupStorage owner = {0};
    reset_ledger();
    api->prepare(owner.bytes, initial, count, seed);
    uint64_t before = api->geometry(owner.bytes), oldcap = before >> 32;
    if (fault == RESERVE_LENGTH) before ^= 1;
    require((uint32_t)before == count, "reserve prepared population");
    if (api->peer == 0) require(oldcap == reserve_slots(initial), "reserve WF exact initial physical request");
    require(reserve_usable(api, owner.bytes, oldcap) >= initial, "reserve initial entry floor");
    if (grow) require(reserve_usable(api, owner.bytes, oldcap) < target,
                      "reserve growth is required before call");
#ifdef ACCOUNT_ONLY
    Ledger prepared = ledger;
    require(prepared.releases == 0 && prepared.requests == (api->peer == 2 ? count + 1 : 1),
            "reserve preparation request ledger");
    require(prepared.live == prepared.bytes, "reserve preparation live bytes");
    uint64_t oldbacking = reserve_backing_bytes(api, oldcap);
    uint64_t nodebytes = 0;
    if (api->peer == 2) {
        require(prepared.live >= oldbacking && (prepared.live - oldbacking) % count == 0,
                "reserve node allocation geometry");
        nodebytes = (prepared.live - oldbacking) / count;
        require(nodebytes >= (api->wide ? 264 : 16), "reserve node contains complete pair");
#if defined(__APPLE__) && defined(__aarch64__)
        /* Current libc++ __hash_node: next pointer, size_t hash, complete pair.
         * This source-grounded arm64 layout is separate from the observed delta. */
        require(nodebytes == (api->wide ? 280 : 32), "reserve libc++ node layout");
#endif
    } else require(prepared.live == oldbacking, "reserve prepared backing bytes");
#endif
    uint64_t receipt = 1;
    if (fault == RESERVE_RAW) {
        receipt = api->wide ? wf_map_cost_reserve_record_raw(owner.bytes, target)
                            : wf_map_cost_reserve_word_raw(owner.bytes, target);
    } else if (fault != RESERVE_OMIT) receipt = api->step(owner.bytes, target);
    require(receipt == 1, "reserve public success result");
    uint64_t after = api->geometry(owner.bytes), newcap = after >> 32;
    require((uint32_t)after == count, "reserve preserves population");
    if (fault == RESERVE_CAPACITY) newcap = 0;
    /* The raw-slot negative proceeds beyond the insufficient logical floor so
     * the stronger future-put observation, not this numeric check, rejects it. */
    if (fault != RESERVE_RAW)
        require(reserve_usable(api, owner.bytes, newcap) >= target, "reserve application entry floor");
    else require(newcap >= target, "reserve raw physical floor");
    if (api->peer == 0 && fault != RESERVE_RAW)
        require(newcap == reserve_slots(target), "reserve WF exact final physical request");
    if (!grow) require(newcap == oldcap, "reserve noop geometry");
#ifdef ACCOUNT_ONLY
    if (fault == RESERVE_ALLOCATION) wf_ecosystem_note_alloc(8);
    Ledger reserved = ledger;
    uint64_t newbacking = reserve_backing_bytes(api, newcap);
    if (grow) {
        require(reserved.requests == prepared.requests + 1 && reserved.releases == 1,
                "reserve growth request/release ledger");
        require(reserved.bytes == prepared.bytes + newbacking &&
                reserved.live == prepared.live - oldbacking + newbacking &&
                reserved.peak == prepared.live + newbacking,
                "reserve growth exact backing bytes");
    } else require(memcmp(&reserved, &prepared, sizeof reserved) == 0,
                   "reserve noop has no allocation");
#endif
    uint64_t final_count = count;
    if (fill) {
        for (uint64_t i = count; i < target; ++i) {
            require(api->insert(owner.bytes, i, fault == RESERVE_PAYLOAD && i + 1 == target) == 1,
                    "reserve future put inserts fresh owner");
            uint64_t current = api->geometry(owner.bytes);
            if ((current >> 32) != (after >> 32)) {
                fprintf(stderr, "reserve headroom first growth at insertion %" PRIu64 " of %" PRIu64 "\n", i + 1, target);
                require(false, "reserve future puts retain backing capacity");
            }
            require((uint32_t)current == i + 1, "reserve future put population");
        }
        final_count = target;
    }
#ifdef ACCOUNT_ONLY
    Ledger filled = ledger;
    uint64_t added = final_count - count;
    require(filled.releases == reserved.releases &&
            filled.requests == reserved.requests + (api->peer == 2 ? added : 0) &&
            filled.bytes == reserved.bytes + nodebytes * added &&
            filled.live == reserved.live + nodebytes * added,
            "reserve future puts allocate only C++ nodes");
#endif
    double maximum_load = api->load_factor ? api->load_factor(owner.bytes) : 0.0;
    uint64_t cleanup = api->finish(owner.bytes);
    require(cleanup == lookup_cleanup_oracle(api->wide, final_count, seed),
            "reserve independent complete-owner oracle");
#ifdef ACCOUNT_ONLY
    if (fault == RESERVE_LEAK) wf_ecosystem_note_alloc(8);
    clean_ledger();
    require(ledger.releases == ledger.requests, "reserve complete release ledger");
    if (print) printf("isolated-reserve,%u,%s,%s,%s,%.17g,%s,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%u,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
        api->wide ? 256 : 8, api->series ? "aligned-hash" : "native-default", api->name,
        api->peer == 0 ? "physical-slots" : api->peer == 1 ? "usable-entries" : "chaining-buckets",
        maximum_load, grow ? "grow" : "noop", initial, count, target, oldcap, after >> 32, final_count, fill,
        prepared.requests, prepared.bytes, prepared.live,
        reserved.requests - prepared.requests, reserved.releases - prepared.releases,
        reserved.bytes - prepared.bytes, reserved.live, reserved.peak,
        filled.requests - reserved.requests, filled.bytes - reserved.bytes,
        ledger.requests, ledger.releases, ledger.live, cleanup);
#else
    (void)print; (void)maximum_load;
#endif
    observed ^= cleanup;
}
#ifdef ACCOUNT_ONLY
static void reserve_batch_bound(const ReserveApi *api, uint64_t initial, bool grow) {
    LookupStorage owners[256];
    uint64_t contexts = initial == 64 ? 256 : 8, count = initial / 2;
    reset_ledger();
    for (uint64_t i = 0; i < contexts; ++i) api->prepare(owners[i].bytes, initial, count, 101 + i);
    for (uint64_t i = 0; i < contexts; ++i)
        require(api->step(owners[i].bytes, grow ? initial * 2 : initial) == 1, "reserve batch success");
    for (uint64_t i = 0; i < contexts; ++i)
        require(api->finish(owners[i].bytes) == lookup_cleanup_oracle(api->wide, count, 101 + i),
                "reserve batch complete-owner oracle");
    clean_ledger();
    require(ledger.peak <= UINT64_C(67108864), "reserve fixed batch memory ceiling");
}
#endif
static void reserve_check(bool print) {
    const uint64_t sizes[] = {64,4096}, seeds[] = {17,101,UINT64_MAX};
    size_t checks = 0;
    if (print) puts("contract,element_bytes,series,variant,capacity_kind,max_load_factor,path,initial_floor,initial_count,target_floor,old_capacity,new_capacity,final_count,headroom_fill,prepare_requests,prepare_bytes,prepare_live,reserve_requests,reserve_releases,reserve_bytes,reserve_live,reserve_peak,fill_requests,fill_bytes,total_requests,total_releases,final_live,cleanup_checksum");
    for (size_t a = 0; a < ELEMENTS(reserve_apis); ++a)
        for (size_t n = 0; n < ELEMENTS(sizes); ++n)
            for (unsigned grow = 0; grow < 2; ++grow) {
                for (unsigned fill = 0; fill < 2; ++fill)
                    for (size_t s = 0; s < (print ? 1 : ELEMENTS(seeds)); ++s) {
                        reserve_case(&reserve_apis[a], sizes[n], seeds[s], grow != 0, fill != 0, RESERVE_OK, print);
                        ++checks;
                    }
#ifdef ACCOUNT_ONLY
                reserve_batch_bound(&reserve_apis[a], sizes[n], grow != 0);
#endif
            }
    if (!print) printf("map reserve: %zu ordinary reserve/owner/headroom cases passed\n", checks);
}
static void reserve_negative(const char *fault, unsigned peer) {
    require(peer < 3, "reserve negative peer 0..2");
    unsigned kind = RESERVE_OK;
    bool grow = true;
    if (!strcmp(fault,"omitted")) kind = RESERVE_OMIT;
    else if (!strcmp(fault,"raw-slots")) { require(peer == 0, "raw-slots is WF only"); kind = RESERVE_RAW; }
    else if (!strcmp(fault,"payload")) kind = RESERVE_PAYLOAD;
    else if (!strcmp(fault,"population")) kind = RESERVE_LENGTH;
    else if (!strcmp(fault,"capacity")) kind = RESERVE_CAPACITY;
#ifdef ACCOUNT_ONLY
    else if (!strcmp(fault,"leak")) kind = RESERVE_LEAK;
    else if (!strcmp(fault,"allocation")) { kind = RESERVE_ALLOCATION; grow = false; }
#endif
    else require(false, "unknown reserve fault");
    reserve_case(&reserve_apis[6 + peer],64,17,grow,true,kind,false);
    require(false,"reserve negative unexpectedly returned");
}


#ifndef ACCOUNT_ONLY
static uint64_t reserve_now(void) {
#if defined(__APPLE__)
    struct timespec t;
    require(clock_gettime(CLOCK_MONOTONIC_RAW, &t) == 0, "reserve RAW clock read");
    return (uint64_t)t.tv_sec * UINT64_C(1000000000) + (uint64_t)t.tv_nsec;
#elif defined(_WIN32)
    LARGE_INTEGER count, frequency;
    QueryPerformanceCounter(&count); QueryPerformanceFrequency(&frequency);
    return (uint64_t)((long double)count.QuadPart * 1000000000.0L / frequency.QuadPart);
#else
    struct timespec t;
    require(clock_gettime(CLOCK_MONOTONIC, &t) == 0, "reserve monotonic clock read");
    return (uint64_t)t.tv_sec * UINT64_C(1000000000) + (uint64_t)t.tv_nsec;
#endif
}
static uint64_t reserve_clock_check(unsigned fault, bool intervals) {
    uint64_t previous = reserve_now(), minimum = UINT64_MAX, maximum_empty = 0;
    if (fault == 1) previous -= previous % 1000;
    for (unsigned i = 0; i < 200000; ++i) {
        uint64_t current = reserve_now();
        if (fault == 1) current -= current % 1000;
        if (fault == 2 && i == 10) current = previous - 1;
        require(current >= previous, "reserve clock nondecreasing");
        if (current > previous && current - previous < minimum) minimum = current - previous;
        previous = current;
    }
    fprintf(stderr,"reserve clock: minimum_nonzero_ns=%" PRIu64 " fault=%u\n",minimum,fault);
    require(minimum <= 100, "reserve clock quantum at most 100 ns");
    uint64_t empty[1024];
    for (unsigned i = 0; i < 1024; ++i) {
        uint64_t start = reserve_now(), end = reserve_now();
        require(end >= start, "reserve clock nondecreasing");
        empty[i] = end - start;
        if (empty[i] > maximum_empty) maximum_empty = empty[i];
    }
    if (intervals) {
        fputs("reserve clock-only: index,elapsed_ns\n",stderr);
        for (unsigned i = 0; i < 1024; ++i) fprintf(stderr,"%u,%" PRIu64 "\n",i,empty[i]);
    }
    fprintf(stderr,"reserve clock: maximum_empty_ns=%" PRIu64 "\n",maximum_empty);
    return maximum_empty;
}
static uint64_t reserve_loop(const ReserveApi *api, LookupStorage *owners,
                             uint64_t contexts, uint64_t repeats, uint64_t target, bool control) {
    uint64_t receipt = 0;
    if (control) {
        for (uint64_t r = 0; r < repeats; ++r)
            for (uint64_t i = 0; i < contexts; ++i) receipt += api->geometry(owners[i].bytes);
    } else {
        for (uint64_t r = 0; r < repeats; ++r)
            for (uint64_t i = 0; i < contexts; ++i) receipt += api->step(owners[i].bytes,target);
    }
    return receipt;
}
typedef struct { uint64_t elapsed[2], receipt[2], cleanup, oldcap, newcap; bool overhead_ok; } ReserveInterval;
static void reserve_row(const ReserveApi *api, unsigned cohort, uint64_t initial, bool grow,
                        unsigned sample, unsigned control, const char *kind, uint64_t batch,
                        uint64_t contexts, uint64_t calls, uint64_t elapsed, uint64_t receipt,
                        uint64_t cleanup, uint64_t oldcap, uint64_t newcap, bool valid) {
    printf("isolated-reserve,%u,%u,%s,%s,%s,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%u,%u,%s,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%u\n",
           cohort,api->wide ? 256 : 8,api->series ? "aligned-hash" : "native-default",api->name,
           grow ? "grow" : "noop",initial,initial/2,grow ? 2*initial : initial,sample,control,
           kind,batch,contexts,calls,elapsed,receipt,cleanup,oldcap,newcap,valid);
}
static bool reserve_sample(const ReserveApi *api, unsigned cohort, uint64_t initial, bool grow,
                           unsigned sample, uint64_t clock_cost) {
    uint64_t contexts = initial == 64 ? 256 : 8;
    uint64_t calls = grow ? (initial == 64 ? 16384 : 256) : UINT64_C(1048576);
    uint64_t batches = grow ? calls / contexts : 1, repeats = grow ? 1 : calls / contexts;
    uint64_t target = grow ? initial * 2 : initial, count = initial / 2;
    LookupStorage owners[256];
    ReserveInterval records[64] = {0};
    uint64_t elapsed[2] = {0}, receipts[2] = {0}, cleanup = 0;
    bool valid = true;
    for (uint64_t batch = 0; batch < batches; ++batch) {
        ReserveInterval *record = &records[batch];
        uint64_t first_seed = 101 + sample + batch * contexts;
        for (uint64_t i = 0; i < contexts; ++i) {
            api->prepare(owners[i].bytes,initial,count,first_seed+i);
            uint64_t geometry = api->geometry(owners[i].bytes);
            require((uint32_t)geometry == count, "reserve timed prepared population");
            require(reserve_usable(api,owners[i].bytes,geometry>>32) >= initial,
                    "reserve timed initial floor");
            if (grow) require(reserve_usable(api,owners[i].bytes,geometry>>32) < target,
                              "reserve timed call requires growth");
            if (i == 0) record->oldcap = geometry >> 32;
            else require(record->oldcap == geometry >> 32,"reserve batch initial geometry");
        }
        uint64_t start = reserve_now();
        record->receipt[0] = reserve_loop(api,owners,contexts,repeats,target,false);
        uint64_t end = reserve_now();
        require(end >= start,"reserve clock nondecreasing");
        record->elapsed[0] = end-start;
        require(record->receipt[0] == contexts*repeats,"reserve timed success receipts");
        uint64_t expected_control = 0;
        for (uint64_t i = 0; i < contexts; ++i) {
            uint64_t geometry = api->geometry(owners[i].bytes);
            require((uint32_t)geometry == count,"reserve timed preserved population");
            require(reserve_usable(api,owners[i].bytes,geometry>>32) >= target,
                    "reserve timed application floor");
            if (i == 0) record->newcap = geometry >> 32;
            else require(record->newcap == geometry >> 32,"reserve batch final geometry");
            if (!grow) require(record->newcap == record->oldcap,"reserve timed noop geometry");
            expected_control += geometry*repeats;
        }
        /* This post-reserve snapshot loop is an unsubtracted dispatch control.
         * It does not prime the real interval or replace its public operation. */
        start = reserve_now();
        record->receipt[1] = reserve_loop(api,owners,contexts,repeats,target,true);
        end = reserve_now();
        require(end >= start,"reserve clock nondecreasing");
        record->elapsed[1] = end-start;
        require(record->receipt[1] == expected_control,"reserve snapshot control receipts");
        for (uint64_t i = 0; i < contexts; ++i) {
            uint64_t result = api->finish(owners[i].bytes);
            require(result == lookup_cleanup_oracle(api->wide,count,first_seed+i),
                    "reserve timed complete-owner oracle");
            record->cleanup ^= result;
        }
        record->overhead_ok = record->elapsed[0]/100 >= clock_cost;
        valid = valid && record->overhead_ok;
        for (unsigned c = 0; c < 2; ++c) { elapsed[c] += record->elapsed[c]; receipts[c] += record->receipt[c]; }
        cleanup ^= record->cleanup;
    }
    valid = valid && elapsed[0] >= UINT64_C(1000000);
    /* No output during preparation/reserve/inspection batches. Keep all rows,
     * including instrument failures; only the final process verdict can fail. */
    for (uint64_t b = 0; b < batches; ++b)
        for (unsigned c = 0; c < 2; ++c)
            reserve_row(api,cohort,initial,grow,sample,c,"batch",b,contexts,contexts*repeats,
                        records[b].elapsed[c],records[b].receipt[c],records[b].cleanup,
                        records[b].oldcap,records[b].newcap,records[b].overhead_ok);
    for (unsigned c = 0; c < 2; ++c)
        reserve_row(api,cohort,initial,grow,sample,c,"sample",batches,contexts,calls,
                    elapsed[c],receipts[c],cleanup,records[0].oldcap,records[0].newcap,valid);
    observed ^= cleanup ^ receipts[0] ^ receipts[1];
    return valid;
}
static void reserve_measure(unsigned cohort) {
    uint64_t clock_cost = reserve_clock_check(0,true);
    bool valid = true;
    puts("contract,cohort,element_bytes,series,variant,path,initial_floor,initial_count,target_floor,sample,control,row_kind,batch_or_batches,contexts,calls,elapsed_ns,receipt,cleanup_checksum,old_capacity,new_capacity,instrument_valid");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned series = 0; series < 2; ++series)
            for (unsigned size = 0; size < 2; ++size)
                for (unsigned op = 0; op < 2; ++op)
                    for (unsigned sample = 0; sample < 9; ++sample)
                        for (unsigned offset = 0; offset < 3; ++offset) {
                            unsigned peer = (sample+offset)%3;
                            if (cohort) peer = 2-peer;
                            uint64_t initial = (size ^ cohort) ? 4096 : 64;
                            bool passed = reserve_sample(&reserve_apis[wide*6+series*3+peer],
                                                         cohort,initial,op != 0,sample,clock_cost);
                            valid = valid && passed;
                        }
    require(valid,"reserve fixed-panel instrument qualification");
}
#endif

static void occupancy_check(void) {
    const uint64_t rounds[] = {0, 1, 3}, seeds[] = {17, 101, UINT64_MAX};
    size_t executions = 0;
    for (size_t v = 0; v < ELEMENTS(variants); ++v)
        for (size_t s = 0; s < ELEMENTS(occupancy_shapes); ++s)
            for (size_t p = 0; p < ELEMENTS(occupancy_paths); ++p)
                for (size_t r = 0; r < ELEMENTS(rounds); ++r)
                    for (size_t n = 0; n < ELEMENTS(seeds); ++n) {
                        checked_trace(&variants[v], 1, occupancy_shapes[s], rounds[r],
                                      seeds[n], occupancy_paths[p], false);
                        ++executions;
                    }
    printf("map occupancy: %zu oracle-checked traces passed\n", executions);
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

static void lookup_measure(unsigned cohort, uint64_t work) {
    const uint64_t slots[] = {64, 4096};
    puts("contract,cohort,element_bytes,path,physical_backing_target,count,hash,variant,reported_capacity_kind,reported_capacity,sample,rounds,lookups,elapsed_ns,query_checksum,cleanup_checksum");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (size_t s = 0; s < ELEMENTS(slots); ++s)
            for (unsigned miss = 0; miss < 2; ++miss) {
                uint64_t target = slots[cohort ? ELEMENTS(slots) - 1 - s : s];
                uint64_t count = target / 2;
                uint64_t rounds = lookup_rounds(count, work);
                for (unsigned warmup = 0; warmup < 2; ++warmup)
                    for (unsigned offset = 0; offset < LOOKUP_IMPLEMENTATIONS; ++offset) {
                        unsigned peer = cohort ? LOOKUP_IMPLEMENTATIONS - 1 - offset : offset;
                        lookup_check_one(&lookup_apis[wide * LOOKUP_IMPLEMENTATIONS + peer], target,
                                         71 + warmup, miss != 0, rounds, LOOKUP_FAULT_NONE);
                    }
                for (unsigned sample = 0; sample < LOOKUP_SAMPLE_COUNT; ++sample)
                    for (unsigned offset = 0; offset < LOOKUP_IMPLEMENTATIONS; ++offset) {
                        unsigned peer = (sample + offset) % LOOKUP_IMPLEMENTATIONS;
                        if (cohort) peer = LOOKUP_IMPLEMENTATIONS - 1 - peer;
                        const LookupApi *api = &lookup_apis[wide * LOOKUP_IMPLEMENTATIONS + peer];
                        uint64_t seed = 101 + sample;
                        LookupStorage storage = {0};
                        api->prepare(storage.bytes, target, count, seed);
                        uint64_t encoded = api->geometry(storage.bytes);
                        require((uint32_t)encoded == count, "lookup timed filled population");
                        require(encoded >> 32 == lookup_expected_geometry(api, target),
                                "lookup timed filled backing geometry");
                        uint64_t start = nanoseconds();
                        uint64_t checksum = api->query(storage.bytes, rounds, miss);
                        uint64_t elapsed = nanoseconds() - start;
                        require(checksum == lookup_query_oracle(count, rounds, seed, miss),
                                "lookup timed independent query oracle");
                        uint64_t cleanup = api->finish(storage.bytes);
                        require(cleanup == lookup_cleanup_oracle(api->wide, count, seed),
                                "lookup timed independent cleanup oracle");
                        observed ^= checksum ^ cleanup;
                        printf("isolated-lookup,%u,%u,%s,%" PRIu64 ",%" PRIu64
                               ",salted-mix64,%s,%s,%" PRIu64 ",%u,%" PRIu64
                               ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                               cohort, wide ? 256 : 8, miss ? "miss" : "hit",
                               target, count, api->name, api->capacity_kind, encoded >> 32,
                               sample, rounds, rounds * count, elapsed, checksum, cleanup);
                    }
            }
}
static uint64_t batch_oracle(bool wide, Shape shape, unsigned path, uint64_t rounds,
                             uint64_t traces, uint64_t seed) {
    uint64_t checksum = 0;
    for (uint64_t i = 0; i < traces; ++i)
        checksum = checksum * UINT64_C(257) + oracle(wide, shape.count, rounds, seed + i, path);
    return checksum;
}

static void measure(unsigned cohort, uint64_t work, bool occupancy) {
    const Shape *shapes = occupancy ? occupancy_shapes : measured_shapes;
    const unsigned *selected_paths = occupancy ? occupancy_paths : paths;
    size_t shape_count = occupancy ? ELEMENTS(occupancy_shapes) : ELEMENTS(measured_shapes);
    size_t path_count = occupancy ? ELEMENTS(occupancy_paths) : ELEMENTS(paths);
    puts("contract,cohort,series,element_bytes,path,requested_capacity,count,hash,variant,sample,rounds,traces,elapsed_ns,checksum");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (size_t s = 0; s < shape_count; ++s)
            for (size_t p = 0; p < path_count; ++p)
                for (unsigned series = occupancy ? 1 : 0; series < 2; ++series) {
                    Shape shape = shapes[occupancy && cohort ? shape_count - 1 - s : s];
                    unsigned path = selected_paths[p];
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
                            printf("%s,%u,%s,%u,%s,%" PRIu64 ",%" PRIu64 ",%s,%s,%u,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                                   occupancy ? "capacity-sweep" : "normal", cohort,
                                   series ? "aligned-hash" : "native-default", wide ? 256 : 8,
                                   path_names[path], shape.capacity, shape.count, hasher_for(variant, series),
                                   variant->name, sample, rounds, traces, elapsed, checksum);
                        }
                    }
                }
}
#else
static void account(uint64_t work, bool occupancy) {
    const Shape *shapes = occupancy ? occupancy_shapes : measured_shapes;
    const unsigned *selected_paths = occupancy ? occupancy_paths : paths;
    size_t shape_count = occupancy ? ELEMENTS(occupancy_shapes) : ELEMENTS(measured_shapes);
    size_t path_count = occupancy ? ELEMENTS(occupancy_paths) : ELEMENTS(paths);
    puts("contract,series,element_bytes,path,requested_capacity,count,hash,variant,rounds,traces,requests,releases,requested_bytes,peak_bytes,live_bytes,checksum");
    for (size_t v = 0; v < sizeof variants / sizeof variants[0]; ++v)
        for (size_t s = 0; s < shape_count; ++s)
            for (size_t p = 0; p < path_count; ++p)
                for (unsigned series = occupancy ? 1 : 0; series < 2; ++series) {
                    const Variant *variant = &variants[v];
                    Shape shape = shapes[s];
                    unsigned path = selected_paths[p];
                    uint64_t rounds, traces, checksum = 0, expected = 0;
                    trace_size(shape, path, work, &rounds, &traces);
                    for (uint64_t i = 0; i < traces; ++i)
                        expected = expected * UINT64_C(257) + oracle(variant->wide, shape.count, rounds, 101 + i, path);
                    reset_ledger();
                    for (uint64_t i = 0; i < traces; ++i)
                        checksum = checksum * UINT64_C(257)
                            + trace_for(variant, series)(shape.capacity, shape.count, rounds, 101 + i, path, 0);
                    require(checksum == expected, "accounting independent oracle"); clean_ledger();
                    printf("%s,%s,%u,%s,%" PRIu64 ",%" PRIu64 ",%s,%s,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                           occupancy ? "capacity-sweep-accounting" : "accounting",
                           series ? "aligned-hash" : "native-default", variant->wide ? 256 : 8, path_names[path],
                           shape.capacity, shape.count, hasher_for(variant, series), variant->name, rounds, traces,
                           ledger.requests, ledger.releases, ledger.bytes, ledger.peak, ledger.live, checksum);
                }
}

static void lookup_account(uint64_t work) {
    const uint64_t slots[] = {64, 4096};
    puts("contract,element_bytes,path,physical_backing_target,count,hash,variant,reported_capacity_kind,reported_capacity,rounds,lookups,filled_live_bytes,requests,releases,requested_bytes,peak_bytes,final_live_bytes,query_checksum,cleanup_checksum");
    for (size_t v = 0; v < ELEMENTS(lookup_apis); ++v)
        for (size_t s = 0; s < ELEMENTS(slots); ++s)
            for (unsigned miss = 0; miss < 2; ++miss) {
                const LookupApi *api = &lookup_apis[v];
                uint64_t target = slots[s], count = target / 2, seed = 101;
                uint64_t rounds = lookup_rounds(count, work);
                LookupStorage storage = {0};
                reset_ledger();
                api->prepare(storage.bytes, target, count, seed);
                uint64_t encoded = api->geometry(storage.bytes);
                require((uint32_t)encoded == count, "lookup accounted population");
                require(encoded >> 32 == lookup_expected_geometry(api, target),
                        "lookup accounted backing geometry");
                Ledger filled = ledger;
                uint64_t checksum = api->query(storage.bytes, rounds, miss);
                require(checksum == lookup_query_oracle(count, rounds, seed, miss),
                        "lookup accounted query oracle");
                require(memcmp(&ledger, &filled, sizeof ledger) == 0,
                        "lookup accounted query has no allocation");
                uint64_t cleanup = api->finish(storage.bytes);
                require(cleanup == lookup_cleanup_oracle(api->wide, count, seed),
                        "lookup accounted cleanup oracle");
                clean_ledger();
                printf("isolated-lookup-account,%u,%s,%" PRIu64 ",%" PRIu64
                       ",salted-mix64,%s,%s,%" PRIu64 ",%" PRIu64 ",%" PRIu64
                       ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64
                       ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                       api->wide ? 256 : 8, miss ? "miss" : "hit", target, count,
                       api->name, api->capacity_kind, encoded >> 32, rounds, rounds * count,
                       filled.live, ledger.requests, ledger.releases, ledger.bytes, ledger.peak,
                       ledger.live, checksum, cleanup);
            }
}

typedef uint64_t (*GeometryTrace)(uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_library_word_geometry(uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_library_record_geometry(uint64_t, uint64_t, uint64_t);
extern uint64_t eco_c_map_word_geometry(uint64_t, uint64_t, uint64_t);
extern uint64_t eco_c_map_record_geometry(uint64_t, uint64_t, uint64_t);
extern const char *eco_cpp_map_library_identity(void);
extern const char *eco_absl_map_library_identity(void);

enum { WF_GEOMETRY, RUST_GEOMETRY, CPP_GEOMETRY, ABSL_GEOMETRY, C_GEOMETRY };
typedef struct {
    uint64_t calls, kind, count, usable, slots, buckets;
    double load_factor, max_load_factor;
    Ledger filled, complete;
    uint64_t checksum;
    bool diagnostic_consistent;
} Geometry;
static bool capture_geometry;
static Geometry snapshot;

/* Called only by the separately compiled native accounting adapters. Unknown
 * public quantities use UINT64_MAX/-1 internally and empty CSV fields. */
void wf_ecosystem_map_geometry(uint64_t kind, uint64_t count, uint64_t usable,
                               uint64_t slots, uint64_t buckets,
                               double load_factor, double max_load_factor) {
    if (!capture_geometry) return;
    ++snapshot.calls;
    snapshot.kind = kind; snapshot.count = count; snapshot.usable = usable;
    snapshot.slots = slots; snapshot.buckets = buckets;
    snapshot.load_factor = load_factor; snapshot.max_load_factor = max_load_factor;
    snapshot.filled = ledger;
}

/* Setup has no tombstones: apply the current pre-insertion pressure rule.
 * These are independent expectations, never substitutes for observed ledgers. */
static Ledger wf_geometry_expected(bool wide, uint64_t initial, uint64_t count,
                                   uint64_t *final_capacity) {
    uint64_t stride = wide ? 272 : 24, capacity = initial;
    Ledger expected = {capacity ? 1 : 0, 0, stride * capacity,
                       stride * capacity, stride * capacity};
    for (uint64_t live = 0; live < count; ++live) {
        if (capacity >= 16384 || live < capacity - capacity / 4) continue;
        uint64_t target = capacity ? capacity * 2 : 1;
        if (target > 16384) target = 16384;
        uint64_t bytes = stride * target;
        ++expected.requests;
        if (capacity) ++expected.releases;
        expected.bytes += bytes;
        if (expected.live + bytes > expected.peak) expected.peak = expected.live + bytes;
        expected.live = bytes;
        capacity = target;
    }
    *final_capacity = capacity;
    return expected;
}

static void validate_geometry(Geometry value, uint64_t kind, Shape shape, uint64_t expected, bool wide) {
    require(value.checksum == expected, "geometry independent setup oracle");
    require(value.calls == 1, "one filled geometry snapshot");
    require(value.count == shape.count, "filled geometry population");
    require(value.kind == kind, "geometry implementation identity");
    bool valid = false;
    if (kind == RUST_GEOMETRY) {
        valid = value.usable >= shape.capacity && value.usable != UINT64_MAX
            && value.slots == UINT64_MAX && value.buckets == UINT64_MAX
            && value.load_factor == -1.0 && value.max_load_factor == -1.0;
    } else if (kind == CPP_GEOMETRY) {
        valid = value.usable == UINT64_MAX && value.slots == UINT64_MAX
            && value.buckets != 0 && value.buckets != UINT64_MAX
            && isfinite(value.max_load_factor) && value.max_load_factor > 0
            && value.buckets * value.max_load_factor >= shape.capacity
            && fabs(value.load_factor - (double)value.count / value.buckets) <= 0.000001;
    } else if (kind == ABSL_GEOMETRY) {
        valid = value.usable == UINT64_MAX && value.buckets == UINT64_MAX
            && value.slots >= shape.capacity && value.slots != UINT64_MAX
            && isfinite(value.max_load_factor) && value.max_load_factor > 0
            && fabs(value.load_factor - (double)value.count / value.slots) <= 0.000001;
    } else {
        uint64_t expected_capacity = shape.capacity;
        if (kind == WF_GEOMETRY) (void)wf_geometry_expected(false, shape.capacity, shape.count, &expected_capacity);
        valid = value.usable == expected_capacity && value.slots == expected_capacity
            && value.buckets == UINT64_MAX && value.max_load_factor == -1.0
            && fabs(value.load_factor - (double)value.count / value.slots) <= 0.000001;
    }
    require(valid, "exposed capacity geometry");
    require(value.diagnostic_consistent, "diagnostic matches trace allocation");
    Ledger expected_filled = {0};
    uint64_t expected_capacity;
    if (kind == WF_GEOMETRY) expected_filled = wf_geometry_expected(wide, shape.capacity, shape.count, &expected_capacity);
    require((kind == WF_GEOMETRY ? memcmp(&value.filled, &expected_filled, sizeof expected_filled) == 0 : value.filled.live == value.complete.peak)
            && value.filled.requests == value.complete.requests
            && value.filled.bytes == value.complete.bytes,
            "filled geometry allocation snapshot");
    require(value.complete.live == 0 && value.complete.requests == value.complete.releases,
            "every allocation is reclaimed");
}

static Geometry read_geometry(size_t variant_index, unsigned series, Shape shape) {
    const Variant *variant = &variants[variant_index];
    uint64_t kind = variant_index % IMPLEMENTATIONS;
    const uint64_t seed = 101;
    uint64_t expected = oracle(variant->wide, shape.count, 0, seed, SETUP);
    reset_ledger();
    snapshot = (Geometry){0}; capture_geometry = true;
    uint64_t checksum = trace_for(variant, series)(shape.capacity, shape.count, 0, seed, SETUP, 0);
    capture_geometry = false;
    clean_ledger();
    Geometry value = snapshot;
    value.complete = ledger; value.checksum = checksum; value.diagnostic_consistent = true;
    if (kind == WF_GEOMETRY) {
        const LookupApi *api = &lookup_apis[variant->wide ? LOOKUP_IMPLEMENTATIONS : 0];
        LookupStorage owner = {0};
        reset_ledger();
        api->prepare(owner.bytes, shape.capacity, shape.count, seed);
        uint64_t encoded = api->geometry(owner.bytes);
        Ledger actual_filled = ledger; /* Owner still exists: genuine live snapshot. */
        uint64_t cleanup = api->finish(owner.bytes);
        require(cleanup == lookup_cleanup_oracle(variant->wide, shape.count, seed),
                "geometry diagnostic complete-owner oracle");
        clean_ledger();
        value.diagnostic_consistent = ledger.requests == value.complete.requests
            && ledger.releases == value.complete.releases && ledger.bytes == value.complete.bytes
            && ledger.peak == value.complete.peak;
        value.calls = 1; value.kind = kind;
        value.count = encoded & UINT64_C(0xffffffff);
        value.slots = encoded >> 32; value.usable = value.slots;
        value.buckets = UINT64_MAX;
        value.load_factor = value.slots ? (double)value.count / value.slots : 0;
        value.max_load_factor = -1.0;
        value.filled = actual_filled;
    } else if (kind == C_GEOMETRY) {
        GeometryTrace diagnostic = variant->wide ? eco_c_map_record_geometry : eco_c_map_word_geometry;
        reset_ledger();
        uint64_t encoded = diagnostic(shape.capacity, shape.count, seed) ^ expected;
        clean_ledger();
        value.diagnostic_consistent = ledger.requests == 1 && ledger.requests == value.complete.requests
            && ledger.bytes == value.complete.bytes && ledger.peak == value.complete.peak;
        value.calls = 1; value.kind = kind;
        value.count = encoded & UINT64_C(0xffffffff);
        value.slots = encoded >> 32; value.usable = value.slots;
        value.buckets = UINT64_MAX;
        value.load_factor = value.slots ? (double)value.count / value.slots : 0;
        value.max_load_factor = -1.0;
        value.filled = value.complete;
        value.filled.live = value.complete.peak;
        value.filled.releases = 0;
    }
    validate_geometry(value, kind, shape, expected, variant->wide);
    observed ^= checksum;
    return value;
}

static void optional_u64(uint64_t value) {
    if (value != UINT64_MAX) printf("%" PRIu64, value);
    putchar(',');
}
static void optional_double(double value) {
    if (value >= 0) printf("%.9g", value);
    putchar(',');
}

static void geometry(bool print_rows) {
    const Shape shapes[] = {{3, 2}, {64, 56}, {3584, 3584}, {4096, 3584},
                            {5120, 3584}, {6144, 3584}, {8192, 3584}};
    size_t rows = 0;
    if (print_rows)
        puts("contract,series,element_bytes,requested_capacity,count,hash,variant,library,geometry_source,reserve_entry_floor,usable_entry_lower_bound,physical_slots,chaining_buckets,entries_per_slot_or_bucket,reported_max_load_factor,bucket_load_product,filled_live_bytes,requests,releases,requested_bytes,peak_bytes,final_live_bytes,checksum");
    for (size_t v = 0; v < ELEMENTS(variants); ++v)
        for (size_t s = 0; s < ELEMENTS(shapes); ++s)
            for (unsigned series = 0; series < 2; ++series) {
                const Variant *variant = &variants[v];
                Shape shape = shapes[s];
                Geometry value = read_geometry(v, series, shape);
                ++rows;
                if (!print_rows) continue;
                const char *library = value.kind == CPP_GEOMETRY ? eco_cpp_map_library_identity()
                    : value.kind == ABSL_GEOMETRY ? eco_absl_map_library_identity()
                    : value.kind == RUST_GEOMETRY ? "rust-std"
                    : value.kind == WF_GEOMETRY ? "whitefoot-bundled-std" : "direct-sparse-c";
                const char *source = value.kind == C_GEOMETRY ? "filled-control-fields"
                    : value.kind == WF_GEOMETRY ? "filled-owner-snapshot" : "filled-public-snapshot";
                printf("capacity-geometry,%s,%u,%" PRIu64 ",%" PRIu64 ",%s,%s,%s,%s,%" PRIu64 ",",
                       series ? "aligned-hash" : "native-default", variant->wide ? 256 : 8,
                       shape.capacity, value.count, hasher_for(variant, series), variant->name,
                       library, source, shape.capacity);
                optional_u64(value.usable); optional_u64(value.slots); optional_u64(value.buckets);
                optional_double(value.load_factor); optional_double(value.max_load_factor);
                optional_double(value.kind == CPP_GEOMETRY ? value.buckets * value.max_load_factor : -1.0);
                printf("%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                       value.filled.live, value.complete.requests, value.complete.releases,
                       value.complete.bytes, value.complete.peak, value.complete.live, value.checksum);
            }
    if (!print_rows) printf("map geometry: %zu filled-map observations passed\n", rows);
}

static void negative_geometry(const char *failure) {
    bool wf_growth = strcmp(failure, "wf-growth-allocation") == 0
        || strcmp(failure, "wf-growth-capacity") == 0;
    Shape shape = wf_growth ? (Shape){64, 56} : (Shape){3, 2};
    uint64_t kind = wf_growth || strcmp(failure, "diagnostic") == 0 ? WF_GEOMETRY : RUST_GEOMETRY;
    Geometry value = read_geometry(kind, 1, shape);
    if (strcmp(failure, "wf-growth-allocation") == 0) value.filled.live = value.complete.peak;
    else if (strcmp(failure, "wf-growth-capacity") == 0) value.usable = value.slots = shape.capacity;
    else if (strcmp(failure, "missing") == 0) value.calls = 0;
    else if (strcmp(failure, "population") == 0) ++value.count;
    else if (strcmp(failure, "identity") == 0) value.kind = CPP_GEOMETRY;
    else if (strcmp(failure, "capacity") == 0) value.usable = 0;
    else if (strcmp(failure, "allocation") == 0) ++value.filled.live;
    else if (strcmp(failure, "checksum") == 0) value.checksum ^= UINT64_C(1);
    else if (strcmp(failure, "diagnostic") == 0) value.diagnostic_consistent = false;
    else require(false, "unknown geometry negative control");
    validate_geometry(value, kind, shape, oracle(false, shape.count, 0, 101, SETUP), false);
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
    require(argc >= 2, "usage: check | occupancy-check | lookup-check | lookup-account work | lookup-measure 0|1 work | geometry | geometry-check | geometry-identities | negative-geometry kind | negative-checksum | negative-leak | negative-reserve variant | account work | occupancy-account work | measure 0|1 work | occupancy-measure 0|1 work");
    if (strcmp(argv[1], "check") == 0) {
        require(argc == 2, "check takes no arguments"); check();
    } else if (strcmp(argv[1], "occupancy-check") == 0) {
        require(argc == 2, "occupancy-check takes no arguments"); occupancy_check();
    } else if (strcmp(argv[1], "reserve-check") == 0) {
        require(argc == 2, "reserve-check takes no arguments"); reserve_check(false);
    } else if (strcmp(argv[1], "negative-reserve-api") == 0) {
        require(argc == 4 && strlen(argv[3]) == 1 && argv[3][0] >= '0' && argv[3][0] <= '2', "reserve negative fault peer");
        reserve_negative(argv[2], (unsigned)(argv[3][0] - '0'));
    } else if (strcmp(argv[1], "lookup-check") == 0) {
        require(argc == 2, "lookup-check takes no arguments"); lookup_check();
    } else if (strcmp(argv[1], "negative-lookup") == 0) {
        require(argc == 3, "negative-lookup requires a fault kind"); lookup_negative(argv[2]);
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
    else if (strcmp(argv[1], "reserve-account") == 0) {
        require(argc == 2, "reserve-account takes no arguments"); reserve_check(true);
    } else if (strcmp(argv[1], "negative-leak") == 0) {
        require(argc == 2, "negative-leak takes no arguments");
        checked_trace(&variants[0], 1, (Shape){3, 2}, 1, 17, HIT, false);
        wf_ecosystem_note_alloc(8); clean_ledger();
    } else if (strcmp(argv[1], "account") == 0) {
        require(argc == 3, "account requires a work count"); account(parse_work(argv[2]), false);
    } else if (strcmp(argv[1], "occupancy-account") == 0) {
        require(argc == 3, "occupancy-account requires a work count"); account(parse_work(argv[2]), true);
    } else if (strcmp(argv[1], "lookup-account") == 0) {
        require(argc == 3, "lookup-account requires a work count"); lookup_account(parse_work(argv[2]));
    } else if (strcmp(argv[1], "geometry") == 0 || strcmp(argv[1], "geometry-check") == 0) {
        require(argc == 2, "geometry modes take no arguments"); geometry(strcmp(argv[1], "geometry") == 0);
    } else if (strcmp(argv[1], "geometry-identities") == 0) {
        require(argc == 2, "geometry-identities takes no arguments");
        printf("C++ standard library headers: %s\nAbseil headers: %s\n",
               eco_cpp_map_library_identity(), eco_absl_map_library_identity());
    } else if (strcmp(argv[1], "negative-geometry") == 0) {
        require(argc == 3, "negative-geometry requires a failure kind"); negative_geometry(argv[2]);
    }
#else
    else if (strcmp(argv[1], "reserve-clock-check") == 0) {
        require(argc == 2, "reserve-clock-check takes no arguments"); (void)reserve_clock_check(0,false);
    } else if (strcmp(argv[1], "negative-reserve-clock") == 0) {
        require(argc == 3 && (strcmp(argv[2],"quantized") == 0 || strcmp(argv[2],"backwards") == 0), "reserve clock fault");
        (void)reserve_clock_check(strcmp(argv[2],"quantized") == 0 ? 1 : 2,false);
        require(false,"reserve clock negative unexpectedly returned");
    } else if (strcmp(argv[1], "reserve-measure") == 0) {
        require(argc == 3 && (strcmp(argv[2],"0") == 0 || strcmp(argv[2],"1") == 0), "reserve-measure requires cohort 0 or 1");
        reserve_measure((unsigned)(argv[2][0]-'0'));
    } else if (strcmp(argv[1], "measure") == 0 || strcmp(argv[1], "occupancy-measure") == 0) {
        require(argc == 4 && (strcmp(argv[2], "0") == 0 || strcmp(argv[2], "1") == 0),
                "measure requires cohort 0 or 1 and a work count");
        measure((unsigned)(argv[2][0] - '0'), parse_work(argv[3]), strcmp(argv[1], "occupancy-measure") == 0);
    } else if (strcmp(argv[1], "lookup-measure") == 0) {
        require(argc == 4 && (strcmp(argv[2], "0") == 0 || strcmp(argv[2], "1") == 0),
                "lookup-measure requires cohort 0 or 1 and a work count");
        lookup_measure((unsigned)(argv[2][0] - '0'), parse_work(argv[3]));
    }
#endif
    else require(false, "unknown or unavailable mode");
    return 0;
}
