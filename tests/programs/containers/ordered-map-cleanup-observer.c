// Observe ordered-map-cleanup-order.wf: one child Box per nodrop value and
// a 16-key split. Payload and node releases share one event stream, so
// preserving callback order cannot conceal a parent released after its child.
#include <inttypes.h>
#include <stdbool.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "sched/prim.h"

extern int wf_fixture_main(int argc, char **argv);
enum { PAYLOAD_BYTES = 8, NODE_BYTES = 504, ALLOCATIONS = 19 };
typedef struct { void *pointer; uint64_t bytes; unsigned node; bool released; } Allocation;
typedef struct { char kind; unsigned identity; } Event;
static Allocation allocations[ALLOCATIONS];
static unsigned allocation_count, node_count, payload_count, event_count;
static atomic_flag ledger_lock = ATOMIC_FLAG_INIT;
static const Event expected[ALLOCATIONS] = {
    {'P', 7}, {'P', 15}, {'P', 14}, {'P', 13}, {'P', 12}, {'P', 11},
    {'P', 10}, {'P', 9}, {'P', 8}, {'N', 2}, {'N', 3},
    {'P', 6}, {'P', 5}, {'P', 4}, {'P', 3}, {'P', 2}, {'P', 1},
    {'P', 0}, {'N', 1},
};
static void require(bool condition, const char *reason) {
    if (!condition) {
        fprintf(stderr, "ordered cleanup observer: %s\n", reason);
        exit(1);
    }
}
static void lock(void) {
    while (atomic_flag_test_and_set_explicit(&ledger_lock, memory_order_acquire))
        wf_prim_spin_hint();
}
static void unlock(void) {
    atomic_flag_clear_explicit(&ledger_lock, memory_order_release);
}
void *wf_observe_allocate(uint64_t bytes) {
    require(bytes == PAYLOAD_BYTES || bytes == NODE_BYTES, "allocation extent");
    void *pointer = malloc((size_t)bytes);
    require(pointer != NULL, "host allocation");
    memset(pointer, 0xcc, (size_t)bytes);
    lock();
    require(allocation_count < ALLOCATIONS, "allocation count");
    unsigned node = bytes == NODE_BYTES ? ++node_count : 0;
    if (bytes == PAYLOAD_BYTES) ++payload_count;
    allocations[allocation_count++] = (Allocation){pointer, bytes, node, false};
    unlock();
    return pointer;
}
void wf_observe_release(void *pointer) {
    if (pointer == NULL) return;
    lock();
    for (unsigned index = 0; index < allocation_count; ++index) {
        Allocation *allocation = &allocations[index];
        if (allocation->pointer != pointer) continue;
        require(!allocation->released, "allocation released twice");
        require(event_count < ALLOCATIONS, "release count");
        Event actual = {'N', allocation->node};
        if (allocation->bytes == PAYLOAD_BYTES) {
            uint64_t key;
            memcpy(&key, pointer, sizeof key);
            require(key < 16, "payload identity");
            actual = (Event){'P', (unsigned)key};
        }
        Event want = expected[event_count];
        require(actual.kind == want.kind,
                "parent release before leading callback");
        require(actual.identity == want.identity,
                actual.kind == 'P' ? "callback order" : "node release order");
        ++event_count;
        allocation->released = true;
        memset(pointer, 0xa5, (size_t)allocation->bytes);
        unlock();
        return;
    }
    require(false, "release did not return an allocated address");
}
int main(void) {
    int result = wf_fixture_main(0, NULL);
    require(result == 0, "fixture callback ledger");
    lock();
    require(allocation_count == ALLOCATIONS && payload_count == 16 && node_count == 3,
            "complete allocation count");
    require(event_count == ALLOCATIONS, "complete release count");
    for (unsigned index = 0; index < allocation_count; ++index) {
        require(allocations[index].released, "unreleased owner");
        free(allocations[index].pointer);
    }
    puts("ordered cleanup observer: 16 callbacks and 3 node releases preserve order");
    unlock();
    return 0;
}
