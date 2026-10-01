/* The retained native sparse algorithm is the control, not a translation of WF.
 * Allocation quarantine makes premature release and address substitution visible. */
#if !defined(_WIN32)
#define _POSIX_C_SOURCE 200809L
#endif
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void *wf_observe_allocate(uint64_t bytes);
void wf_observe_release(void *pointer);
#define malloc wf_observe_allocate
#define free wf_observe_release
#include <assert.h>
#include <stdbool.h>
#include <inttypes.h>
#define MAX_RESOURCES 16384u
#define EMPTY 0x80u
#define DELETED 0xfeu
#define MAX_FULL 0x7fu


typedef struct Resource {
    uint64_t id;
    uint64_t data[4];
} Resource;

typedef struct Cell {
    uint64_t key;
    Resource *resource;
} Cell;

typedef struct Ledger {
    bool created[MAX_RESOURCES];
    bool dropped[MAX_RESOURCES];
    Resource *live[MAX_RESOURCES];
    size_t created_count;
    size_t dropped_count;
} Ledger;

static Ledger ledger;
static size_t backing_live_bytes;
static size_t backing_peak_bytes;
static size_t backing_live_allocations;
static size_t backing_peak_allocations;
static size_t control_bytes_initialized;

static void reset_ledger(void) {
    memset(&ledger, 0, sizeof(ledger));
}
static void resource_drop(Resource *resource) {
    assert(resource != NULL);
    const uint64_t id = resource->id;
    assert(id < MAX_RESOURCES && ledger.created[id] && !ledger.dropped[id]);
    assert(ledger.live[id] == resource);
    ledger.dropped[id] = true;
    ledger.live[id] = NULL;
    ++ledger.dropped_count;
    free(resource);
}

static void *backing_allocate(size_t bytes) {
    void *memory = malloc(bytes == 0 ? 1 : bytes);
    if (memory != NULL) {
        backing_live_bytes += bytes;
        ++backing_live_allocations;
        if (backing_live_bytes > backing_peak_bytes) {
            backing_peak_bytes = backing_live_bytes;
        }
        if (backing_live_allocations > backing_peak_allocations) {
            backing_peak_allocations = backing_live_allocations;
        }
    }
    return memory;
}

static void backing_release(void *memory, size_t bytes) {
    if (memory == NULL) {
        return;
    }
    assert(backing_live_bytes >= bytes && backing_live_allocations > 0);
    backing_live_bytes -= bytes;
    --backing_live_allocations;
    free(memory);
}

static bool checked_mul(size_t a, size_t b, size_t *out) {
    if (a != 0 && b > SIZE_MAX / a) {
        return false;
    }
    *out = a * b;
    return true;
}
static uint8_t fingerprint(uint64_t key) {
    return (uint8_t)(key & MAX_FULL);
}

static size_t bucket(uint64_t key, size_t capacity) {
    assert(capacity > 0);
    return (size_t)(key % capacity);
}

static size_t probe_index(size_t start, size_t probe, size_t capacity) {
    assert(start < capacity && probe < capacity);
    return probe >= capacity - start ? probe - (capacity - start) : start + probe;
}

static bool is_full(uint8_t control) {
    return control <= MAX_FULL;
}

static bool is_vacant(uint8_t control) {
    return control == EMPTY || control == DELETED;
}

typedef struct ISlot {
    uint8_t control;
    Cell cell;
} ISlot;

typedef struct ITable {
    size_t capacity;
    size_t bytes;
    ISlot *slots;
} ITable;


static bool i_init(ITable *table, size_t capacity) {
    size_t bytes;
    if (capacity == 0 || !checked_mul(capacity, sizeof(ISlot), &bytes)) {
        return false;
    }
    ISlot *slots = backing_allocate(bytes);
    if (slots == NULL) {
        return false;
    }
    for (size_t i = 0; i < capacity; ++i) {
        slots[i].control = EMPTY;
    }
    assert(control_bytes_initialized <= SIZE_MAX - capacity);
    control_bytes_initialized += capacity;
    *table = (ITable){.capacity = capacity, .bytes = bytes, .slots = slots};
    return true;
}

static void i_release_backing(ITable *table) {
    backing_release(table->slots, table->bytes);
    *table = (ITable){0};
}

static uint8_t *i_ctl(ITable *table, size_t index) {
    assert(index < table->capacity);
    return &table->slots[index].control;
}

static Cell *i_cell(ITable *table, size_t index) {
    assert(index < table->capacity);
    return &table->slots[index].cell;
}
typedef enum PutResult { PUT_INSERTED, PUT_REPLACED, PUT_FULL } PutResult;
typedef enum StepResult { STEP_PAUSED, STEP_DONE, STEP_BLOCKED_FULL } StepResult;
typedef enum Phase { PHASE_SCAN, PHASE_PROBE, PHASE_BLOCKED, PHASE_DONE } Phase;

    typedef struct iRehash {
        ITable source;
        ITable target;
        Phase phase;
        size_t source_next;
        Cell held;
        bool held_valid;
        size_t probe_start;
        size_t next_probe;
    } iRehash;
    static PutResult i_put(ITable *table, uint64_t key, Resource *offered,
                             Resource **old, size_t *placed_index) {
        assert(table->capacity > 0 && offered != NULL);
        *old = NULL;
        size_t first_deleted = SIZE_MAX;
        const size_t start = bucket(key, table->capacity);
        for (size_t probe = 0; probe < table->capacity; ++probe) {
            const size_t index = probe_index(start, probe, table->capacity);
            const uint8_t control = *i_ctl(table, index);
            if (control == DELETED) {
                if (first_deleted == SIZE_MAX) {
                    first_deleted = index;
                }
                continue;
            }
            if (control == EMPTY) {
                const size_t destination =
                    first_deleted == SIZE_MAX ? index : first_deleted;
                Cell *cell = i_cell(table, destination);
                cell->key = key;
                cell->resource = offered;
                *i_ctl(table, destination) = fingerprint(key);
                *placed_index = destination;
                return PUT_INSERTED;
            }
            assert(is_full(control));
            Cell *cell = i_cell(table, index);
            if (cell->key == key) {
                *old = cell->resource;
                cell->resource = offered;
                *i_ctl(table, index) = fingerprint(key);
                *placed_index = index;
                return PUT_REPLACED;
            }
        }
        if (first_deleted != SIZE_MAX) {
            Cell *cell = i_cell(table, first_deleted);
            cell->key = key;
            cell->resource = offered;
            *i_ctl(table, first_deleted) = fingerprint(key);
            *placed_index = first_deleted;
            return PUT_INSERTED;
        }
        return PUT_FULL;
    }

    static Resource *i_find(ITable *table, uint64_t key, size_t *examined) {
        const size_t start = bucket(key, table->capacity);
        *examined = 0;
        for (size_t probe = 0; probe < table->capacity; ++probe) {
            const size_t index = probe_index(start, probe, table->capacity);
            const uint8_t control = *i_ctl(table, index);
            ++*examined;
            if (control == EMPTY) {
                return NULL;
            }
            if (is_full(control)) {
                Cell *cell = i_cell(table, index);
                if (control == fingerprint(key) && cell->key == key) {
                    return cell->resource;
                }
            }
        }
        return NULL;
    }

    static Resource *i_remove(ITable *table, uint64_t key) {
        const size_t start = bucket(key, table->capacity);
        for (size_t probe = 0; probe < table->capacity; ++probe) {
            const size_t index = probe_index(start, probe, table->capacity);
            const uint8_t control = *i_ctl(table, index);
            if (control == EMPTY) {
                return NULL;
            }
            if (is_full(control)) {
                Cell *cell = i_cell(table, index);
                if (control == fingerprint(key) && cell->key == key) {
                    Resource *resource = cell->resource;
                    *i_ctl(table, index) = DELETED;
                    return resource;
                }
            }
        }
        return NULL;
    }

    static uint64_t i_digest(ITable *table) {
        uint64_t digest = UINT64_C(1469598103934665603);
        digest ^= table->capacity;
        digest *= UINT64_C(1099511628211);
        for (size_t i = 0; i < table->capacity; ++i) {
            const uint8_t control = *i_ctl(table, i);
            digest ^= control;
            digest *= UINT64_C(1099511628211);
            if (is_full(control)) {
                Cell *cell = i_cell(table, i);
                digest ^= cell->key;
                digest *= UINT64_C(1099511628211);
                digest ^= cell->resource->id;
                digest *= UINT64_C(1099511628211);
            }
        }
        return digest;
    }

    static void i_drop_contents(ITable *table) {
        for (size_t i = 0; i < table->capacity; ++i) {
            if (is_full(*i_ctl(table, i))) {
                resource_drop(i_cell(table, i)->resource);
                *i_ctl(table, i) = EMPTY;
            }
        }
    }

    static bool i_begin(ITable *source, size_t target_capacity, iRehash *rehash) {
        ITable target = {0};
        if (!i_init(&target, target_capacity)) {
            return false;
        }
        *rehash = (iRehash){
            .source = *source, .target = target, .phase = PHASE_SCAN, .source_next = 0,
        };
        *source = (ITable){0};
        return true;
    }

    static void i_begin_into(ITable *source, ITable *target, iRehash *rehash) {
        assert(source->capacity > 0 && target->capacity > 0);
        *rehash = (iRehash){
            .source = *source, .target = *target, .phase = PHASE_SCAN, .source_next = 0,
        };
        *source = (ITable){0};
        *target = (ITable){0};
    }

    static StepResult i_step(iRehash *rehash, size_t budget, size_t *examined,
                               size_t *moved) {
        *examined = 0;
        *moved = 0;
        if (rehash->phase == PHASE_BLOCKED) {
            return STEP_BLOCKED_FULL;
        }
        if (rehash->phase == PHASE_DONE) {
            return STEP_DONE;
        }
        while (*examined < budget) {
            if (rehash->phase == PHASE_SCAN) {
                if (rehash->source_next == rehash->source.capacity) {
                    rehash->phase = PHASE_DONE;
                    return STEP_DONE;
                }
                const size_t index = rehash->source_next++;
                const uint8_t control = *i_ctl(&rehash->source, index);
                ++*examined;
                if (is_full(control)) {
                    rehash->held = *i_cell(&rehash->source, index);
                    rehash->held_valid = true;
                    *i_ctl(&rehash->source, index) = DELETED;
                    rehash->probe_start = bucket(rehash->held.key, rehash->target.capacity);
                    rehash->next_probe = 0;
                    rehash->phase = PHASE_PROBE;
                }
            } else {
                assert(rehash->phase == PHASE_PROBE && rehash->held_valid);
                if (rehash->next_probe == rehash->target.capacity) {
                    rehash->phase = PHASE_BLOCKED;
                    return STEP_BLOCKED_FULL;
                }
                const size_t index = probe_index(
                    rehash->probe_start, rehash->next_probe, rehash->target.capacity);
                ++rehash->next_probe;
                const uint8_t control = *i_ctl(&rehash->target, index);
                ++*examined;
                if (is_vacant(control)) {
                    *i_cell(&rehash->target, index) = rehash->held;
                    *i_ctl(&rehash->target, index) = fingerprint(rehash->held.key);
                    rehash->held_valid = false;
                    ++*moved;
                    rehash->phase = PHASE_SCAN;
                }
            }
        }
        if (rehash->phase == PHASE_SCAN &&
            rehash->source_next == rehash->source.capacity) {
            rehash->phase = PHASE_DONE;
            return STEP_DONE;
        }
        return STEP_PAUSED;
    }

    static void i_cleanup_rehash(iRehash *rehash) {
        if (rehash->held_valid) {
            resource_drop(rehash->held.resource);
            rehash->held_valid = false;
        }
        i_drop_contents(&rehash->source);
        i_drop_contents(&rehash->target);
        i_release_backing(&rehash->source);
        i_release_backing(&rehash->target);
        rehash->phase = PHASE_DONE;
    }


#undef free
#undef malloc

extern uint64_t wf_map_contract(uint64_t, uint64_t, uint64_t);
#ifdef BEHAVIOR_DEMOS
extern uint8_t wf_behavior_contract(uint64_t);
#endif

typedef struct {
    void *pointer;
    uint64_t bytes, id;
    bool is_resource;
    bool released;
} ObservedAllocation;

typedef struct {
    ObservedAllocation allocations[16];
    size_t requests, allocated, released, live, peak;
} Observation;

static Observation observation;
static size_t compared_runs, compared_owners, compared_backings;
static uint64_t expected_requests[7];
static bool expected_resources[7];
static size_t expected_request_count;
static const uint64_t wf_slot_stride_bytes = 24;

static void require(bool condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "owning-growth: %s\n", message);
        exit(1);
    }
}

void *wf_observe_allocate(uint64_t bytes) {
    require(observation.requests < expected_request_count, "unexpected allocation request");
    const size_t request = observation.requests;
    require(bytes == expected_requests[request],
            "allocation differs from its exact request-position extent");
    ++observation.requests;
    void *pointer = malloc((size_t)bytes);
    require(pointer != NULL, "host allocation failed");
    memset(pointer, 0xcc, (size_t)bytes);
    observation.allocations[observation.allocated++] = (ObservedAllocation){
        .pointer = pointer,
        .bytes = bytes,
        .id = 0,
        .is_resource = expected_resources[request],
        .released = false,
    };
    observation.live += (size_t)bytes;
    if (observation.live > observation.peak) observation.peak = observation.live;
    return pointer;
}

void wf_observe_release(void *pointer) {
    if (!pointer) return;
    for (size_t i = 0; i < observation.allocated; ++i) {
        ObservedAllocation *entry = &observation.allocations[i];
        if (entry->pointer != pointer) continue;
        require(!entry->released, "owner released twice");
        if (entry->is_resource) {
            require(entry->bytes == sizeof(Resource), "resource extent changed");
            Resource resource;
            memcpy(&resource, pointer, sizeof resource);
            entry->id = resource.id;
            for (size_t word = 0; word < 4; ++word)
                require(resource.data[word] == resource.id * 17 + word,
                        "resource identity or contents changed");
        }
#ifdef BEHAVIOR_DEMOS
        if (entry->bytes == sizeof(uint64_t)) {
            memcpy(&entry->id, pointer, sizeof entry->id);
            require(entry->id == 9, "branded key identity or contents changed");
        }
#endif
        entry->released = true;
        ++observation.released;
        observation.live -= (size_t)entry->bytes;
        memset(pointer, 0xa5, (size_t)entry->bytes);
        return;
    }
    require(false, "release did not return the original allocation address");
}

/* A moving realloc keeps the observer's allocation identities explicit. */
void *wf_observe_reallocate(void *old, uint64_t bytes) {
    for (size_t i = 0; i < observation.allocated; ++i) {
        ObservedAllocation *entry = &observation.allocations[i];
        if (entry->pointer != old) continue;
        require(!entry->released && !entry->is_resource, "realloc backing owner");
        uint64_t old_bytes = entry->bytes;
        require(bytes >= old_bytes, "realloc growth extent");
        void *fresh = wf_observe_allocate(bytes);
        memcpy(fresh, old, (size_t)old_bytes);
        wf_observe_release(old);
        return fresh;
    }
    require(false, "realloc of a foreign allocation");
    return NULL;
}

static void observation_reset(uint64_t scenario, bool whitefoot) {
    require(observation.live == 0, "live allocations before reset");
    for (size_t i = 0; i < observation.allocated; ++i)
        free(observation.allocations[i].pointer);
    memset(&observation, 0, sizeof observation);
    memset(expected_resources, 0, sizeof expected_resources);
    /* LLVM lays out the Whitefoot Slot and sparse-owned.c's ISlot at the same
     * 24-byte element stride. Both allocate only their payload. Check each side's
     * exact allocation sequence, not the larger source OP-9 ceiling. */
    const uint64_t stride = whitefoot ? wf_slot_stride_bytes : sizeof(ISlot);
    expected_requests[0] = (scenario >= 3 ? 1 : 8) * stride;
    const size_t resources = scenario >= 3 ? 1 : scenario == 0 ? 5 : 3;
    for (size_t i = 1; i <= resources; ++i) {
        expected_requests[i] = sizeof(Resource);
        expected_resources[i] = true;
    }
    expected_request_count = resources + 1;
    if (scenario == 4) {
        expected_requests[expected_request_count] = sizeof(Resource);
        expected_resources[expected_request_count++] = true;
    } else {
        expected_requests[expected_request_count++] =
            (scenario == 0 ? 16 : scenario == 3 ? 1 : 8) * stride;
        if (scenario == 3) {
            expected_requests[expected_request_count] = sizeof(Resource);
            expected_resources[expected_request_count++] = true;
        }
    }
    reset_ledger();
    require(backing_live_bytes == 0 && backing_live_allocations == 0,
            "native control retained a backing");
    backing_peak_bytes = backing_peak_allocations = control_bytes_initialized = 0;
}

static Observation completed_observation(void) {
    require(observation.allocated == observation.released && observation.live == 0,
            "not every admitted owner was returned exactly once");
    require(observation.requests == expected_request_count,
            "execution omitted a required allocation");
    return observation;
}

static Resource *try_resource(uint64_t id) {
    Resource *resource = wf_observe_allocate(sizeof *resource);
    if (!resource) return NULL;
    resource->id = id;
    for (size_t i = 0; i < 4; ++i) resource->data[i] = id * 17 + i;
    require(id < MAX_RESOURCES && !ledger.created[id], "native owner reused");
    ledger.created[id] = true;
    ledger.live[id] = resource;
    ++ledger.created_count;
    return resource;
}

static void drop_table(ITable *table) {
    i_drop_contents(table);
    i_release_backing(table);
}

static uint64_t native_state_hash(iRehash *state) {
    uint64_t hash = i_digest(&state->source) * 131 + i_digest(&state->target);
    hash = hash * 131 + state->source_next;
    hash = hash * 131 + state->next_probe;
    hash = hash * 131 + state->phase;
    hash *= 131;
    if (state->held_valid) hash += state->held.key;
    hash *= 131;
    if (state->held_valid) hash += state->held.resource->id;
    return hash;
}

static uint64_t native_contract(uint64_t seed, uint64_t scenario, size_t budget) {
    size_t initial = scenario >= 3 ? 1 : 8;
    size_t entries = scenario >= 3 ? 1 : scenario == 0 ? 5 : 3;
    uint64_t base = seed * 100;
    ITable source = {0};
    if (!i_init(&source, initial)) return 70;
    for (size_t ordinal = 0; ordinal < entries; ++ordinal) {
        uint64_t offset = ordinal * 8, kind = 1, expected = 0;
        if (scenario == 0) {
            if (ordinal == 2) { offset = 8; kind = 2; expected = base + 2; }
            if (ordinal == 3) {
                Resource *removed = i_remove(&source, seed);
                size_t work;
                require(removed != NULL, "native removal");
                require(removed->id == base + 1, "native removed owner");
                resource_drop(removed);
                Resource *found = i_find(&source, seed + 8, &work);
                require(found && found->id == base + 3 && work == 2,
                        "native lookup past tombstone");
                offset = 8; kind = 2; expected = base + 3;
            }
            if (ordinal == 4) offset = 16;
        }
        Resource *offered = try_resource(base + ordinal + 1);
        if (!offered) { drop_table(&source); return 70; }
        Resource *returned = NULL;
        size_t placed;
        PutResult result = i_put(&source, seed + offset, offered, &returned, &placed);
        require((kind == 1 && result == PUT_INSERTED) ||
                (kind == 2 && result == PUT_REPLACED), "native insertion kind");
        if (kind == 2) {
            require(returned->id == expected, "native displaced owner");
            resource_drop(returned);
        }
    }
    if (scenario == 4) {
        Resource *offered = try_resource(base + 2);
        if (!offered) { drop_table(&source); return 70; }
        Resource *returned = NULL;
        size_t placed;
        PutResult result = i_put(&source, seed + 1, offered, &returned, &placed);
        require(result == PUT_FULL && returned == NULL, "native full return");
        resource_drop(offered);
        uint64_t hash = i_digest(&source);
        drop_table(&source);
        return hash;
    }
    size_t target_capacity = scenario == 0 ? 16 : scenario == 3 ? 1 : 8;
    iRehash migration;
    uint64_t before = i_digest(&source);
    if (!i_begin(&source, target_capacity, &migration)) {
        require(i_digest(&source) == before, "native allocation changed source");
        drop_table(&source);
        return 70;
    }
    if (scenario == 3) {
        Resource *offered = try_resource(base + 2);
        if (!offered) { i_cleanup_rehash(&migration); return 70; }
        Resource *returned = NULL;
        size_t placed;
        PutResult result = i_put(&migration.target, seed + 1, offered, &returned, &placed);
        require(result == PUT_INSERTED, "native full target setup");
    }
    size_t total_work = 0, total_moved = 0;
    if (scenario == 2) {
        for (size_t round = 0; round < 14; ++round) {
            size_t work, moved;
            uint64_t before_idle = native_state_hash(&migration);
            i_step(&migration, 0, &work, &moved);
            require(native_state_hash(&migration) == before_idle && !work && !moved,
                    "native zero-budget state");
            i_step(&migration, 1, &work, &moved);
            require(work == 1, "native one-unit progress");
            total_work += work; total_moved += moved;
        }
    } else {
        i_step(&migration, budget, &total_work, &total_moved);
        if (scenario == 3) {
            size_t work, moved;
            uint64_t before_retry = native_state_hash(&migration);
            i_step(&migration, 99, &work, &moved);
            require(native_state_hash(&migration) == before_retry && !work && !moved,
                    "native blocked retry");
        }
    }
    uint64_t result = (native_state_hash(&migration) * 131 + total_work) * 131 + total_moved;
    i_cleanup_rehash(&migration);
    return result;
}

static void compare(uint64_t seed, uint64_t scenario, size_t budget) {
    observation_reset(scenario, false);
    uint64_t expected = native_contract(seed, scenario, budget);
    Observation native = completed_observation();
    observation_reset(scenario, true);
    uint64_t actual = wf_map_contract(seed, scenario, budget);
    if (actual != expected) {
        fprintf(stderr, "seed=%" PRIu64 " scenario=%" PRIu64 " budget=%zu"
                        " expected=%" PRIu64 " actual=%" PRIu64 "\n",
                seed, scenario, budget, expected, actual);
        exit(1);
    }
    Observation wf = completed_observation();
    require(wf.requests == native.requests && wf.allocated == native.allocated,
            "allocation ledger differs from native control");
    for (size_t i = 0; i < wf.allocated; ++i) {
        require(wf.allocations[i].id == native.allocations[i].id,
                "exact owner release ledger differs from native control");
        require(wf.allocations[i].is_resource == native.allocations[i].is_resource,
                "allocation role differs from native control");
        if (native.allocations[i].is_resource) {
            require(wf.allocations[i].bytes == sizeof(Resource), "resource extent changed");
            ++compared_owners;
        } else {
            uint64_t wf_payload = wf.allocations[i].bytes;
            require(wf_payload % wf_slot_stride_bytes == 0,
                    "released Whitefoot backing has a partial slot payload");
            require(native.allocations[i].bytes % sizeof(ISlot) == 0,
                    "released native backing has a partial slot payload");
            require(wf_payload / wf_slot_stride_bytes ==
                        native.allocations[i].bytes / sizeof(ISlot),
                    "released backing capacity differs from native control");
            ++compared_backings;
        }
    }
    ++compared_runs;
}

#ifdef BEHAVIOR_DEMOS
static void check_behavior_demos(void) {
    /* Both concrete Slot instances have a 24-byte target stride. Each first
     * request is the payload alone; later requests are scalar Box allocations
     * with their exact content extents. */
    const uint64_t extents[3][4] = {{96, 40, 0, 0}, {96, 8, 8, 40}, {48, 40, 40, 0}};
    const bool resources[3][4] = {
        {false, true, false, false},
        {false, false, false, true},
        {false, true, true, false},
    };
    const uint64_t owners[3][4] = {{0, 81, 0, 0}, {0, 9, 9, 81}, {0, 81, 82, 0}};
    const size_t counts[3] = {2, 4, 3};
    size_t executions = 0;
    for (size_t variant = 0; variant < 3; ++variant) {
        observation_reset(0, true);
        expected_request_count = counts[variant];
        memcpy(expected_requests, extents[variant], sizeof extents[variant]);
        memcpy(expected_resources, resources[variant], sizeof resources[variant]);
        uint8_t result = wf_behavior_contract(variant);
        Observation done = completed_observation();
        require(result == 0, "behavior result changed");
        require(done.requests == counts[variant], "behavior allocation path changed");
        require(done.allocated == counts[variant],
                "behavior did not return every owner");
        for (size_t i = 0; i < done.allocated; ++i)
            require(done.allocations[i].id == owners[variant][i],
                    "behavior exact release ledger changed");
        ++executions;
    }
    observation_reset(0, false);
    printf("behavior: %zu stateful, branded-key and hostile-equality executions; every owner and release checked\n", executions);
}
#endif

int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    for (uint64_t seed = 0; seed < 16; ++seed) {
        for (uint64_t scenario = 0; scenario < 5; ++scenario) {
            size_t stops = scenario == 1 ? 15 : 1;
            for (size_t stop = 0; stop < stops; ++stop) {
                size_t budget = scenario == 0 ? 100 : scenario == 3 ? 3 : stop;
                compare(seed, scenario, budget);
            }
        }
    }
    observation_reset(0, false);
    printf("owning-growth: %zu matched executions; %zu resource releases; %zu backing releases\n",
           compared_runs, compared_owners, compared_backings);
#ifdef BEHAVIOR_DEMOS
    check_behavior_demos();
#endif
    return 0;
}

extern int wf__floor_run(int, char **);
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
