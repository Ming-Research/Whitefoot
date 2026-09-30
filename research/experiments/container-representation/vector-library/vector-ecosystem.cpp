#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <new>
#include <utility>
#include <vector>
#if defined(ACCOUNT_ONLY)
#include "../ecosystem-allocator.hpp"
#endif

struct ApiObservation {
    std::uint64_t length;
    std::uint64_t capacity;
    std::uint64_t checksum;
    std::uint64_t valid;
};

namespace {
struct Record {
    std::array<std::uint64_t, 32> words;
    explicit Record(std::uint64_t seed) {
        for (std::size_t index = 0; index < words.size(); ++index)
            words[index] = seed + index;
    }
    Record(const Record &) = delete;
    Record &operator=(const Record &) = delete;
    Record(Record &&) noexcept = default;
    Record &operator=(Record &&) noexcept = default;
};
static_assert(sizeof(Record) == 256);

void consume(std::uint64_t &digest, std::uint64_t value) {
    digest = digest * UINT64_C(131) + value;
}

void consume(std::uint64_t &digest, Record value) {
    for (auto word : value.words) consume(digest, word);
}

#if defined(ACCOUNT_ONLY)
template<class T> using Vector = std::vector<T, EcosystemAllocator<T>>;
#else
template<class T> using Vector = std::vector<T>;
#endif

// The C driver supplies max_align_t-aligned storage for three pointer words.
static_assert(sizeof(Vector<std::uint64_t>) <= 3 * sizeof(void *));
static_assert(sizeof(Vector<Record>) <= 3 * sizeof(void *));
static_assert(alignof(Vector<std::uint64_t>) <= alignof(std::max_align_t));
static_assert(alignof(Vector<Record>) <= alignof(std::max_align_t));

template<class T>
void api_prepare(std::uint64_t capacity, void *storage) {
    auto *values = ::new (storage) Vector<T>;
    values->reserve(static_cast<std::size_t>(capacity));
}

template<class T>
std::uint64_t api_append_batch(void *storage, std::uint64_t count,
                               std::uint64_t seed) {
    auto &values = *static_cast<Vector<T> *>(storage);
    for (std::uint64_t index = 0; index < count; ++index)
        values.emplace_back(seed + index);
    return values.size();
}

bool api_inspect(std::uint64_t value, std::uint64_t expected, std::uint64_t &digest) {
    consume(digest, value);
    return value == expected;
}

bool api_inspect(const Record &value, std::uint64_t expected, std::uint64_t &digest) {
    bool valid = true;
    for (std::size_t word = 0; word < value.words.size(); ++word) {
        consume(digest, value.words[word]);
        valid &= value.words[word] == expected + word;
    }
    return valid;
}

template<class T>
std::uint64_t api_inspect_reset(void *storage, std::uint64_t count,
                                std::uint64_t seed, ApiObservation *observation) {
    auto &values = *static_cast<Vector<T> *>(storage);
    std::uint64_t digest = seed;
    bool valid = values.size() == count;
    for (std::size_t index = 0; index < values.size(); ++index)
        valid &= api_inspect(values[index], seed + index, digest);
    *observation = {values.size(), values.capacity(), digest,
                    static_cast<std::uint64_t>(valid)};
    values.clear();
    return observation->valid;
}

template<class T>
std::uint8_t api_destroy(void *storage) {
    std::destroy_at(static_cast<Vector<T> *>(storage));
    return 0;
}

template<class T>
void consume_suffix(Vector<T> &values, std::size_t retained, std::uint64_t &digest) {
    // vector::erase does not return its erased owners. Move each result to the
    // consumer first, then erase the suffix once, retaining capacity.
    for (std::size_t index = retained; index < values.size(); ++index)
        consume(digest, std::move(values[index]));
    values.erase(values.begin() + static_cast<std::ptrdiff_t>(retained), values.end());
}

template<class T>
std::uint64_t work(Vector<T> &values, std::size_t count, std::uint64_t seed) {
    std::uint64_t digest = seed;
    for (std::size_t index = 0; index < count; ++index)
        values.emplace_back(seed + index);
    const auto middle = count / 2;
    values.emplace(values.begin() + static_cast<std::ptrdiff_t>(middle),
                   seed ^ UINT64_C(11400714819323198485));
    T removed = std::move(values[middle]);
    values.erase(values.begin() + static_cast<std::ptrdiff_t>(middle));
    consume(digest, std::move(removed));
    if (count != 0) {
        T first = std::move(values.front());
        if (values.size() > 1) values.front() = std::move(values.back());
        values.pop_back();
        consume(digest, std::move(first));
    }
    consume_suffix(values, values.size() / 2, digest);
    consume_suffix(values, 0, digest);
    return digest;
}

template<class T>
std::uint64_t trace(std::size_t count, std::uint64_t rounds, std::uint64_t seed,
                    std::uint64_t path) {
    std::uint64_t checksum = seed;
    if (path >= 3) {
        const auto removed = static_cast<std::size_t>(std::min(path - 3, static_cast<std::uint64_t>(count)));
        const auto retained = count - removed;
        Vector<T> values;
        values.reserve(count + 1);
        for (std::size_t index = 0; index < retained; ++index)
            values.emplace_back(seed + index);
        for (std::uint64_t round = 0; round < rounds; ++round) {
            const auto base = seed + round;
            auto digest = base;
            for (std::size_t index = retained; index < count; ++index)
                values.emplace_back(base + index);
            consume_suffix(values, retained, digest);
            checksum = checksum * UINT64_C(257) + digest;
        }
        auto digest = seed;
        consume_suffix(values, 0, digest);
        return checksum * UINT64_C(257) + digest;
    }
    if (path == 2) {
        Vector<T> values;
        values.reserve(count + 1);
        for (std::uint64_t round = 0; round < rounds; ++round)
            checksum = checksum * UINT64_C(257) + work(values, count, seed + round);
    } else {
        for (std::uint64_t round = 0; round < rounds; ++round) {
            Vector<T> values;
            if (path == 0) values.reserve(count + 1);
            checksum = checksum * UINT64_C(257) + work(values, count, seed + round);
        }
    }
    return checksum;
}
} // namespace

extern "C" std::uint64_t cpp_vector_word_trace(std::uint64_t count, std::uint64_t rounds,
                                               std::uint64_t seed, std::uint64_t path) {
    return trace<std::uint64_t>(static_cast<std::size_t>(count), rounds, seed, path);
}

extern "C" std::uint64_t cpp_vector_record_trace(std::uint64_t count, std::uint64_t rounds,
                                                 std::uint64_t seed, std::uint64_t path) {
    return trace<Record>(static_cast<std::size_t>(count), rounds, seed, path);
}

extern "C" void cpp_vector_api_word_prepare(std::uint64_t capacity, void *storage) {
    api_prepare<std::uint64_t>(capacity, storage);
}

extern "C" std::uint64_t cpp_vector_api_word_append_batch(void *storage,
                                                          std::uint64_t count,
                                                          std::uint64_t seed) {
    return api_append_batch<std::uint64_t>(storage, count, seed);
}

extern "C" std::uint64_t cpp_vector_api_word_inspect_reset(void *storage,
                                                           std::uint64_t count,
                                                           std::uint64_t seed,
                                                           ApiObservation *observation) {
    return api_inspect_reset<std::uint64_t>(storage, count, seed, observation);
}

extern "C" std::uint8_t cpp_vector_api_word_destroy(void *storage) {
    return api_destroy<std::uint64_t>(storage);
}

extern "C" void cpp_vector_api_record_prepare(std::uint64_t capacity, void *storage) {
    api_prepare<Record>(capacity, storage);
}

extern "C" std::uint64_t cpp_vector_api_record_append_batch(void *storage,
                                                            std::uint64_t count,
                                                            std::uint64_t seed) {
    return api_append_batch<Record>(storage, count, seed);
}

extern "C" std::uint64_t cpp_vector_api_record_inspect_reset(void *storage,
                                                             std::uint64_t count,
                                                             std::uint64_t seed,
                                                             ApiObservation *observation) {
    return api_inspect_reset<Record>(storage, count, seed, observation);
}

extern "C" std::uint8_t cpp_vector_api_record_destroy(void *storage) {
    return api_destroy<Record>(storage);
}
