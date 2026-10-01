/* Observe the caller's real WF growth and target-limit transitions, without
 * timing or private C container layouts. The reference is a flat digest. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern uint64_t wf_growth_trace(uint8_t, uint64_t, uint64_t, uint64_t);
extern int wf__floor_run(int, char **);
static void *owners[2];
static size_t requests, releases, initial_size, grown_size;
static unsigned live[2];

static void require(int okay, const char *message) {
    if (!okay) { fprintf(stderr, "growth observer: %s\n", message); exit(1); }
}
void *wf_observe_allocate(uint64_t bytes) {
    size_t at = requests++;
    require(at < 2, "extra allocation");
    uint64_t payload = at ? grown_size : initial_size;
    require(bytes == payload, "wrong requested extent");
    owners[at] = malloc((size_t)bytes);
    require(owners[at] != NULL, "host allocation failed");
    memset(owners[at], 0xa5, (size_t)bytes);
    live[at] = 1;
    return owners[at];
}
void wf_observe_release(void *owner) {
    for (size_t at = 0; at < requests; ++at) {
        if (owners[at] != owner) continue;
        require(live[at], "repeated release");
        live[at] = 0; ++releases;
        /* Quarantine addresses until the whole case has returned. */
        memset(owner, 0xdd, (at ? grown_size : initial_size));
        return;
    }
    require(0, "release of a foreign address");
}
/* Force realloc to move, preserving the exact allocation/retirement ledger. */
void *wf_observe_reallocate(void *old, uint64_t bytes) {
    require(requests == 1 && old == owners[0] && live[0], "realloc owner");
    require(bytes >= initial_size, "realloc cannot shrink this growth");
    void *fresh = wf_observe_allocate(bytes);
    memcpy(fresh, old, initial_size);
    wf_observe_release(old);
    return fresh;
}
static uint64_t expected(uint8_t seed, size_t initial, size_t count,
                         size_t limit) {
    uint64_t status = 1;
    size_t length = count;
    if (count > initial) {
        if (count > limit) { status = 2; length = initial; }
    }
    for (size_t at = 0; at < length; ++at) {
        seed = (uint8_t)(seed * 73u + 41u);
        status = status * 131u + seed;
    }
    return status;
}
int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    const size_t initial[] = {1, 16, 256};
    const uint8_t seeds[] = {0, 255};
    size_t checked = 0;
    for (size_t cap = 0; cap < 3; ++cap)
        for (size_t growth = 0; growth < 3; ++growth)
            for (size_t limit_case = 0; limit_case < 2; ++limit_case)
                for (size_t seed = 0; seed < 2; ++seed) {
                    initial_size = initial[cap];
                    grown_size = growth == 0 ? initial_size
                        : growth == 1 ? initial_size + 1 : initial_size * 2;
                    size_t limit = grown_size - limit_case;
                    requests = releases = 0;
                    memset(owners, 0, sizeof owners); memset(live, 0, sizeof live);
                    uint64_t actual = wf_growth_trace(seeds[seed], initial_size,
                                                     grown_size, limit);
                    require(actual == expected(seeds[seed], initial_size, grown_size,
                                               limit), "data/status digest");
                    size_t wanted_requests =
                        grown_size > initial_size && grown_size <= limit ? 2 : 1;
                    require(requests == wanted_requests && releases == wanted_requests,
                            "allocation path or cleanup count");
                    require(!live[0] && !live[1], "live owner on return");
                    for (size_t at = 0; at < requests; ++at) free(owners[at]);
                    ++checked;
                }
    printf("growth: %zu boundary traces; all owners returned\n", checked);
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
