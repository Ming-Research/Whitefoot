/* The runtime's concurrent map: a hash index from 64-bit keys to 64-bit
 * values that many threads read and change at once. A change to a key runs
 * once with that key's entry held exclusively; a read takes no lock and
 * copies the value out. Its design and the measurements that chose it are in
 * research/investigations/concurrent-map/DESIGN.md.
 *
 * Keys are never zero. A thread calls wf_cmap_enter before its first
 * operation on a map and wf_cmap_leave after its last.
 */
#ifndef WF_CONCURRENT_MAP_H
#define WF_CONCURRENT_MAP_H

#include <stdint.h>

typedef struct wf_cmap wf_cmap;

/* A map sized for capacity keys, or a small default when capacity is zero;
 * NULL when memory is short. */
wf_cmap *wf_cmap_create(uint64_t capacity);
/* Frees the map; no thread may use it any more. */
void wf_cmap_destroy(wf_cmap *map);
void wf_cmap_enter(wf_cmap *map);
void wf_cmap_leave(wf_cmap *map);
/* 1 and the value when key is present, else 0. */
int wf_cmap_get(wf_cmap *map, uint64_t key, uint64_t *value);
/* Inserts or replaces; 1 when key was absent. */
int wf_cmap_insert(wf_cmap *map, uint64_t key, uint64_t value);
/* 1 when key was present and is now removed. */
int wf_cmap_remove(wf_cmap *map, uint64_t key);
/* Runs edit once on key's value with its entry held exclusively; 1 when key
 * was present, 0 without running edit otherwise. */
int wf_cmap_update(wf_cmap *map, uint64_t key, void (*edit)(uint64_t *value, void *env), void *env);

#endif
