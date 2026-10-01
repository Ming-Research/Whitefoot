/* Serves concurrent-map-bench: the runtime's concurrent map
 * (compiler/src/backend/concurrent_map.c) behind the driver's functions.
 * The runtime source is compiled into this file, so the driver measures the
 * runtime's own code with one call per operation, as the comparators get.
 * Built with WF_CMAP_LOCKED_READ it measures the locked read the runtime
 * keeps for measurement. */
#include "concurrent_map.c"

#include "cmap.h"

#if defined(WF_CMAP_LOCKED_READ)
const char *CM(name)(void) { return "wf-index-locked"; }
#else
const char *CM(name)(void) { return "wf-index"; }
#endif
int CM(flags)(void) { return 0; }
cm_map *CM(create)(uint64_t capacity) { return (cm_map *)wf_cmap_create(capacity); }
void CM(destroy)(cm_map *map) { wf_cmap_destroy((wf_cmap *)map); }
void CM(enter)(cm_map *map) { wf_cmap_enter((wf_cmap *)map); }
void CM(leave)(cm_map *map) { wf_cmap_leave((wf_cmap *)map); }
int CM(get)(cm_map *map, uint64_t key, uint64_t *value) { return wf_cmap_get((wf_cmap *)map, key, value); }
int CM(insert)(cm_map *map, uint64_t key, uint64_t value) {
    return wf_cmap_insert((wf_cmap *)map, key, value);
}
int CM(remove)(cm_map *map, uint64_t key) { return wf_cmap_remove((wf_cmap *)map, key); }

static void add_one(uint64_t *value, void *env) {
    (void)env;
    *value += 1;
}

int CM(update)(cm_map *map, uint64_t key) { return wf_cmap_update((wf_cmap *)map, key, add_one, NULL); }
