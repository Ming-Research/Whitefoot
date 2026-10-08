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

void wf__heap_give(void *block, uint64_t bytes) {
    if (block != NULL) {
        wf__heap_change(-(int64_t)bytes);
    }
    free(block);
}
