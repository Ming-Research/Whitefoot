#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#endif
#if !defined(_WIN32)
#define _POSIX_C_SOURCE 200809L
#endif
#include <assert.h>
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

#define NOINLINE __attribute__((noinline))
#if defined(RETAIN_HELPERS)
#define HELPER NOINLINE
#define CONTRACT "retained"
#else
#define HELPER
#define CONTRACT "normal"
#endif

enum { RECORD_WORDS = 32, CEILING = 8193 };
extern uint64_t wf_vector_library_word_trace(uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_vector_library_record_trace(uint64_t, uint64_t, uint64_t, uint64_t);
#if defined(ECOSYSTEM)
extern uint64_t rust_vector_word_trace(uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t rust_vector_record_trace(uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t cpp_vector_word_trace(uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t cpp_vector_record_trace(uint64_t, uint64_t, uint64_t, uint64_t);
#endif

// Current inline owner layout; retained thin-compiler replay sets this to 0
// explicitly through ECO_CFLAGS so the build configuration records the ABI.
#ifndef WF_INLINE_SLOTS_OWNER
#define WF_INLINE_SLOTS_OWNER 1
#endif

#if !defined(ECOSYSTEM) || defined(ACCOUNT_ONLY)
typedef union {
    struct { size_t bytes; uint64_t magic; } value;
    max_align_t alignment;
} AllocationHeader;
static size_t requests, live_bytes, peak_bytes, requested_bytes;
static size_t realloc_requests, releases, peak_overlap_upper_bytes;
#ifndef WF_FULL_SLOTS_REALLOC
#define WF_FULL_SLOTS_REALLOC 0
#endif
// Physical requested extent: C controls retain their two-word header.
static size_t owned_allocation_bytes(bool whitefoot, uint64_t capacity, size_t stride) {
    size_t payload = (size_t)capacity * stride;
    if (whitefoot && WF_INLINE_SLOTS_OWNER) return payload ? payload : 1;
    return 16 + payload;
}
#endif
static volatile uint64_t observed;

static void require(bool condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "vector library costs: %s\n", message);
        exit(1);
    }
}

#if defined(ECOSYSTEM) && defined(ACCOUNT_ONLY)
void wf_ecosystem_note_alloc(uint64_t bytes) {
    require(bytes <= SIZE_MAX - live_bytes, "native allocation extent");
    ++requests; requested_bytes += (size_t)bytes; live_bytes += (size_t)bytes;
    if (live_bytes > peak_bytes) peak_bytes = live_bytes;
    if (live_bytes > peak_overlap_upper_bytes) peak_overlap_upper_bytes = live_bytes;
}
void wf_ecosystem_note_dealloc(uint64_t bytes) {
    require(live_bytes >= bytes, "native live-byte accounting");
    live_bytes -= (size_t)bytes; ++releases;
}
void wf_ecosystem_note_realloc(uint64_t old_bytes, uint64_t new_bytes) {
    require(live_bytes >= old_bytes, "native realloc old extent");
    require(new_bytes <= SIZE_MAX - live_bytes, "native realloc extent");
    // System may resize in place. This possible overlap is an upper bound on
    // requested backing, not an observation of hidden allocator residency.
    if (live_bytes + new_bytes > peak_overlap_upper_bytes)
        peak_overlap_upper_bytes = live_bytes + (size_t)new_bytes;
    ++requests; ++realloc_requests; requested_bytes += (size_t)new_bytes;
    live_bytes = live_bytes - (size_t)old_bytes + (size_t)new_bytes;
    if (live_bytes > peak_bytes) peak_bytes = live_bytes;
}
#endif
#if defined(ECOSYSTEM) && !defined(ACCOUNT_ONLY)
// Practical timed traces use ordinary allocation without observer work.
#define wf_cost_allocate(bytes) malloc((size_t)(bytes))
#define wf_cost_reallocate(pointer, bytes) realloc(pointer, (size_t)(bytes))
#define wf_cost_release(pointer) free(pointer)
#else
NOINLINE void *wf_cost_allocate(uint64_t bytes) {
    require(bytes <= SIZE_MAX - sizeof(AllocationHeader), "allocation extent");
    AllocationHeader *header = malloc(sizeof *header + (size_t)bytes);
    require(header != NULL, "host allocation failure");
    header->value.bytes = (size_t)bytes;
    header->value.magic = UINT64_C(0x766563746f726c69);
#if defined(ECOSYSTEM)
    wf_ecosystem_note_alloc(bytes);
#else
    ++requests; requested_bytes += (size_t)bytes; live_bytes += (size_t)bytes;
    if (live_bytes > peak_bytes) peak_bytes = live_bytes;
    if (live_bytes > peak_overlap_upper_bytes) peak_overlap_upper_bytes = live_bytes;
#endif
    return header + 1;
}
NOINLINE void *wf_cost_reallocate(void *pointer, uint64_t bytes) {
    if (pointer == NULL) return wf_cost_allocate(bytes);
    require(bytes <= SIZE_MAX - sizeof(AllocationHeader), "reallocation extent");
    AllocationHeader *header = (AllocationHeader *)pointer - 1;
    require(header->value.magic == UINT64_C(0x766563746f726c69), "allocation identity");
    const size_t old_bytes = header->value.bytes;
    require(live_bytes >= old_bytes, "live-byte accounting");
    require(bytes <= SIZE_MAX - live_bytes, "reallocation overlap extent");
    AllocationHeader *resized = realloc(header, sizeof *header + (size_t)bytes);
    require(resized != NULL, "host reallocation failure");
    resized->value.bytes = (size_t)bytes;
    resized->value.magic = UINT64_C(0x766563746f726c69);
#if defined(ECOSYSTEM)
    wf_ecosystem_note_realloc(old_bytes, bytes);
#else
    if (live_bytes + bytes > peak_overlap_upper_bytes)
        peak_overlap_upper_bytes = live_bytes + (size_t)bytes;
    ++requests; ++realloc_requests; requested_bytes += (size_t)bytes;
    live_bytes = live_bytes - old_bytes + (size_t)bytes;
    if (live_bytes > peak_bytes) peak_bytes = live_bytes;
#endif
    return resized + 1;
}
NOINLINE void wf_cost_release(void *pointer) {
    if (pointer == NULL) return;
    AllocationHeader *header = (AllocationHeader *)pointer - 1;
    require(header->value.magic == UINT64_C(0x766563746f726c69), "allocation identity");
    require(live_bytes >= header->value.bytes, "live-byte accounting");
#if defined(ECOSYSTEM)
    wf_ecosystem_note_dealloc(header->value.bytes);
#else
    live_bytes -= header->value.bytes; ++releases;
#endif
    header->value.magic = 0; free(header);
}
#endif
static void reset_accounting(void) {
#if !defined(ECOSYSTEM) || defined(ACCOUNT_ONLY)
    require(live_bytes == 0, "allocation left live between traces");
    requests = requested_bytes = peak_bytes = 0;
    realloc_requests = releases = peak_overlap_upper_bytes = 0;
#endif
}

#if !defined(ECOSYSTEM) || defined(ACCOUNT_ONLY)
static void allocator_check(bool fail_values, bool fail_accounting) {
    const unsigned char expected[24] = {
        3, 17, 29, 43, 59, 71, 89, 101, 127, 131, 149, 163,
        179, 191, 211, 223, 239, 251, 5, 23, 47, 83, 137, 197
    };
    reset_accounting();
    unsigned char *payload = wf_cost_allocate(sizeof expected);
    memcpy(payload, expected, sizeof expected);
    payload = wf_cost_reallocate(payload, 80);
    if (fail_values) payload[0] ^= 1;
    require((uintptr_t)payload % _Alignof(max_align_t) == 0,
            "reallocation observer alignment");
    require(memcmp(payload, expected, sizeof expected) == 0,
            "reallocation observer preserved bytes");
    payload = wf_cost_reallocate(payload, 8);
    require((uintptr_t)payload % _Alignof(max_align_t) == 0
            && memcmp(payload, expected, 8) == 0,
            "reallocation observer preserved bytes");
    wf_cost_release(payload);
    if (fail_accounting) ++requests;
    require(requests == 3 && realloc_requests == 2 && releases == 1
            && requested_bytes == 112 && live_bytes == 0 && peak_bytes == 80
            && peak_overlap_upper_bytes == 104,
            "reallocation observer exact ledger");
    puts("vector allocator observer: preserved bytes, alignment and exact ledger passed");
}
#endif

typedef struct { uint64_t words[RECORD_WORDS]; } Record;
static HELPER uint64_t make_word(uint64_t seed) { return seed; }
static HELPER Record make_record(uint64_t seed) {
    Record value;
    for (size_t i = 0; i < RECORD_WORDS; ++i) value.words[i] = seed + i;
    return value;
}
static HELPER void accept_word(uint64_t *digest, uint64_t value) {
    *digest = *digest * UINT64_C(131) + value;
}
static HELPER void accept_record(uint64_t *digest, Record value) {
    for (size_t i = 0; i < RECORD_WORDS; ++i)
        *digest = *digest * UINT64_C(131) + value.words[i];
}

// Concrete controls keep element size and the consumption algorithm
// static, as in WF instantiation; no function pointer or width dispatch enters
// an element operation. CONSUME differs only inside consuming truncation:
// 0 reverses then consumes, 1 consumes directly, 2 interleaves swap then take,
// 3 interleaves take then swap through a private local.
#define DEFINE_VECTOR(P, T, MAKE, ACCEPT, CONSUME)                             \
typedef struct { uint64_t length, capacity; T data[]; } P##_Block;              \
_Static_assert(offsetof(P##_Block, data) == 16, "Slots header extent");          \
static HELPER P##_Block *P##_new(void) {                                       \
    P##_Block *block = wf_cost_allocate(sizeof *block);                        \
    block->length = block->capacity = 0;                                      \
    return block;                                                            \
}                                                                            \
static HELPER uint64_t P##_reserve(P##_Block **owner, uint64_t total) {         \
    P##_Block *old = *owner;                                                  \
    if (old->capacity >= total) return old->capacity;                          \
    P##_Block *fresh = wf_cost_allocate(sizeof *fresh + total * sizeof(T));    \
    fresh->length = old->length; fresh->capacity = total;                      \
    memmove(fresh->data, old->data, old->length * sizeof(T));                   \
    wf_cost_release(old); *owner = fresh;                                    \
    return total;                                                            \
}                                                                            \
static HELPER uint64_t P##_append(P##_Block **owner, T value) {                \
    if ((*owner)->length == (*owner)->capacity) {                             \
        uint64_t total = (*owner)->capacity ? (*owner)->capacity * 2 : 1;     \
        if (total > CEILING) total = CEILING;                                \
        P##_reserve(owner, total);                                           \
    }                                                                        \
    P##_Block *block = *owner;                                                \
    block->data[block->length++] = value;                                    \
    return block->length;                                                    \
}                                                                            \
static HELPER uint64_t P##_insert(P##_Block **owner, uint64_t at, T value) {    \
    if ((*owner)->length == (*owner)->capacity) {                             \
        uint64_t total = (*owner)->capacity ? (*owner)->capacity * 2 : 1;     \
        if (total > CEILING) total = CEILING;                                \
        P##_reserve(owner, total);                                           \
    }                                                                        \
    P##_Block *block = *owner;                                                \
    memmove(block->data + at + 1, block->data + at,                            \
            (block->length - at) * sizeof(T));                               \
    block->data[at] = value;                                                 \
    return ++block->length;                                                  \
}                                                                            \
static HELPER T P##_remove(P##_Block **owner, uint64_t at) {                   \
    P##_Block *block = *owner;                                                \
    T value = block->data[at];                                               \
    --block->length;                                                         \
    memmove(block->data + at, block->data + at + 1,                            \
            (block->length - at) * sizeof(T));                               \
    return value;                                                            \
}                                                                            \
static HELPER T P##_swap_remove(P##_Block **owner, uint64_t at) {              \
    P##_Block *block = *owner;                                                \
    uint64_t last = block->length - 1;                                       \
    T saved = block->data[at];                                               \
    block->data[at] = block->data[last];                                      \
    block->data[last] = saved;                                               \
    T result = block->data[--block->length];                                  \
    return result;                                                           \
}                                                                            \
static HELPER void P##_truncate(P##_Block **owner, uint64_t retained,          \
                                uint64_t *digest) {                          \
    P##_Block *block = *owner;                                                \
    uint64_t count = block->length;                                          \
    if (CONSUME == 1) {                                                      \
        for (uint64_t i = retained; i < count; ++i) ACCEPT(digest, block->data[i]); \
        block->length = retained;                                           \
    } else if (CONSUME == 3) {                                               \
        uint64_t half = (count - retained) / 2;                               \
        for (uint64_t i = 0; i < half; ++i) {                                \
            uint64_t left = retained + i;                                   \
            T taken = block->data[--block->length];                           \
            T saved = block->data[left];                                     \
            block->data[left] = taken;                                       \
            taken = saved;                                                  \
            ACCEPT(digest, taken);                                          \
        }                                                                    \
        while (block->length != retained) ACCEPT(digest, block->data[--block->length]); \
    } else {                                                                 \
        uint64_t half = (count - retained) / 2;                               \
        for (uint64_t i = 0; i < half; ++i) {                                \
            uint64_t left = retained + i;                                   \
            uint64_t right = CONSUME == 2 ? block->length - 1 : count - 1 - i; \
            T saved = block->data[left];                                    \
            block->data[left] = block->data[right];                          \
            block->data[right] = saved;                                      \
            if (CONSUME == 2) ACCEPT(digest, block->data[--block->length]);   \
        }                                                                    \
        while (block->length != retained) ACCEPT(digest, block->data[--block->length]); \
    }                                                                        \
}                                                                            \
static HELPER void P##_drain(P##_Block **owner, uint64_t *digest) {            \
    P##_truncate(owner, 0, digest);                                           \
}                                                                            \
static HELPER void P##_free_empty(P##_Block *owner) { wf_cost_release(owner); } \
static HELPER void P##_work(P##_Block **owner, uint64_t count, uint64_t seed,  \
                            uint64_t *digest) {                              \
    for (uint64_t i = 0; i < count; ++i) P##_append(owner, MAKE(seed + i));    \
    uint64_t middle = count / 2;                                             \
    P##_insert(owner, middle, MAKE(seed ^ UINT64_C(11400714819323198485)));    \
    ACCEPT(digest, P##_remove(owner, middle));                                \
    if (count) ACCEPT(digest, P##_swap_remove(owner, 0));                      \
    P##_truncate(owner, (*owner)->length / 2, digest);                         \
    P##_drain(owner, digest);                                                \
}                                                                            \
static HELPER uint64_t P##_round(uint64_t count, uint64_t seed, bool reserve) { \
    P##_Block *owner = P##_new();                                             \
    if (reserve) P##_reserve(&owner, count + 1);                              \
    uint64_t digest = seed;                                                  \
    P##_work(&owner, count, seed, &digest);                                   \
    P##_free_empty(owner);                                                    \
    return digest;                                                           \
}                                                                            \
static HELPER void P##_tail_work(P##_Block **owner, uint64_t retained,        \
                                 uint64_t removed, uint64_t seed,             \
                                 uint64_t *digest) {                          \
    for (uint64_t i = 0; i < removed; ++i)                                   \
        P##_append(owner, MAKE(seed + retained + i));                        \
    P##_truncate(owner, retained, digest);                                   \
}                                                                            \
static NOINLINE uint64_t P##_trace(uint64_t count, uint64_t rounds,            \
                                  uint64_t seed, uint64_t path) {             \
    uint64_t checksum = seed;                                                \
    if (path >= 3) {                                                         \
        uint64_t removed = path - 3;                                         \
        if (removed > count) removed = count;                               \
        uint64_t retained = count - removed;                                \
        P##_Block *owner = P##_new();                                         \
        P##_reserve(&owner, count + 1);                                      \
        for (uint64_t i = 0; i < retained; ++i)                             \
            P##_append(&owner, MAKE(seed + i));                              \
        for (uint64_t r = 0; r < rounds; ++r) {                              \
            uint64_t digest = seed + r;                                     \
            P##_tail_work(&owner, retained, removed, seed + r, &digest);     \
            checksum = checksum * UINT64_C(257) + digest;                    \
        }                                                                    \
        uint64_t digest = seed;                                              \
        P##_drain(&owner, &digest);                                          \
        P##_free_empty(owner);                                                \
        return checksum * UINT64_C(257) + digest;                            \
    }                                                                        \
    if (path == 2) {                                                         \
        P##_Block *owner = P##_new();                                         \
        P##_reserve(&owner, count + 1);                                      \
        for (uint64_t r = 0; r < rounds; ++r) {                              \
            uint64_t digest = seed + r;                                     \
            P##_work(&owner, count, seed + r, &digest);                      \
            checksum = checksum * UINT64_C(257) + digest;                    \
        }                                                                    \
        P##_free_empty(owner);                                                \
    } else {                                                                 \
        for (uint64_t r = 0; r < rounds; ++r)                                \
            checksum = checksum * UINT64_C(257) + P##_round(count, seed + r, path == 0); \
    }                                                                        \
    return checksum;                                                         \
}

DEFINE_VECTOR(word_reverse, uint64_t, make_word, accept_word, 0)
DEFINE_VECTOR(word_direct, uint64_t, make_word, accept_word, 1)
DEFINE_VECTOR(word_interleaved, uint64_t, make_word, accept_word, 2)
DEFINE_VECTOR(word_take_swap, uint64_t, make_word, accept_word, 3)
DEFINE_VECTOR(record_reverse, Record, make_record, accept_record, 0)
DEFINE_VECTOR(record_direct, Record, make_record, accept_record, 1)
DEFINE_VECTOR(record_interleaved, Record, make_record, accept_record, 2)
DEFINE_VECTOR(record_take_swap, Record, make_record, accept_record, 3)

enum Variant { WHITEFOOT, REVERSE_C, DIRECT_C, INTERLEAVED_C, TAKE_SWAP_C,
#if defined(ECOSYSTEM)
    RUST_VECTOR, CPP_VECTOR,
#endif
    VARIANT_COUNT };
static NOINLINE uint64_t run(enum Variant variant, bool wide, uint64_t count,
                            uint64_t rounds, uint64_t seed, uint64_t path) {
    uint64_t result;
    if (variant == WHITEFOOT)
        result = wide ? wf_vector_library_record_trace(count, rounds, seed, path)
                      : wf_vector_library_word_trace(count, rounds, seed, path);
    else if (variant == REVERSE_C)
        result = wide ? record_reverse_trace(count, rounds, seed, path)
                      : word_reverse_trace(count, rounds, seed, path);
    else if (variant == DIRECT_C)
        result = wide ? record_direct_trace(count, rounds, seed, path)
                      : word_direct_trace(count, rounds, seed, path);
    else if (variant == INTERLEAVED_C)
        result = wide ? record_interleaved_trace(count, rounds, seed, path)
                      : word_interleaved_trace(count, rounds, seed, path);
#if defined(ECOSYSTEM)
    else if (variant == RUST_VECTOR)
        result = wide ? rust_vector_record_trace(count, rounds, seed, path)
                      : rust_vector_word_trace(count, rounds, seed, path);
    else if (variant == CPP_VECTOR)
        result = wide ? cpp_vector_record_trace(count, rounds, seed, path)
                      : cpp_vector_word_trace(count, rounds, seed, path);
#endif
    else
        result = wide ? record_take_swap_trace(count, rounds, seed, path)
                      : word_take_swap_trace(count, rounds, seed, path);
    observed = result;
    return result;
}

// Independent logical-order oracle: after swap-removing the first item, the
// last original item occupies index zero. Truncation visits the suffix before
// drain visits the retained prefix. It never allocates or mutates a vector.
static uint64_t oracle_accept(uint64_t digest, uint64_t seed, unsigned words) {
    for (unsigned word = 0; word < words; ++word)
        digest = digest * UINT64_C(131) + seed + word;
    return digest;
}
static uint64_t oracle(uint64_t count, uint64_t rounds, uint64_t seed, bool wide,
                       uint64_t path) {
    unsigned words = wide ? RECORD_WORDS : 1;
    uint64_t checksum = seed;
    if (path >= 3) {
        uint64_t removed = path - 3;
        if (removed > count) removed = count;
        uint64_t retained = count - removed;
        for (uint64_t round = 0; round < rounds; ++round) {
            uint64_t base = seed + round, digest = base;
            for (uint64_t at = retained; at < count; ++at)
                digest = oracle_accept(digest, base + at, words);
            checksum = checksum * UINT64_C(257) + digest;
        }
        uint64_t digest = seed;
        for (uint64_t at = 0; at < retained; ++at)
            digest = oracle_accept(digest, seed + at, words);
        return checksum * UINT64_C(257) + digest;
    }
    for (uint64_t round = 0; round < rounds; ++round) {
        uint64_t base = seed + round;
        uint64_t digest = oracle_accept(base, base ^ UINT64_C(11400714819323198485), words);
        if (count) digest = oracle_accept(digest, base, words);
        uint64_t live = count ? count - 1 : 0;
        uint64_t retained = live / 2;
        for (unsigned part = 0; part < 2; ++part) {
            uint64_t start = part ? 0 : retained, end = part ? retained : live;
            for (uint64_t at = start; at < end; ++at)
                digest = oracle_accept(digest, base + (at ? at : count - 1), words);
        }
        checksum = checksum * UINT64_C(257) + digest;
    }
    return checksum;
}

static void check_accounting(enum Variant variant, bool wide, uint64_t count,
                             uint64_t rounds, uint64_t path) {
#if !defined(ECOSYSTEM) || defined(ACCOUNT_ONLY)
    require(live_bytes == 0, "complete cleanup");
    require(requests == releases + realloc_requests, "allocation lifecycle balance");
    require(peak_overlap_upper_bytes >= peak_bytes, "requested-overlap upper bound");
#if defined(ECOSYSTEM)
    if (variant >= RUST_VECTOR) return;
#endif
    // Empty-capacity growth allocates a new backing and releases the old one.
    // The selected WF route may reallocate only after positive full capacity.
    const bool resizing = variant == WHITEFOOT && WF_FULL_SLOTS_REALLOC;
    const size_t stride = wide ? sizeof(Record) : sizeof(uint64_t);
    const size_t empty_bytes = owned_allocation_bytes(variant == WHITEFOOT, 0, stride);
    const size_t reserved_bytes = owned_allocation_bytes(variant == WHITEFOOT, count + 1, stride);
    size_t per_requests = 2, per_bytes = empty_bytes + reserved_bytes;
    size_t per_reallocs = 0;
    size_t per_peak = per_bytes;
    size_t per_overlap = per_bytes;
    if (path == 1) {
        per_requests = 1; per_bytes = empty_bytes; per_peak = per_overlap = empty_bytes;
        per_reallocs = 0;
        size_t old_bytes = empty_bytes;
        for (uint64_t capacity = 1;;) {
            size_t bytes = owned_allocation_bytes(variant == WHITEFOOT, capacity, stride);
            ++per_requests; per_bytes += bytes;
            if (resizing && capacity > 1) ++per_reallocs;
            per_peak = resizing && capacity > 1 ? bytes : old_bytes + bytes;
            per_overlap = old_bytes + bytes;
            if (capacity >= count + 1) break;
            old_bytes = bytes;
            capacity = capacity > CEILING / 2 ? CEILING : capacity * 2;
        }
    }
    size_t traces = path < 2 ? rounds : 1;
    require(requests == per_requests * traces && requested_bytes == per_bytes * traces
            && peak_bytes == (traces ? per_peak : 0),
            "independent allocation count, byte and peak formula");
    require(realloc_requests == per_reallocs * traces
            && releases == (per_requests - per_reallocs) * traces
            && peak_overlap_upper_bytes == (traces ? per_overlap : 0),
            "independent reallocation, release and overlap formula");
#else
    (void)variant; (void)wide; (void)count; (void)rounds; (void)path;
#endif
}

#if !defined(ECOSYSTEM) || !defined(ACCOUNT_ONLY)
static uint64_t nanos(void) {
#if defined(_WIN32)
    LARGE_INTEGER value, frequency;
    assert(QueryPerformanceCounter(&value) != 0);
    assert(QueryPerformanceFrequency(&frequency) != 0);
    return (uint64_t)((long double)value.QuadPart * 1.0e9L / frequency.QuadPart);
#else
    struct timespec value;
#if defined(__APPLE__)
    assert(clock_gettime(CLOCK_MONOTONIC_RAW, &value) == 0);
#else
    assert(clock_gettime(CLOCK_MONOTONIC, &value) == 0);
#endif
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
#endif
}

#endif

#if !defined(ECOSYSTEM)
static void check(void) {
    const uint64_t counts[] = {0, 1, 2, 3, 8, 16, 63, 256, 4096, 8192};
    const uint64_t rounds[] = {0, 1, 3};
    const uint64_t seeds[] = {0, 17, UINT64_MAX};
    size_t configurations = 0;
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned path = 0; path < 7; ++path)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n)
                for (size_t r = 0; r < sizeof rounds / sizeof rounds[0]; ++r)
                    for (size_t s = 0; s < sizeof seeds / sizeof seeds[0]; ++s) {
                        uint64_t expected = oracle(counts[n], rounds[r], seeds[s], wide != 0, path);
                        for (unsigned v = 0; v < VARIANT_COUNT; ++v) {
                            reset_accounting();
                            uint64_t actual = run((enum Variant)v, wide != 0, counts[n], rounds[r], seeds[s], path);
                            require(actual == expected, "independent logical-order checksum");
                            check_accounting((enum Variant)v, wide != 0,
                                             counts[n], rounds[r], path);
                        }
                        ++configurations;
                    }
    require(configurations == 1260, "configuration matrix");
    printf("vector library costs: %zu five-way configurations, %zu executions passed\n",
           configurations, configurations * VARIANT_COUNT);
}

static void measure(void) {
    const uint64_t counts[] = {16, 256, 4096};
    const char *paths[] = {"reserved", "growth", "reuse", "suffix-0", "suffix-1", "suffix-2", "suffix-3"};
    const char *variants[] = {"whitefoot", "reverse-c", "direct-c", "swap-take-c", "take-swap-c"};
    puts("contract,cohort,element_bytes,path,count,variant,sample,rounds,elapsed_ns,checksum,requests,requested_bytes,peak_bytes");
    for (unsigned cohort = 0; cohort < 2; ++cohort)
        for (unsigned wide = 0; wide < 2; ++wide)
            for (unsigned path = 0; path < 7; ++path)
                for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n)
                    for (unsigned sample = 0; sample < 11; ++sample) {
                        if (path == 3 || path == 5 || (path >= 3 && counts[n] == 256)) continue;
                        uint64_t count = counts[n];
                        uint64_t rounds = path >= 3 ? UINT64_C(65536) / (path - 3)
                                                   : UINT64_C(16384) / count;
                        uint64_t seed = UINT64_C(101) + sample;
                        uint64_t expected = oracle(count, rounds, seed, wide != 0, path);
                        for (unsigned offset = 0; offset < VARIANT_COUNT; ++offset) {
                            unsigned position = (sample + offset) % VARIANT_COUNT;
                            unsigned v = cohort ? VARIANT_COUNT - 1 - position : position;
                            reset_accounting();
                            uint64_t before = nanos();
                            uint64_t checksum = run((enum Variant)v, wide != 0, count, rounds, seed, path);
                            uint64_t elapsed = nanos() - before;
                            require(checksum == expected, "timed checksum");
                            require(live_bytes == 0, "timed cleanup");
                            printf("%s,%u,%zu,%s,%" PRIu64 ",%s,%u,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%zu,%zu,%zu\n",
                                   CONTRACT, cohort, wide ? sizeof(Record) : sizeof(uint64_t),
                                   paths[path], count, variants[v], sample, rounds, elapsed,
                                   checksum, requests, requested_bytes, peak_bytes);
                        }
                    }
}

int main(int argc, char **argv) {
    require(argc == 2, "usage: vector-costs check|measure");
    if (strcmp(argv[1], "check") == 0) check();
    else if (strcmp(argv[1], "allocator-check") == 0) allocator_check(false, false);
    else if (strcmp(argv[1], "allocator-fail-values") == 0) allocator_check(true, false);
    else if (strcmp(argv[1], "allocator-fail-accounting") == 0) allocator_check(false, true);
    else {
        require(strcmp(argv[1], "measure") == 0, "usage: vector-costs check|measure");
        measure();
    }
    return 0;
}

#else
static const char *const ecosystem_paths[] = {
    "reserved", "growth", "reuse", "suffix-0", "suffix-1", "suffix-2", "suffix-3"
};
static const char *const ecosystem_variants[] = {
    "whitefoot", "reverse-c", "direct-c", "swap-take-c", "take-swap-c", "rust-vec", "cpp-std-vector"
};

#if WF_INLINE_SLOTS_OWNER
typedef struct { uint64_t length, capacity; void *payload; } ApiOwner;
#else
typedef struct { void *state; } ApiOwner;
#endif
// Both source prepare wrappers use the shared destination-first result ABI.
// Only setup copies the returned owner's representation into the opaque slot;
// mutation and consumption receive that slot directly for either layout.
typedef struct { ApiOwner owner; uint64_t abi_padding[3]; } ApiPrepared;
_Static_assert(offsetof(ApiPrepared, owner) == 0, "prepared owner prefix");
_Static_assert(offsetof(ApiPrepared, abi_padding) == sizeof(ApiOwner),
               "prepared result layout");
typedef union { max_align_t alignment; unsigned char bytes[3 * sizeof(void *)]; } ApiStorage;
typedef struct { uint64_t length, capacity, checksum, valid; } ApiObservation;
typedef struct {
    void (*prepare)(uint64_t, void *);
    uint64_t (*append_batch)(void *, uint64_t, uint64_t);
    uint64_t (*inspect_reset)(void *, uint64_t, uint64_t, ApiObservation *);
    uint8_t (*destroy)(void *);
    uint64_t (*append_one)(void *, uint64_t);
    uint64_t (*snapshot)(void *, ApiObservation *);
    uint64_t (*reserve)(void *, uint64_t);
    uint64_t (*insert)(void *, uint64_t, uint64_t);
    uint64_t (*remove)(void *, uint64_t, uint64_t, ApiObservation *);
    uint64_t (*drain)(void *, uint64_t, ApiObservation *);
    uint64_t (*inspect_shape)(void *, uint64_t, uint64_t, uint64_t, uint64_t,
                              uint64_t, ApiObservation *);
    const char *name;
} ApiOperations;
#define DECLARE_APPEND_OPERATIONS(prefix, width)                            \
    extern uint64_t prefix##_##width##_append_batch(void *, uint64_t, uint64_t); \
    extern uint64_t prefix##_##width##_inspect_reset(void *, uint64_t, uint64_t, ApiObservation *); \
    extern uint8_t prefix##_##width##_destroy(void *);                        \
    extern uint64_t prefix##_##width##_append_one(void *, uint64_t);          \
    extern uint64_t prefix##_##width##_snapshot(void *, ApiObservation *); \
    extern uint64_t prefix##_##width##_reserve(void *, uint64_t); \
    extern uint64_t prefix##_##width##_insert(void *, uint64_t, uint64_t); \
    extern uint64_t prefix##_##width##_remove(void *, uint64_t, uint64_t, ApiObservation *); \
    extern uint64_t prefix##_##width##_drain(void *, uint64_t, ApiObservation *); \
    extern uint64_t prefix##_##width##_inspect_shape(void *, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, ApiObservation *)
#define DECLARE_APPEND_API(prefix, width)                                   \
    extern void prefix##_##width##_prepare(uint64_t, void *);               \
    DECLARE_APPEND_OPERATIONS(prefix, width)
#define DECLARE_WF_APPEND_API(width)                                        \
    extern void wf_vector_api_##width##_prepare_for_c(void *, uint64_t);   \
    DECLARE_APPEND_OPERATIONS(wf_vector_api, width);                         \
    static void wf_vector_api_##width##_prepare_slot(uint64_t capacity, void *storage) { \
        ApiPrepared prepared;                                             \
        wf_vector_api_##width##_prepare_for_c(&prepared, capacity);         \
        memcpy(storage, &prepared.owner, sizeof prepared.owner);           \
    }
DECLARE_WF_APPEND_API(word)
DECLARE_WF_APPEND_API(record)
DECLARE_APPEND_API(rust_vector_api, word);
DECLARE_APPEND_API(rust_vector_api, record);
DECLARE_APPEND_API(cpp_vector_api, word);
DECLARE_APPEND_API(cpp_vector_api, record);
#define APPEND_API(prefix, width, label) { prefix##_##width##_prepare,       \
    prefix##_##width##_append_batch, prefix##_##width##_inspect_reset,       \
    prefix##_##width##_destroy, prefix##_##width##_append_one,               \
    prefix##_##width##_snapshot, prefix##_##width##_reserve, \
    prefix##_##width##_insert, prefix##_##width##_remove, \
    prefix##_##width##_drain, prefix##_##width##_inspect_shape, label }
static const ApiOperations append_api[2][3] = {
    { { wf_vector_api_word_prepare_slot, wf_vector_api_word_append_batch,
        wf_vector_api_word_inspect_reset, wf_vector_api_word_destroy,
        wf_vector_api_word_append_one, wf_vector_api_word_snapshot,
        wf_vector_api_word_reserve, wf_vector_api_word_insert,
        wf_vector_api_word_remove, wf_vector_api_word_drain,
        wf_vector_api_word_inspect_shape, "whitefoot" },
      APPEND_API(rust_vector_api, word, "rust-vec"),
      APPEND_API(cpp_vector_api, word, "cpp-std-vector") },
    { { wf_vector_api_record_prepare_slot, wf_vector_api_record_append_batch,
        wf_vector_api_record_inspect_reset, wf_vector_api_record_destroy,
        wf_vector_api_record_append_one, wf_vector_api_record_snapshot,
        wf_vector_api_record_reserve, wf_vector_api_record_insert,
        wf_vector_api_record_remove, wf_vector_api_record_drain,
        wf_vector_api_record_inspect_shape, "whitefoot" },
      APPEND_API(rust_vector_api, record, "rust-vec"),
      APPEND_API(cpp_vector_api, record, "cpp-std-vector") }
};

static uint64_t append_oracle(uint64_t count, uint64_t seed, bool wide) {
    uint64_t checksum = seed;
    for (uint64_t index = 0; index < count; ++index)
        for (unsigned word = 0; word < (wide ? RECORD_WORDS : 1); ++word)
            checksum = checksum * UINT64_C(131) + seed + index + word;
    return checksum;
}

enum EditCase {
    RESERVE_NOOP, RESERVE_GROW, INSERT_FRONT_SPARE, INSERT_MID_SPARE,
    INSERT_FRONT_FULL, INSERT_MID_FULL, REMOVE_FRONT, REMOVE_MID,
    REMOVE_BACK, DRAIN_ALL, EDIT_CASES
};
#if !defined(ACCOUNT_ONLY)
static const char *const edit_names[EDIT_CASES] = {
    "reserve-noop", "reserve-grow", "insert-front-spare", "insert-mid-spare",
    "insert-front-full", "insert-mid-full", "remove-front", "remove-mid",
    "remove-back", "drain-all"
};
#endif
static bool edit_insert(enum EditCase kind) {
    return kind >= INSERT_FRONT_SPARE && kind <= INSERT_MID_FULL;
}
static bool edit_remove(enum EditCase kind) {
    return kind >= REMOVE_FRONT && kind <= REMOVE_BACK;
}
static bool edit_growth(enum EditCase kind) {
    return kind == RESERVE_GROW || kind == INSERT_FRONT_FULL || kind == INSERT_MID_FULL;
}
static uint64_t edit_index(enum EditCase kind, uint64_t count) {
    return kind == INSERT_MID_SPARE || kind == INSERT_MID_FULL || kind == REMOVE_MID
        ? count / 2 : kind == REMOVE_BACK ? count - 1 : 0;
}
static uint64_t edit_marker(uint64_t seed) {
    return seed ^ UINT64_C(11400714819323198485);
}
static uint64_t edit_oracle(enum EditCase kind, uint64_t count, uint64_t seed,
                            bool wide) {
    uint64_t digest = seed;
    uint64_t length = kind == DRAIN_ALL ? 0
        : count + (edit_insert(kind) ? 1 : 0) - (edit_remove(kind) ? 1 : 0);
    uint64_t at = edit_index(kind, count);
    for (uint64_t position = 0; position < length; ++position) {
        uint64_t base = edit_insert(kind) && position == at ? edit_marker(seed)
            : seed + position - (edit_insert(kind) && position > at)
                             + (edit_remove(kind) && position >= at);
        for (unsigned word = 0; word < (wide ? RECORD_WORDS : 1); ++word)
            digest = digest * UINT64_C(131) + base + word;
    }
    return digest;
}
static uint64_t edit_removed_oracle(uint64_t seed, uint64_t index, bool wide) {
    uint64_t digest = seed + index;
    for (unsigned word = 0; word < (wide ? RECORD_WORDS : 1); ++word)
        digest = digest * UINT64_C(131) + seed + index + word;
    return digest;
}

#if defined(ACCOUNT_ONLY)
typedef struct { size_t requests, reallocations, releases, bytes, live; } ApiAccount;
static ApiAccount append_account_snapshot(void) {
    return (ApiAccount){requests, realloc_requests, releases, requested_bytes, live_bytes};
}
static void append_account_unchanged(ApiAccount before) {
    require(requests == before.requests && realloc_requests == before.reallocations
            && releases == before.releases && requested_bytes == before.bytes
            && live_bytes == before.live, "spare append zero allocation");
}
#endif

static void append_check(bool fail_values, bool fail_allocation) {
    const uint64_t counts[] = {16, 0, 1, 256, 4096};
    const uint64_t seed = UINT64_MAX - 7;
    for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n) {
    const uint64_t count = counts[n];
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned variant = 0; variant < 3; ++variant) {
            const ApiOperations *api = &append_api[wide][variant];
            reset_accounting();
            ApiStorage owner;
            api->prepare(count, &owner);
            ApiObservation prepared = {0};
            require(api->inspect_reset(&owner, 0, seed, &prepared) == 1
                    && prepared.length == 0 && prepared.capacity == count,
                    "spare append prepared state");
#if defined(ACCOUNT_ONLY)
            ApiAccount before = append_account_snapshot();
#endif
            uint64_t length = api->append_batch(&owner, count, seed + fail_values);
#if defined(ACCOUNT_ONLY)
            if (fail_allocation) {
                void *extra = wf_cost_allocate(1);
                wf_cost_release(extra);
            }
            append_account_unchanged(before);
#else
            (void)fail_allocation;
#endif
            ApiObservation actual = {0};
            uint64_t valid = api->inspect_reset(&owner, count, seed, &actual);
            require(length == count && actual.length == count
                    && actual.capacity == prepared.capacity,
                    "spare append length and capacity");
            require(valid == 1 && actual.valid == 1
                    && actual.checksum == append_oracle(count, seed, wide != 0),
                    "spare append full values and checksum");
            (void)api->destroy(&owner);
#if defined(ACCOUNT_ONLY)
            require(live_bytes == 0 && requests == releases + realloc_requests,
                    "spare append complete cleanup");
#endif
        }
    }
    puts("vector spare append: scalar/256B, five counts, three APIs passed");
}

static uint64_t growth_prepare(const ApiOperations *api, ApiStorage *owner,
                               uint64_t requested, uint64_t seed) {
    api->prepare(requested, owner);
    ApiObservation state = {0};
    uint64_t length = api->snapshot(owner, &state);
    if (requested >= 16 && state.capacity != requested)
        fprintf(stderr, "growth append capacity: %s requested=%" PRIu64
                " observed=%" PRIu64 "\n", api->name, requested, state.capacity);
    require(length == 0 && state.length == 0 && state.valid == 1
            && state.checksum == 0 && state.capacity >= requested
            && state.capacity < CEILING, "growth append reserved state");
    if (requested >= 16)
        require(state.capacity == requested, "growth append matched initial capacity");
    const uint64_t initial = state.capacity;
    require(api->append_batch(owner, initial, seed) == initial,
            "growth append prepared length");
    length = api->snapshot(owner, &state);
    require(length == initial && state.length == initial
            && state.capacity == initial && state.checksum == 0 && state.valid == 1,
            "growth append prepared state");
    return initial;
}

static uint64_t growth_inspect_reset(const ApiOperations *api, ApiStorage *owner,
                                     uint64_t initial, uint64_t seed,
                                     uint64_t returned, bool wide, bool fail_state) {
    ApiObservation state = {0};
    uint64_t length = api->snapshot(owner, &state);
    const uint64_t expected_length = initial + 1 + fail_state;
    if (initial >= 16 && state.capacity != 2 * initial)
        fprintf(stderr, "growth append capacity: %s bytes=%zu old=%" PRIu64
                " new=%" PRIu64 " expected=%" PRIu64 "\n", api->name,
                wide ? sizeof(Record) : sizeof(uint64_t), initial, state.capacity,
                2 * initial);
    require(returned == initial + 1 && length == expected_length
            && state.length == expected_length && state.capacity > initial
            && state.valid == 1 && state.checksum == 0
            && (initial < 16 || state.capacity == 2 * initial),
            "growth append post state");
    ApiObservation actual = {0};
    uint64_t valid = api->inspect_reset(owner, initial + 1, seed, &actual);
    require(valid == 1 && actual.valid == 1 && actual.length == initial + 1
            && actual.capacity == state.capacity
            && actual.checksum == append_oracle(initial + 1, seed, wide),
            "growth append full values and checksum");
    ApiObservation reset = {0};
    require(api->snapshot(owner, &reset) == 0 && reset.length == 0
            && reset.capacity == state.capacity && reset.valid == 1,
            "growth append reset state");
    observed = actual.checksum;
    return state.capacity;
}

static void growth_check(bool fail_values, bool fail_state, bool report) {
    const uint64_t capacities[] = {16, 256, 4096, 0, 1};
    const uint64_t seed = UINT64_MAX - 7;
#if defined(ACCOUNT_ONLY)
    if (report)
        puts("contract,element_bytes,requested_capacity,initial_capacity,variant,length,capacity,requests,reallocations,releases,requested_bytes,live_before,live_after");
#else
    (void)report;
#endif
    for (size_t n = 0; n < sizeof capacities / sizeof capacities[0]; ++n)
        for (unsigned wide = 0; wide < 2; ++wide)
            for (unsigned variant = 0; variant < 3; ++variant) {
                const ApiOperations *api = &append_api[wide][variant];
                const size_t stride = wide ? sizeof(Record) : sizeof(uint64_t);
                reset_accounting();
                ApiStorage owner;
                uint64_t initial = growth_prepare(api, &owner, capacities[n], seed);
#if defined(ACCOUNT_ONLY)
                ApiAccount before = append_account_snapshot();
                const size_t initial_bytes = variant == 0
                    ? owned_allocation_bytes(true, initial, stride) : initial * stride;
                require(before.live == initial_bytes,
                        "growth append initial live bytes");
                if (variant == 0)
                    require(before.requests == (initial == 0 ? 1u : 2u)
                            && before.reallocations == 0
                            && before.releases == (initial != 0 ? 1u : 0u),
                            "growth append WF preparation ledger");
#endif
                uint64_t length = api->append_one(&owner, seed + initial + fail_values);
#if defined(ACCOUNT_ONLY)
                ApiAccount after = append_account_snapshot();
#endif
                uint64_t capacity = growth_inspect_reset(api, &owner, initial, seed,
                                                         length, wide != 0, fail_state);
#if defined(ACCOUNT_ONLY)
                require(after.requests > before.requests && after.bytes > before.bytes
                        && after.live == (variant == 0
                            ? owned_allocation_bytes(true, capacity, stride) : capacity * stride)
                        && after.requests - before.requests
                            == after.releases - before.releases
                               + after.reallocations - before.reallocations
                               + (initial == 0 && variant != 0),
                        "growth append allocation ledger");
                if (variant == 0)
                    require(after.requests - before.requests == 1
                            && after.reallocations - before.reallocations == (initial != 0 && WF_FULL_SLOTS_REALLOC ? 1u : 0u)
                            && after.releases - before.releases == (initial != 0 && WF_FULL_SLOTS_REALLOC ? 0u : 1u)
                            && after.bytes - before.bytes == owned_allocation_bytes(true, capacity, stride),
                            "growth append WF route ledger");
                if (report)
                    printf("append-growth-o3,%zu,%" PRIu64 ",%" PRIu64 ",%s,%" PRIu64 ",%" PRIu64 ",%zu,%zu,%zu,%zu,%zu,%zu\n",
                           stride, capacities[n], initial, api->name, length, capacity,
                           after.requests - before.requests,
                           after.reallocations - before.reallocations,
                           after.releases - before.releases, after.bytes - before.bytes,
                           before.live, after.live);
#endif
                if (capacities[n] < 16 && !report)
                    printf("growth append policy: %s bytes=%zu requested=%" PRIu64
                           " initial=%" PRIu64 " length=%" PRIu64 " capacity=%" PRIu64 "\n",
                           api->name, stride, capacities[n], initial, length, capacity);
                (void)api->destroy(&owner);
#if defined(ACCOUNT_ONLY)
                require(live_bytes == 0 && requests == releases + realloc_requests,
                        "growth append complete cleanup");
#endif
            }
    if (!report) puts("vector append growth: scalar/256B, five capacities, three APIs passed");
}

static uint64_t edit_setup(const ApiOperations *api, ApiStorage *owner,
                           enum EditCase kind, uint64_t count, uint64_t seed) {
    uint64_t requested = count + (kind == INSERT_FRONT_SPARE || kind == INSERT_MID_SPARE);
    api->prepare(requested, owner);
    ApiObservation state = {0};
    require(api->snapshot(owner, &state) == 0 && state.length == 0
            && state.capacity == requested && state.valid == 1,
            "edit prepared capacity");
    require(api->append_batch(owner, count, seed) == count,
            "edit prepared contents length");
    require(api->snapshot(owner, &state) == count && state.length == count
            && state.capacity == requested && state.valid == 1,
            "edit prepared state");
    return state.capacity;
}

static uint64_t edit_run(const ApiOperations *api, ApiStorage *owner,
                         enum EditCase kind, uint64_t count, uint64_t seed,
                         ApiObservation *operation) {
    uint64_t at = edit_index(kind, count);
    if (kind == RESERVE_NOOP) return api->reserve(owner, count);
    if (kind == RESERVE_GROW) return api->reserve(owner, count + 1);
    if (edit_insert(kind)) return api->insert(owner, at, edit_marker(seed));
    if (edit_remove(kind)) return api->remove(owner, at, seed + at, operation);
    return api->drain(owner, seed, operation);
}

static void edit_verify(const ApiOperations *api, ApiStorage *owner,
                        enum EditCase kind, uint64_t count, uint64_t seed,
                        bool wide, uint64_t initial, uint64_t returned,
                        ApiObservation operation, bool fail_state) {
    const uint64_t at = edit_index(kind, count);
    const uint64_t length = count + (edit_insert(kind) ? 1 : 0)
                                  - (edit_remove(kind) ? 1 : 0)
                                  - (kind == DRAIN_ALL ? count : 0);
    ApiObservation state = {0};
    require(api->snapshot(owner, &state) == length && state.length == length + fail_state
            && state.valid == 1 && state.checksum == 0,
            "edit post state");
    require(state.capacity == initial || (edit_growth(kind) && state.capacity > initial),
            "edit post capacity");
    if (edit_growth(kind)) require(state.capacity > initial, "edit growth capacity");
    else require(state.capacity == initial, "edit retained capacity");
    if (kind == RESERVE_NOOP || kind == RESERVE_GROW)
        require(returned == state.capacity, "edit reserve result");
    else if (edit_insert(kind))
        require(returned == count + 1, "edit insert result");
    else if (edit_remove(kind))
        require(returned == edit_removed_oracle(seed, at, wide)
                && operation.length == count - 1 && operation.capacity == initial
                && operation.checksum == returned && operation.valid == 1,
                "edit removed owner and result");
    else
        require(returned == append_oracle(count, seed, wide)
                && operation.length == 0 && operation.capacity == initial
                && operation.checksum == returned && operation.valid == 1,
                "edit drained owners and order");
    ApiObservation content = {0};
    uint64_t valid = edit_insert(kind) || edit_remove(kind)
        ? api->inspect_shape(owner, length, seed, edit_insert(kind) ? 1 : 2,
                             at, edit_marker(seed), &content)
        : api->inspect_reset(owner, length, seed, &content);
    require(valid == 1 && content.valid == 1 && content.length == length
            && content.capacity == state.capacity
            && content.checksum == edit_oracle(kind, count, seed, wide),
            "edit full ordered values and checksum");
    ApiObservation reset = {0};
    require(api->snapshot(owner, &reset) == 0 && reset.length == 0
            && reset.capacity == state.capacity, "edit reset state");
    observed = content.checksum ^ returned;
}

static void edit_check(unsigned fault) {
    const uint64_t counts[] = {16, 256, 4096};
    const uint64_t seed = UINT64_MAX - 7;
    for (unsigned kind = 0; kind < EDIT_CASES; ++kind)
        for (unsigned wide = 0; wide < 2; ++wide)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n)
                for (unsigned variant = 0; variant < 3; ++variant) {
                    const ApiOperations *api = &append_api[wide][variant];
                    const uint64_t count = counts[n];
                    reset_accounting();
                    ApiStorage owner;
                    uint64_t initial = edit_setup(api, &owner, (enum EditCase)kind,
                                                  count, seed + (fault == 1));
#if defined(ACCOUNT_ONLY)
                    ApiAccount before = append_account_snapshot();
#endif
                    ApiObservation operation = {0};
                    uint64_t returned = edit_run(api, &owner, (enum EditCase)kind,
                                                 count, seed, &operation);
#if defined(ACCOUNT_ONLY)
                    ApiAccount after = append_account_snapshot();
                    if (!edit_growth((enum EditCase)kind))
                        require(after.requests == before.requests
                                && after.reallocations == before.reallocations
                                && after.releases == before.releases
                                && after.live == before.live,
                                "edit spare operation zero allocation");
                    else {
                        require(after.requests == before.requests + 1
                                && after.bytes > before.bytes
                                && after.live > before.live,
                                "edit growth allocation");
                        if (variant == 0)
                            require(after.reallocations - before.reallocations
                                    == (WF_FULL_SLOTS_REALLOC ? 1u : 0u)
                                    && after.releases - before.releases
                                    == (WF_FULL_SLOTS_REALLOC ? 0u : 1u),
                                    "edit Whitefoot full-growth route");
                    }
#endif
                    edit_verify(api, &owner, (enum EditCase)kind, count, seed,
                                wide != 0, initial, returned, operation, fault == 2);
                    (void)api->destroy(&owner);
#if defined(ACCOUNT_ONLY)
                    if (fault == 3) (void)wf_cost_allocate(1);
                    require(live_bytes == 0 && requests == releases + realloc_requests,
                            "edit complete cleanup");
#else
                    (void)fault;
#endif
                }
    puts("vector reserve/insert/remove/drain: two widths, three counts, three APIs passed");
}

#if !defined(ACCOUNT_ONLY)
static void append_inspect_batch(const ApiOperations *api, ApiStorage *owners,
                                 size_t contexts, uint64_t count, uint64_t capacity,
                                 uint64_t seed, bool wide) {
    for (size_t k = 0; k < contexts; ++k) {
        ApiObservation actual = {0};
        uint64_t valid = api->inspect_reset(&owners[k], count, seed + k, &actual);
        require(valid == 1 && actual.valid == 1 && actual.length == count
                && actual.capacity == capacity
                && actual.checksum == append_oracle(count, seed + k, wide),
                "timed spare append complete observation");
        observed = actual.checksum;
    }
}

static void append_measure(uint64_t work, unsigned samples) {
    const uint64_t counts[] = {16, 256, 4096};
    puts("contract,cohort,element_bytes,count,variant,sample,control,contexts,payload_bytes,descriptor_bytes,cycles,operations,elapsed_ns");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n) {
            const uint64_t count = counts[n];
            const size_t stride = wide ? sizeof(Record) : sizeof(uint64_t);
            size_t contexts = 1048576 / (count * stride);
            if (contexts > 1024) contexts = 1024;
            if (contexts == 0) contexts = 1;
            const uint64_t batch_work = count * contexts;
            const uint64_t cycles = (work + batch_work - 1) / batch_work;
            ApiStorage *owners[3];
            for (unsigned v = 0; v < 3; ++v) {
                owners[v] = malloc(contexts * sizeof(ApiStorage));
                require(owners[v] != NULL, "append descriptor storage");
                const ApiOperations *api = &append_api[wide][v];
                for (size_t k = 0; k < contexts; ++k)
                    api->prepare(count, &owners[v][k]);
                append_inspect_batch(api, owners[v], contexts, 0, count, 97, wide != 0);
                for (size_t k = 0; k < contexts; ++k)
                    (void)api->append_batch(&owners[v][k], count, 97 + k);
                append_inspect_batch(api, owners[v], contexts, count, count, 97, wide != 0);
            }
            for (unsigned cohort = 0; cohort < 2; ++cohort)
                for (unsigned sample = 0; sample < samples; ++sample)
                    for (unsigned offset = 0; offset < 3; ++offset) {
                        unsigned position = (sample + offset) % 3;
                        unsigned v = cohort ? 2 - position : position;
                        const ApiOperations *api = &append_api[wide][v];
                        for (unsigned control = 0; control < 2; ++control) {
                            const uint64_t appended = control ? 0 : count;
                            uint64_t elapsed = 0;
                            for (uint64_t cycle = 0; cycle < cycles; ++cycle) {
                                const uint64_t seed = 101 + sample + cycle;
                                const uint64_t start = nanos();
                                for (size_t k = 0; k < contexts; ++k)
                                    (void)api->append_batch(&owners[v][k], appended, seed + k);
                                elapsed += nanos() - start;
                                append_inspect_batch(api, owners[v], contexts, appended,
                                                     count, seed, wide != 0);
                            }
                            printf("append-spare-o3,%u,%zu,%" PRIu64 ",%s,%u,%u,%zu,%zu,%zu,%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                                   cohort, stride, count, api->name, sample, control,
                                   contexts, contexts * (size_t)count * stride,
                                   contexts * sizeof(ApiStorage), cycles,
                                   cycles * contexts * appended, elapsed);
                        }
                    }
            for (unsigned v = 0; v < 3; ++v) {
                for (size_t k = 0; k < contexts; ++k)
                    (void)append_api[wide][v].destroy(&owners[v][k]);
                free(owners[v]);
            }
        }
}

static void growth_clock_check(bool quantized) {
    const unsigned reads = 200000;
    uint64_t previous = nanos(), minimum = UINT64_MAX;
    if (quantized) previous -= previous % 1000;
    for (unsigned i = 0; i < reads; ++i) {
        uint64_t current = nanos();
        if (quantized) current -= current % 1000;
        require(current >= previous, "growth append clock nondecreasing");
        if (current > previous && current - previous < minimum)
            minimum = current - previous;
        previous = current;
    }
    fprintf(stderr, "growth append clock: reads=%u minimum_nonzero_ns=%" PRIu64
            " quantized=%u\n", reads, minimum, quantized);
    require(minimum <= 100, "growth append clock precision");
}

static void growth_measure(uint64_t work_bytes, unsigned samples, uint64_t large_work_bytes) {
    const uint64_t capacities[] = {0, 1, 16, 256, 4096};
    // Sizing allowance only; it neither models latency nor adjusts a sample.
    const uint64_t allowance = 256;
    puts("contract,cohort,element_bytes,count,variant,sample,control,contexts,payload_bytes,descriptor_bytes,cycles,operations,elapsed_ns,initial_capacity,capacity,work_bytes,allowance_bytes");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (size_t n = 0; n < sizeof capacities / sizeof capacities[0]; ++n) {
            const uint64_t requested = capacities[n];
            const size_t stride = wide ? sizeof(Record) : sizeof(uint64_t);
            const uint64_t budget = wide && requested == 4096 ? large_work_bytes : work_bytes;
            uint64_t initial_caps[3], post_caps[3];
            for (unsigned variant = 0; variant < 3; ++variant) {
                const ApiOperations *api = &append_api[wide][variant];
                ApiStorage owner;
                initial_caps[variant] = growth_prepare(api, &owner, requested, 97);
                uint64_t length = api->append_one(&owner, 97 + initial_caps[variant]);
                post_caps[variant] = growth_inspect_reset(api, &owner,
                    initial_caps[variant], 97, length, wide != 0, false);
                (void)api->destroy(&owner);
            }
            for (unsigned cohort = 0; cohort < 2; ++cohort)
                for (unsigned sample = 0; sample < samples; ++sample)
                    for (unsigned offset = 0; offset < 3; ++offset) {
                        unsigned position = (sample + offset) % 3;
                        unsigned variant = cohort ? 2 - position : position;
                        const ApiOperations *api = &append_api[wide][variant];
                        const uint64_t initial = initial_caps[variant];
                        const uint64_t unit = initial * stride + allowance;
                        size_t contexts = 1048576 / unit;
                        if (contexts > 1024) contexts = 1024;
                        if (contexts == 0) contexts = 1;
                        const uint64_t batch_work = contexts * unit;
                        const uint64_t cycles = (budget + batch_work - 1) / batch_work;
                        ApiStorage *owners = malloc(contexts * sizeof *owners);
                        ApiObservation *snapshots = malloc(contexts * sizeof *snapshots);
                        uint64_t *returned = malloc(contexts * sizeof *returned);
                        uint64_t *offered = malloc(contexts * sizeof *offered);
                        require(owners && snapshots && returned && offered,
                                "growth append context storage");
                        for (unsigned control = 0; control < 2; ++control) {
                            uint64_t elapsed = 0;
                            for (uint64_t cycle = 0; cycle < cycles; ++cycle) {
                                const uint64_t seed = 101 + sample + cycle;
                                for (size_t k = 0; k < contexts; ++k) {
                                    uint64_t actual = growth_prepare(api, &owners[k],
                                                                      requested, seed + k);
                                    if (actual != initial)
                                        fprintf(stderr, "growth append capacity: %s requested=%" PRIu64
                                                " initial=%" PRIu64 " observed=%" PRIu64 "\n",
                                                api->name, requested, initial, actual);
                                    require(actual == initial, "growth append stable initial capacity");
                                    offered[k] = seed + k + initial;
                                }
                                const uint64_t start = nanos();
                                for (size_t k = 0; k < contexts; ++k)
                                    returned[k] = control
                                        ? api->snapshot(&owners[k], &snapshots[k])
                                        : api->append_one(&owners[k], offered[k]);
                                elapsed += nanos() - start;
                                for (size_t k = 0; k < contexts; ++k) {
                                    if (control) {
                                        require(returned[k] == initial && snapshots[k].length == initial
                                                && snapshots[k].capacity == initial
                                                && snapshots[k].valid == 1 && snapshots[k].checksum == 0,
                                                "growth append snapshot control state");
                                        ApiObservation actual = {0};
                                        require(api->inspect_reset(&owners[k], initial, seed + k, &actual) == 1
                                                && actual.length == initial && actual.capacity == initial
                                                && actual.valid == 1
                                                && actual.checksum == append_oracle(initial, seed + k, wide != 0),
                                                "growth append snapshot control values");
                                        observed = actual.checksum;
                                    } else {
                                        uint64_t capacity = growth_inspect_reset(api, &owners[k], initial,
                                            seed + k, returned[k], wide != 0, false);
                                        if (capacity != post_caps[variant])
                                            fprintf(stderr, "growth append capacity: %s old=%" PRIu64
                                                    " new=%" PRIu64 " expected=%" PRIu64 "\n",
                                                    api->name, initial, capacity, post_caps[variant]);
                                        require(capacity == post_caps[variant], "growth append stable postcapacity");
                                    }
                                    (void)api->destroy(&owners[k]);
                                }
                            }
                            printf("%s,%u,%zu,%" PRIu64 ",%s,%u,%u,%zu,%zu,%zu,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                                   control ? "append-growth-snapshot-control-o3" : "append-growth-o3",
                                   cohort, stride, requested, api->name, sample, control, contexts,
                                   contexts * (size_t)initial * stride, contexts * sizeof(ApiStorage), cycles,
                                   control ? 0 : cycles * contexts, elapsed, initial,
                                   control ? initial : post_caps[variant], budget, allowance);
                        }
                        free(offered); free(returned); free(snapshots); free(owners);
                    }
        }
}

static void edit_measure(uint64_t work_bytes, unsigned samples) {
    const uint64_t counts[] = {16, 256, 4096};
    puts("contract,cohort,operation,element_bytes,count,variant,sample,contexts,cycles,operations,payload_bytes,descriptor_bytes,elapsed_ns,control_elapsed_ns,initial_capacity,capacity");
    for (unsigned kind = 0; kind < EDIT_CASES; ++kind)
        for (unsigned wide = 0; wide < 2; ++wide)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n) {
                const uint64_t count = counts[n];
                const size_t stride = wide ? sizeof(Record) : sizeof(uint64_t);
                const uint64_t per_owner = count * stride;
                size_t contexts = (size_t)(UINT64_C(16777216) / per_owner);
                if (contexts > 256) contexts = 256;
                if (contexts == 0) contexts = 1;
                const uint64_t batch_bytes = contexts * per_owner;
                const uint64_t cycles = (work_bytes + batch_bytes - 1) / batch_bytes;
                const uint64_t repeat = kind == RESERVE_NOOP ? 64 : 1;
                for (unsigned cohort = 0; cohort < 2; ++cohort)
                    for (unsigned sample = 0; sample < samples; ++sample)
                        for (unsigned offset = 0; offset < 3; ++offset) {
                            unsigned position = (sample + offset) % 3;
                            unsigned variant = cohort ? 2 - position : position;
                            const ApiOperations *api = &append_api[wide][variant];
                            ApiStorage *owners = malloc(contexts * sizeof *owners);
                            ApiObservation *operations = calloc(contexts, sizeof *operations);
                            uint64_t *returned = malloc(contexts * sizeof *returned);
                            require(owners && operations && returned, "edit timing contexts");
                            uint64_t elapsed = 0, control_elapsed = 0;
                            uint64_t final_capacity = 0, initial_capacity = 0;
                            for (uint64_t cycle = 0; cycle < cycles; ++cycle) {
                                const uint64_t seed = 101 + sample + cycle;
                                for (size_t k = 0; k < contexts; ++k) {
                                    uint64_t cap = edit_setup(api, &owners[k],
                                        (enum EditCase)kind, count, seed + k);
                                    if (k == 0) initial_capacity = cap;
                                    require(cap == initial_capacity, "edit stable initial capacity");
                                }
                                const uint64_t control_start = nanos();
                                for (uint64_t pass = 0; pass < repeat; ++pass)
                                    for (size_t k = 0; k < contexts; ++k)
                                        (void)api->snapshot(&owners[k], &operations[k]);
                                control_elapsed += nanos() - control_start;
                                for (size_t k = 0; k < contexts; ++k)
                                    require(operations[k].length == count
                                            && operations[k].capacity == initial_capacity
                                            && operations[k].valid == 1,
                                            "edit snapshot control state");
                                const uint64_t start = nanos();
                                for (uint64_t pass = 0; pass < repeat; ++pass)
                                    for (size_t k = 0; k < contexts; ++k)
                                        returned[k] = edit_run(api, &owners[k],
                                            (enum EditCase)kind, count, seed + k,
                                            &operations[k]);
                                elapsed += nanos() - start;
                                for (size_t k = 0; k < contexts; ++k) {
                                    ApiObservation state = {0};
                                    (void)api->snapshot(&owners[k], &state);
                                    final_capacity = state.capacity;
                                    edit_verify(api, &owners[k], (enum EditCase)kind,
                                        count, seed + k, wide != 0, initial_capacity,
                                        returned[k], operations[k], false);
                                    (void)api->destroy(&owners[k]);
                                }
                            }
                            printf("vector-edit-o3,%u,%s,%zu,%" PRIu64 ",%s,%u,%zu,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%zu,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                                   cohort, edit_names[kind], stride, count, api->name, sample,
                                   contexts, cycles, cycles * contexts * repeat,
                                   batch_bytes, contexts * sizeof(ApiStorage), elapsed, control_elapsed,
                                   initial_capacity, final_capacity);
                            free(returned); free(operations); free(owners);
                        }
            }
}

#endif

static uint64_t checked_run(enum Variant variant, bool wide, uint64_t count,
                             uint64_t rounds, uint64_t seed, uint64_t path) {
    uint64_t expected = oracle(count, rounds, seed, wide, path);
    reset_accounting();
    uint64_t actual = run(variant, wide, count, rounds, seed, path);
    require(actual == expected, "independent logical-order checksum");
    check_accounting(variant, wide, count, rounds, path);
    return actual;
}

static void check(void) {
    const uint64_t counts[] = {0, 1, 2, 3, 8, 16, 63, 256, 4096, 8192};
    const uint64_t rounds[] = {0, 1, 3}, seeds[] = {0, 17, UINT64_MAX};
    size_t configurations = 0;
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned path = 0; path < 7; ++path)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n)
                for (size_t r = 0; r < sizeof rounds / sizeof rounds[0]; ++r)
                    for (size_t s = 0; s < sizeof seeds / sizeof seeds[0]; ++s) {
                        for (unsigned v = 0; v < VARIANT_COUNT; ++v)
                            (void)checked_run((enum Variant)v, wide != 0, counts[n], rounds[r], seeds[s], path);
                        ++configurations;
                    }
    require(configurations == 1260, "configuration matrix");
    printf("vector ecosystem: %zu configurations, %zu executions passed\n",
           configurations, configurations * VARIANT_COUNT);
}

#if defined(ACCOUNT_ONLY)
static void account(void) {
    const uint64_t counts[] = {16, 256, 4096};
    puts("contract,element_bytes,path,count,variant,rounds,traces,checksum,requests,realloc_requests,releases,requested_bytes,peak_bytes,peak_overlap_upper_bytes");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned path = 0; path < 7; ++path)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n)
                for (unsigned v = 0; v < VARIANT_COUNT; ++v) {
                    uint64_t checksum = checked_run((enum Variant)v, wide != 0, counts[n], 3, 101, path);
                    printf("account,%zu,%s,%" PRIu64 ",%s,3,1,%" PRIu64 ",%zu,%zu,%zu,%zu,%zu,%zu\n",
                           wide ? sizeof(Record) : sizeof(uint64_t), ecosystem_paths[path], counts[n],
                           ecosystem_variants[v], checksum, requests, realloc_requests, releases,
                           requested_bytes, peak_bytes, peak_overlap_upper_bytes);
                }
}
#else
static uint64_t positive_argument(const char *text, uint64_t limit) {
    require(*text != '\0', "empty numeric argument");
    uint64_t value = 0;
    for (; *text; ++text) {
        require(*text >= '0' && *text <= '9', "numeric argument");
        unsigned digit = (unsigned)(*text - '0');
        require(value <= (limit - digit) / 10, "numeric argument bound");
        value = value * 10 + digit;
    }
    require(value != 0 && value <= limit, "positive numeric argument bound");
    return value;
}

static void measure(uint64_t work, unsigned samples) {
    const uint64_t counts[] = {16, 256, 4096};
    puts("contract,cohort,element_bytes,path,count,variant,sample,work,rounds,traces,elapsed_ns,checksum");
    for (unsigned cohort = 0; cohort < 2; ++cohort)
        for (unsigned wide = 0; wide < 2; ++wide)
            for (unsigned path = 0; path < 7; ++path)
                for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n) {
                    uint64_t count = counts[n], unit = path >= 4 ? path - 3 : path == 3 ? 1 : count;
                    uint64_t rounds = work / unit;
                    if (rounds == 0) rounds = 1;
                    uint64_t warm_rounds = rounds / 16;
                    if (warm_rounds == 0) warm_rounds = 1;
                    for (unsigned v = 0; v < VARIANT_COUNT; ++v)
                        (void)checked_run((enum Variant)v, wide != 0, count, warm_rounds, 97, path);
                    for (unsigned sample = 0; sample < samples; ++sample) {
                        uint64_t seed = 101 + sample;
                        uint64_t expected = oracle(count, rounds, seed, wide != 0, path);
                        for (unsigned offset = 0; offset < VARIANT_COUNT; ++offset) {
                            unsigned position = (sample + offset) % VARIANT_COUNT;
                            unsigned v = cohort ? VARIANT_COUNT - 1 - position : position;
                            uint64_t before = nanos();
                            uint64_t checksum = run((enum Variant)v, wide != 0, count, rounds, seed, path);
                            uint64_t elapsed = nanos() - before;
                            require(checksum == expected, "timed logical-order checksum");
                            printf("normal-o3,%u,%zu,%s,%" PRIu64 ",%s,%u,%" PRIu64 ",%" PRIu64 ",1,%" PRIu64 ",%" PRIu64 "\n",
                                   cohort, wide ? sizeof(Record) : sizeof(uint64_t), ecosystem_paths[path], count,
                                   ecosystem_variants[v], sample, work, rounds, elapsed, checksum);
                        }
                    }
                }
}
#endif

int main(int argc, char **argv) {
    require(argc >= 2, "usage: vector ecosystem check|account|measure work samples");
    if (strcmp(argv[1], "check") == 0) { require(argc == 2, "check argument count"); check(); }
    else if (strcmp(argv[1], "api-check") == 0) { require(argc == 2, "api-check argument count"); append_check(false, false); }
    else if (strcmp(argv[1], "growth-api-check") == 0) {
        require(argc == 2, "growth-api-check argument count");
#if !defined(ACCOUNT_ONLY)
        growth_clock_check(false);
#endif
        growth_check(false, false, false);
    }
    else if (strcmp(argv[1], "edit-api-check") == 0) { require(argc == 2, "edit-api-check argument count"); edit_check(0); }
    else if (strcmp(argv[1], "edit-api-fail-values") == 0) { require(argc == 2, "edit-api-fail-values argument count"); edit_check(1); }
    else if (strcmp(argv[1], "edit-api-fail-state") == 0) { require(argc == 2, "edit-api-fail-state argument count"); edit_check(2); }
    else if (strcmp(argv[1], "edit-api-fail-cleanup") == 0) { require(argc == 2, "edit-api-fail-cleanup argument count"); edit_check(3); }
    else if (strcmp(argv[1], "growth-api-fail-values") == 0) { require(argc == 2, "growth-api-fail-values argument count"); growth_check(true, false, false); }
    else if (strcmp(argv[1], "growth-api-fail-state") == 0) { require(argc == 2, "growth-api-fail-state argument count"); growth_check(false, true, false); }
    else if (strcmp(argv[1], "api-fail-values") == 0) { require(argc == 2, "api-fail-values argument count"); append_check(true, false); }
    else if (strcmp(argv[1], "fail-checksum") == 0) {
        uint64_t actual = checked_run(WHITEFOOT, true, 3, 1, 17, 1);
        require((actual ^ 1) == oracle(3, 1, 17, true, 1), "independent logical-order checksum");
    }
#if defined(ACCOUNT_ONLY)
    else if (strcmp(argv[1], "account") == 0) { require(argc == 2, "account argument count"); account(); }
    else if (strcmp(argv[1], "allocator-check") == 0) { require(argc == 2, "allocator-check argument count"); allocator_check(false, false); }
    else if (strcmp(argv[1], "allocator-fail-values") == 0) { require(argc == 2, "allocator-fail-values argument count"); allocator_check(true, false); }
    else if (strcmp(argv[1], "allocator-fail-accounting") == 0) { require(argc == 2, "allocator-fail-accounting argument count"); allocator_check(false, true); }
    else if (strcmp(argv[1], "growth-api-account") == 0) { require(argc == 2, "growth-api-account argument count"); growth_check(false, false, true); }
    else if (strcmp(argv[1], "api-fail-allocation") == 0) { require(argc == 2, "api-fail-allocation argument count"); append_check(false, true); }
    else if (strcmp(argv[1], "fail-cleanup") == 0) {
        (void)checked_run(WHITEFOOT, true, 3, 1, 17, 1);
        (void)wf_cost_allocate(1);
        check_accounting(WHITEFOOT, true, 3, 1, 1);
    }
#else
    else if (strcmp(argv[1], "growth-api-fail-clock") == 0) {
        require(argc == 2, "growth-api-fail-clock argument count");
        growth_clock_check(true);
    }
    else if (strcmp(argv[1], "growth-api-measure") == 0) {
        require(argc == 4 || argc == 5,
                "growth-api-measure requires byte budget, samples and optional large-cell budget");
        const uint64_t work = positive_argument(argv[2], UINT64_C(4294967296));
        const uint64_t large_work = argc == 5
            ? positive_argument(argv[4], UINT64_C(68719476736)) : work;
        require(large_work >= work, "growth append large-cell budget at least base");
        growth_clock_check(false);
        growth_measure(work, (unsigned)positive_argument(argv[3], 101), large_work);
    }
    else if (strcmp(argv[1], "edit-api-measure") == 0) {
        require(argc == 4, "edit-api-measure requires byte budget and samples");
        edit_measure(positive_argument(argv[2], UINT64_C(4294967296)),
                     (unsigned)positive_argument(argv[3], 101));
    }
    else if (strcmp(argv[1], "api-measure") == 0) {
        require(argc == 4, "api-measure requires work and samples");
        append_measure(positive_argument(argv[2], UINT64_C(4294967296)),
                       (unsigned)positive_argument(argv[3], 101));
    }
    else if (strcmp(argv[1], "measure") == 0) {
        require(argc == 4, "measure requires work and samples");
        measure(positive_argument(argv[2], UINT64_C(4294967296)),
                (unsigned)positive_argument(argv[3], 101));
    }
#endif
    else require(false, "unsupported ecosystem command");
    return 0;
}
#endif
