/* Serves concurrent-map-bench: the functions every native implementation
 * provides to the one driver, named with the implementation's prefix.
 *
 * Build an implementation and the driver with -DCM_PREFIX=<prefix>; the
 * driver then calls <prefix>_get and the rest directly, without inlining
 * across the boundary. Keys are never zero and never reach 2^62.
 */
#ifndef CMAP_H
#define CMAP_H

#include <stdint.h>

#ifndef CM_PREFIX
#error "CM_PREFIX names the implementation"
#endif

#define CM_CAT2(a, b) a##_##b
#define CM_CAT(a, b) CM_CAT2(a, b)
#define CM(x) CM_CAT(CM_PREFIX, x)

/* Flags an implementation reports, printed in every row. */
enum {
    CM_OPTIMISTIC_UPDATE = 1, /* update may run its function more than once */
    CM_ATOMIC_ADD_UPDATE = 2, /* update is an atomic addition, not a section */
    CM_NO_STORAGE = 4,        /* the empty control: nothing is stored */
    CM_ONE_THREAD = 8,        /* a floor: run at one thread only */
};

#ifdef __cplusplus
extern "C" {
#endif

typedef struct cm_map cm_map;

const char *CM(name)(void);
int CM(flags)(void);
cm_map *CM(create)(uint64_t capacity);
void CM(destroy)(cm_map *map);
/* A thread calls enter before its first operation on map and leave after
 * its last. */
void CM(enter)(cm_map *map);
void CM(leave)(cm_map *map);
/* 1 and the value when key is present, else 0. */
int CM(get)(cm_map *map, uint64_t key, uint64_t *value);
/* Inserts or replaces; 1 when key was absent. */
int CM(insert)(cm_map *map, uint64_t key, uint64_t value);
/* 1 when key was present and is now removed. */
int CM(remove)(cm_map *map, uint64_t key);
/* Adds one to key's value with exclusive access to the entry; 1 when key was
 * present. */
int CM(update)(cm_map *map, uint64_t key);

#ifdef __cplusplus
}
#endif

/* The hash every native implementation uses: one multiplication by the
 * 64-bit golden ratio. */
#define CM_GOLDEN 0x9E3779B97F4A7C15ull

#endif
