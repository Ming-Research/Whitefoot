// Serves concurrent-map-bench: Boost's unordered_flat_map, the fast
// single-thread table. Built with -DFLAT_LOCKED it sits behind one mutex,
// the `mutex-flat` control that separates table speed from lock cost;
// without it, the `flat` floor at one thread.
#include <boost/unordered/unordered_flat_map.hpp>
#include <mutex>
#include <type_traits>

#include "cmap.h"

namespace {
struct Golden {
    using is_avalanching = std::true_type;
    std::size_t operator()(uint64_t k) const noexcept { return k * CM_GOLDEN; }
};
} // namespace

struct cm_map {
    boost::unordered_flat_map<uint64_t, uint64_t, Golden> table;
#ifdef FLAT_LOCKED
    std::mutex lock;
#endif
};

#ifdef FLAT_LOCKED
#define HOLD std::lock_guard<std::mutex> hold(map->lock)
#define FLAT_NAME "mutex-flat"
#define FLAT_FLAGS 0
#else
#define HOLD (void)0
#define FLAT_NAME "flat"
#define FLAT_FLAGS CM_ONE_THREAD
#endif

extern "C" {
const char *CM(name)(void) { return FLAT_NAME; }
int CM(flags)(void) { return FLAT_FLAGS; }
cm_map *CM(create)(uint64_t capacity) {
    auto *map = new cm_map;
    map->table.reserve(capacity);
    return map;
}
void CM(destroy)(cm_map *map) { delete map; }
void CM(enter)(cm_map *) {}
void CM(leave)(cm_map *) {}
int CM(get)(cm_map *map, uint64_t key, uint64_t *value) {
    HOLD;
    auto it = map->table.find(key);
    if (it == map->table.end())
        return 0;
    *value = it->second;
    return 1;
}
int CM(insert)(cm_map *map, uint64_t key, uint64_t value) {
    HOLD;
    return map->table.insert_or_assign(key, value).second;
}
int CM(remove)(cm_map *map, uint64_t key) {
    HOLD;
    return (int)map->table.erase(key);
}
int CM(update)(cm_map *map, uint64_t key) {
    HOLD;
    auto it = map->table.find(key);
    if (it == map->table.end())
        return 0;
    it->second++;
    return 1;
}
}
