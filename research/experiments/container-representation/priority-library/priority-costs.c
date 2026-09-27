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
#if defined(AUDIT_EVENTS)
static uint64_t comparisons, transfers;
#define EVENT(name, amount) (name += (amount))
#else
#define EVENT(name, amount) ((void)0)
#endif

#if defined(ECOSYSTEM)
enum { VARIANT_COUNT = 5 };
#else
enum { VARIANT_COUNT = 3 };
#endif
enum { RECORD_WORDS = 32, CEILING = 4096 };
typedef struct { uint64_t words[RECORD_WORDS]; } Record;
typedef struct { uint8_t unused; } Order;
#if defined(WITH_WF)
extern uint64_t wf_priority_cost_word_trace(uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_priority_cost_record_trace(uint64_t, uint64_t, uint64_t, uint64_t);
#endif
#if defined(ECOSYSTEM)
extern uint64_t priority_rust_word_trace(uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t priority_rust_record_trace(uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t priority_cpp_word_trace(uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t priority_cpp_record_trace(uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t priority_rust_refusal(uint64_t);
extern uint64_t priority_cpp_refusal(uint64_t);
#endif
#if !defined(ECOSYSTEM) || defined(ACCOUNT_ONLY)
typedef union {
    struct { size_t bytes; uint64_t magic; } value;
    max_align_t alignment;
} AllocationHeader;
static size_t requests, requested_bytes, live_bytes, peak_bytes;
#if defined(ECOSYSTEM)
static size_t realloc_requests, releases, peak_overlap_upper_bytes;
#endif
#endif
static volatile uint64_t observed;
static const char *const path_names[] = {
    "pop-push", "replace-top", "grow-pop", "heapify-pop", "setup-cleanup"
};
static const char *const variant_names[] = { "whitefoot", "swap-c", "hole-c"
#if defined(ECOSYSTEM)
    , "rust-binary-heap", "cpp-std-heap"
#endif
};

static void require(bool condition, const char *message) {
    if (!condition) { fprintf(stderr, "priority library costs: %s\n", message); exit(1); }
}
#if defined(ECOSYSTEM) && defined(ACCOUNT_ONLY)
void wf_ecosystem_note_alloc(uint64_t bytes) {
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
    // The System realloc call is preserved. Its internal transient footprint
    // is hidden: record logical live bytes and a separate old+new upper bound.
    if (live_bytes + new_bytes > peak_overlap_upper_bytes)
        peak_overlap_upper_bytes = live_bytes + (size_t)new_bytes;
    ++requests; ++realloc_requests; requested_bytes += (size_t)new_bytes;
    live_bytes = live_bytes - (size_t)old_bytes + (size_t)new_bytes;
    if (live_bytes > peak_bytes) peak_bytes = live_bytes;
}
#endif
#if defined(ECOSYSTEM) && !defined(ACCOUNT_ONLY)
// Practical timed images have no accounting header, hook or counter.
#define wf_cost_allocate(bytes) malloc((size_t)(bytes))
#define wf_cost_release(pointer) free(pointer)
#else
NOINLINE void *wf_cost_allocate(uint64_t bytes) {
    require(bytes <= SIZE_MAX - sizeof(AllocationHeader), "allocation extent");
    AllocationHeader *header = malloc(sizeof *header + (size_t)bytes);
    require(header != NULL, "host allocation failure");
    header->value.bytes = (size_t)bytes;
    header->value.magic = UINT64_C(0x7072696f72697479);
#if defined(ECOSYSTEM)
    wf_ecosystem_note_alloc(bytes);
#else
    ++requests; requested_bytes += (size_t)bytes; live_bytes += (size_t)bytes;
    if (live_bytes > peak_bytes) peak_bytes = live_bytes;
#endif
    return header + 1;
}
NOINLINE void wf_cost_release(void *pointer) {
    if (pointer == NULL) return;
    AllocationHeader *header = (AllocationHeader *)pointer - 1;
    require(header->value.magic == UINT64_C(0x7072696f72697479), "allocation identity");
    require(live_bytes >= header->value.bytes, "live-byte accounting");
#if defined(ECOSYSTEM)
    wf_ecosystem_note_dealloc(header->value.bytes);
#else
    live_bytes -= header->value.bytes;
#endif
    header->value.magic = 0; free(header);
}
#endif
static void reset_accounting(void) {
#if !defined(ECOSYSTEM) || defined(ACCOUNT_ONLY)
    require(live_bytes == 0, "allocation left live between traces");
    requests = requested_bytes = peak_bytes = 0;
#if defined(ECOSYSTEM)
    realloc_requests = releases = peak_overlap_upper_bytes = 0;
#endif
#endif
}
static uint64_t next_state(uint64_t state) {
    return state * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
}
static HELPER uint64_t word_make(uint64_t seed) { return seed; }
static HELPER Record record_make(uint64_t seed) {
    Record value;
    for (size_t i = 0; i < RECORD_WORDS; ++i) value.words[i] = seed + i;
    return value;
}
static HELPER void word_accept(uint64_t *digest, uint64_t value) {
    *digest = *digest * UINT64_C(131) + value;
}
static HELPER void record_accept(uint64_t *digest, Record value) {
    for (size_t i = 0; i < RECORD_WORDS; ++i)
        *digest = *digest * UINT64_C(131) + value.words[i];
}
static HELPER int32_t word_compare(const Order *env, const uint64_t *left, const uint64_t *right) {
    (void)env;
    EVENT(comparisons, 1);
    return (*left > *right) - (*left < *right);
}
static HELPER int32_t record_compare(const Order *env, const Record *left, const Record *right) {
    (void)env;
    EVENT(comparisons, 1);
    return (left->words[0] > right->words[0]) - (left->words[0] < right->words[0]);
}
static HELPER uint64_t word_observe(uint64_t *digest, const uint64_t *value) { (void)digest; return *value; }
static HELPER uint64_t record_observe(uint64_t *digest, const Record *value) { (void)digest; return value->words[0]; }

/* The swap control follows the library's valid-prefix operations. The hole
 * control keeps the same public boundaries and uses a conventional private
 * pending value while sifting. EVENT counts C element assignments outside
 * timed builds; it is not a count of optimized machine loads or stores. */
#define DEFINE_QUEUE(P, T, MAKE, ACCEPT, COMPARE, OBSERVE, HOLE)               \
typedef struct { uint64_t length, capacity; T data[]; } P##_Block;             \
typedef struct { uint32_t tag; T refused; } P##_Result;                        \
_Static_assert(offsetof(P##_Block, data) == 16, "Slots header extent");        \
static P##_Block *P##_storage(uint64_t capacity) {                             \
    P##_Block *block = wf_cost_allocate(16 + capacity * sizeof(T));            \
    block->length = 0; block->capacity = capacity; return block;               \
}                                                                            \
static HELPER P##_Block *P##_new(void) { return P##_storage(0); }              \
static HELPER uint64_t P##_len(P##_Block **owner) { return (*owner)->length; } \
static HELPER uint64_t P##_reserve(P##_Block **owner, uint64_t total) {        \
    P##_Block *old = *owner;                                                  \
    if (old->capacity >= total) return old->capacity;                         \
    P##_Block *fresh = P##_storage(total); fresh->length = old->length;        \
    memcpy(fresh->data, old->data, old->length * sizeof(T));                   \
    EVENT(transfers, old->length);                                           \
    wf_cost_release(old); *owner = fresh; return total;                       \
}                                                                            \
static void P##_make_room(P##_Block **owner) {                                \
    P##_Block *block = *owner;                                                \
    if (block->capacity > block->length) return;                              \
    if (block->capacity == 0) { (void)P##_reserve(owner, 1); return; }         \
    if (block->capacity <= UINT64_MAX / 2) {                                  \
        uint64_t doubled = block->capacity + block->capacity;                \
        if (doubled <= CEILING) { (void)P##_reserve(owner, doubled); return; } \
    }                                                                        \
    (void)P##_reserve(owner, CEILING);                                        \
}                                                                            \
static void P##_rise(P##_Block **owner, uint64_t count, const Order *env) {     \
    P##_Block *block = *owner; uint64_t at = count - 1;                        \
    if (HOLE) {                                                              \
        T pending = block->data[at]; EVENT(transfers, 1);                     \
        while (at != 0) {                                                    \
            uint64_t parent = (at - 1) / 2;                                 \
            if (COMPARE(env, &block->data[parent], &pending) <= 0) break;     \
            block->data[at] = block->data[parent]; EVENT(transfers, 1);       \
            at = parent;                                                     \
        }                                                                    \
        block->data[at] = pending; EVENT(transfers, 1);                       \
    } else {                                                                 \
        while (at != 0) {                                                    \
            uint64_t parent = (at - 1) / 2;                                 \
            if (COMPARE(env, &block->data[parent], &block->data[at]) <= 0) break; \
            T temporary = block->data[parent];                              \
            block->data[parent] = block->data[at];                           \
            block->data[at] = temporary; EVENT(transfers, 3);                 \
            at = parent;                                                     \
        }                                                                    \
    }                                                                        \
}                                                                            \
static uint64_t P##_child(P##_Block **owner, uint64_t left, const Order *env) { \
    P##_Block *block = *owner; uint64_t right = left + 1;                     \
    if (right < block->length && COMPARE(env, &block->data[right], &block->data[left]) < 0) return right; \
    return left;                                                             \
}                                                                            \
static void P##_sink(P##_Block **owner, uint64_t start, uint64_t count, const Order *env) { \
    P##_Block *block = *owner; uint64_t at = start, parents = count / 2;       \
    if (HOLE) {                                                              \
        T pending = block->data[at]; EVENT(transfers, 1);                     \
        while (at < parents) {                                               \
            uint64_t left = 2 * at + 1, best = P##_child(owner, left, env);   \
            if (COMPARE(env, &pending, &block->data[best]) <= 0) break;       \
            block->data[at] = block->data[best]; EVENT(transfers, 1);          \
            at = best;                                                       \
        }                                                                    \
        block->data[at] = pending; EVENT(transfers, 1);                       \
    } else {                                                                 \
        while (at < parents) {                                               \
            uint64_t left = 2 * at + 1, best = P##_child(owner, left, env);   \
            if (COMPARE(env, &block->data[at], &block->data[best]) <= 0) break; \
            T temporary = block->data[at];                                   \
            block->data[at] = block->data[best];                              \
            block->data[best] = temporary; EVENT(transfers, 3);               \
            at = best;                                                       \
        }                                                                    \
    }                                                                        \
}                                                                            \
static HELPER P##_Result P##_push(P##_Block **owner, T value, const Order *env) { \
    P##_Result result;                                                       \
    if ((*owner)->length >= CEILING) {                                       \
        result.tag = 1; result.refused = value; EVENT(transfers, 1);          \
        return result;                                                       \
    }                                                                        \
    P##_make_room(owner); P##_Block *block = *owner;                          \
    block->data[block->length++] = value; EVENT(transfers, 1);                \
    P##_rise(owner, block->length, env); result.tag = 0; return result;       \
}                                                                            \
static HELPER uint64_t P##_peek(P##_Block **owner, uint64_t *digest) {        \
    return OBSERVE(digest, &(*owner)->data[0]);                                \
}                                                                            \
static HELPER T P##_pop(P##_Block **owner, const Order *env) {                \
    P##_Block *block = *owner; T removed = block->data[--block->length];       \
    EVENT(transfers, 1);                                                      \
    if (block->length == 0) return removed;                                  \
    T temporary = block->data[0]; block->data[0] = removed; removed = temporary; \
    EVENT(transfers, 3);                                                      \
    P##_sink(owner, 0, block->length, env); return removed;                   \
}                                                                            \
static HELPER T P##_replace_top(P##_Block **owner, T value, const Order *env) { \
    P##_Block *block = *owner;                                                \
    T temporary = block->data[0]; block->data[0] = value; value = temporary;   \
    EVENT(transfers, 3);                                                      \
    P##_sink(owner, 0, block->length, env); return value;                     \
}                                                                            \
static HELPER P##_Block *P##_heapify(P##_Block *block, const Order *env) {    \
    for (uint64_t at = block->length / 2; at != 0; --at)                      \
        P##_sink(&block, at - 1, block->length, env);                         \
    return block;                                                            \
}                                                                            \
static HELPER void P##_drain(P##_Block **owner, const Order *env, uint64_t *digest) { \
    while ((*owner)->length != 0) ACCEPT(digest, P##_pop(owner, env));         \
}                                                                            \
static HELPER void P##_free(P##_Block *block, uint64_t *digest) {             \
    while (block->length != 0) {                                             \
        T removed = block->data[--block->length]; EVENT(transfers, 1);        \
        ACCEPT(digest, removed);                                             \
    }                                                                        \
    wf_cost_release(block);                                                  \
}                                                                            \
static NOINLINE uint64_t P##_trace(uint64_t count, uint64_t rounds, uint64_t seed, uint64_t path) { \
    uint64_t digest = seed, state = seed; const Order order = {0};            \
    if (path >= 3) {                                                         \
        P##_Block *block = P##_storage(count);                               \
        for (uint64_t i = 0; i < count; ++i) {                                \
            state = next_state(state); block->data[block->length++] = MAKE(state); EVENT(transfers, 1); \
        }                                                                    \
        if (path != 4) { block = P##_heapify(block, &order); P##_drain(&block, &order, &digest); } \
        P##_free(block, &digest); return digest;                              \
    }                                                                        \
    P##_Block *block = P##_new();                                            \
    if (path < 2) (void)P##_reserve(&block, count);                           \
    for (uint64_t i = 0; i < count; ++i) {                                    \
        state = next_state(state); P##_Result result = P##_push(&block, MAKE(state), &order); \
        if (result.tag != 0) ACCEPT(&digest, result.refused);                 \
    }                                                                        \
    if (path < 2) {                                                          \
        if (P##_len(&block) != 0) digest = digest * UINT64_C(131) + P##_peek(&block, &digest); \
        for (uint64_t round = 0; round < rounds; ++round)                     \
            for (uint64_t i = 0; i < count; ++i) {                            \
                state = next_state(state); T value = MAKE(state);            \
                if (P##_len(&block) != 0) {                                  \
                    if (path == 0) {                                        \
                        ACCEPT(&digest, P##_pop(&block, &order));            \
                        P##_Result result = P##_push(&block, value, &order); \
                        if (result.tag != 0) ACCEPT(&digest, result.refused); \
                    } else ACCEPT(&digest, P##_replace_top(&block, value, &order)); \
                } else ACCEPT(&digest, value);                               \
            }                                                                \
    }                                                                        \
    P##_drain(&block, &order, &digest); P##_free(block, &digest); return digest; \
}

DEFINE_QUEUE(word_swap, uint64_t, word_make, word_accept, word_compare, word_observe, 0)
DEFINE_QUEUE(word_hole, uint64_t, word_make, word_accept, word_compare, word_observe, 1)
DEFINE_QUEUE(record_swap, Record, record_make, record_accept, record_compare, record_observe, 0)
DEFINE_QUEUE(record_hole, Record, record_make, record_accept, record_compare, record_observe, 1)
_Static_assert(sizeof(Record) == 256, "wide element extent");
_Static_assert(sizeof(word_swap_Result) == 16, "scalar Result extent");
_Static_assert(sizeof(record_swap_Result) == 264, "wide Result extent");

#define DEFINE_REFUSAL_CHECK(P, T, MAKE, ACCEPT)                              \
static uint64_t P##_refusal_check(void) {                                    \
    P##_Block *block = P##_new();                                            \
    (void)P##_reserve(&block, CEILING);                                      \
    uint64_t state = 101, digest = 101; const Order order = {0};              \
    for (uint64_t i = 0; i < CEILING; ++i) {                                 \
        state = next_state(state);                                          \
        P##_Result added = P##_push(&block, MAKE(state), &order);            \
        require(added.tag == 0, "native full-capacity insertion");           \
    }                                                                        \
    state = next_state(state); T value = MAKE(state);                        \
    P##_Result refused = P##_push(&block, value, &order);                    \
    require(refused.tag == 1 && memcmp(&refused.refused, &value, sizeof(T)) == 0, "native refusal preserves every payload byte"); \
    require(P##_len(&block) == CEILING, "native refusal preserves length"); \
    ACCEPT(&digest, P##_pop(&block, &order));                                \
    P##_Result retried = P##_push(&block, refused.refused, &order);           \
    require(retried.tag == 0, "native refusal owner can be retried");        \
    P##_drain(&block, &order, &digest); P##_free(block, &digest); return digest; \
}
DEFINE_REFUSAL_CHECK(word_swap, uint64_t, word_make, word_accept)
DEFINE_REFUSAL_CHECK(word_hole, uint64_t, word_make, word_accept)
DEFINE_REFUSAL_CHECK(record_swap, Record, record_make, record_accept)
DEFINE_REFUSAL_CHECK(record_hole, Record, record_make, record_accept)

static uint64_t run(unsigned variant, bool wide, uint64_t count, uint64_t rounds, uint64_t seed, uint64_t path) {
#if defined(ECOSYSTEM)
    if (variant == 3) return wide ? priority_rust_record_trace(count, rounds, seed, path) : priority_rust_word_trace(count, rounds, seed, path);
    if (variant == 4) return wide ? priority_cpp_record_trace(count, rounds, seed, path) : priority_cpp_word_trace(count, rounds, seed, path);
#endif
#if defined(WITH_WF)
    if (variant == 0) return wide ? wf_priority_cost_record_trace(count, rounds, seed, path) : wf_priority_cost_word_trace(count, rounds, seed, path);
#endif
    require(variant == 1 || variant == 2, "available variant");
    if (wide) return variant == 1 ? record_swap_trace(count, rounds, seed, path) : record_hole_trace(count, rounds, seed, path);
    return variant == 1 ? word_swap_trace(count, rounds, seed, path) : word_hole_trace(count, rounds, seed, path);
}
static int sort_word(const void *left, const void *right) {
    uint64_t a = *(const uint64_t *)left, b = *(const uint64_t *)right;
    return (a > b) - (a < b);
}
static uint64_t oracle_accept(uint64_t digest, uint64_t value, bool wide) {
    for (unsigned i = 0; i < (wide ? RECORD_WORDS : 1); ++i)
        digest = digest * UINT64_C(131) + value + i;
    return digest;
}
static uint64_t oracle(bool wide, uint64_t count, uint64_t rounds, uint64_t seed, uint64_t path) {
    uint64_t values[CEILING], state = seed, digest = seed;
    for (uint64_t i = 0; i < count; ++i) { state = next_state(state); values[i] = state; }
    if (path == 4) {
        for (uint64_t i = count; i != 0; --i) digest = oracle_accept(digest, values[i - 1], wide);
        return digest;
    }
    qsort(values, (size_t)count, sizeof values[0], sort_word);
    if (path < 2 && count != 0) {
        digest = digest * UINT64_C(131) + values[0];
        for (uint64_t round = 0; round < rounds; ++round)
            for (uint64_t i = 0; i < count; ++i) {
                digest = oracle_accept(digest, values[0], wide);
                state = next_state(state);
                uint64_t at = 1;
                while (at < count && values[at] < state) ++at;
                memmove(values, values + 1, (size_t)(at - 1) * sizeof values[0]);
                values[at - 1] = state;
            }
    }
    for (uint64_t i = 0; i < count; ++i) digest = oracle_accept(digest, values[i], wide);
    return digest;
}
static uint64_t refusal_oracle(bool wide) {
    uint64_t values[CEILING], state = 101, digest = 101;
    for (size_t i = 0; i < CEILING; ++i) { state = next_state(state); values[i] = state; }
    qsort(values, CEILING, sizeof values[0], sort_word);
    digest = oracle_accept(digest, values[0], wide);
    values[0] = next_state(state);
    qsort(values, CEILING, sizeof values[0], sort_word);
    for (size_t i = 0; i < CEILING; ++i) digest = oracle_accept(digest, values[i], wide);
    return digest;
}
#if !defined(ACCOUNT_ONLY)
static uint64_t nanos(void) {
#if defined(_WIN32)
    LARGE_INTEGER value, frequency;
    assert(QueryPerformanceCounter(&value) != 0); assert(QueryPerformanceFrequency(&frequency) != 0);
    return (uint64_t)((long double)value.QuadPart * 1.0e9L / frequency.QuadPart);
#else
    struct timespec value; assert(clock_gettime(CLOCK_MONOTONIC, &value) == 0);
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
#endif
}
#endif
static void check_accounting(unsigned variant, bool wide, uint64_t count, uint64_t path, uint64_t traces) {
#if defined(ECOSYSTEM) && !defined(ACCOUNT_ONLY)
    (void)variant; (void)wide; (void)count; (void)path; (void)traces;
#else
    size_t stride = wide ? sizeof(Record) : sizeof(uint64_t);
#if defined(ECOSYSTEM)
    require(releases + realloc_requests == requests,
            "every observed backing request is released or reallocated");
    if (variant >= 3) {
        require(live_bytes == 0, "native complete backing cleanup");
        require(count == 0 || (requests >= traces && requested_bytes >= count * stride * traces && peak_bytes >= count * stride),
                "native allocation observation covers the live population");
        return;
    }
#else
    (void)variant;
#endif
    size_t expected_requests = 1, expected_bytes = 16, expected_peak = 16;
    if (path >= 3) expected_bytes = expected_peak = 16 + count * stride;
    else if (path < 2 && count != 0) {
        expected_requests = 2; expected_bytes = expected_peak = 32 + count * stride;
    } else if (path == 2) {
        uint64_t previous = 0;
        for (uint64_t capacity = 1; previous < count; capacity *= 2) {
            ++expected_requests; expected_bytes += 16 + capacity * stride;
            expected_peak = 32 + (previous + capacity) * stride;
            previous = capacity;
        }
    }
    require(requests == expected_requests * traces && requested_bytes == expected_bytes * traces && peak_bytes == expected_peak,
            "independent allocation count, requested-byte and peak formula");
    require(live_bytes == 0, "complete backing cleanup");
#endif
}
static void check(void) {
    const uint64_t counts[] = {0, 1, 2, 3, 16, 63, 256, 4096};
    const uint64_t rounds[] = {0, 1, 3}, seeds[] = {0, 17, UINT64_MAX};
    size_t executions = 0;
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned path = 0; path < 5; ++path)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n)
                for (size_t r = 0; r < sizeof rounds / sizeof rounds[0]; ++r)
                    for (size_t s = 0; s < sizeof seeds / sizeof seeds[0]; ++s) {
                        uint64_t expected = oracle(wide != 0, counts[n], rounds[r], seeds[s], path);
#if defined(WITH_WF)
                        const unsigned first_variant = 0;
#else
                        const unsigned first_variant = 1;
#endif
                        for (unsigned v = first_variant; v < VARIANT_COUNT; ++v) {
                            reset_accounting();
                            require(run(v, wide != 0, counts[n], rounds[r], seeds[s], path) == expected,
                                    "independent sorted-sequence checksum");
                            check_accounting(v, wide != 0, counts[n], path, 1); ++executions;
                        }
                    }
    for (unsigned variant = 1; variant < VARIANT_COUNT; ++variant)
        for (unsigned wide = 0; wide < 2; ++wide) {
            reset_accounting();
            uint64_t actual;
#if defined(ECOSYSTEM)
            if (variant == 3) actual = priority_rust_refusal(wide);
            else if (variant == 4) actual = priority_cpp_refusal(wide);
            else
#endif
            actual = wide ? (variant == 1 ? record_swap_refusal_check() : record_hole_refusal_check())
                                   : (variant == 1 ? word_swap_refusal_check() : word_hole_refusal_check());
            require(actual == refusal_oracle(wide != 0), "independent native refusal/retry sequence");
            check_accounting(variant, wide != 0, CEILING, 0, 1);
        }
    printf("priority library costs: %zu complete scalar/owning trace executions and %u native refusal/retry chains passed\n", executions, (VARIANT_COUNT - 1) * 2);
}
#if !defined(ECOSYSTEM)
static void measure(unsigned cohort) {
    const uint64_t counts[] = {16, 256, 4096};
    puts("contract,cohort,element_bytes,path,count,variant,sample,rounds,traces,elapsed_ns,checksum,requests,requested_bytes,peak_bytes");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned path = 0; path < 5; ++path)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n)
                for (unsigned sample = 0; sample < 7; ++sample) {
                    uint64_t count = counts[n], seed = 101 + sample;
                    uint64_t repetitions = (wide ? UINT64_C(4096) : UINT64_C(16384)) / count;
                    uint64_t rounds = path < 2 ? repetitions : 0;
                    uint64_t traces = path < 2 ? 1 : repetitions, expected = 0;
                    for (uint64_t trace = 0; trace < traces; ++trace)
                        expected = expected * UINT64_C(257) + oracle(wide != 0, count, rounds, seed + trace, path);
                    for (unsigned offset = 0; offset < VARIANT_COUNT; ++offset) {
                        unsigned position = (sample + offset) % VARIANT_COUNT;
                        unsigned variant = cohort ? VARIANT_COUNT - 1 - position : position;
                        reset_accounting(); uint64_t before = nanos(), checksum = 0;
                        for (uint64_t trace = 0; trace < traces; ++trace)
                            checksum = checksum * UINT64_C(257) + run(variant, wide != 0, count, rounds, seed + trace, path);
                        uint64_t elapsed = nanos() - before; observed = checksum;
                        require(checksum == expected, "timed sorted-sequence checksum");
                        check_accounting(variant, wide != 0, count, path, traces);
                        printf("%s,%u,%zu,%s,%" PRIu64 ",%s,%u,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%zu,%zu,%zu\n",
                               CONTRACT, cohort, wide ? sizeof(Record) : sizeof(uint64_t), path_names[path], count,
                               variant_names[variant], sample, rounds, traces, elapsed, checksum, requests, requested_bytes, peak_bytes);
                    }
                }
}
#if defined(AUDIT_EVENTS)
static void events(void) {
    const uint64_t counts[] = {16, 256, 4096};
    puts("element_bytes,path,count,variant,rounds,comparisons,element_assignments,assigned_bytes");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned path = 0; path < 5; ++path)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n)
                for (unsigned variant = 1; variant < VARIANT_COUNT; ++variant) {
                    uint64_t rounds = path < 2 ? 1 : 0;
                    comparisons = transfers = 0; reset_accounting();
                    uint64_t result = run(variant, wide != 0, counts[n], rounds, 101, path);
                    require(result == oracle(wide != 0, counts[n], rounds, 101, path), "counted sorted-sequence checksum");
                    check_accounting(variant, wide != 0, counts[n], path, 1);
                    size_t stride = wide ? sizeof(Record) : sizeof(uint64_t);
                    printf("%zu,%s,%" PRIu64 ",%s,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                           stride, path_names[path], counts[n], variant_names[variant], rounds, comparisons, transfers, transfers * stride);
                }
}
#endif
#endif
#if defined(ECOSYSTEM)
static const char *series(unsigned variant, unsigned path) {
    if (path == 4) return "storage-control";
    return variant == 1 || variant == 2 ? "c-control" : "practical";
}
#if defined(ACCOUNT_ONLY)
static void negative_accounting(void) {
    reset_accounting();
    uint64_t checksum = run(3, true, 16, 1, 101, 0);
    require(checksum == oracle(true, 16, 1, 101, 0), "independent sorted-sequence checksum");
    // A single simulated unreleased backing must fail the ordinary observer
    // checks. This command is never used in an accounting or timing sample.
    wf_ecosystem_note_alloc(8);
    check_accounting(3, true, 16, 0, 1);
}
static void account(void) {
    const uint64_t counts[] = {16, 256, 4096};
    puts("series,element_bytes,path,count,variant,rounds,traces,checksum,allocation_requests,reallocation_requests,deallocations,requested_bytes,peak_live_requested_bytes,reallocation_peak_upper_bound_bytes,live_requested_bytes");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned path = 0; path < 5; ++path)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n) {
                uint64_t rounds = path < 2 ? 1 : 0;
                uint64_t expected = oracle(wide != 0, counts[n], rounds, 101, path);
                for (unsigned variant = 0; variant < VARIANT_COUNT; ++variant) {
                    reset_accounting();
                    uint64_t checksum = run(variant, wide != 0, counts[n], rounds, 101, path);
                    observed = checksum;
                    require(checksum == expected, "accounted sorted-sequence checksum");
                    check_accounting(variant, wide != 0, counts[n], path, 1);
                    printf("%s,%zu,%s,%" PRIu64 ",%s,%" PRIu64 ",1,%" PRIu64 ",%zu,%zu,%zu,%zu,%zu,%zu,%zu\n",
                           series(variant, path), wide ? sizeof(Record) : sizeof(uint64_t), path_names[path], counts[n],
                           variant_names[variant], rounds, checksum, requests, realloc_requests, releases,
                           requested_bytes, peak_bytes, peak_overlap_upper_bytes, live_bytes);
                }
            }
}
#else
static void negative_checksum(void) {
    uint64_t checksum = run(3, true, 16, 1, 101, 0) ^ UINT64_C(1);
    require(checksum == oracle(true, 16, 1, 101, 0), "independent sorted-sequence checksum");
}
static uint64_t run_repeated(unsigned variant, bool wide, uint64_t count, uint64_t rounds,
                             uint64_t seed, uint64_t path, uint64_t traces) {
    uint64_t checksum = 0;
    for (uint64_t trace = 0; trace < traces; ++trace)
        checksum = checksum * UINT64_C(257) + run(variant, wide, count, rounds, seed + trace, path);
    return checksum;
}
static void measure(unsigned cohort, uint64_t multiplier) {
    const uint64_t counts[] = {16, 256, 4096};
    puts("contract,cohort,series,element_bytes,path,count,variant,sample,seed,rounds,traces,work_multiplier,elapsed_ns,checksum");
    for (unsigned wide = 0; wide < 2; ++wide)
        for (unsigned path = 0; path < 5; ++path)
            for (size_t n = 0; n < sizeof counts / sizeof counts[0]; ++n) {
                uint64_t count = counts[n];
                uint64_t repetitions = multiplier * (wide ? UINT64_C(4096) : UINT64_C(16384)) / count;
                uint64_t rounds = path < 2 ? repetitions : 0;
                uint64_t traces = path < 2 ? 1 : repetitions;
                for (unsigned sample = 0; sample < 7; ++sample) {
                    uint64_t seed = 101 + sample, expected = 0;
                    for (uint64_t trace = 0; trace < traces; ++trace)
                        expected = expected * UINT64_C(257) + oracle(wide != 0, count, rounds, seed + trace, path);
                    if (sample == 0)
                        for (unsigned offset = 0; offset < VARIANT_COUNT; ++offset) {
                            unsigned variant = cohort ? VARIANT_COUNT - 1 - offset : offset;
                            uint64_t checksum = run_repeated(variant, wide != 0, count, rounds, seed, path, traces);
                            observed = checksum;
                            require(checksum == expected, "warmup sorted-sequence checksum");
                        }
                    for (unsigned offset = 0; offset < VARIANT_COUNT; ++offset) {
                        unsigned position = (sample + offset) % VARIANT_COUNT;
                        unsigned variant = cohort ? VARIANT_COUNT - 1 - position : position;
                        uint64_t before = nanos();
                        uint64_t checksum = run_repeated(variant, wide != 0, count, rounds, seed, path, traces);
                        uint64_t elapsed = nanos() - before;
                        observed = checksum;
                        require(checksum == expected, "timed sorted-sequence checksum");
                        printf("%s,%u,%s,%zu,%s,%" PRIu64 ",%s,%u,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                               CONTRACT, cohort, series(variant, path), wide ? sizeof(Record) : sizeof(uint64_t),
                               path_names[path], count, variant_names[variant], sample, seed, rounds, traces,
                               multiplier, elapsed, checksum);
                    }
                }
            }
}
#endif
#endif
int main(int argc, char **argv) {
    require(argc >= 2, "usage: priority-costs check|measure COHORT [WORK_MULTIPLIER]|account|events");
    if (strcmp(argv[1], "check") == 0) { require(argc == 2, "check arguments"); check(); }
#if defined(ECOSYSTEM) && defined(ACCOUNT_ONLY)
    else if (strcmp(argv[1], "negative-accounting") == 0) {
        require(argc == 2, "negative accounting arguments");
        negative_accounting();
    }
    else {
        require(argc == 2 && strcmp(argv[1], "account") == 0, "account arguments");
        account();
    }
#else
#if defined(ECOSYSTEM)
    else if (strcmp(argv[1], "negative-checksum") == 0) {
        require(argc == 2, "negative checksum arguments");
        negative_checksum();
    }
#endif
#if defined(AUDIT_EVENTS)
    else if (strcmp(argv[1], "events") == 0) { require(argc == 2, "events arguments"); events(); }
#endif
    else {
#if defined(ECOSYSTEM)
        require((argc == 3 || argc == 4) && strcmp(argv[1], "measure") == 0 && (strcmp(argv[2], "0") == 0 || strcmp(argv[2], "1") == 0), "measure cohort 0 or 1 and optional work multiplier");
        uint64_t multiplier = 16;
        if (argc == 4) {
            char *end = NULL;
            multiplier = strtoull(argv[3], &end, 10);
            require(*argv[3] >= '0' && *argv[3] <= '9' && *end == '\0' && multiplier >= 1 && multiplier <= 64,
                    "work multiplier from 1 through 64");
        }
        measure((unsigned)(argv[2][0] - '0'), multiplier);
#else
        require(argc == 3 && strcmp(argv[1], "measure") == 0 && (strcmp(argv[2], "0") == 0 || strcmp(argv[2], "1") == 0), "measure cohort 0 or 1");
        measure((unsigned)(argv[2][0] - '0'));
#endif
    }
#endif
    return 0;
}
