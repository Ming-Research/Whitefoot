/* Emitted heap storage alone references this unit. Keep allocator calls out
 * of the unconditional runtime so a heap-free link needs no malloc or free. */
#if !defined(_WIN32) && !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200112L
#endif
#include "completion/bridge.h"
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#include <malloc.h>
#endif

/* Every runtime window header fits before its first element for each
 * supported power-of-two alignment, including an empty 4096-aligned value.
 * Identity, rather than extent, distinguishes this immutable shared header
 * from writable zero-stride allocations. STOR-1 charges it no heap bytes. */
_Alignas(WF_HEAP_MAX_ALIGNMENT) const unsigned char
    wf__empty_window[WF_HEAP_MAX_ALIGNMENT] = {0};

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

/* Native aligned allocators retain the requested extent: alignment padding
 * internal to the host allocator is not a program allocation [PRE-2]. */
void *wf__heap_take_aligned(uint64_t bytes, uint64_t alignment) {
    if (alignment == 0 || alignment > WF_HEAP_MAX_ALIGNMENT ||
        (alignment & (alignment - 1)) != 0 || bytes > INT64_MAX || bytes > SIZE_MAX)
        return NULL;
    if (alignment <= 8)
        return wf__heap_take(bytes);
    void *block = NULL;
#if defined(_WIN32)
    block = _aligned_malloc((size_t)bytes, (size_t)alignment);
#else
    if (posix_memalign(&block, (size_t)alignment, (size_t)bytes) != 0)
        return NULL;
#endif
    if (block != NULL)
        wf__heap_change((int64_t)bytes);
    return block;
}

void wf__heap_give_aligned(void *block, uint64_t bytes, uint64_t alignment) {
    if (block == (const void *)&wf__empty_window)
        return;
    if (alignment <= 8) {
        wf__heap_give(block, bytes);
        return;
    }
    if (block != NULL)
        wf__heap_change(-(int64_t)bytes);
#if defined(_WIN32)
    _aligned_free(block);
#else
    free(block);
#endif
}

/* realloc does not preserve an extended alignment. Leave the old block and
 * its accounting intact when allocation fails, as ordinary retake does. */
void *wf__heap_retake_aligned(void *block, uint64_t old_bytes,
                              uint64_t new_bytes, uint64_t alignment) {
    if (alignment <= 8)
        return wf__heap_retake(block, old_bytes, new_bytes);
    void *fresh = wf__heap_take_aligned(new_bytes, alignment);
    if (fresh != NULL) {
        memcpy(fresh, block, (size_t)(old_bytes < new_bytes ? old_bytes : new_bytes));
        wf__heap_give_aligned(block, old_bytes, alignment);
    }
    return fresh;
}
