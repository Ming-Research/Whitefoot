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
static bool refuse_next_reallocation;
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

void wf_observe_release(void *pointer) {
    if (pointer == NULL) return;
    lock_ledger();
    for (size_t index = 0; index < allocation_count; ++index) {
        Allocation *allocation = &allocations[index];
        if (allocation->pointer != pointer) continue;
        require(!allocation->released, "allocation released twice");
        allocation->released = true;
        ++release_count;
        memset(pointer, 0xa5,
               allocation->bytes == 0 ? 1 : (size_t)allocation->bytes);
        unlock_ledger();
        return;
    }
    require(false, "release did not return an allocated address");
}

void *wf_observe_reallocate(void *pointer, uint64_t bytes) {
    if (pointer == NULL) return wf_observe_allocate(bytes);
    lock_ledger();
    size_t old_bytes = 0;
    bool found = false;
    for (size_t index = 0; index < allocation_count; ++index) {
        const Allocation *allocation = &allocations[index];
        if (allocation->pointer != pointer) continue;
        require(!allocation->released, "reallocation of a released allocation");
        old_bytes = allocation->bytes == 0 ? 1 : (size_t)allocation->bytes;
        found = true;
        break;
    }
    require(found, "reallocation did not use an allocated address");
    if (refuse_next_reallocation) {
        refuse_next_reallocation = false;
        ++request_count;
        unlock_ledger();
        return NULL;
    }
    unlock_ledger();
    // This functional observer forces a move and quarantines the old block.
    // Calling real realloc would release storage the final ledger still owns.
    // Real in-place/moving/NULL growth is covered by backend exhaustion tests.
    void *replacement = wf_observe_allocate(bytes);
    size_t new_bytes = bytes == 0 ? 1 : (size_t)bytes;
    memcpy(replacement, pointer, old_bytes < new_bytes ? old_bytes : new_bytes);
    wf_observe_release(pointer);
    return replacement;
}

// Observer controls, not source allocation-refusal programs. Refuse each of
// three successive requests in turn and prove the old identity, byte extent,
// contents and eventual release survive; successful moves preserve the prefix.
static void exercise_reallocation(int refused_position) {
    size_t bytes = 8;
    unsigned char *pointer = wf_observe_allocate(bytes);
    memset(pointer, 0x37, bytes);
    for (int position = 0; position < 3; ++position) {
        refuse_next_reallocation = position == refused_position;
        unsigned char *replacement = wf_observe_reallocate(pointer, bytes + 8);
        require((replacement == NULL) == (position == refused_position),
                "reallocation result did not match its control");
        if (replacement == NULL) {
            lock_ledger();
            const Allocation *owner = &allocations[allocation_count - 1];
            require(owner->pointer == pointer && owner->bytes == bytes &&
                    !owner->released, "failed reallocation changed its owner");
            unlock_ledger();
        }
        unsigned char *retained = replacement == NULL ? pointer : replacement;
        for (size_t index = 0; index < bytes; ++index)
            require(retained[index] == 0x37, "reallocation lost retained contents");
        if (replacement != NULL) {
            pointer = replacement;
            bytes += 8;
            memset(pointer, 0x37, bytes);
        }
    }
    wf_observe_release(pointer);
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
        wf_observe_release(cross_release[other][index]);
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
    } else if (argc == 2 && strcmp(argv[1], "reallocate") == 0) {
        exercise_reallocation(-1);
    } else if (argc == 2 && strcmp(argv[1], "reallocate-null") == 0) {
        for (int position = 0; position < 3; ++position)
            exercise_reallocation(position);
    } else if (argc == 2 && strcmp(argv[1], "released-reallocation") == 0) {
        void *pointer = wf_observe_allocate(8);
        wf_observe_release(pointer);
        (void)wf_observe_reallocate(pointer, 16);
    } else if (argc == 2 && strcmp(argv[1], "foreign-reallocation") == 0) {
        unsigned char *pointer = wf_observe_allocate(8);
        (void)wf_observe_reallocate(pointer + 1, 16);
    } else if (argc == 2 && strcmp(argv[1], "double-release") == 0) {
        void *pointer = wf_observe_allocate(8);
        wf_observe_release(pointer);
        wf_observe_release(pointer);
    } else if (argc == 2 && strcmp(argv[1], "foreign-release") == 0) {
        unsigned char *pointer = wf_observe_allocate(8);
        wf_observe_release(pointer + 1);
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
    printf("container allocation observer: %zu allocations, each released exactly once\n",
           allocation_count);
    unlock_ledger();
    return 0;
}
