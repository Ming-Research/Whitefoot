/* Compile the same leaf twice; opaque destinations prevent dispatch folding. */
#include <stdint.h>
#ifndef WF_ENTRY_LEAF
#error WF_ENTRY_LEAF must name the witness destination
#endif
__attribute__((noinline)) uint64_t WF_ENTRY_LEAF(uint64_t x) { return x + 1; }
