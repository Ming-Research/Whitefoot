#include <array>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <utility>
#if defined(ACCOUNT_ONLY)
#include "../ecosystem-allocator.hpp"
#endif

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

void consume(std::uint64_t &digest, std::uint64_t value) {
    digest = digest * UINT64_C(131) + value;
}

void consume(std::uint64_t &digest, Record value) {
    for (auto word : value.words) consume(digest, word);
}

#if defined(ACCOUNT_ONLY)
template<class T> using Deque = std::deque<T, EcosystemAllocator<T>>;
#else
template<class T> using Deque = std::deque<T>;
#endif

template<class T>
std::uint64_t trace(std::size_t count, std::uint64_t rounds, std::uint64_t seed,
                    std::uint64_t path) {
    if (path == 3) {
        std::uint64_t checksum = seed;
        for (std::uint64_t round = 0; round < rounds; ++round) {
            const auto base = seed + round;
            auto digest = base;
            Deque<T> values;
            for (std::size_t index = 0; index < count; ++index)
                values.emplace_back(base + index);
            for (std::size_t index = count; index < 2 * count + 1; ++index)
                values.emplace_back(base + index);
            while (!values.empty()) {
                T value = std::move(values.front());
                values.pop_front();
                consume(digest, std::move(value));
            }
            checksum = checksum * UINT64_C(257) + digest;
        }
        return checksum;
    }
    // std::deque has no reserve operation. Its block allocation and release
    // policy remains part of this standard-container comparison.
    Deque<T> values;
    std::uint64_t digest = seed;
    for (std::size_t index = 0; index < count; ++index)
        values.emplace_back(seed + index);
    for (std::uint64_t round = 0; round < rounds; ++round) {
        const auto base = seed + (round + 1) * count;
        if (path == 1) {
            for (std::size_t index = 0; index < count; ++index) {
                T value = std::move(values.back());
                values.pop_back();
                consume(digest, std::move(value));
                values.emplace_front(base + index);
            }
        } else {
            for (std::size_t index = 0; index < count; ++index) {
                T value = std::move(values.front());
                values.pop_front();
                consume(digest, std::move(value));
                values.emplace_back(base + index);
            }
        }
    }
    while (!values.empty()) {
        T value = std::move(values.front());
        values.pop_front();
        consume(digest, std::move(value));
    }
    return digest;
}
} // namespace

extern "C" std::uint64_t cpp_deque_word_trace(std::uint64_t count, std::uint64_t rounds,
                                              std::uint64_t seed, std::uint64_t path) {
    return trace<std::uint64_t>(static_cast<std::size_t>(count), rounds, seed, path);
}

extern "C" std::uint64_t cpp_deque_record_trace(std::uint64_t count, std::uint64_t rounds,
                                                std::uint64_t seed, std::uint64_t path) {
    return trace<Record>(static_cast<std::size_t>(count), rounds, seed, path);
}
