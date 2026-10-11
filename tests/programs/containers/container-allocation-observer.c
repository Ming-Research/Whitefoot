// The release ledger of each bundled container test program, observed by
// interposing every generated allocation and release.
//
// Retired subject: the allocation-refusal sweep. v0.59 returned null at each
// allocation request in turn and read the source-visible fallback the library
// installed -- an `Err(unit)` reserve, an `Err(value)` append, a retained
// owner -- and pinned the exact byte count of every request beside it.
// [STOR-8] makes allocation total in the source: it never returns a failure,
// never traps, and no allocating operation carries a `Result`, and an
// exhausted heap ends the program from the trusted base outside the language
// [SCOPE-3]. There is no refusal for a program to observe and no fallback arm
// to reach, so the sweep and its expected-byte table both go with the rule.
//
// The successor kept here is the identity half, which [STOR-3] and [WIN-3]
// still fix: every allocation the program makes is released exactly once,
// never twice, and none is left behind when the entry returns. That property
// is what catches a missed release walk over an owning element, a double free
// of a superseded backing, and a release of storage the program never owned.
#include <inttypes.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "sched/prim.h"

extern int wf_fixture_main(int argc, char **argv);

// [STOR-1] a capacity-0 runtime window is the shared empty header: it is
// never allocated, so its release or growth is not a ledger event. Each
// capacity-0 construction ends in exactly one such release or growth, which
// the observer counts so that its total equals the allocations a
// per-window header would have made.
extern const unsigned char wf__empty_window[64];
static atomic_size_t shared_header_events;

enum { MAX_ALLOCATIONS = 256 };

typedef struct {
    void *pointer;
    uint64_t bytes;
    size_t request;
    bool released;
} Allocation;

static Allocation allocations[MAX_ALLOCATIONS];
static size_t allocation_count;
static size_t request_count;
static size_t release_count;
static atomic_flag ledger_lock = ATOMIC_FLAG_INIT;

// Parallel lowering may allocate and consume independent owners on different
// workers. The observer's shared ledger needs synchronization even though the
// source owners themselves are disjoint. Only observation is serialized.
static void lock_ledger(void) {
    while (atomic_flag_test_and_set_explicit(&ledger_lock, memory_order_acquire))
        wf_prim_spin_hint();
}

static void unlock_ledger(void) {
    atomic_flag_clear_explicit(&ledger_lock, memory_order_release);
}

static void require(bool condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "container allocation observer: %s\n", message);
        exit(1);
    }
}

void *wf_observe_allocate(uint64_t bytes) {
    // Allocation is total: the trusted base either hands back storage or ends
    // the process, so this observer never returns null.
    void *pointer = malloc(bytes == 0 ? 1 : (size_t)bytes);
    require(pointer != NULL, "host allocation failed during observation");
    memset(pointer, 0xcc, bytes == 0 ? 1 : (size_t)bytes);
    lock_ledger();
    ++request_count;
    require(allocation_count < MAX_ALLOCATIONS, "allocation ledger overflow");
    allocations[allocation_count++] =
        (Allocation){pointer, bytes, request_count, false};
    unlock_ledger();
    return pointer;
}

void wf_observe_release(void *pointer, uint64_t bytes) {
    if (pointer == NULL) return;
    if (pointer == (const void *)wf__empty_window) {
        atomic_fetch_add(&shared_header_events, 1);
        return;
    }
    lock_ledger();
    for (size_t index = 0; index < allocation_count; ++index) {
        Allocation *allocation = &allocations[index];
        if (allocation->pointer != pointer) continue;
        require(!allocation->released, "allocation released twice");
        require(allocation->bytes == bytes, "release size differs from allocation request");
        allocation->released = true;
        ++release_count;
        memset(pointer, 0xa5,
               allocation->bytes == 0 ? 1 : (size_t)allocation->bytes);
        unlock_ledger();
        return;
    }
    require(false, "release did not return an allocated address");
}

// `grow` [OP-10] reallocates its block: the observer books that as a fresh
// allocation that takes over the old block's bytes, plus the release of the
// old one, so a growth is one request and one release exactly as the former
// allocate, copy and free were. The old block stays quarantined, never handed
// back to the host allocator until the end, because a real realloc could
// return the address of an earlier released entry and make the ledger's
// pointer identity ambiguous.
void *wf_observe_reallocate(void *pointer, uint64_t old_bytes, uint64_t bytes) {
    if (pointer == NULL) return wf_observe_allocate(bytes);
    if (pointer == (const void *)wf__empty_window) {
        atomic_fetch_add(&shared_header_events, 1);
        void *fresh = wf_observe_allocate(bytes);
        memcpy(fresh, pointer, (size_t)old_bytes);
        return fresh;
    }
    void *moved = wf_observe_allocate(bytes);
    lock_ledger();
    for (size_t index = 0; index < allocation_count; ++index) {
        Allocation *allocation = &allocations[index];
        if (allocation->pointer != pointer) continue;
        require(!allocation->released, "allocation released twice");
        require(allocation->bytes == old_bytes, "retake size differs from allocation request");
        uint64_t kept = allocation->bytes < bytes ? allocation->bytes : bytes;
        memcpy(moved, pointer, (size_t)kept);
        allocation->released = true;
        ++release_count;
        memset(pointer, 0xa5,
               allocation->bytes == 0 ? 1 : (size_t)allocation->bytes);
        unlock_ledger();
        return moved;
    }
    require(false, "release did not return an allocated address");
    return NULL;
}

enum { OBSERVER_WORKERS = 4, REQUESTS_PER_WORKER = 8 };
static wf_prim_thread observer_threads[OBSERVER_WORKERS];
static size_t worker_numbers[OBSERVER_WORKERS];
static void *cross_release[OBSERVER_WORKERS][REQUESTS_PER_WORKER];
static _Atomic unsigned started, filled, finished;

static void wait_for_workers(_Atomic unsigned *count) {
    while (atomic_load_explicit(count, memory_order_acquire) != OBSERVER_WORKERS)
        wf_prim_yield();
}

static void exercise_worker(void *argument) {
    size_t worker = *(size_t *)argument;
    atomic_fetch_add_explicit(&started, 1, memory_order_acq_rel);
    wait_for_workers(&started);
    for (size_t index = 0; index < REQUESTS_PER_WORKER; ++index)
        cross_release[worker][index] = wf_observe_allocate(index * 8);
    atomic_fetch_add_explicit(&filled, 1, memory_order_acq_rel);
    wait_for_workers(&filled);
    size_t other = (worker + 1) % OBSERVER_WORKERS;
    for (size_t index = 0; index < REQUESTS_PER_WORKER; ++index)
        wf_observe_release(cross_release[other][index], index * 8);
    atomic_fetch_add_explicit(&finished, 1, memory_order_acq_rel);
}

static void exercise_concurrent_observation(void) {
    for (size_t worker = 0; worker < OBSERVER_WORKERS; ++worker) {
        worker_numbers[worker] = worker;
        require(wf_prim_thread_start(&observer_threads[worker], exercise_worker,
                                    &worker_numbers[worker], 0) == 0,
                "could not start observer worker");
    }
    wait_for_workers(&finished);
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "concurrent") == 0) {
        exercise_concurrent_observation();
    } else if (argc == 2 && strcmp(argv[1], "double-release") == 0) {
        void *pointer = wf_observe_allocate(8);
        wf_observe_release(pointer, 8);
        wf_observe_release(pointer, 8);
    } else if (argc == 2 && strcmp(argv[1], "foreign-release") == 0) {
        unsigned char *pointer = wf_observe_allocate(8);
        wf_observe_release(pointer + 1, 8);
    } else if (argc == 2 && strcmp(argv[1], "wrong-size") == 0) {
        void *pointer = wf_observe_allocate(8);
        wf_observe_release(pointer, 7);
    } else if (argc == 2 && strcmp(argv[1], "missing-release") == 0) {
        (void)wf_observe_allocate(8);
    } else {
        require(argc == 1, "unknown observer control");
        int status = wf_fixture_main(0, NULL);
        if (status != 0) {
            fprintf(stderr,
                    "container allocation observer: the fixture returned status %d, "
                    "expected 0\n",
                    status);
            exit(1);
        }
    }
    lock_ledger();
    require(request_count > 0, "the fixture must reach the heap at all");
    require(release_count == allocation_count,
            "every allocation is released exactly once");
    for (size_t index = 0; index < allocation_count; ++index) {
        if (!allocations[index].released) {
            fprintf(stderr,
                    "container allocation observer: request %zu (%" PRIu64
                    " bytes) was never released\n",
                    allocations[index].request, allocations[index].bytes);
            exit(1);
        }
    }
    for (size_t index = 0; index < allocation_count; ++index)
        free(allocations[index].pointer);
    printf("container allocation observer: %zu allocations or shared empty headers, each released exactly once\n",
           allocation_count + atomic_load(&shared_header_events));
    unlock_ledger();
    return 0;
}
