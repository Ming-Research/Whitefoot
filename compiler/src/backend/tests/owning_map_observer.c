// Native ownership evidence for the owning-map program, at v0.60.
//
// Retired subject: the allocation-refusal sweep this observer used to drive by
// returning NULL from the replaced `malloc` at each position in turn. [STOR-8]
// makes allocation total in the source -- it never returns a failure and never
// traps -- and the emitted module now answers a null allocation by calling
// `wf_resource_abort` and falling into `unreachable`, so a refusal has no
// source-visible subject and the program has no refusal exit code to check.
// Successor: the identity half below, which is the part an exit code still
// cannot provide -- every owner is a distinct allocation, still carries the
// value its own `box_new` was given when it is released, and is released
// exactly once, with nothing left live at exit.
//
// The payload check is what makes this an identity check rather than an
// allocation count. `exercise(seed)` runs twelve `allocate_put` calls in
// source order, each one `box_new::<u64>(value: v)` over the twelve values in
// `payloads` below, and `main` runs it for the sixteen seeds 0..16, so
// allocation `id` carries `payloads[(id - 1) % 12]`. The map moves those
// owners through swaps, tombstones, a replacement and a recycle; [STOR-7]
// lets any of that copy bytes between places, and [TYPE-9] keeps the content
// in the one heap object the `Box` owns throughout, so the value read back at
// the free must be the value that owner was constructed with. A map that
// crossed two owners over passes every count and fails here.
//
// The twelfth allocation of each seed is the genuinely full table's
// `Full(offered: back)`: `put` returns the owner it was offered rather than
// installing it, so that owner is released while it is still the newest
// request and carries 999.
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum { max_allocations = 4096, per_seed = 12 };
struct Cell {
    void *pointer;
    unsigned live;
    unsigned releases;
};
static struct Cell cells[max_allocations + 1];
static unsigned requests, allocated, released, full_returned;
static int ledger;
// The twelve values one `exercise` seed hands to `box_new`, in source order.
static const uint64_t payloads[per_seed] = {
    11, 22, 33, 44, 55, 66, 100, 101, 102, 103, 104, 999
};

static void fail(const char *reason, unsigned id) {
    fprintf(stderr, "FAIL requests=%u id=%u: %s\n", requests, id, reason);
    exit(100);
}

void *wf_observe_allocate(size_t bytes) {
    unsigned id = ++requests;
    if (id > max_allocations) fail("too many allocation requests", id);
    if (bytes != sizeof(uint64_t)) fail("unexpected Box allocation size", id);
    void *pointer = malloc(bytes);
    if (!pointer) fail("observer host allocation failed", id);
    for (unsigned prior = 1; prior < id; ++prior) {
        if (cells[prior].pointer == pointer) fail("identity reused within a run", id);
    }
    cells[id] = (struct Cell){ .pointer = pointer, .live = 1, .releases = 0 };
    ++allocated;
    if (ledger) printf("A%u;", id);
    return pointer;
}

void wf_observe_release(void *pointer, uint64_t bytes) {
    if (bytes != sizeof(uint64_t)) fail("unexpected Box release size", 0);
    if (!pointer) fail("null release", 0);
    unsigned id = 0;
    for (unsigned candidate = 1; candidate <= requests; ++candidate) {
        if (cells[candidate].pointer == pointer) {
            id = candidate;
            break;
        }
    }
    if (!id) fail("release does not name a supplied owner", 0);
    if (!cells[id].live || cells[id].releases) fail("duplicate release", id);
    uint64_t value;
    memcpy(&value, pointer, sizeof value);
    if (value != payloads[(id - 1) % per_seed]) fail("owner payload identity changed", id);
    if (id % per_seed == 0) {
        if (requests != id || value != 999) fail("Full did not return the current offered owner", id);
        ++full_returned;
    }
    cells[id].live = 0;
    ++cells[id].releases;
    ++released;
    if (ledger) printf("F%u=%" PRIu64 ";", id, value);
    // Quarantine until the run finishes so a stale pointer cannot alias a later
    // owner. This observer changes allocator address reuse, not the fixture's
    // algorithm.
    memset(pointer, 0xa5, sizeof(uint64_t));
}

extern int wf_fixture_main(int argc, char **argv);

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "--ledger") == 0) {
        ledger = 1;
    } else if (argc != 1) {
        fprintf(stderr, "usage: owning-map-observer [--ledger]\n");
        return 64;
    }
    memset(cells, 0, sizeof cells);
    int status = wf_fixture_main(argc, argv);
    if (status != 0) fail("unexpected command exit status", 0);
    if (!requests) fail("the fixture allocated no owner", 0);
    if (requests % per_seed) fail("every seed supplies twelve owners", requests);
    if (allocated != requests || released != requests) fail("incomplete owner cleanup", 0);
    if (full_returned != requests / per_seed) fail("wrong Full owner count", full_returned);
    for (unsigned id = 1; id <= requests; ++id) {
        if (!cells[id].pointer || cells[id].live || cells[id].releases != 1) {
            fail("owner was not released exactly once", id);
        }
        free(cells[id].pointer);
    }
    printf("PASS status=%d allocations=%u releases=%u full_original_owners=%u live=0\n",
        status, allocated, released, full_returned);
    return 0;
}
