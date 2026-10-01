// Serves concurrent-map-bench: libcuckoo's cuckoohash_map, bucketized cuckoo
// hashing with striped spinlocks. An update runs once under the locks of the
// key's two buckets.
#include <libcuckoo/cuckoohash_map.hh>

#include "cmap.h"

namespace {
struct Golden {
    std::size_t operator()(uint64_t k) const noexcept { return k * CM_GOLDEN; }
};
using Table = libcuckoo::cuckoohash_map<uint64_t, uint64_t, Golden>;
} // namespace

struct cm_map {
    Table table;
    explicit cm_map(uint64_t capacity) : table(capacity) {}
};

extern "C" {
const char *CM(name)(void) { return "libcuckoo"; }
int CM(flags)(void) { return 0; }
cm_map *CM(create)(uint64_t capacity) { return new cm_map(capacity); }
void CM(destroy)(cm_map *map) { delete map; }
void CM(enter)(cm_map *) {}
void CM(leave)(cm_map *) {}
int CM(get)(cm_map *map, uint64_t key, uint64_t *value) {
    return map->table.find(key, *value);
}
int CM(insert)(cm_map *map, uint64_t key, uint64_t value) {
    return map->table.insert_or_assign(key, value);
}
int CM(remove)(cm_map *map, uint64_t key) { return map->table.erase(key); }
int CM(update)(cm_map *map, uint64_t key) {
    return map->table.update_fn(key, [](uint64_t &v) { v++; });
}
}
