// Serves concurrent-map-bench: the parallel-hashmap library's
// parallel_flat_hash_map with std::mutex (its `_m` alias), 2^4 flat Swiss
// tables each behind one mutex, chosen by the hash's high bits. An update
// runs once under its submap's mutex.
#include <parallel_hashmap/phmap.h>

#include "cmap.h"

namespace {
struct Golden {
    std::size_t operator()(uint64_t k) const noexcept { return k * CM_GOLDEN; }
};
using Table = phmap::parallel_flat_hash_map<uint64_t, uint64_t, Golden, phmap::priv::hash_default_eq<uint64_t>,
                                            phmap::priv::Allocator<phmap::priv::Pair<const uint64_t, uint64_t>>, 4,
                                            std::mutex>;
} // namespace

struct cm_map {
    Table table;
};

extern "C" {
const char *CM(name)(void) { return "phmap"; }
int CM(flags)(void) { return 0; }
cm_map *CM(create)(uint64_t capacity) {
    auto *map = new cm_map;
    map->table.reserve(capacity);
    return map;
}
void CM(destroy)(cm_map *map) { delete map; }
void CM(enter)(cm_map *) {}
void CM(leave)(cm_map *) {}
int CM(get)(cm_map *map, uint64_t key, uint64_t *value) {
    return map->table.if_contains(key, [value](const auto &x) { *value = x.second; });
}
int CM(insert)(cm_map *map, uint64_t key, uint64_t value) {
    return map->table.insert_or_assign(key, value).second;
}
int CM(remove)(cm_map *map, uint64_t key) { return (int)map->table.erase(key); }
int CM(update)(cm_map *map, uint64_t key) {
    return map->table.modify_if(key, [](auto &x) { x.second++; });
}
}
