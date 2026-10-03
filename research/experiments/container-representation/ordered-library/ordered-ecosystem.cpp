#include <absl/container/btree_map.h>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <memory>
#include <optional>
#include <type_traits>
#include <utility>

#if defined(ACCOUNT_ONLY)
#include "../ecosystem-allocator.hpp"
template <class T> using NativeAllocator = EcosystemAllocator<T>;
#else
template <class T> using NativeAllocator = std::allocator<T>;
#endif

namespace {
constexpr std::size_t ceiling = 8192;
// Match the movable inline owner without adding per-element heap storage.
struct Record {
    std::array<std::uint64_t, 32> words;
    Record() = default;
    Record(const Record &) = delete;
    Record &operator=(const Record &) = delete;
    Record(Record &&) noexcept = default;
    Record &operator=(Record &&) noexcept = default;
    constexpr std::size_t size() const { return words.size(); }
    auto begin() { return words.begin(); }
    auto end() { return words.end(); }
    auto begin() const { return words.begin(); }
    auto end() const { return words.end(); }
    auto &operator[](std::size_t index) { return words[index]; }
    bool operator==(const Record &) const = default;
};
static_assert(sizeof(Record) == 256);
template <class V> using StandardMap = std::map<std::uint64_t, V,
    std::less<std::uint64_t>, NativeAllocator<std::pair<const std::uint64_t, V>>>;
template <class V> using AbseilMap = absl::btree_map<std::uint64_t, V,
    std::less<std::uint64_t>, NativeAllocator<std::pair<const std::uint64_t, V>>>;

void require(bool condition, const char *message) {
    if (!condition) {
        std::fprintf(stderr, "ordered ecosystem C++: %s\n", message);
        std::abort();
    }
}
std::uint64_t mix(std::uint64_t value) {
    value = (value ^ (value >> 30)) * UINT64_C(13787848793156543929);
    value = (value ^ (value >> 27)) * UINT64_C(10723151780598845931);
    return value ^ (value >> 31);
}
struct Digest {
    std::uint64_t ordered, sum = 0, parity = 0, consumed = 0;
    void visit(std::uint64_t value) { ordered = ordered * 131 + value; }
    void consume(std::uint64_t value) { sum += value; parity ^= mix(value); ++consumed; }
    std::uint64_t finish() const {
        return mix(ordered) ^ mix(sum) ^ parity ^ consumed * UINT64_C(11400714819323198485);
    }
};
template <class V> V make_value(std::uint64_t seed) {
    if constexpr (std::is_same_v<V, std::uint64_t>) return seed;
    else {
        V value;
        for (std::size_t i = 0; i < value.size(); ++i) value[i] = seed + i;
        return value;
    }
}
template <class V> std::uint64_t content(std::uint64_t key, const V &value) {
    if constexpr (std::is_same_v<V, std::uint64_t>) return key * 131 + value;
    else {
        for (auto word : value) key = key * 131 + word;
        return key;
    }
}
template <class V> void edit_value(V &value) {
    if constexpr (std::is_same_v<V, std::uint64_t>) ++value;
    else for (auto &word : value) ++word;
}
template <class V> struct Put {
    unsigned status;
    std::optional<V> returned;
};

// The application returns the old value, or the offered value at its ceiling.
// Equal u64 keys are identical; this does not replace distinct equal key owners.
template <class Map> Put<typename Map::mapped_type> put(
    Map &map, std::uint64_t key, typename Map::mapped_type value,
    std::size_t limit = ceiling) {
    if (map.size() >= limit) {
        auto found = map.find(key);
        if (found == map.end()) return {2, std::move(value)};
        return {1, std::exchange(found->second, std::move(value))};
    }
    auto [position, inserted] = map.try_emplace(key, std::move(value));
    if (inserted) return {0, std::nullopt};
    return {1, std::exchange(position->second, std::move(value))};
}
template <class V> void consume_put(Put<V> result, std::uint64_t key, Digest &digest) {
    digest.visit(result.status);
    if (result.returned) digest.consume(content(key, *result.returned));
}
template <class Map> auto remove(Map &map, std::uint64_t key)
    -> std::optional<std::pair<std::uint64_t, typename Map::mapped_type>> {
    auto found = map.find(key);
    if (found == map.end()) return std::nullopt;
    auto result = std::make_pair(found->first, std::move(found->second));
    map.erase(found);
    return result;
}
template <class Map> void range(const Map &map, std::uint64_t lower,
                               std::uint64_t upper, Digest &digest) {
    if (lower >= upper) return;
    for (auto it = map.lower_bound(lower); it != map.end() && it->first < upper; ++it)
        digest.visit(content(it->first, it->second));
}
template <class Map> void consume_map(Map &map, Digest &digest) {
    // No physical destruction order is required. Consume mapped owners once,
    // then use the library's native bulk destruction instead of repeated erase.
    for (auto &entry : map) {
        auto value = std::move(entry.second);
        digest.consume(content(entry.first, value));
    }
    map.clear();
}
template <template <class> class Container, class V>
std::uint64_t trace(std::uint64_t count, std::uint64_t rounds,
                    std::uint64_t seed, std::uint64_t path) {
    Container<V> map;
    Digest digest{seed};
    for (std::uint64_t i = 0; i < count; ++i) {
        auto key = 2 * ((73 * i) % count) + 1;
        consume_put(put(map, key, make_value<V>(seed + i)), key, digest);
    }
    digest.visit(map.size());
    for (std::uint64_t round = 0; round < rounds; ++round) {
        for (std::uint64_t i = 0; i < count; ++i) {
            auto key = 2 * ((73 * i) % count) + 1;
            if (path == 1) {
                auto hit = map.find(key);
                if (hit != map.end()) digest.visit(content(hit->first, hit->second));
                digest.visit(hit != map.end());
                auto miss = map.find(key - 1);
                if (miss != map.end()) digest.visit(content(miss->first, miss->second));
                digest.visit(miss != map.end());
            } else if (path == 2) {
                auto next = seed + count + (round * count + i) * 2;
                consume_put(put(map, key, make_value<V>(next)), key, digest);
                auto found = map.find(key);
                if (found != map.end()) {
                    edit_value(found->second);
                    digest.visit(content(found->first, found->second));
                }
                digest.visit(found != map.end());
                auto taken = remove(map, key);
                digest.visit(taken.has_value());
                if (taken) digest.consume(content(taken->first, taken->second));
                consume_put(put(map, key, make_value<V>(next + 1)), key, digest);
            } else if (path == 3) {
                range(map, key - 1, key + 31, digest);
            } else if (path == 4) {
                consume_put(put(map, key, make_value<V>(seed + count + round * count + i)), key, digest);
            }
        }
    }
    digest.visit(map.size());
    for (const auto &entry : map) digest.visit(content(entry.first, entry.second));
    consume_map(map, digest);
    return digest.finish();
}
template <template <class> class Container, class V> void audit_values() {
    Container<V> map;
    for (std::uint64_t i = 0; i < ceiling; ++i) {
        auto key = 2 * ((73 * i) % ceiling) + 1;
        require(put(map, key, make_value<V>(key)).status == 0, "audit insertion");
    }
    auto replaced = put(map, 1, make_value<V>(42));
    require(replaced.status == 1 && *replaced.returned == make_value<V>(1), "full replacement owner");
    auto refused = put(map, 1000001, make_value<V>(37));
    require(refused.status == 2 && *refused.returned == make_value<V>(37), "full refusal owner");
    require(map.size() == ceiling, "logical ceiling");
    require(map.find(1000001) == map.end(), "refusal preserves membership");
    Digest digest{0};
    range(map, 3, 3, digest); range(map, 9, 3, digest);
    require(digest.ordered == 0, "empty and inverted ranges");
    for (std::uint64_t i = 0; i < ceiling; ++i) {
        auto rank = i % 2 ? ceiling - 1 - i / 2 : i / 2;
        auto key = 2 * rank + 1;
        auto taken = remove(map, key);
        require(taken && taken->first == key && taken->second == make_value<V>(key == 1 ? 42 : key), "removed payload");
        require(!remove(map, key), "absent removal");
    }
    require(map.empty(), "complete removal");
    require(put(map, 7, make_value<V>(99)).status == 0, "empty reuse");
    consume_map(map, digest);
}
struct Owner {
    unsigned id;
    std::array<unsigned, 6> &released;
    ~Owner() { require(released[id]++ == 0, "owner released once"); }
};
template <template <class> class Container> void audit_owners() {
    std::array<unsigned, 6> released{};
    {
        Container<std::unique_ptr<Owner>> map;
        auto first = std::unique_ptr<Owner>(new Owner{0, released});
        require(put(map, 3, std::move(first), 2).status == 0, "owned insertion");
        auto old = put(map, 3, std::unique_ptr<Owner>(new Owner{1, released}), 2);
        require(old.status == 1 && (*old.returned)->id == 0, "owned replacement");
        old.returned.reset();
        require(put(map, 5, std::unique_ptr<Owner>(new Owner{2, released}), 2).status == 0, "owned fill");
        auto at_ceiling = put(map, 3, std::unique_ptr<Owner>(new Owner{3, released}), 2);
        require(at_ceiling.status == 1 && (*at_ceiling.returned)->id == 1, "owned replacement at ceiling");
        at_ceiling.returned.reset();
        auto full = put(map, 7, std::unique_ptr<Owner>(new Owner{4, released}), 2);
        require(full.status == 2 && (*full.returned)->id == 4, "owned refusal");
        full.returned.reset();
        auto removed = remove(map, 3);
        require(removed && removed->second->id == 3, "owned removal");
        removed.reset();
        for (auto &entry : map) { auto owner = std::move(entry.second); }
        map.clear();
        require(put(map, 7, std::unique_ptr<Owner>(new Owner{5, released}), 2).status == 0, "owned reuse");
        map.clear();
    }
    for (auto count : released) require(count == 1, "complete owner ledger");
}
template <template <class> class Container> void audit() {
    audit_values<Container, std::uint64_t>();
    audit_values<Container, Record>();
    audit_owners<Container>();
}
} // namespace

extern "C" std::uint64_t cpp_ordered_word_trace(std::uint64_t n, std::uint64_t r, std::uint64_t s, std::uint64_t p) noexcept { return trace<StandardMap, std::uint64_t>(n, r, s, p); }
extern "C" std::uint64_t cpp_ordered_record_trace(std::uint64_t n, std::uint64_t r, std::uint64_t s, std::uint64_t p) noexcept { return trace<StandardMap, Record>(n, r, s, p); }
extern "C" std::uint64_t absl_ordered_word_trace(std::uint64_t n, std::uint64_t r, std::uint64_t s, std::uint64_t p) noexcept { return trace<AbseilMap, std::uint64_t>(n, r, s, p); }
extern "C" std::uint64_t absl_ordered_record_trace(std::uint64_t n, std::uint64_t r, std::uint64_t s, std::uint64_t p) noexcept { return trace<AbseilMap, Record>(n, r, s, p); }
extern "C" void cpp_ordered_audit() noexcept { audit<StandardMap>(); }
extern "C" void absl_ordered_audit() noexcept { audit<AbseilMap>(); }
