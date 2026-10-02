// Serves concurrent-map-bench: Boost's concurrent_flat_map, open addressing
// with a lock per group of slots and lock-free group metadata. An update
// visits the element under its group's exclusive lock and runs once.
#include <boost/unordered/concurrent_flat_map.hpp>
#include <type_traits>

#include "cmap.h"

namespace {
struct Golden {
    using is_avalanching = std::true_type;
    std::size_t operator()(uint64_t k) const noexcept { return k * CM_GOLDEN; }
};
} // namespace

struct cm_map {
    boost::concurrent_flat_map<uint64_t, uint64_t, Golden> table;
};

extern "C" {
const char *CM(name)(void) { return "boost-cfm"; }
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
    return (int)map->table.cvisit(key, [value](const auto &x) { *value = x.second; });
}
int CM(insert)(cm_map *map, uint64_t key, uint64_t value) {
    return map->table.insert_or_assign(key, value);
}
int CM(remove)(cm_map *map, uint64_t key) { return (int)map->table.erase(key); }
int CM(update)(cm_map *map, uint64_t key) {
    return (int)map->table.visit(key, [](auto &x) { x.second++; });
}
}
