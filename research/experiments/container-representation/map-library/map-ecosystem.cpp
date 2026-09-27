// Native container traces; the C driver independently checks every outcome.
#include <absl/container/flat_hash_map.h>
#include <array>
#include <cstdint>
#include <cstdlib>
#include <functional>
#include <memory>
#include <unordered_map>
#include <utility>

#ifdef ACCOUNT_ONLY
#include "../ecosystem-allocator.hpp"
template<class T> using NativeAllocator = EcosystemAllocator<T>;
extern "C" void wf_ecosystem_map_geometry(std::uint64_t, std::uint64_t,
    std::uint64_t, std::uint64_t, std::uint64_t, double, double);
#else
template<class T> using NativeAllocator = std::allocator<T>;
#endif

namespace {
using Word = std::uint64_t;
struct Record {
    std::array<Word, 32> words;
    Record() = default;
    Record(const Record&) = delete;
    Record& operator=(const Record&) = delete;
    Record(Record&&) noexcept = default;
    Record& operator=(Record&&) noexcept = default;
};
static_assert(sizeof(Record) == 256);
enum : Word { Hit, Miss, Replace, Churn, Grow, Rehash, Setup, Edit, Policy, ReserveCheck, ReserveOmitted };

Word mix(Word value) {
    value ^= value >> 30; value *= UINT64_C(0xbf58476d1ce4e5b9);
    value ^= value >> 27; value *= UINT64_C(0x94d049bb133111eb);
    return value ^ (value >> 31);
}
struct AlignedHash {
    Word salt;
    bool collide;
    std::size_t operator()(Word key) const { return collide ? 0 : mix(key ^ salt); }
};

template<class V> struct Payload;
template<> struct Payload<Word> {
    static Word make(Word seed) { return seed; }
    static Word identity(const Word& value) { return value; }
    static Word content(Word key, Word value) { return key * 131 + value; }
    static Word increment(Word& value) { return ++value; }
};
template<> struct Payload<Record> {
    static Record make(Word seed) {
        Record value;
        for (std::size_t index = 0; index < value.words.size(); ++index) value.words[index] = seed + index;
        return value;
    }
    static Word identity(const Record& value) { return value.words[0]; }
    static Word content(Word key, Record value) {
        for (Word word : value.words) key = key * 131 + word;
        return key;
    }
    static Word increment(Record& value) { return ++value.words[0]; }
};
struct Digest {
    Word ordered_value, sum = 0, parity = 0, count = 0;
    explicit Digest(Word seed) : ordered_value(seed) {}
    void ordered(Word value) { ordered_value = ordered_value * 131 + value; }
    void consume(Word value) { sum += value; parity ^= mix(value); ++count; }
    Word finish() const { return mix(ordered_value) ^ mix(sum) ^ parity
        ^ (count * UINT64_C(0x9e3779b97f4a7c15)); }
};
Word key_at(Word index) { return index * 2 + 1; }

template<class V, class Hash>
using StdMap = std::unordered_map<Word, V, Hash, std::equal_to<Word>,
                                NativeAllocator<std::pair<const Word, V>>>;
template<class V, class Hash>
using FlatMap = absl::flat_hash_map<Word, V, Hash, std::equal_to<Word>,
                                  NativeAllocator<std::pair<const Word, V>>>;

template<class Map>
void put(Map& map, Digest& digest, Word key, typename Map::mapped_type offered, Word ceiling) {
    using V = typename Map::mapped_type;
    if (map.size() == ceiling) {
        // Replacement succeeds at the application ceiling. Only this path
        // needs a prior lookup to keep a missing insertion from allocating.
        auto entry = map.find(key);
        if (entry == map.end()) {
            digest.ordered(2);
            digest.ordered(Payload<V>::content(key, std::move(offered)));
        } else {
            digest.ordered(1);
            V old = std::exchange(entry->second, std::move(offered));
            digest.ordered(Payload<V>::content(entry->first, std::move(old)));
        }
        return;
    }
    auto [entry, inserted] = map.try_emplace(key, std::move(offered));
    digest.ordered(inserted ? 0 : 1);
    if (!inserted) {
        // try_emplace leaves the offered value intact on an occupied key.
        // Consume the displaced value rather than discarding it via assignment.
        V old = std::exchange(entry->second, std::move(offered));
        digest.ordered(Payload<V>::content(entry->first, std::move(old)));
    }
}

template<class Map>
Word trace(Word capacity, Word count, Word rounds, Word seed, Word path,
           typename Map::hasher hash) {
    if (path > ReserveOmitted || path == Rehash) std::abort();
    using V = typename Map::mapped_type;
    Map map(0, hash);
    map.reserve(capacity);
    Digest digest(seed);
    const Word ceiling = path == Policy ? 3 : 16384;
    for (Word index = 0; index < count; ++index)
        put(map, digest, key_at(index), Payload<V>::make(seed + index), ceiling);
#ifdef ACCOUNT_ONLY
    if constexpr (requires { map.capacity(); }) {
        // Abseil reports physical element slots, including empty/deleted ones.
        // Its compatibility maximum load factor is not a usable-entry bound.
        wf_ecosystem_map_geometry(3, map.size(), UINT64_MAX, map.capacity(),
            UINT64_MAX, map.load_factor(), map.max_load_factor());
    } else {
        // Standard unordered_map exposes chaining buckets, not element slots.
        wf_ecosystem_map_geometry(2, map.size(), UINT64_MAX, UINT64_MAX,
            map.bucket_count(), map.load_factor(), map.max_load_factor());
    }
#endif
    if (path == Policy) {
        put(map, digest, key_at(0), Payload<V>::make(seed + 10), ceiling);
        put(map, digest, key_at(count), Payload<V>::make(seed + 20), ceiling);
    }
    for (Word round = 0; round < rounds; ++round) {
        if (path == Grow || path == ReserveCheck || path == ReserveOmitted) {
            Word target = capacity ? capacity * 2 : 1;
            if (path != ReserveOmitted) map.reserve(target);
            digest.ordered(1);
            if (path != Grow) {
                if constexpr (requires { map.capacity(); })
                    digest.ordered(map.capacity() >= target);
                else
                    digest.ordered(map.bucket_count() * map.max_load_factor() >= target);
            }
        }
        for (Word index = 0; index < count; ++index) {
            Word key = key_at(path == Miss ? count + index : index);
            if (path == Hit || path == Miss || path == Grow || path == ReserveCheck || path == ReserveOmitted) {
                auto found = map.find(key);
                digest.ordered(found != map.end());
                if (found != map.end()) digest.ordered(Payload<V>::identity(found->second));
            } else if (path == Replace) {
                put(map, digest, key, Payload<V>::make(seed + (round + 1) * count + index), ceiling);
            } else if (path == Churn) {
                auto removed = map.extract(key);
                digest.ordered(!removed.empty());
                if (!removed.empty())
                    digest.ordered(Payload<V>::content(removed.key(), std::move(removed.mapped())));
                // Release a detached node before inserting its replacement.
                removed = {};
                auto absent = map.find(key);
                digest.ordered(absent != map.end());
                if (absent != map.end()) digest.ordered(Payload<V>::identity(absent->second));
                put(map, digest, key, Payload<V>::make(seed + (round + 1) * count + index), ceiling);
            } else if (path == Edit) {
                auto found = map.find(key);
                digest.ordered(found != map.end());
                if (found != map.end()) digest.ordered(Payload<V>::increment(found->second));
            }
        }
    }
    for (auto& [key, value] : map) digest.consume(Payload<V>::content(key, std::move(value)));
    return digest.finish(); // map destruction reclaims all native backing/nodes.
}
} // namespace

#define DEFAULT_ENTRY(NAME, MAP, VALUE, HASH)                             \
extern "C" Word NAME(Word capacity, Word count, Word rounds, Word seed,    \
                      Word path, Word collide) {                         \
    if (collide != 0) std::abort();                                       \
    return trace<MAP<VALUE, HASH>>(capacity, count, rounds, seed, path, {}); \
}
#define ALIGNED_ENTRY(NAME, MAP, VALUE)                                   \
extern "C" Word NAME(Word capacity, Word count, Word rounds, Word seed,    \
                      Word path, Word collide) {                         \
    return trace<MAP<VALUE, AlignedHash>>(capacity, count, rounds, seed, path, \
        AlignedHash{seed ^ UINT64_C(0x9e3779b97f4a7c15), collide != 0});     \
}

DEFAULT_ENTRY(eco_cpp_map_word_default, StdMap, Word, std::hash<Word>)
DEFAULT_ENTRY(eco_cpp_map_record_default, StdMap, Record, std::hash<Word>)
ALIGNED_ENTRY(eco_cpp_map_word_aligned, StdMap, Word)
ALIGNED_ENTRY(eco_cpp_map_record_aligned, StdMap, Record)
using FlatDefaultHash = absl::flat_hash_map<Word, Word>::hasher;
DEFAULT_ENTRY(eco_absl_map_word_default, FlatMap, Word, FlatDefaultHash)
DEFAULT_ENTRY(eco_absl_map_record_default, FlatMap, Record, FlatDefaultHash)
ALIGNED_ENTRY(eco_absl_map_word_aligned, FlatMap, Word)
ALIGNED_ENTRY(eco_absl_map_record_aligned, FlatMap, Record)

#ifdef ACCOUNT_ONLY
#define ECO_STRING_INNER(VALUE) #VALUE
#define ECO_STRING(VALUE) ECO_STRING_INNER(VALUE)
extern "C" const char* eco_cpp_map_library_identity() {
#if defined(_LIBCPP_VERSION)
    return "libc++-" ECO_STRING(_LIBCPP_VERSION);
#elif defined(__GLIBCXX__)
    return "libstdc++-" ECO_STRING(__GLIBCXX__);
#elif defined(_MSVC_STL_VERSION)
    return "msvc-stl-" ECO_STRING(_MSVC_STL_VERSION);
#else
    return "cpp-standard-library-unidentified";
#endif
}
extern "C" const char* eco_absl_map_library_identity() {
    return "abseil-" ECO_STRING(ABSL_LTS_RELEASE_VERSION) "." ECO_STRING(ABSL_LTS_RELEASE_PATCH_LEVEL);
}
#endif
