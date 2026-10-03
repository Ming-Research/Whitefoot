// Practical standard heap algorithms over an owning vector. priority_queue's
// const top()/void pop() cannot return an arbitrary move-only owner. The native
// algorithms below retain their own sift implementation. Retire with RESULTS.
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdlib>
#include <optional>
#include <utility>
#include <vector>

#if defined(ACCOUNT_ONLY)
#include "../ecosystem-allocator.hpp"
#endif

namespace {
constexpr std::size_t ceiling = 4096;
void require(bool condition) { if (!condition) std::abort(); }

struct Record {
    std::array<std::uint64_t, 32> words;
    explicit Record(std::uint64_t seed) {
        for (std::size_t i = 0; i < words.size(); ++i) words[i] = seed + i;
    }
    Record(const Record &) = delete;
    Record &operator=(const Record &) = delete;
    Record(Record &&) = default;
    Record &operator=(Record &&) = default;
};
static_assert(sizeof(Record) == 256);

template<class T> T make(std::uint64_t seed) { return T(seed); }
std::uint64_t key(const std::uint64_t &value) { return value; }
std::uint64_t key(const Record &value) { return value.words[0]; }
void accept(std::uint64_t &digest, std::uint64_t value) { digest = digest * 131 + value; }
void accept(std::uint64_t &digest, Record value) {
    for (auto word : value.words) accept(digest, word);
}
void verify(const std::uint64_t &value, std::uint64_t seed) { require(value == seed); }
void verify(const Record &value, std::uint64_t seed) {
    for (std::size_t i = 0; i < value.words.size(); ++i) require(value.words[i] == seed + i);
}
std::uint64_t next(std::uint64_t state) {
    return state * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
}
template<class T> struct Minimum {
    bool operator()(const T &left, const T &right) const { return key(left) > key(right); }
};
#if defined(ACCOUNT_ONLY)
template<class T> using Heap = std::vector<T, EcosystemAllocator<T>>;
#else
template<class T> using Heap = std::vector<T>;
#endif

template<class T> std::optional<T> push(Heap<T> &heap, T value) {
    if (heap.size() == ceiling) return std::optional<T>(std::in_place, std::move(value));
    heap.push_back(std::move(value));
    std::push_heap(heap.begin(), heap.end(), Minimum<T>{});
    return std::nullopt;
}
template<class T> T pop(Heap<T> &heap) {
    std::pop_heap(heap.begin(), heap.end(), Minimum<T>{});
    T value = std::move(heap.back());
    heap.pop_back();
    return value;
}
template<class T> T replace_top(Heap<T> &heap, T value) {
    // No standard fused replace-top algorithm: expose the native two-repair
    // cost instead of supplying a handwritten WF-shaped/private sift.
    std::pop_heap(heap.begin(), heap.end(), Minimum<T>{});
    T removed = std::move(heap.back());
    heap.back() = std::move(value);
    std::push_heap(heap.begin(), heap.end(), Minimum<T>{});
    return removed;
}
template<class T> void drain(Heap<T> &heap, std::uint64_t &digest) {
    while (!heap.empty()) accept(digest, pop(heap));
}
template<class T> std::uint64_t trace(std::uint64_t count, std::uint64_t rounds,
                                     std::uint64_t seed, std::uint64_t path) {
    require(count <= ceiling && path <= 4);
    std::uint64_t state = seed, digest = seed;
    Heap<T> heap;
    if (path != 2) heap.reserve(static_cast<std::size_t>(count));
    for (std::uint64_t i = 0; i < count; ++i) {
        state = next(state);
        if (path >= 3) heap.push_back(make<T>(state));
        else if (auto refused = push(heap, make<T>(state))) accept(digest, std::move(*refused));
    }
    if (path == 4) {
        // Same raw-prefix storage-only control; no heap is constructed here.
        while (!heap.empty()) {
            accept(digest, std::move(heap.back()));
            heap.pop_back();
        }
        return digest;
    }
    if (path == 3) std::make_heap(heap.begin(), heap.end(), Minimum<T>{});
    if (path < 2) {
        if (!heap.empty()) accept(digest, key(heap.front()));
        for (std::uint64_t round = 0; round < rounds; ++round)
            for (std::uint64_t i = 0; i < count; ++i) {
                state = next(state);
                T value = make<T>(state);
                if (heap.empty()) accept(digest, std::move(value));
                else if (path == 0) {
                    accept(digest, pop(heap));
                    if (auto refused = push(heap, std::move(value))) accept(digest, std::move(*refused));
                } else accept(digest, replace_top(heap, std::move(value)));
            }
    }
    drain(heap, digest);
    return digest;
}
template<class T> std::uint64_t refusal() {
    Heap<T> heap;
    heap.reserve(ceiling);
    std::uint64_t state = 101, digest = 101;
    for (std::size_t i = 0; i < ceiling; ++i) {
        state = next(state);
        auto result = push(heap, make<T>(state));
        require(!result);
    }
    state = next(state);
    auto result = push(heap, make<T>(state));
    require(result && heap.size() == ceiling);
    verify(*result, state);
    accept(digest, pop(heap));
    auto retried = push(heap, std::move(*result));
    require(!retried);
    drain(heap, digest);
    return digest;
}
} // namespace

extern "C" __attribute__((noinline)) std::uint64_t priority_cpp_word_trace(
    std::uint64_t count, std::uint64_t rounds, std::uint64_t seed, std::uint64_t path) {
    return trace<std::uint64_t>(count, rounds, seed, path);
}
extern "C" __attribute__((noinline)) std::uint64_t priority_cpp_record_trace(
    std::uint64_t count, std::uint64_t rounds, std::uint64_t seed, std::uint64_t path) {
    return trace<Record>(count, rounds, seed, path);
}
extern "C" std::uint64_t priority_cpp_refusal(std::uint64_t wide) {
    return wide ? refusal<Record>() : refusal<std::uint64_t>();
}
