/* Emitted heap storage alone references this unit. Keep allocator calls out
 * of the unconditional runtime so a heap-free link needs no malloc or free. */
#include "completion/bridge.h"
#include <stddef.h>
#include <stdlib.h>
#include <string.h>

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
 * Both extents have been checked against the target's signed size domain. */
void *wf__heap_retake(void *block, uint64_t old_bytes, uint64_t new_bytes) {
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
    if (block != NULL) {
        wf__heap_change(-(int64_t)bytes);
    }
    free(block);
}
