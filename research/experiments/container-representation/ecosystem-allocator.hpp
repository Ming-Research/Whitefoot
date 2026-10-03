#ifndef WHITEFOOT_ECOSYSTEM_ALLOCATOR_HPP
#define WHITEFOOT_ECOSYSTEM_ALLOCATOR_HPP

// Accounting-only observer used by the five explicit ecosystem comparisons.
// Timed adapters use their library's ordinary default allocator. Retire this
// header with the comparison or when a maintained runner supersedes it.
#if !defined(ACCOUNT_ONLY)
#error "Include this observer only in an ACCOUNT_ONLY image"
#endif

#include <cstddef>
#include <cstdint>
#include <memory>
#include <type_traits>

extern "C" void wf_ecosystem_note_alloc(std::uint64_t bytes);
extern "C" void wf_ecosystem_note_dealloc(std::uint64_t bytes);

template <class T> class EcosystemAllocator {
public:
    using value_type = T;
    using size_type = std::size_t;
    using difference_type = std::ptrdiff_t;
    using propagate_on_container_move_assignment = std::true_type;
    using is_always_equal = std::true_type;

    EcosystemAllocator() noexcept = default;
    template <class U> EcosystemAllocator(const EcosystemAllocator<U> &) noexcept {}

    [[nodiscard]] T *allocate(std::size_t count) {
        T *pointer = std::allocator<T>{}.allocate(count);
        wf_ecosystem_note_alloc(static_cast<std::uint64_t>(count * sizeof(T)));
        return pointer;
    }

    void deallocate(T *pointer, std::size_t count) noexcept {
        wf_ecosystem_note_dealloc(static_cast<std::uint64_t>(count * sizeof(T)));
        std::allocator<T>{}.deallocate(pointer, count);
    }

    template <class U>
    bool operator==(const EcosystemAllocator<U> &) const noexcept { return true; }
};

#endif
