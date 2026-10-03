// One ordinary-allocation byte/occupancy caller for all frozen grow helpers.
// This is a component attribution probe, not a source-language acceptance test.
#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

typedef uint8_t (*Grow)(void **, uint64_t);
typedef struct { uint64_t length, capacity; uint64_t data[]; } Slots;
_Static_assert(offsetof(Slots, data) == 16, "runtime Slots header");
_Static_assert(sizeof(void *) == 8 && sizeof(size_t) == 8, "frozen target ABI");
extern uint8_t wf_probe_grow_A(void **, uint64_t);
extern uint8_t wf_probe_grow_R(void **, uint64_t);
extern uint8_t wf_probe_grow_P(void **, uint64_t);
static const struct { const char *name; Grow function; } arms[] = {
    {"A", wf_probe_grow_A}, {"R", wf_probe_grow_R}, {"P", wf_probe_grow_P}
};
static void require(int condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "Hbyte: %s\n", message);
        exit(1);
    }
}
static uint64_t number(const char *text) {
    require(*text != 0, "empty numeric argument");
    uint64_t value = 0;
    for (; *text; ++text) {
        require(*text >= '0' && *text <= '9', "numeric argument");
        unsigned digit = (unsigned)(*text - '0');
        require(value <= (UINT64_MAX - digit) / 10, "numeric bound");
        value = value * 10 + digit;
    }
    return value;
}
static uint64_t extent(uint64_t capacity) { return 16 + 8 * capacity; }
static uint64_t next_capacity(uint64_t capacity) { return capacity ? 2 * capacity : 1; }
static void admitted_cell(uint64_t capacity, uint64_t length, uint64_t rounds) {
    require(capacity == 0 || (capacity <= 131072 && (capacity & (capacity - 1)) == 0),
            "predeclared capacity grid");
    require(length <= capacity && (length == 0 || length == 1 || length == capacity),
            "predeclared initialized prefix");
    require(rounds > 0 && rounds <= 65536, "predeclared rounds bound");
}
static uint64_t triangle(uint64_t n) {
    return n % 2 ? n * ((n - 1) / 2) : (n / 2) * (n - 1);
}
static uint64_t oracle(uint64_t capacity, uint64_t length, uint64_t rounds, uint64_t seed) {
    // Algebraic sum of all prescribed values and descriptor measures. Unsigned
    // wrap is intentional; divide before multiplying the triangular terms.
    return seed + rounds * (length + next_capacity(capacity))
        + length * (rounds * seed + triangle(rounds)) + rounds * triangle(length);
}

// One source grow-call expression uses a runtime-selected arm without LTO.
// Native loop versions depend on initialized length; every fixed cell uses
// the same caller code and indirect site across all three arms.
__attribute__((noinline))
uint64_t hbyte_trace(Grow grow, uint64_t capacity, uint64_t length,
                     uint64_t rounds, uint64_t seed) {
    uint64_t checksum = seed;
    const uint64_t total = next_capacity(capacity);
    for (uint64_t round = 0; round < rounds; ++round) {
        void *owner = malloc((size_t)extent(capacity));
        require(owner != NULL, "host setup allocation");
        Slots *initial = owner;
        initial->length = length;
        initial->capacity = capacity;
        for (uint64_t index = 0; index < length; ++index)
            initial->data[index] = seed + round + index;
        uint8_t status = grow(&owner, total);
        require(status == 0, "grow status");
        require(owner != NULL, "published nonnull owner");
        Slots *fresh = owner;
        require(fresh->length == length, "published length");
        require(fresh->capacity == total, "published capacity");
        checksum += fresh->length + fresh->capacity;
        for (uint64_t index = 0; index < length; ++index) {
            require(fresh->data[index] == seed + round + index, "initialized payload");
            checksum += fresh->data[index];
        }
        free(owner);
    }
    return checksum;
}
static uint64_t nanos(void) {
    struct timespec now;
    require(clock_gettime(CLOCK_MONOTONIC, &now) == 0, "monotonic clock");
    return (uint64_t)now.tv_sec * UINT64_C(1000000000) + (uint64_t)now.tv_nsec;
}
static void check(void) {
    size_t cells = 0, traces = 0;
    const uint64_t seeds[] = {101, UINT64_MAX - 3};
    for (uint64_t capacity = 0;; capacity = capacity ? 2 * capacity : 1) {
        uint64_t lengths[] = {0, 1, capacity};
        for (size_t index = 0; index < 3; ++index) {
            uint64_t length = lengths[index];
            if (length > capacity || (index == 2 && capacity <= 1)) continue;
            ++cells;
            admitted_cell(capacity, length, 2);
            for (size_t arm = 0; arm < 3; ++arm) {
                for (size_t seed = 0; seed < 2; ++seed) {
                    uint64_t actual = hbyte_trace(arms[arm].function, capacity, length, 2, seeds[seed]);
                    require(actual == oracle(capacity, length, 2, seeds[seed]), "independent checksum");
                    ++traces;
                }
            }
        }
        if (capacity == 131072) break;
    }
    require(cells == 54 && traces == 324, "complete bounded check matrix");
    printf("Hbyte: %zu cells, %zu traces; all arms match independent observations\n", cells, traces);
}

// Bounded host mutants exercise each result observation without reading a
// freed owner or claiming linear-value coverage from this copyable fixture.
static uint8_t bad_content(void **cell, uint64_t capacity) {
    uint8_t status = wf_probe_grow_A(cell, capacity);
    Slots *fresh = *cell;
    fresh->data[0] ^= 1;
    return status;
}
static uint8_t bad_length(void **cell, uint64_t capacity) {
    uint8_t status = wf_probe_grow_A(cell, capacity);
    ((Slots *)*cell)->length = 0;
    return status;
}
static uint8_t no_publication(void **cell, uint64_t capacity) {
    Slots *original = *cell;
    void *shadow = malloc((size_t)extent(original->capacity));
    require(shadow != NULL, "fault setup allocation");
    memcpy(shadow, original, (size_t)extent(original->length));
    uint8_t status = wf_probe_grow_A(&shadow, capacity);
    free(shadow);
    // The constructed replacement was deliberately discarded, leaving the
    // real owner valid but unchanged. The capacity observation must reject it.
    return status;
}
static uint8_t bad_status(void **cell, uint64_t capacity) {
    (void)cell; (void)capacity;
    return 1;
}
static uint8_t null_publication(void **cell, uint64_t capacity) {
    uint8_t status = wf_probe_grow_A(cell, capacity);
    free(*cell);
    *cell = NULL;
    return status;
}
static void fault(const char *name) {
    Grow selected = NULL;
    if (strcmp(name, "content") == 0) selected = bad_content;
    else if (strcmp(name, "length") == 0) selected = bad_length;
    else if (strcmp(name, "publication") == 0) selected = no_publication;
    else if (strcmp(name, "status") == 0) selected = bad_status;
    else if (strcmp(name, "null") == 0) selected = null_publication;
    else if (strcmp(name, "checksum") == 0) {
        uint64_t value = hbyte_trace(arms[0].function, 1, 1, 2, 101);
        require((value ^ 1) == oracle(1, 1, 2, 101), "independent checksum");
        return;
    } else require(0, "unknown fault");
    (void)hbyte_trace(selected, 1, 1, 1, 101);
    require(0, "fault escaped observation");
}
int main(int argc, char **argv) {
    require(argc >= 2, "command required");
    if (strcmp(argv[1], "check") == 0) {
        require(argc == 2, "check arguments");
        check();
    } else if (strcmp(argv[1], "fault") == 0) {
        require(argc == 3, "fault arguments");
        fault(argv[2]);
    } else if (strcmp(argv[1], "run") == 0) {
        require(argc == 7, "run arm capacity length rounds seed");
        size_t arm = 0;
        while (arm < 3 && strcmp(argv[2], arms[arm].name) != 0) ++arm;
        require(arm < 3, "unknown arm");
        uint64_t capacity = number(argv[3]), length = number(argv[4]);
        uint64_t rounds = number(argv[5]), seed = number(argv[6]);
        admitted_cell(capacity, length, rounds);
        uint64_t before = nanos();
        uint64_t checksum = hbyte_trace(arms[arm].function, capacity, length, rounds, seed);
        uint64_t elapsed = nanos() - before;
        require(checksum == oracle(capacity, length, rounds, seed), "independent checksum");
        printf("%s,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64
               ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n", arms[arm].name,
               capacity, length, next_capacity(capacity), extent(capacity), extent(next_capacity(capacity)),
               rounds, seed, elapsed, checksum);
    } else require(0, "unknown command");
    return 0;
}
