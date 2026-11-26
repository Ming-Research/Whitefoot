/* Emitted heap storage alone references this unit. Keep allocator calls out
 * of the unconditional runtime so a heap-free link needs no malloc or free. */
#include "completion/bridge.h"
#include <stddef.h>
#include <stdlib.h>
#include <string.h>

/* target.rs records 16 bytes as the alignment of every heap block this unit
 * hands out. C guarantees that through `max_align_t`, the alignment malloc and
 * realloc return storage suitable for, wherever it is 16: x86-64 and aarch64
 * Linux and x86-64 Windows. Apple's `max_align_t` is a long double, 8 bytes on
 * arm64, which understates its allocator: libmalloc returns 16-byte-aligned
 * blocks on both architectures, as clang's default `new` alignment of 16 on
 * Darwin assumes, so there only the 64-bit pointer is asserted. The alignment
 * tests check the pointers the allocator returns on the hosts that run them. A
 * host the assertion rejects fails to build here instead of returning an
 * under-aligned element block. */
#if defined(__APPLE__)
_Static_assert(sizeof(void *) == 8,
               "the heap blocks must meet the 16-byte alignment target.rs records");
#else
_Static_assert(_Alignof(max_align_t) >= 16,
               "the heap blocks must meet the 16-byte alignment target.rs records");
#endif

/* Array stores one 8-byte word, Slots two, and Ring and Paged three. Paged's
 * empty directory begins after those three words and has no entries. target.rs
 * qualifies runtime elements against every supported allocator's 16-byte
 * alignment, so the largest padded element offset is 32 bytes. The 64-byte,
 * 64-aligned object also covers rounding that header to any power-of-two
 * element alignment up to 64, with the element base at most one past its end.
 * STOR-1: capacity zero shares these immutable zeros and occupies no heap. */
_Alignas(64) const unsigned char wf__empty_window[64] = {0};

/* Growth of a block smaller than this copies into a fresh block instead of
 * calling realloc. glibc serves small malloc and free from a per-thread cache
 * without a lock, while realloc always takes the owning arena's lock, so
 * parallel workers growing many small buffers queue on that lock. */
#ifndef WF_HEAP_RETAKE_COPY_BELOW
#define WF_HEAP_RETAKE_COPY_BELOW UINT64_C(1024)
#endif

void *wf__heap_take(uint64_t bytes) {
    void *block = malloc((size_t)bytes);
    if (block != NULL) {
        wf__heap_change((int64_t)bytes);
    }
    return block;
}

/* Like take, refusal returns NULL to the emitted heap-record abort edge.
 * Both extents have been checked against the target's signed size domain.
 * Emitted growth calls this only for a positive new capacity, so growth from
 * the shared empty header always takes a writable block, even when a
 * zero-stride element leaves the byte extent unchanged. */
void *wf__heap_retake(void *block, uint64_t old_bytes, uint64_t new_bytes) {
    if (block == (const void *)&wf__empty_window) {
        void *fresh = wf__heap_take(new_bytes);
        if (fresh != NULL) {
            memcpy(fresh, block, (size_t)old_bytes);
        }
        return fresh;
    }
    if (old_bytes < WF_HEAP_RETAKE_COPY_BELOW) {
        void *fresh = wf__heap_take(new_bytes);
        if (fresh != NULL) {
            memcpy(fresh, block, (size_t)(old_bytes < new_bytes ? old_bytes : new_bytes));
            wf__heap_give(block, old_bytes);
        }
        return fresh;
    }
    void *resized = realloc(block, (size_t)new_bytes);
    if (resized != NULL) {
        wf__heap_change((int64_t)new_bytes - (int64_t)old_bytes);
    }
    return resized;
}

void wf__heap_give(void *block, uint64_t bytes) {
    if (block == (const void *)&wf__empty_window) {
        return;
    }
    if (block != NULL) {
        wf__heap_change(-(int64_t)bytes);
    }
    free(block);
}
