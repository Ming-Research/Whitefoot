// Serves concurrent-map-bench: growt (Maier, Sanders and Dementiev,
// "Concurrent hash tables: fast and general(?)!", TOPC 2019), linear probing
// with atomic cells and asynchronous migration on growth (uaGrow, the table
// its configuration selects for growable tables with deletion). Each thread
// uses its own handle. Its update is a compare-and-swap loop on the cell, so
// it is flagged optimistic; removed keys stay as tombstones until the next
// migration.
#include "allocator/alignedallocator.hpp"
#include "data-structures/hash_table_mods.hpp"
#include "data-structures/table_config.hpp"

#include "cmap.h"

namespace {
struct Golden {
    [[maybe_unused]] static constexpr std::string_view name = "golden";
    [[maybe_unused]] static constexpr size_t significant_digits = 64;
    uint64_t operator()(uint64_t k) const { return k * CM_GOLDEN; }
};
struct Increment {
    using mapped_type = uint64_t;
    uint64_t operator()(uint64_t &value, const uint64_t &by) const { return value += by; }
};
using Table = growt::table_config<uint64_t, uint64_t, Golden, growt::AlignedAllocator<>, hmod::growable,
                                  hmod::deletion>::table_type;
using Handle = Table::handle_type;
thread_local Handle *handle;
} // namespace

struct cm_map {
    Table table;
    explicit cm_map(uint64_t capacity) : table(capacity ? capacity : 4096) {}
};

extern "C" {
const char *CM(name)(void) { return "growt"; }
int CM(flags)(void) { return CM_OPTIMISTIC_UPDATE; }
cm_map *CM(create)(uint64_t capacity) { return new cm_map(capacity); }
void CM(destroy)(cm_map *map) { delete map; }
void CM(enter)(cm_map *map) { handle = new Handle(map->table.get_handle()); }
void CM(leave)(cm_map *) {
    delete handle;
    handle = nullptr;
}
int CM(get)(cm_map *, uint64_t key, uint64_t *value) {
    auto it = handle->find(key);
    if (it == handle->end())
        return 0;
    *value = (*it).second;
    return 1;
}
int CM(insert)(cm_map *, uint64_t key, uint64_t value) { return handle->insert_or_assign(key, value).second; }
int CM(remove)(cm_map *, uint64_t key) { return (int)handle->erase(key); }
int CM(update)(cm_map *, uint64_t key) {
    return handle->update(key, Increment{}, uint64_t{1}).second;
}
}
