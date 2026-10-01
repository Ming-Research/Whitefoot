/* The runtime's concurrent map: a hash index from 64-bit keys to 64-bit
 * values that many threads read and change at once. A change to a key runs
 * once with that key's entry held exclusively; a read takes no lock and
 * copies the value out. Its design and the measurements that chose it are in
 * research/investigations/concurrent-map/DESIGN.md.
 *
 * Keys lie in [1, 2^62 - 2]. Every operation goes through a user, which one
 * thread holds at a time from wf_cmap_enter to wf_cmap_leave.
 */
#ifndef WF_CONCURRENT_MAP_H
#define WF_CONCURRENT_MAP_H

#include <stdint.h>

typedef struct wf_cmap wf_cmap;
typedef struct wf_cmap_user wf_cmap_user;

/* Users a map can have at once. */
#define WF_CMAP_MAX_USERS 256

/* A map sized for capacity keys, or a small default when capacity is zero;
 * NULL when memory is short. */
wf_cmap *wf_cmap_create(uint64_t capacity);
/* Frees the map; it has no users left. */
void wf_cmap_destroy(wf_cmap *map);
/* A user of map for the calling thread; NULL when map already has
 * WF_CMAP_MAX_USERS. */
wf_cmap_user *wf_cmap_enter(wf_cmap *map);
void wf_cmap_leave(wf_cmap_user *user);
/* 1 and the value when key is present, else 0. */
int wf_cmap_get(wf_cmap_user *user, uint64_t key, uint64_t *value);
/* Inserts or replaces; 1 when key was absent. */
int wf_cmap_insert(wf_cmap_user *user, uint64_t key, uint64_t value);
/* 1 when key was present and is now removed. */
int wf_cmap_remove(wf_cmap_user *user, uint64_t key);
/* Runs edit once on key's value with its entry held exclusively; 1 when key
 * was present, 0 without running edit otherwise. */
int wf_cmap_update(wf_cmap_user *user, uint64_t key, void (*edit)(uint64_t *value, void *env), void *env);

#endif
