/* Serves concurrent-map-bench: the runtime's concurrent map
 * (compiler/src/backend/concurrent_map.c) behind the driver's functions.
 * The runtime source is compiled into this file, so the driver measures the
 * runtime's own code with one call per operation, as the comparators get.
 * Built with WF_CMAP_LOCKED_READ it measures the locked read the runtime
 * keeps for measurement. */
#define _GNU_SOURCE
#include <sched.h>
#include <stdlib.h>

/* The host the runtime supplies the map, here from the C library. */
#define WF_CMAP_TAKE(bytes) aligned_alloc(16, ((size_t)(bytes) + 15) / 16 * 16)
#define WF_CMAP_GIVE(block, bytes) free(block)
#define WF_CMAP_YIELD() sched_yield()
#define WF_CMAP_EXHAUSTED() abort()
#include "concurrent_map.c"

#include "cmap.h"

/* The driver gives each thread one map at a time, so a thread keeps its
 * user of that map here. */
static _Thread_local wf_cmap_user *user;

#if defined(WF_CMAP_LOCKED_READ)
const char *CM(name)(void) { return "wf-index-locked"; }
#else
const char *CM(name)(void) { return "wf-index"; }
#endif
int CM(flags)(void) { return 0; }
cm_map *CM(create)(uint64_t capacity) { return (cm_map *)wf_cmap_create(capacity); }
void CM(destroy)(cm_map *map) { wf_cmap_destroy((wf_cmap *)map); }
void CM(enter)(cm_map *map) {
    user = wf_cmap_enter((wf_cmap *)map);
    if (user == NULL)
        abort();
}
void CM(leave)(cm_map *map) {
    (void)map;
    wf_cmap_leave(user);
    user = NULL;
}
int CM(get)(cm_map *map, uint64_t key, uint64_t *value) {
    (void)map;
    return wf_cmap_get(user, key, value);
}
int CM(insert)(cm_map *map, uint64_t key, uint64_t value) {
    (void)map;
    return wf_cmap_insert(user, key, value);
}
int CM(remove)(cm_map *map, uint64_t key) {
    (void)map;
    return wf_cmap_remove(user, key);
}

static void add_one(uint64_t *value, void *env) {
    (void)env;
    *value += 1;
}

int CM(update)(cm_map *map, uint64_t key) {
    (void)map;
    return wf_cmap_update(user, key, add_one, NULL);
}
