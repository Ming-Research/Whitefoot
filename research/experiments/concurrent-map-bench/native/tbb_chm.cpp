// Serves concurrent-map-bench: Intel TBB's concurrent_hash_map, chained
// buckets with a reader-writer lock per element reached through accessors.
// An update holds the element's write lock and runs once.
#include <oneapi/tbb/concurrent_hash_map.h>

#include "cmap.h"

namespace {
struct Golden {
    std::size_t hash(uint64_t k) const { return k * CM_GOLDEN; }
    bool equal(uint64_t a, uint64_t b) const { return a == b; }
};
using Table = oneapi::tbb::concurrent_hash_map<uint64_t, uint64_t, Golden>;
} // namespace

struct cm_map {
    Table table;
    explicit cm_map(uint64_t capacity) : table(capacity) {}
};

extern "C" {
const char *CM(name)(void) { return "tbb-chm"; }
int CM(flags)(void) { return 0; }
cm_map *CM(create)(uint64_t capacity) { return new cm_map(capacity); }
void CM(destroy)(cm_map *map) { delete map; }
void CM(enter)(cm_map *) {}
void CM(leave)(cm_map *) {}
int CM(get)(cm_map *map, uint64_t key, uint64_t *value) {
    Table::const_accessor a;
    if (!map->table.find(a, key))
        return 0;
    *value = a->second;
    return 1;
}
int CM(insert)(cm_map *map, uint64_t key, uint64_t value) {
    Table::accessor a;
    bool fresh = map->table.insert(a, key);
    a->second = value;
    return fresh;
}
int CM(remove)(cm_map *map, uint64_t key) { return map->table.erase(key); }
int CM(update)(cm_map *map, uint64_t key) {
    Table::accessor a;
    if (!map->table.find(a, key))
        return 0;
    a->second++;
    return 1;
}
}
