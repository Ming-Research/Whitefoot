/* Emitted heap storage alone references this unit. Keep allocator calls out
 * of the unconditional runtime so a heap-free link needs no malloc or free. */
#include "completion/bridge.h"
#include <stddef.h>
#include <stdlib.h>

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
    void *resized = realloc(block, (size_t)new_bytes);
    if (resized != NULL) {
        /* Origin-tag pass: resize must charge the block's retained origin. */
        wf__heap_change((int64_t)new_bytes - (int64_t)old_bytes);
    }
    return resized;
}

void wf__heap_give(void *block, uint64_t bytes) {
    if (block != NULL) {
        /* Origin-tag pass: free must debit the block's retained origin. */
        wf__heap_change(-(int64_t)bytes);
    }
    free(block);
}
