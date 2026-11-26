/* C control for witness.wf: the same kind of dependent wrapping integer mixing
 * leaf, called twice per batch from a non-inlined loop, so perf unwinding of
 * ordinary compiler output can be compared with the Whitefoot witness.
 * Build: /usr/bin/clang -O2 -g0 control.c -o control
 * Usage: control [extra args]; N extra arguments select N batches (default 100),
 * as in the witness. Each batch is two hot_leaf calls of 1,000,000 iterations. */
#include <stdint.h>
#include <stdio.h>

static inline uint32_t rotl(uint32_t x, int n) { return (x << n) | (x >> (32 - n)); }
static inline uint32_t rotr(uint32_t x, int n) { return (x >> n) | (x << (32 - n)); }

__attribute__((noinline)) uint32_t hot_leaf(uint32_t seed) {
  uint32_t a = seed, b = seed + 2654435769u, c = seed ^ 2246822519u, d = seed + 3266489917u;
  for (uint64_t round = 0; round < 1000000u; round++) {
    uint32_t a_next = (rotl(a, 7) ^ d) * 1664525u + b;
    uint32_t b_next = (rotl(b, 11) ^ a_next) * 22695477u + c;
    uint32_t c_next = (rotl(c, 13) ^ b_next) * 1103515245u + d;
    uint32_t d_next = (rotl(d, 17) ^ c_next) * 747796405u + a_next;
    uint32_t a_final = a_next ^ rotr(c_next, 3);
    uint32_t b_final = b_next ^ rotr(d_next, 5);
    uint32_t c_final = c_next + rotr(a_final, 9);
    uint32_t d_final = d_next + rotr(b_final, 15);
    uint32_t e_next = (rotl(a_final, 19) ^ d_final) * 2891336453u + b_final;
    uint32_t f_next = (rotl(b_final, 23) ^ e_next) * 277803737u + c_final;
    uint32_t g_next = (rotl(c_final, 29) ^ f_next) * 1597334677u + d_final;
    uint32_t h_next = (rotl(d_final, 3) ^ g_next) * 3812015801u + e_next;
    uint32_t e_final = e_next ^ rotr(g_next, 7);
    uint32_t f_final = f_next ^ rotr(h_next, 11);
    uint32_t g_final = g_next + rotr(e_final, 13);
    uint32_t h_final = h_next + rotr(f_final, 17);
    uint32_t i_next = (rotl(e_final, 5) ^ h_final) * 3266489917u + f_final;
    uint32_t j_next = (rotl(f_final, 9) ^ i_next) * 2246822519u + g_final;
    uint32_t k_next = (rotl(g_final, 15) ^ j_next) * 747796405u + h_final;
    uint32_t l_next = (rotl(h_final, 21) ^ k_next) * 2891336453u + i_next;
    a = i_next ^ rotr(k_next, 19);
    b = j_next ^ rotr(l_next, 23);
    c = k_next + rotr(a, 29);
    d = l_next + rotr(b, 3);
  }
  return (a ^ b) + (c ^ d);
}

__attribute__((noinline)) uint32_t caller_loop(uint64_t batches, uint32_t seed) {
  uint32_t checksum = seed;
  for (uint64_t batch = 0; batch < batches; batch++) {
    uint32_t descendant = hot_leaf(checksum + 1013904223u);
    uint32_t following = hot_leaf(descendant ^ checksum);
    checksum = (following ^ descendant) + 2654435769u;
  }
  return checksum;
}

int main(int argc, char **argv) {
  (void)argv;
  uint64_t batches = argc > 1 ? (uint64_t)(argc - 1) : 100u;
  uint32_t checksum = caller_loop(batches, (uint32_t)argc);
  printf("%08x\n", checksum);
  return 0;
}
