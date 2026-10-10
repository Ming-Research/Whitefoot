/* Emitted heap storage alone references this unit. Keep allocator calls out
 * of the unconditional runtime so a heap-free link needs no malloc or free. */
#include "completion/bridge.h"
#include <stddef.h>
#include <stdlib.h>

/* The only representation boundary for emitted allocations. Keeping origin
 * lookup here lets an arena implementation replace the prefix without
 * changing emitted calls or their requested-byte accounting. malloc/realloc
 * on the supported native targets preserve 16-byte alignment; the prefix
 * preserves it for the payload too, including an empty payload. */
typedef struct {
    _Alignas(16) unsigned origin;
} wf_heap_origin;
_Static_assert(sizeof(wf_heap_origin) == 16, "heap origin prefix is 16 bytes");

static wf_heap_origin *wf_heap_header(void *block) {
    return (wf_heap_origin *)block - 1;
}

static int wf_heap_extent(uint64_t bytes, size_t *extent) {
    if (bytes > INT64_MAX || bytes > SIZE_MAX - sizeof(wf_heap_origin)) return 0;
    *extent = (size_t)bytes + sizeof(wf_heap_origin);
    return 1;
}

void *wf__heap_take(uint64_t bytes) {
    size_t extent;
    if (!wf_heap_extent(bytes, &extent)) return NULL;
    wf_heap_origin *header = malloc(extent);
    if (header == NULL) return NULL;
    header->origin = wf__scope_current();
    wf__scope_retain(header->origin);
    wf__heap_change(header->origin, (int64_t)bytes);
    return header + 1;
}

/* Like take, refusal returns NULL to the emitted heap-record abort edge.
 * Both extents have been checked against the target's signed size domain. */
void *wf__heap_retake(void *block, uint64_t old_bytes, uint64_t new_bytes) {
    if (block == NULL) return wf__heap_take(new_bytes);
    size_t extent;
    if (!wf_heap_extent(new_bytes, &extent)) return NULL;
    wf_heap_origin *header = wf_heap_header(block);
    unsigned origin = header->origin;
    wf_heap_origin *resized = realloc(header, extent);
    if (resized != NULL) {
        wf__heap_change(origin, (int64_t)new_bytes - (int64_t)old_bytes);
        return resized + 1;
    }
    return NULL;
}

void wf__heap_give(void *block, uint64_t bytes) {
    if (block != NULL) {
        wf_heap_origin *header = wf_heap_header(block);
        unsigned origin = header->origin;
        free(header);
        wf__heap_change(origin, -(int64_t)bytes);
        wf__scope_release(origin);
    }
}
