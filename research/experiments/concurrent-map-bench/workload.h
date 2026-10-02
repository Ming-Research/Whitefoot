/* Serves concurrent-map-bench: the workload every driver draws, written once
 * here for the C driver and the Zipf generator; the Java, Go, .NET and
 * Whitefoot drivers restate these functions and must draw the same numbers.
 */
#ifndef WORKLOAD_H
#define WORKLOAD_H

#include <stdint.h>

#define WL_GOLDEN 0x9E3779B97F4A7C15ull
#define WL_MIX1 0xBF58476D1CE4E5B9ull
#define WL_MIX2 0x94D049BB133111EBull
#define WL_M62 ((1ull << 62) - 1)

/* Entries per thread in a Zipf rank buffer. */
#define WL_ZIPF_LENGTH (1u << 20)

/* The SplitMix64 finalizer. */
static inline uint64_t wl_mix64(uint64_t z) {
    z = (z ^ (z >> 30)) * WL_MIX1;
    z = (z ^ (z >> 27)) * WL_MIX2;
    return z ^ (z >> 31);
}

/* SplitMix64: the next number of a stream. */
static inline uint64_t wl_next(uint64_t *state) {
    *state += WL_GOLDEN;
    return wl_mix64(*state);
}

/* The key of an index: one plus the finalizer's steps modulo 2^62, a
 * bijection, so keys are distinct, never zero and below 2^62 + 1. */
static inline uint64_t wl_key(uint64_t index) {
    uint64_t x = index & WL_M62;
    x ^= x >> 31;
    x = (x * WL_MIX1) & WL_M62;
    x ^= x >> 29;
    x = (x * WL_MIX2) & WL_M62;
    x ^= x >> 32;
    return x + 1;
}

/* The seed of one thread's stream in one cell. */
static inline uint64_t wl_seed(unsigned mix, unsigned threads, unsigned thread) {
    return wl_mix64(((uint64_t)mix << 48) ^ ((uint64_t)threads << 32) ^ thread);
}

/* The operation roll in [0, 100), from the low half of a number. */
static inline uint32_t wl_roll(uint64_t z) {
    return (uint32_t)(((z & 0xffffffffull) * 100) >> 32);
}

/* A uniform index in [0, range), range at most 2^32, from the high half. */
static inline uint64_t wl_pick(uint64_t z, uint64_t range) {
    return ((z >> 32) * range) >> 32;
}

/* A Zipf rank file: this header, then streams * length little-endian 32-bit
 * ranks, stream after stream. */
struct wl_zipf_header {
    char magic[8]; /* "WFZIPF1" */
    uint64_t n;
    uint64_t streams;
    uint64_t length;
    double theta;
};

#endif
