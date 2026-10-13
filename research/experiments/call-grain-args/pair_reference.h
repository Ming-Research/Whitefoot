/* Independent C reference of pair.wf (generate.py), shared by the linked
 * oracle and the stand-alone reference. MIX_STEPS must be defined. */
#ifndef CALL_GRAIN_PAIR_REFERENCE_H
#define CALL_GRAIN_PAIR_REFERENCE_H
#include <stdint.h>
#include <stdlib.h>
#ifndef MIX_STEPS
#error "define MIX_STEPS"
#endif
#define PAIR_LIMIT UINT64_C(16777216)
static inline uint64_t pair_rotl(uint64_t x, unsigned s) { return (x << s) | (x >> (64 - s)); }
static inline unsigned pair_rotation(unsigned k) { return 1 + (k * 7) % 61; }
static inline uint64_t pair_multiplier(unsigned k) {
    return (((UINT64_C(6364136223846793005) * (k + 1)) + UINT64_C(1442695040888963407)) &
            ((UINT64_C(1) << 63) - 1)) | 1;
}
static inline uint64_t pair_mix(uint64_t *words, uint64_t count, uint64_t salt) {
    uint64_t state = salt;
    for (uint64_t i = 0; i < count; ++i) {
        uint64_t value = words[i] ^ state;
        uint64_t rounds = 4 + (value & 7);
        for (uint64_t r = 0; r < rounds; ++r)
            for (unsigned k = 0; k < MIX_STEPS; ++k)
                value = ((pair_rotl(value, pair_rotation(k)) * pair_multiplier(k)) ^ r) + (k + 1);
        words[i] = value;
        state = value;
    }
    return state;
}
static inline uint64_t pair_reference(uint64_t repetitions, uint64_t extent, uint64_t seed) {
    if (extent > PAIR_LIMIT) return 0;
    uint64_t *left = malloc((extent ? extent : 1) * sizeof *left);
    uint64_t *right = malloc((extent ? extent : 1) * sizeof *right);
    if (!left || !right) abort();
    for (uint64_t i = 0; i < extent; ++i) { left[i] = seed; right[i] = seed + 1; }
    uint64_t total = seed;
    for (uint64_t i = 0; i < repetitions; ++i) {
        uint64_t a = pair_mix(left, extent, i);
        uint64_t b = pair_mix(right, extent, i + seed);
        total += a ^ b;
    }
    free(left);
    free(right);
    return total;
}
#endif
