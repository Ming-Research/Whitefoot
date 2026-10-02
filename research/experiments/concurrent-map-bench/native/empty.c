/* Serves concurrent-map-bench: the empty control, the driver's own floor.
 * It stores nothing and answers every operation as if the key were there. */
#include <stdlib.h>

#include "cmap.h"

struct cm_map {
    char unused;
};

const char *CM(name)(void) { return "empty"; }
int CM(flags)(void) { return CM_NO_STORAGE; }
cm_map *CM(create)(uint64_t capacity) {
    (void)capacity;
    return calloc(1, sizeof(cm_map));
}
void CM(destroy)(cm_map *map) { free(map); }
void CM(enter)(cm_map *map) { (void)map; }
void CM(leave)(cm_map *map) { (void)map; }
int CM(get)(cm_map *map, uint64_t key, uint64_t *value) {
    (void)map;
    *value = key;
    return 1;
}
int CM(insert)(cm_map *map, uint64_t key, uint64_t value) {
    (void)map, (void)key, (void)value;
    return 1;
}
int CM(remove)(cm_map *map, uint64_t key) {
    (void)map, (void)key;
    return 1;
}
int CM(update)(cm_map *map, uint64_t key) {
    (void)map, (void)key;
    return 1;
}
