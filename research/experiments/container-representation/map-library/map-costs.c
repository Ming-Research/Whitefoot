#if !defined(_WIN32)
#define _POSIX_C_SOURCE 200809L
#endif
#include <assert.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(WITH_WF)
#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <time.h>
#endif
#endif

#ifdef NDEBUG
#error "The map comparison requires its correctness checks"
#endif

#define NOINLINE __attribute__((noinline))
#ifdef RETAIN_HELPERS
#define HELPER NOINLINE
#define CONTRACT "retained"
#else
#define HELPER
#define CONTRACT "normal"
#endif

enum { EMPTY, DELETED, LIVE };
enum { INSERTED, REPLACED, REFUSED };
enum { HIT, MISS, REPLACE, CHURN, GROW, REHASH, SETUP, EDIT, PATH_COUNT };
enum { WORDS = 32, CEILING = 16384 };
#define EMPTY_INDEX UINT64_MAX
#define DELETED_INDEX (UINT64_MAX - UINT64_C(1))

typedef struct { uint64_t words[WORDS]; } Record;
typedef struct { uint64_t salt; bool collide; } HashEnv;
typedef struct { uint64_t ordered, sum, parity, count; } Digest;
typedef struct { bool found; uint64_t index; } Probe;
typedef struct { bool found; uint64_t identity; } Lookup;
typedef union {
    struct { size_t bytes; uint64_t magic; } data;
    max_align_t alignment;
} AllocationHeader;
typedef struct { size_t requests, releases, bytes, live, peak; } Ledger;
static Ledger ledger;
#if defined(WITH_WF)
static volatile uint64_t observed;
extern uint64_t wf_map_cost_sparse_word_trace(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_sparse_record_trace(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_dense_word_trace(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_dense_record_trace(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_slot_word_trace(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_slot_record_trace(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_staged_word_trace(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_staged_record_trace(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_library_word_trace(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_map_cost_library_record_trace(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
#endif

static void require(bool condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "map library costs: %s\n", message);
        exit(1);
    }
}

#if defined(ECO_MAP_CONTROL)
#if defined(ACCOUNT_ONLY)
extern void *wf_cost_allocate(uint64_t bytes);
extern void wf_cost_release(void *pointer);
#else
/* Practical C attribution image: ordinary allocation, without an observer
 * wrapper or a forced operation boundary. */
#define wf_cost_allocate malloc
#define wf_cost_release free
#endif
#else
NOINLINE void *wf_cost_allocate(uint64_t bytes) {
    require(bytes <= SIZE_MAX - sizeof(AllocationHeader), "allocation extent");
    AllocationHeader *header = malloc(sizeof *header + (size_t)bytes);
    require(header != NULL, "host allocation failure");
    header->data.bytes = (size_t)bytes;
    header->data.magic = UINT64_C(0x6d61706c69627279);
    ++ledger.requests;
    ledger.bytes += (size_t)bytes;
    ledger.live += (size_t)bytes;
    if (ledger.live > ledger.peak) ledger.peak = ledger.live;
    return header + 1;
}

NOINLINE void wf_cost_release(void *pointer) {
    if (!pointer) return;
    AllocationHeader *header = (AllocationHeader *)pointer - 1;
    require(header->data.magic == UINT64_C(0x6d61706c69627279), "allocation header signature");
    require(ledger.live >= header->data.bytes, "release extent");
    ledger.live -= header->data.bytes;
    ++ledger.releases;
    header->data.magic = 0;
    free(header);
}
#endif

static void reset_ledger(void) {
    require(ledger.live == 0, "owner remained live between traces");
    ledger = (Ledger){0};
}

static uint64_t mix(uint64_t value) {
    value ^= value >> 30;
    value *= UINT64_C(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value *= UINT64_C(0x94d049bb133111eb);
    return value ^ (value >> 31);
}

static HELPER uint64_t key_hash(const HashEnv *env, const uint64_t *key) {
    return env->collide ? 0 : mix(*key ^ env->salt);
}

static HELPER bool key_equal(const HashEnv *env, const uint64_t *left, const uint64_t *right) {
    (void)env;
    return *left == *right;
}

static uint64_t key_at(uint64_t index) { return index * 2 + 1; }
static uint64_t successor(uint64_t index, uint64_t capacity) {
    return index + 1 == capacity ? 0 : index + 1;
}
static uint64_t next_capacity(uint64_t capacity, uint64_t ceiling) {
    return capacity == 0 ? 1 : (capacity > ceiling / 2 ? ceiling : capacity * 2);
}
static void ordered(Digest *digest, uint64_t value) {
    digest->ordered = digest->ordered * UINT64_C(131) + value;
}
static void final_value(Digest *digest, uint64_t value) {
    digest->sum += value;
    digest->parity ^= mix(value);
    ++digest->count;
}
static uint64_t finish(Digest digest) {
    return mix(digest.ordered) ^ mix(digest.sum) ^ digest.parity
        ^ (digest.count * UINT64_C(0x9e3779b97f4a7c15));
}

static uint64_t word_make(uint64_t seed) { return seed; }
static uint64_t word_identity(const uint64_t *value) { return *value; }
static uint64_t word_content(uint64_t key, uint64_t value) {
    return (key * UINT64_C(131)) + value;
}
static Record record_make(uint64_t seed) {
    Record value;
    for (size_t i = 0; i < WORDS; ++i) value.words[i] = seed + i;
    return value;
}
static uint64_t record_identity(const Record *value) { return value->words[0]; }
static uint64_t record_content(uint64_t key, Record value) {
    for (size_t i = 0; i < WORDS; ++i) key = key * UINT64_C(131) + value.words[i];
    return key;
}
static uint64_t word_increment(uint64_t *value) { return ++*value; }
static uint64_t record_increment(Record *value) { return ++value->words[0]; }

#define CALLBACKS(B, T)                                                   \
static HELPER uint64_t B##_observe(Digest *env, const uint64_t *key, const T *value) { \
    (void)env; (void)key; return B##_identity(value);                      \
}                                                                         \
static HELPER void B##_consume(Digest *env, uint64_t key, T value) {       \
    final_value(env, B##_content(key, value));                            \
}                                                                         \
static HELPER uint64_t B##_edit(Digest *env, const uint64_t *key, T *value) { \
    (void)env; (void)key; return B##_increment(value);                     \
}
CALLBACKS(word, uint64_t)
CALLBACKS(record, Record)

#define PAIR_TYPES(B, T)                                                    \
typedef struct { uint64_t key; T value; } B##_Pair;                          \
typedef struct { uint32_t kind; B##_Pair owner; } B##_Put;                   \
typedef struct { bool found; B##_Pair owner; } B##_Removed;
PAIR_TYPES(word, uint64_t)
PAIR_TYPES(record, Record)
_Static_assert(sizeof(Record) == 256, "wide inline value");
_Static_assert(sizeof(word_Put) == 24 && sizeof(record_Put) == 272, "compact Put result");

/* This floor initializes tags, never the inactive payload. Rehash moves each
 * live pair directly into a new table; it is not the WF permutation algorithm. */
/* Compile-time cells/direction keep the original ascending enum control and
 * isolate descending migration and single-slot layout with the same native
 * algorithm. SET never initializes an inactive Pair payload. */
#define ENUM_CELL(B) struct { uint32_t tag; B##_Pair pair; }
#define SINGLE_CELL(B) struct { uint64_t length; B##_Pair pair; bool deleted; }
#define ENUM_TAG(cell) ((cell)->tag)
#define ENUM_SET(cell, value) ((cell)->tag = (value))
#define SINGLE_TAG(cell) ((cell)->length != 0 ? LIVE : ((cell)->deleted ? DELETED : EMPTY))
#define SINGLE_SET(cell, value) do { (cell)->length = (value) == LIVE; (cell)->deleted = (value) == DELETED; } while (0)

#define SPARSE_NATIVE_LAYOUT(P, B, T, CELL, TAG, SET, DESCENDING, ZERO_NOOP, LIMIT)                                       \
typedef CELL(B) P##_Cell;                    \
typedef struct { uint64_t length, capacity; P##_Cell cells[]; } P##_Block;   \
typedef struct { P##_Block *slots; uint64_t count; } P##_Map;                \
_Static_assert(sizeof(P##_Block) == 16, "sparse backing header");           \
static P##_Block *P##_block(uint64_t capacity) {                            \
    P##_Block *block = wf_cost_allocate(sizeof(*block)                      \
                                        + capacity * sizeof(P##_Cell));    \
    block->length = block->capacity = capacity;                            \
    for (uint64_t i = 0; i < capacity; ++i) SET(&block->cells[i], EMPTY);     \
    return block;                                                          \
}                                                                          \
static HELPER P##_Map P##_new(uint64_t capacity) {                          \
    return (P##_Map){P##_block(capacity), 0};                               \
}                                                                          \
static Probe P##_probe(const P##_Map *map, const HashEnv *env,               \
                       const uint64_t *key) {                              \
    uint64_t capacity = map->slots->capacity, vacancy = capacity;           \
    if (capacity == 0) return (Probe){false, capacity};                     \
    uint64_t index = key_hash(env, key) % capacity;                         \
    for (uint64_t visited = 0; visited < capacity; ++visited) {              \
        const P##_Cell *cell = &map->slots->cells[index];                   \
        if (TAG(cell) == LIVE && key_equal(env, &cell->pair.key, key))       \
            return (Probe){true, index};                                   \
        if (TAG(cell) != LIVE && vacancy == capacity) vacancy = index;     \
        if (TAG(cell) == EMPTY) break;                                     \
        index = successor(index, capacity);                               \
    }                                                                      \
    return (Probe){false, vacancy};                                        \
}                                                                          \
static bool P##_rehash_to(P##_Map *map, const HashEnv *env, uint64_t target) { \
    if (target > (LIMIT) || target < map->count) return false;              \
    if ((ZERO_NOOP) && target == 0) return true;                        \
    P##_Block *next = P##_block(target);                                   \
    P##_Block *old = map->slots;                                           \
    for (uint64_t position = 0; position < old->capacity; ++position) { \
        uint64_t i = (DESCENDING) ? old->capacity - 1 - position : position;                          \
        if (TAG(&old->cells[i]) != LIVE) continue;                           \
        uint64_t index = key_hash(env, &old->cells[i].pair.key) % target;    \
        while (TAG(&next->cells[index]) == LIVE)                             \
            index = successor(index, target);                             \
        next->cells[index].pair = old->cells[i].pair;                       \
        SET(&next->cells[index], LIVE);                                    \
    }                                                                      \
    wf_cost_release(old);                                                  \
    map->slots = next;                                                     \
    return true;                                                           \
}                                                                          \
static HELPER bool P##_reserve(P##_Map *map, const HashEnv *env, uint64_t target) { \
    if (target > (LIMIT)) return false;                                    \
    if (target <= map->slots->capacity) return true;                        \
    return P##_rehash_to(map, env, target);                                \
}                                                                          \
static HELPER bool P##_rehash(P##_Map *map, const HashEnv *env) {            \
    return P##_rehash_to(map, env, map->slots->capacity);                   \
}                                                                          \
static HELPER B##_Put P##_put(P##_Map *map, const HashEnv *env, B##_Pair offered) { \
    for (;;) {                                                             \
        Probe probe = P##_probe(map, env, &offered.key);                    \
        if (probe.found) {                                                 \
            B##_Pair previous = map->slots->cells[probe.index].pair;       \
            map->slots->cells[probe.index].pair = offered;                 \
            return (B##_Put){REPLACED, previous};                          \
        }                                                                  \
        if (probe.index < map->slots->capacity) {                          \
            map->slots->cells[probe.index].pair = offered;                 \
            SET(&map->slots->cells[probe.index], LIVE);                     \
            ++map->count;                                                  \
            B##_Put result; result.kind = INSERTED; return result;        \
        }                                                                  \
        if (map->slots->capacity == (LIMIT))                               \
            return (B##_Put){REFUSED, offered};                            \
        uint64_t target = next_capacity(map->slots->capacity, (LIMIT));     \
        if (!P##_reserve(map, env, target)) return (B##_Put){REFUSED, offered}; \
    }                                                                      \
}                                                                          \
static HELPER B##_Removed P##_remove(P##_Map *map, const HashEnv *env,       \
                                     const uint64_t *key) {                \
    Probe probe = P##_probe(map, env, key);                                \
    if (!probe.found) { B##_Removed result; result.found = false; return result; } \
    B##_Pair previous = map->slots->cells[probe.index].pair;                \
    SET(&map->slots->cells[probe.index], DELETED);                           \
    --map->count;                                                          \
    return (B##_Removed){true, previous};                                  \
}                                                                          \
static HELPER Lookup P##_lookup(const P##_Map *map, const HashEnv *env,      \
                                const uint64_t *key, Digest *digest) {     \
    Probe probe = P##_probe(map, env, key);                                \
    if (!probe.found) return (Lookup){false, 0};                            \
    const B##_Pair *pair = &map->slots->cells[probe.index].pair;            \
    return (Lookup){true, B##_observe(digest, &pair->key, &pair->value)};   \
}                                                                          \
static HELPER Lookup P##_edit(P##_Map *map, const HashEnv *env, const uint64_t *key, Digest *digest) { \
    Probe probe = P##_probe(map, env, key);                                \
    if (!probe.found) return (Lookup){false, 0};                           \
    B##_Pair *pair = &map->slots->cells[probe.index].pair;                  \
    return (Lookup){true, B##_edit(digest, &pair->key, &pair->value)};       \
}                                                                          \
static HELPER void P##_free(P##_Map map, Digest *digest) {                  \
    for (uint64_t i = 0; i < map.slots->capacity; ++i)                      \
        if (TAG(&map.slots->cells[i]) == LIVE)                               \
            B##_consume(digest, map.slots->cells[i].pair.key,              \
                        map.slots->cells[i].pair.value);                  \
    wf_cost_release(map.slots);                                            \
}
#define SPARSE_NATIVE(P, B, T, LIMIT) \
    SPARSE_NATIVE_LAYOUT(P, B, T, ENUM_CELL, ENUM_TAG, ENUM_SET, false, false, LIMIT)

SPARSE_NATIVE(word_sparse, word, uint64_t, CEILING)
SPARSE_NATIVE(record_sparse, record, Record, CEILING)
SPARSE_NATIVE(word_sparse_small, word, uint64_t, 3)
SPARSE_NATIVE(record_sparse_small, record, Record, 3)

/* Descending enum retains the ascending control's zero-capacity allocation.
 * Single-slot rebuild at zero is the candidate's explicit no-allocation path. */
SPARSE_NATIVE_LAYOUT(word_descending, word, uint64_t, ENUM_CELL, ENUM_TAG, ENUM_SET, true, false, CEILING)
SPARSE_NATIVE_LAYOUT(record_descending, record, Record, ENUM_CELL, ENUM_TAG, ENUM_SET, true, false, CEILING)
SPARSE_NATIVE_LAYOUT(word_single, word, uint64_t, SINGLE_CELL, SINGLE_TAG, SINGLE_SET, true, true, CEILING)
SPARSE_NATIVE_LAYOUT(record_single, record, Record, SINGLE_CELL, SINGLE_TAG, SINGLE_SET, true, true, CEILING)
_Static_assert(sizeof(word_single_Cell) == 32 && sizeof(record_single_Cell) == 280, "single-slot cell strides");
_Static_assert(offsetof(word_single_Cell, pair) == 8 && offsetof(record_single_Cell, pair) == 8, "single-slot Pair offset");
_Static_assert(offsetof(word_single_Cell, deleted) == 24 && offsetof(record_single_Cell, deleted) == 272, "single-slot Bool offset");

typedef struct { uint32_t tag; uint64_t index; } TaggedIndex;
static inline uint64_t compact_read(const uint64_t *slot) { return *slot; }
static inline void compact_write(uint64_t *slot, uint64_t index) { *slot = index; }
static inline uint64_t tagged_read(const TaggedIndex *slot) {
    return slot->tag == LIVE ? slot->index
        : (slot->tag == EMPTY ? EMPTY_INDEX : DELETED_INDEX);
}
static inline void tagged_write(TaggedIndex *slot, uint64_t index) {
    if (index == EMPTY_INDEX) slot->tag = EMPTY;
    else if (index == DELETED_INDEX) slot->tag = DELETED;
    else { slot->tag = LIVE; slot->index = index; }
}

/* The two dense controls share algorithm and result ABI. Only the index-cell
 * representation differs. Both charge reserved entries and reverse repair. */
#define DENSE_NATIVE(P, B, T, I, READ, WRITE, LIMIT)                        \
typedef struct { uint64_t bucket; B##_Pair pair; } P##_Entry;               \
typedef struct { uint64_t length, capacity; P##_Entry entries[]; } P##_Entries; \
typedef struct { uint64_t length; I slots[]; } P##_Index;                    \
typedef struct { P##_Entries *entries; P##_Index *index; } P##_Map;         \
_Static_assert(sizeof(P##_Entries) == 16, "dense backing header");          \
_Static_assert(sizeof(P##_Index) == 8, "index backing header");             \
static P##_Entries *P##_entries(uint64_t capacity) {                       \
    P##_Entries *entries = wf_cost_allocate(sizeof(*entries)                \
                                             + capacity * sizeof(P##_Entry)); \
    entries->length = 0; entries->capacity = capacity; return entries;      \
}                                                                          \
static P##_Index *P##_index(uint64_t capacity) {                           \
    P##_Index *index = wf_cost_allocate(sizeof(*index) + capacity * sizeof(I)); \
    index->length = capacity;                                              \
    for (uint64_t i = 0; i < capacity; ++i) WRITE(&index->slots[i], EMPTY_INDEX); \
    return index;                                                          \
}                                                                          \
static HELPER P##_Map P##_new(uint64_t capacity) {                          \
    P##_Entries *entries = P##_entries(capacity);                           \
    return (P##_Map){entries, P##_index(capacity)};                          \
}                                                                          \
static Probe P##_probe(const P##_Map *map, const HashEnv *env, const uint64_t *key) { \
    uint64_t capacity = map->index->length, vacancy = capacity;             \
    if (capacity == 0) return (Probe){false, capacity};                     \
    uint64_t bucket = key_hash(env, key) % capacity;                        \
    for (uint64_t visited = 0; visited < capacity; ++visited) {              \
        uint64_t index = READ(&map->index->slots[bucket]);                  \
        if (index < DELETED_INDEX                                          \
            && key_equal(env, &map->entries->entries[index].pair.key, key)) \
            return (Probe){true, bucket};                                  \
        if (index >= DELETED_INDEX && vacancy == capacity) vacancy = bucket; \
        if (index == EMPTY_INDEX) break;                                   \
        bucket = successor(bucket, capacity);                             \
    }                                                                      \
    return (Probe){false, vacancy};                                        \
}                                                                          \
static bool P##_rehash_to(P##_Map *map, const HashEnv *env, uint64_t target) { \
    if (target > (LIMIT) || target < map->entries->length) return false;    \
    P##_Index *next = P##_index(target);                                   \
    if (target > map->entries->capacity) {                                 \
        P##_Entries *entries = P##_entries(target);                        \
        entries->length = map->entries->length;                            \
        memcpy(entries->entries, map->entries->entries,                    \
               entries->length * sizeof(P##_Entry));                      \
        wf_cost_release(map->entries);                                    \
        map->entries = entries;                                            \
    }                                                                      \
    for (uint64_t i = 0; i < map->entries->length; ++i) {                   \
        P##_Entry *entry = &map->entries->entries[i];                      \
        uint64_t bucket = key_hash(env, &entry->pair.key) % target;         \
        while (READ(&next->slots[bucket]) != EMPTY_INDEX)                   \
            bucket = successor(bucket, target);                           \
        WRITE(&next->slots[bucket], i); entry->bucket = bucket;             \
    }                                                                      \
    wf_cost_release(map->index); map->index = next;                        \
    return true;                                                           \
}                                                                          \
static HELPER bool P##_reserve(P##_Map *map, const HashEnv *env, uint64_t target) { \
    if (target > (LIMIT)) return false;                                    \
    if (target <= map->index->length) return true;                          \
    return P##_rehash_to(map, env, target);                                \
}                                                                          \
static HELPER bool P##_rehash(P##_Map *map, const HashEnv *env) {            \
    return P##_rehash_to(map, env, map->index->length);                     \
}                                                                          \
static HELPER B##_Put P##_put(P##_Map *map, const HashEnv *env, B##_Pair offered) { \
    for (;;) {                                                             \
        Probe probe = P##_probe(map, env, &offered.key);                    \
        if (probe.found) {                                                 \
            uint64_t index = READ(&map->index->slots[probe.index]);        \
            B##_Pair previous = map->entries->entries[index].pair;        \
            map->entries->entries[index].pair = offered;                   \
            return (B##_Put){REPLACED, previous};                          \
        }                                                                  \
        if (probe.index < map->index->length) {                            \
            uint64_t index = map->entries->length++;                      \
            map->entries->entries[index] = (P##_Entry){probe.index, offered}; \
            WRITE(&map->index->slots[probe.index], index);                  \
            B##_Put result; result.kind = INSERTED; return result;        \
        }                                                                  \
        if (map->index->length == (LIMIT))                                 \
            return (B##_Put){REFUSED, offered};                            \
        uint64_t target = next_capacity(map->index->length, (LIMIT));       \
        if (!P##_reserve(map, env, target)) return (B##_Put){REFUSED, offered}; \
    }                                                                      \
}                                                                          \
static HELPER B##_Removed P##_remove(P##_Map *map, const HashEnv *env, const uint64_t *key) { \
    Probe probe = P##_probe(map, env, key);                                \
    if (!probe.found) { B##_Removed result; result.found = false; return result; } \
    uint64_t index = READ(&map->index->slots[probe.index]);                 \
    B##_Pair previous = map->entries->entries[index].pair;                  \
    uint64_t last = --map->entries->length;                                \
    if (index != last) {                                                   \
        map->entries->entries[index] = map->entries->entries[last];        \
        WRITE(&map->index->slots[map->entries->entries[index].bucket], index); \
    }                                                                      \
    WRITE(&map->index->slots[probe.index], DELETED_INDEX);                  \
    return (B##_Removed){true, previous};                                  \
}                                                                          \
static HELPER Lookup P##_lookup(const P##_Map *map, const HashEnv *env, const uint64_t *key, Digest *digest) { \
    Probe probe = P##_probe(map, env, key);                                \
    if (!probe.found) return (Lookup){false, 0};                            \
    uint64_t index = READ(&map->index->slots[probe.index]);                 \
    const B##_Pair *pair = &map->entries->entries[index].pair;             \
    return (Lookup){true, B##_observe(digest, &pair->key, &pair->value)};   \
}                                                                          \
static HELPER Lookup P##_edit(P##_Map *map, const HashEnv *env, const uint64_t *key, Digest *digest) { \
    Probe probe = P##_probe(map, env, key);                                \
    if (!probe.found) return (Lookup){false, 0};                           \
    uint64_t index = READ(&map->index->slots[probe.index]);                 \
    B##_Pair *pair = &map->entries->entries[index].pair;                    \
    return (Lookup){true, B##_edit(digest, &pair->key, &pair->value)};       \
}                                                                          \
static HELPER void P##_free(P##_Map map, Digest *digest) {                  \
    for (uint64_t i = 0; i < map.entries->length; ++i)                      \
        B##_consume(digest, map.entries->entries[i].pair.key,              \
                    map.entries->entries[i].pair.value);                  \
    wf_cost_release(map.entries); wf_cost_release(map.index);              \
}

DENSE_NATIVE(word_tagged, word, uint64_t, TaggedIndex, tagged_read, tagged_write, CEILING)
DENSE_NATIVE(record_tagged, record, Record, TaggedIndex, tagged_read, tagged_write, CEILING)
DENSE_NATIVE(word_compact, word, uint64_t, uint64_t, compact_read, compact_write, CEILING)
DENSE_NATIVE(record_compact, record, Record, uint64_t, compact_read, compact_write, CEILING)
DENSE_NATIVE(word_tagged_small, word, uint64_t, TaggedIndex, tagged_read, tagged_write, 3)
DENSE_NATIVE(record_tagged_small, record, Record, TaggedIndex, tagged_read, tagged_write, 3)
DENSE_NATIVE(word_compact_small, word, uint64_t, uint64_t, compact_read, compact_write, 3)
DENSE_NATIVE(record_compact_small, record, Record, uint64_t, compact_read, compact_write, 3)

typedef struct { uint64_t length; uint64_t values[]; } WordArray;
typedef struct { uint64_t length; bool values[]; } BoolArray;
typedef struct { WordArray *destinations; bool complete; } PlacementPlan;
_Static_assert(sizeof(WordArray) == 8 && sizeof(BoolArray) == 8, "Array headers");
_Static_assert(sizeof(bool) == 1, "Bool metadata stride");

/* Private helpers remain ordinarily optimizable in both lowering modes.
 * Native floors use their simpler equivalent probe loops. */
static uint64_t source_probe(uint64_t home, uint64_t step, uint64_t count) {
    uint64_t until_wrap = count - home;
    return step < until_wrap ? home + step : step - until_wrap;
}
static Probe claim_destination(BoolArray *used, uint64_t hash) {
    for (uint64_t step = 0; step < used->length; ++step) {
        uint64_t index = source_probe(hash % used->length, step, used->length);
        if (!used->values[index]) {
            used->values[index] = true;
            return (Probe){true, index};
        }
    }
    return (Probe){false, 0};
}

/* The source public put forwards the bounded result, and retries once only
 * after an actual Full outcome. Compact C results share one Pair payload;
 * that ABI difference from the WF three-variant product is intentional and
 * must be attributed separately from this algorithm correspondence. */
#define MATCHED_GROWING_API(P, B, LIMIT)                                   \
static HELPER bool P##_reserve(P##_Map *map, const HashEnv *env, uint64_t target) { \
    if (target > (LIMIT)) return false;                                    \
    if (target <= P##_capacity(map)) return true;                          \
    return P##_rebuild(map, env, target);                                  \
}                                                                         \
static HELPER bool P##_rehash(P##_Map *map, const HashEnv *env) {           \
    return P##_rebuild(map, env, P##_capacity(map));                        \
}                                                                         \
static HELPER B##_Put P##_put(P##_Map *map, const HashEnv *env, B##_Pair offered) { \
    B##_Put attempted = P##_try_put(map, env, offered);                     \
    if (attempted.kind != REFUSED) return attempted;                       \
    uint64_t capacity = P##_capacity(map);                                 \
    if (capacity >= (LIMIT)) return attempted;                             \
    if (!P##_reserve(map, env, next_capacity(capacity, (LIMIT))))           \
        return attempted;                                                  \
    return P##_try_put(map, env, attempted.owner);                         \
}

enum { ACTION_STOP, ACTION_SKIP, ACTION_MISS, ACTION_MATCH };

/* Compile-time policy selects either the admitted two-Array plan/permutation
 * or descending direct migration through one local pending Pair. All other
 * operations and public boundaries are shared between the source controls.
 * memcpy moves object representations; inactive payload bytes are never read
 * as values and are not artificially initialized for the C control. */
#define SPARSE_MATCHED(P, N, B, T, DIRECT, LIMIT)                          \
typedef N##_Cell P##_Cell;                                                 \
typedef N##_Block P##_Block;                                               \
typedef N##_Map P##_Map;                                                   \
static inline uint64_t P##_capacity(const P##_Map *map) { return map->slots->length; } \
static inline void P##_swap(P##_Cell *first, P##_Cell *second) {           \
    P##_Cell temporary;                                                    \
    memcpy(&temporary, first, sizeof temporary);                          \
    memmove(first, second, sizeof temporary);                              \
    memcpy(second, &temporary, sizeof temporary);                          \
}                                                                         \
static void P##_extend(P##_Map *map, uint64_t target) {                    \
    P##_Block *old = map->slots;                                           \
    if (target > old->capacity) {                                         \
        P##_Block *next = wf_cost_allocate(16 + target * sizeof(P##_Cell)); \
        next->length = old->length; next->capacity = target;               \
        memcpy(next->cells, old->cells, old->length * sizeof(P##_Cell));    \
        wf_cost_release(old); map->slots = next;                           \
    }                                                                     \
    while (map->slots->length < target) {                                 \
        map->slots->cells[map->slots->length].tag = EMPTY;                  \
        ++map->slots->length;                                              \
    }                                                                     \
}                                                                         \
static HELPER P##_Map P##_new(uint64_t capacity) {                         \
    P##_Block *block = wf_cost_allocate(16 + capacity * sizeof(P##_Cell)); \
    block->length = 0; block->capacity = capacity;                         \
    P##_Map result = {block, 0}; P##_extend(&result, capacity); return result; \
}                                                                         \
static unsigned P##_action(const P##_Cell *slot, const HashEnv *env, const uint64_t *key) { \
    if (slot->tag == EMPTY) return ACTION_STOP;                            \
    if (slot->tag == DELETED) return ACTION_SKIP;                          \
    return key_equal(env, &slot->pair.key, key) ? ACTION_MATCH : ACTION_MISS; \
}                                                                         \
static Probe P##_find(const P##_Map *map, const HashEnv *env, const uint64_t *key) { \
    uint64_t hash = key_hash(env, key), count = map->slots->length;         \
    for (uint64_t step = 0; step < count; ++step) {                        \
        uint64_t index = source_probe(hash % count, step, count);          \
        unsigned action = P##_action(&map->slots->cells[index], env, key); \
        if (action == ACTION_MATCH) return (Probe){true, index};           \
        if (action == ACTION_STOP) break;                                 \
    }                                                                     \
    return (Probe){false, 0};                                              \
}                                                                         \
static B##_Put P##_exchange(P##_Map *map, uint64_t index, B##_Pair pair) {    \
    P##_Cell offered = {.tag = LIVE, .pair = pair};                        \
    P##_swap(&map->slots->cells[index], &offered);                          \
    if (offered.tag == LIVE) return (B##_Put){REPLACED, offered.pair};     \
    B##_Put result; result.kind = INSERTED; return result;                \
}                                                                         \
static B##_Put P##_try_put(P##_Map *map, const HashEnv *env, B##_Pair pair) { \
    uint64_t hash = key_hash(env, &pair.key), count = map->slots->length;   \
    uint64_t available = count;                                           \
    for (uint64_t step = 0; step < count; ++step) {                        \
        uint64_t index = source_probe(hash % count, step, count);          \
        unsigned action = P##_action(&map->slots->cells[index], env, &pair.key); \
        if (action == ACTION_MATCH) return P##_exchange(map, index, pair); \
        if ((action == ACTION_STOP || action == ACTION_SKIP) && available == count) \
            available = index;                                            \
        if (action == ACTION_STOP) break;                                 \
    }                                                                     \
    if (available < count && map->count < (LIMIT)) {                      \
        ++map->count; return P##_exchange(map, available, pair);           \
    }                                                                     \
    return (B##_Put){REFUSED, pair};                                       \
}                                                                         \
static PlacementPlan P##_plan(const P##_Map *map, const HashEnv *env, uint64_t target) { \
    WordArray *plan = wf_cost_allocate(8 + target * sizeof(uint64_t));     \
    plan->length = target;                                                \
    for (uint64_t i = 0; i < target; ++i) plan->values[i] = target;        \
    if (map->slots->length > target) return (PlacementPlan){plan, false}; \
    BoolArray *used = wf_cost_allocate(8 + target * sizeof(bool));         \
    used->length = target;                                                \
    for (uint64_t i = 0; i < target; ++i) used->values[i] = false;         \
    for (uint64_t i = 0; i < map->slots->length; ++i) {                    \
        const P##_Cell *slot = &map->slots->cells[i];                      \
        if (slot->tag != LIVE) continue;                                 \
        Probe claimed = claim_destination(used, key_hash(env, &slot->pair.key)); \
        if (!claimed.found) {                                             \
            wf_cost_release(used); return (PlacementPlan){plan, false};   \
        }                                                                 \
        plan->values[i] = claimed.index;                                  \
    }                                                                     \
    wf_cost_release(used); return (PlacementPlan){plan, true};             \
}                                                                         \
static bool P##_is_deleted(const P##_Cell *slot) { return slot->tag == DELETED; } \
static void P##_canonicalize(P##_Map *map, uint64_t index) {               \
    uint64_t last = map->slots->length - 1;                               \
    P##_swap(&map->slots->cells[index], &map->slots->cells[last]);          \
    P##_Cell selected; memcpy(&selected, &map->slots->cells[last], sizeof selected); \
    --map->slots->length;                                                 \
    if (selected.tag == LIVE) memcpy(&map->slots->cells[last], &selected, sizeof selected); \
    else map->slots->cells[last].tag = EMPTY;                             \
    ++map->slots->length;                                                 \
    P##_swap(&map->slots->cells[index], &map->slots->cells[last]);          \
}                                                                         \
static void P##_clear_deleted(P##_Map *map, uint64_t index) {              \
    if (P##_is_deleted(&map->slots->cells[index])) P##_canonicalize(map, index); \
}                                                                         \
static void P##_apply(P##_Map *map, WordArray *plan) {                     \
    uint64_t count = map->slots->length, cursor = 0;                      \
    for (uint64_t pass = 0; pass < 2; ++pass)                             \
        for (uint64_t tick = 0; tick < count; ++tick) {                   \
            if (cursor >= count) continue;                               \
            uint64_t target = plan->values[cursor];                       \
            if (target < count && target != cursor) {                   \
                P##_swap(&map->slots->cells[cursor], &map->slots->cells[target]); \
                plan->values[cursor] = plan->values[target];              \
                plan->values[target] = target;                           \
            } else ++cursor;                                             \
        }                                                                 \
}                                                                         \
static bool P##_rebuild(P##_Map *map, const HashEnv *env, uint64_t target) { \
    uint64_t count = map->slots->length;                                  \
    if (target > (LIMIT) || target < count) return false;                 \
    if (DIRECT) {                                                         \
        if (target == 0) return true;                                    \
        P##_Block *block = wf_cost_allocate(16 + target * sizeof(P##_Cell)); \
        block->length = 0; block->capacity = target;                      \
        P##_Map fresh = {block, 0}; P##_extend(&fresh, target);            \
        P##_Block *old = map->slots; map->slots = fresh.slots;            \
        while (old->length != 0) {                                       \
            P##_Cell cell;                                               \
            memcpy(&cell, &old->cells[--old->length], sizeof cell);       \
            if (cell.tag != LIVE) continue;                              \
            struct { uint64_t length; B##_Pair pair; } pending;          \
            pending.length = 0;                                         \
            memcpy(&pending.pair, &cell.pair, sizeof pending.pair);      \
            pending.length = 1;                                         \
            while (pending.length != 0) {                               \
                uint64_t hash = key_hash(env, &pending.pair.key);        \
                uint64_t extent = map->slots->length;                    \
                for (uint64_t step = 0; step < extent; ++step) {         \
                    uint64_t index = source_probe(hash % extent, step, extent); \
                    if (map->slots->cells[index].tag != LIVE) {          \
                        B##_Pair offered;                               \
                        memcpy(&offered, &pending.pair, sizeof offered); \
                        --pending.length;                               \
                        B##_Put exchanged = P##_exchange(map, index, offered); \
                        if (exchanged.kind != INSERTED) {                \
                            memcpy(&pending.pair, &exchanged.owner, sizeof pending.pair); \
                            ++pending.length;                           \
                        }                                                \
                        break;                                           \
                    }                                                    \
                }                                                        \
            }                                                            \
        }                                                                \
        wf_cost_release(old); return true;                              \
    }                                                                    \
    PlacementPlan plan = P##_plan(map, env, target);                       \
    if (!plan.complete) { wf_cost_release(plan.destinations); return false; } \
    P##_extend(map, target);                                               \
    for (uint64_t i = 0; i < count; ++i) P##_clear_deleted(map, i);        \
    P##_apply(map, plan.destinations);                                     \
    wf_cost_release(plan.destinations); return true;                      \
}                                                                         \
static HELPER B##_Removed P##_remove(P##_Map *map, const HashEnv *env, const uint64_t *key) { \
    if (map->count == 0) { B##_Removed result; result.found = false; return result; } \
    Probe found = P##_find(map, env, key);                                \
    if (!found.found) { B##_Removed result; result.found = false; return result; } \
    P##_Cell removed; removed.tag = DELETED;                              \
    P##_swap(&map->slots->cells[found.index], &removed); --map->count;      \
    if (removed.tag == LIVE) return (B##_Removed){true, removed.pair};     \
    B##_Removed result; result.found = false; return result;              \
}                                                                         \
static HELPER Lookup P##_lookup(const P##_Map *map, const HashEnv *env, const uint64_t *key, Digest *digest) { \
    Probe found = P##_find(map, env, key);                                \
    if (!found.found) return (Lookup){false, 0};                           \
    const P##_Cell *slot = &map->slots->cells[found.index];                 \
    if (slot->tag != LIVE) return (Lookup){false, 0};                      \
    return (Lookup){true, B##_observe(digest, &slot->pair.key, &slot->pair.value)}; \
}                                                                         \
static HELPER Lookup P##_edit(P##_Map *map, const HashEnv *env, const uint64_t *key, Digest *digest) { \
    Probe found = P##_find(map, env, key);                                \
    if (!found.found) return (Lookup){false, 0};                           \
    P##_Cell *slot = &map->slots->cells[found.index];                       \
    if (slot->tag != LIVE) return (Lookup){false, 0};                      \
    return (Lookup){true, B##_edit(digest, &slot->pair.key, &slot->pair.value)}; \
}                                                                         \
static HELPER void P##_free(P##_Map map, Digest *digest) {                 \
    while (map.slots->length != 0) {                                     \
        P##_Cell slot; memcpy(&slot, &map.slots->cells[--map.slots->length], sizeof slot); \
        if (slot.tag == LIVE) B##_consume(digest, slot.pair.key, slot.pair.value); \
    }                                                                     \
    wf_cost_release(map.slots);                                            \
}                                                                         \
MATCHED_GROWING_API(P, B, LIMIT)

SPARSE_MATCHED(word_planned, word_sparse, word, uint64_t, false, CEILING)
SPARSE_MATCHED(record_planned, record_sparse, record, Record, false, CEILING)
SPARSE_MATCHED(word_planned_small, word_sparse_small, word, uint64_t, false, 3)
SPARSE_MATCHED(record_planned_small, record_sparse_small, record, Record, false, 3)
SPARSE_MATCHED(word_staged, word_sparse, word, uint64_t, true, CEILING)
SPARSE_MATCHED(record_staged, record_sparse, record, Record, true, CEILING)

/* Dense source planning publishes only after all indexes exist. Growth copies
 * live dense entries, then a full sparse-table scan installs reverse indexes.
 * Native floors instead repair each reverse index while placing that entry. */
#define DENSE_MATCHED(P, N, B, T, LIMIT)                                   \
typedef N##_Entry P##_Entry;                                               \
typedef N##_Entries P##_Entries;                                           \
typedef N##_Index P##_Index;                                               \
typedef N##_Map P##_Map;                                                   \
static inline uint64_t P##_capacity(const P##_Map *map) { return map->index->length; } \
static inline void P##_swap(P##_Entry *first, P##_Entry *second) {         \
    P##_Entry temporary;                                                   \
    memcpy(&temporary, first, sizeof temporary);                          \
    memmove(first, second, sizeof temporary);                              \
    memcpy(second, &temporary, sizeof temporary);                          \
}                                                                         \
static HELPER P##_Map P##_new(uint64_t capacity) {                         \
    P##_Entries *entries = N##_entries(capacity);                          \
    return (P##_Map){entries, N##_index(capacity)};                        \
}                                                                         \
static Probe P##_find(const P##_Map *map, const HashEnv *env, const uint64_t *key) { \
    uint64_t hash = key_hash(env, key), count = map->index->length;         \
    for (uint64_t step = 0; step < count; ++step) {                        \
        uint64_t bucket = source_probe(hash % count, step, count);         \
        uint64_t index = tagged_read(&map->index->slots[bucket]);          \
        if (index == EMPTY_INDEX) break;                                 \
        if (index < map->entries->length                                  \
            && key_equal(env, &map->entries->entries[index].pair.key, key)) \
            return (Probe){true, index};                                  \
    }                                                                     \
    return (Probe){false, 0};                                              \
}                                                                         \
static B##_Put P##_try_put(P##_Map *map, const HashEnv *env, B##_Pair pair) { \
    uint64_t hash = key_hash(env, &pair.key), count = map->index->length;   \
    uint64_t available = count;                                           \
    for (uint64_t step = 0; step < count; ++step) {                        \
        uint64_t bucket = source_probe(hash % count, step, count);         \
        uint64_t index = tagged_read(&map->index->slots[bucket]);          \
        if (index >= DELETED_INDEX) {                                     \
            if (available == count) available = bucket;                   \
            if (index == EMPTY_INDEX) break;                             \
        } else if (index < map->entries->length                            \
                   && key_equal(env, &map->entries->entries[index].pair.key, &pair.key)) { \
            P##_Entry replacement = {bucket, pair};                      \
            P##_swap(&map->entries->entries[index], &replacement);        \
            return (B##_Put){REPLACED, replacement.pair};                 \
        }                                                                 \
    }                                                                     \
    if (available < count && map->entries->length < map->entries->capacity) { \
        uint64_t index = map->entries->length++;                          \
        map->entries->entries[index] = (P##_Entry){available, pair};       \
        tagged_write(&map->index->slots[available], index);                \
        B##_Put result; result.kind = INSERTED; return result;            \
    }                                                                     \
    return (B##_Put){REFUSED, pair};                                       \
}                                                                         \
static bool P##_plan_index(P##_Index *indexes, uint64_t hash, uint64_t index) { \
    for (uint64_t step = 0; step < indexes->length; ++step) {              \
        uint64_t bucket = source_probe(hash % indexes->length, step, indexes->length); \
        if (tagged_read(&indexes->slots[bucket]) >= DELETED_INDEX) {       \
            tagged_write(&indexes->slots[bucket], index); return true;    \
        }                                                                 \
    }                                                                     \
    return false;                                                         \
}                                                                         \
static bool P##_rebuild(P##_Map *map, const HashEnv *env, uint64_t target) { \
    if (target > (LIMIT)) return false;                                    \
    P##_Index *planned = N##_index(target);                               \
    for (uint64_t i = 0; i < map->entries->length; ++i) {                  \
        uint64_t hash = key_hash(env, &map->entries->entries[i].pair.key);  \
        if (!P##_plan_index(planned, hash, i)) {                          \
            wf_cost_release(planned); return false;                       \
        }                                                                 \
    }                                                                     \
    if (target > map->entries->capacity) {                                \
        P##_Entries *entries = N##_entries(target);                       \
        entries->length = map->entries->length;                           \
        memcpy(entries->entries, map->entries->entries, entries->length * sizeof(P##_Entry)); \
        wf_cost_release(map->entries); map->entries = entries;            \
    }                                                                     \
    for (uint64_t bucket = 0; bucket < target; ++bucket) {                 \
        uint64_t index = tagged_read(&planned->slots[bucket]);             \
        if (index < map->entries->length)                                 \
            map->entries->entries[index].bucket = bucket;                 \
    }                                                                     \
    wf_cost_release(map->index); map->index = planned; return true;       \
}                                                                         \
static HELPER B##_Removed P##_remove(P##_Map *map, const HashEnv *env, const uint64_t *key) { \
    Probe found = P##_find(map, env, key);                                \
    if (!found.found) { B##_Removed result; result.found = false; return result; } \
    uint64_t index = found.index, last = map->entries->length - 1;        \
    uint64_t bucket = map->entries->entries[index].bucket;                 \
    P##_swap(&map->entries->entries[index], &map->entries->entries[last]);  \
    P##_Entry removed; memcpy(&removed, &map->entries->entries[last], sizeof removed); \
    --map->entries->length;                                                \
    if (index < map->entries->length) {                                   \
        uint64_t moved = map->entries->entries[index].bucket;              \
        if (moved < map->index->length) tagged_write(&map->index->slots[moved], index); \
    }                                                                     \
    if (bucket < map->index->length) tagged_write(&map->index->slots[bucket], DELETED_INDEX); \
    return (B##_Removed){true, removed.pair};                             \
}                                                                         \
static HELPER Lookup P##_lookup(const P##_Map *map, const HashEnv *env, const uint64_t *key, Digest *digest) { \
    Probe found = P##_find(map, env, key);                                \
    if (!found.found) return (Lookup){false, 0};                           \
    const B##_Pair *pair = &map->entries->entries[found.index].pair;        \
    return (Lookup){true, B##_observe(digest, &pair->key, &pair->value)};    \
}                                                                         \
static HELPER Lookup P##_edit(P##_Map *map, const HashEnv *env, const uint64_t *key, Digest *digest) { \
    Probe found = P##_find(map, env, key);                                \
    if (!found.found) return (Lookup){false, 0};                           \
    B##_Pair *pair = &map->entries->entries[found.index].pair;             \
    return (Lookup){true, B##_edit(digest, &pair->key, &pair->value)};        \
}                                                                         \
static HELPER void P##_free(P##_Map map, Digest *digest) {                 \
    while (map.entries->length != 0) {                                   \
        P##_Entry removed;                                                \
        memcpy(&removed, &map.entries->entries[--map.entries->length], sizeof removed); \
        B##_consume(digest, removed.pair.key, removed.pair.value);          \
    }                                                                     \
    wf_cost_release(map.entries); wf_cost_release(map.index);             \
}                                                                         \
MATCHED_GROWING_API(P, B, LIMIT)

DENSE_MATCHED(word_repaired, word_tagged, word, uint64_t, CEILING)
DENSE_MATCHED(record_repaired, record_tagged, record, Record, CEILING)
DENSE_MATCHED(word_repaired_small, word_tagged_small, word, uint64_t, 3)
DENSE_MATCHED(record_repaired_small, record_tagged_small, record, Record, 3)

/* The single-slot source formulation takes every old materialized cell into
 * a local, then drains its zero/one Pair through a bounded vacancy scan. It
 * neither plans destinations nor copies the entire old backing during grow.
 * Private helpers are ordinarily optimizable, including the local snapshots.
 * memcpy may copy inactive bytes as representations, never as payload values. */
#define SLOT_MATCHED(P, N, B, LIMIT)                                      \
typedef N##_Cell P##_Cell;                                                \
typedef N##_Block P##_Block;                                              \
typedef N##_Map P##_Map;                                                  \
static inline uint64_t P##_capacity(const P##_Map *map) { return map->slots->length; } \
static P##_Block *P##_block(uint64_t capacity) {                           \
    P##_Block *block = wf_cost_allocate(16 + capacity * sizeof(P##_Cell)); \
    block->length = 0; block->capacity = capacity;                        \
    while (block->length < capacity) {                                   \
        P##_Cell *cell = &block->cells[block->length++];                   \
        cell->length = 0; cell->deleted = false;                         \
    }                                                                    \
    return block;                                                        \
}                                                                        \
static HELPER P##_Map P##_new(uint64_t capacity) {                         \
    return (P##_Map){P##_block(capacity), 0};                             \
}                                                                        \
static unsigned P##_action(const P##_Cell *cell, const HashEnv *env, const uint64_t *key) { \
    if (cell->length != 0)                                               \
        return key_equal(env, &cell->pair.key, key) ? ACTION_MATCH : ACTION_MISS; \
    return cell->deleted ? ACTION_SKIP : ACTION_STOP;                    \
}                                                                        \
static Probe P##_find(const P##_Map *map, const HashEnv *env, const uint64_t *key) { \
    uint64_t hash = key_hash(env, key), count = map->slots->length;        \
    for (uint64_t step = 0; step < count; ++step) {                       \
        uint64_t index = source_probe(hash % count, step, count);         \
        unsigned action = P##_action(&map->slots->cells[index], env, key); \
        if (action == ACTION_MATCH) return (Probe){true, index};          \
        if (action == ACTION_STOP) break;                                \
    }                                                                    \
    return (Probe){false, 0};                                             \
}                                                                        \
static B##_Put P##_exchange(P##_Map *map, uint64_t index, B##_Pair offered) { \
    P##_Cell *cell = &map->slots->cells[index];                           \
    if (cell->length != 0) {                                             \
        B##_Pair previous;                                               \
        memcpy(&previous, &cell->pair, sizeof previous);                 \
        memcpy(&cell->pair, &offered, sizeof offered);                    \
        memcpy(&offered, &previous, sizeof offered);                      \
        return (B##_Put){REPLACED, offered};                             \
    }                                                                    \
    memcpy(&cell->pair, &offered, sizeof offered);                        \
    cell->length = 1; cell->deleted = false;                             \
    B##_Put result; result.kind = INSERTED; return result;               \
}                                                                        \
static B##_Put P##_try_put(P##_Map *map, const HashEnv *env, B##_Pair offered) { \
    uint64_t hash = key_hash(env, &offered.key), count = map->slots->length; \
    uint64_t available = count;                                          \
    for (uint64_t step = 0; step < count; ++step) {                       \
        uint64_t index = source_probe(hash % count, step, count);         \
        unsigned action = P##_action(&map->slots->cells[index], env, &offered.key); \
        if (action == ACTION_MATCH) return P##_exchange(map, index, offered); \
        if ((action == ACTION_STOP || action == ACTION_SKIP) && available == count) \
            available = index;                                           \
        if (action == ACTION_STOP) break;                                \
    }                                                                    \
    if (available < count && map->count < (LIMIT)) {                     \
        ++map->count; return P##_exchange(map, available, offered);       \
    }                                                                    \
    return (B##_Put){REFUSED, offered};                                  \
}                                                                        \
static bool P##_rebuild(P##_Map *map, const HashEnv *env, uint64_t target) { \
    if (target > (LIMIT) || target < map->slots->length) return false;    \
    if (target == 0) return true;                                        \
    P##_Block *next = P##_block(target), *old = map->slots;              \
    map->slots = next;                                                   \
    while (old->length != 0) {                                          \
        P##_Cell cell;                                                   \
        memcpy(&cell, &old->cells[--old->length], sizeof cell);           \
        uint64_t hash = 0;                                               \
        if (cell.length != 0) hash = key_hash(env, &cell.pair.key);       \
        while (cell.length != 0) {                                      \
            uint64_t count = map->slots->length;                         \
            for (uint64_t step = 0; step < count; ++step) {              \
                uint64_t index = source_probe(hash % count, step, count); \
                P##_Cell *destination = &map->slots->cells[index];       \
                if (destination->length == 0) {                          \
                    memmove(&destination->pair, &cell.pair, cell.length * sizeof(B##_Pair)); \
                    destination->length += cell.length; cell.length = 0; \
                    break;                                               \
                }                                                        \
            }                                                            \
        }                                                                \
    }                                                                    \
    wf_cost_release(old); return true;                                  \
}                                                                        \
static HELPER B##_Removed P##_remove(P##_Map *map, const HashEnv *env, const uint64_t *key) { \
    uint64_t before = map->count;                                        \
    if (before == 0) { B##_Removed result; result.found = false; return result; } \
    Probe found = P##_find(map, env, key);                               \
    if (found.found && map->slots->cells[found.index].length != 0) {      \
        P##_Cell *cell = &map->slots->cells[found.index];                  \
        B##_Pair removed; memcpy(&removed, &cell->pair, sizeof removed); \
        --cell->length; cell->deleted = true; map->count = before - 1;    \
        return (B##_Removed){true, removed};                            \
    }                                                                    \
    B##_Removed result; result.found = false; return result;              \
}                                                                        \
static HELPER Lookup P##_lookup(const P##_Map *map, const HashEnv *env, const uint64_t *key, Digest *digest) { \
    Probe found = P##_find(map, env, key);                               \
    if (!found.found) return (Lookup){false, 0};                          \
    const B##_Pair *pair = &map->slots->cells[found.index].pair;           \
    return (Lookup){true, B##_observe(digest, &pair->key, &pair->value)};  \
}                                                                        \
static HELPER Lookup P##_edit(P##_Map *map, const HashEnv *env, const uint64_t *key, Digest *digest) { \
    Probe found = P##_find(map, env, key);                               \
    if (!found.found) return (Lookup){false, 0};                          \
    B##_Pair *pair = &map->slots->cells[found.index].pair;                 \
    return (Lookup){true, B##_edit(digest, &pair->key, &pair->value)};     \
}                                                                        \
static HELPER void P##_free(P##_Map map, Digest *digest) {                 \
    while (map.slots->length != 0) {                                    \
        P##_Cell cell;                                                   \
        memcpy(&cell, &map.slots->cells[--map.slots->length], sizeof cell); \
        if (cell.length != 0) {                                          \
            B##_Pair pair; memcpy(&pair, &cell.pair, sizeof pair);       \
            --cell.length; B##_consume(digest, pair.key, pair.value);    \
        }                                                                \
    }                                                                    \
    wf_cost_release(map.slots);                                          \
}                                                                        \
MATCHED_GROWING_API(P, B, LIMIT)

SLOT_MATCHED(word_slot, word_single, word, CEILING)
SLOT_MATCHED(record_slot, record_single, record, CEILING)

_Static_assert(sizeof(word_sparse_Map) == 16 && sizeof(word_tagged_Map) == 16, "map owners");
_Static_assert(sizeof(word_sparse_Cell) == 24 && sizeof(record_sparse_Cell) == 272, "sparse cells");
_Static_assert(sizeof(word_tagged_Entry) == 24 && sizeof(record_tagged_Entry) == 272, "dense entries");
_Static_assert(sizeof(TaggedIndex) == 16, "copy-enum index stride");

#define TRACE(P, B, T)                                                      \
static NOINLINE uint64_t P##_trace(uint64_t capacity, uint64_t count,        \
                                   uint64_t rounds, uint64_t seed,          \
                                   uint64_t path, uint64_t collide) {      \
    HashEnv env = {seed ^ UINT64_C(0x9e3779b97f4a7c15), collide != 0};       \
    P##_Map map = P##_new(capacity);                                       \
    Digest digest = {seed, 0, 0, 0};                                       \
    for (uint64_t i = 0; i < count; ++i) {                                 \
        B##_Put result = P##_put(&map, &env, (B##_Pair){key_at(i), B##_make(seed + i)}); \
        ordered(&digest, result.kind);                                    \
        if (result.kind != INSERTED)                                      \
            ordered(&digest, B##_content(result.owner.key, result.owner.value)); \
    }                                                                      \
    for (uint64_t round = 0; round < rounds; ++round) {                    \
        if (path == REHASH) {                                             \
            for (uint64_t i = 0; i < count; i += 2) {                     \
                uint64_t key = key_at(i);                                 \
                B##_Removed removed = P##_remove(&map, &env, &key);       \
                ordered(&digest, removed.found);                          \
                if (removed.found)                                       \
                    ordered(&digest, B##_content(removed.owner.key, removed.owner.value)); \
                Lookup absent = P##_lookup(&map, &env, &key, &digest);    \
                ordered(&digest, absent.found);                           \
                if (absent.found) ordered(&digest, absent.identity);      \
            }                                                              \
            ordered(&digest, P##_rehash(&map, &env));                      \
            for (uint64_t i = 0; i < count; ++i) {                        \
                uint64_t key = key_at(i);                                 \
                Lookup found = P##_lookup(&map, &env, &key, &digest);      \
                ordered(&digest, found.found);                            \
                if (found.found) ordered(&digest, found.identity);        \
            }                                                              \
            for (uint64_t i = 0; i < count; i += 2) {                     \
                B##_Pair offered = {key_at(i), B##_make(seed + (round + 1) * count + i)}; \
                B##_Put result = P##_put(&map, &env, offered);             \
                ordered(&digest, result.kind);                            \
                if (result.kind != INSERTED)                              \
                    ordered(&digest, B##_content(result.owner.key, result.owner.value)); \
            }                                                              \
            continue;                                                      \
        }                                                                  \
        if (path == GROW)                                                  \
            ordered(&digest, P##_reserve(&map, &env, capacity ? capacity * 2 : 1)); \
        for (uint64_t i = 0; i < count; ++i) {                             \
            uint64_t key = key_at(path == MISS ? count + i : i);          \
            if (path == HIT || path == MISS || path == GROW) {            \
                Lookup found = P##_lookup(&map, &env, &key, &digest);      \
                ordered(&digest, found.found);                            \
                if (found.found) ordered(&digest, found.identity);        \
            } else if (path == REPLACE) {                                 \
                B##_Pair offered = {key, B##_make(seed + (round + 1) * count + i)}; \
                B##_Put result = P##_put(&map, &env, offered);             \
                ordered(&digest, result.kind);                            \
                if (result.kind != INSERTED)                              \
                    ordered(&digest, B##_content(result.owner.key, result.owner.value)); \
            } else if (path == CHURN) {                                   \
                B##_Removed removed = P##_remove(&map, &env, &key);        \
                ordered(&digest, removed.found);                          \
                if (removed.found)                                        \
                    ordered(&digest, B##_content(removed.owner.key, removed.owner.value)); \
                Lookup absent = P##_lookup(&map, &env, &key, &digest);     \
                ordered(&digest, absent.found);                           \
                if (absent.found) ordered(&digest, absent.identity);      \
                B##_Pair offered = {key, B##_make(seed + (round + 1) * count + i)}; \
                B##_Put result = P##_put(&map, &env, offered);             \
                ordered(&digest, result.kind);                            \
                if (result.kind != INSERTED)                              \
                    ordered(&digest, B##_content(result.owner.key, result.owner.value)); \
            } else if (path == EDIT) {                                    \
                Lookup edited = P##_edit(&map, &env, &key, &digest);       \
                ordered(&digest, edited.found);                           \
                if (edited.found) ordered(&digest, edited.identity);      \
            }                                                              \
        }                                                                  \
    }                                                                      \
    P##_free(map, &digest);                                                 \
    return finish(digest);                                                 \
}

#define POLICY_CHECK(P, B, T)                                              \
static void P##_policy_check(void) {                                      \
    HashEnv env = {17, true};                                              \
    P##_Map map = P##_new(0);                                             \
    Digest digest = {0};                                                   \
    for (uint64_t i = 0; i < 3; ++i) {                                    \
        B##_Put result = P##_put(&map, &env, (B##_Pair){key_at(i), B##_make(10 + i)}); \
        require(result.kind == INSERTED, "growth from zero and ceiling saturation"); \
    }                                                                      \
    B##_Put replaced = P##_put(&map, &env, (B##_Pair){key_at(1), B##_make(99)}); \
    require(replaced.kind == REPLACED && replaced.owner.key == key_at(1)   \
            && B##_identity(&replaced.owner.value) == 11, "replacement at ceiling"); \
    B##_Put refused = P##_put(&map, &env, (B##_Pair){key_at(3), B##_make(13)}); \
    require(refused.kind == REFUSED && refused.owner.key == key_at(3)      \
            && B##_identity(&refused.owner.value) == 13, "refused offered owner"); \
    uint64_t key = key_at(0);                                              \
    B##_Removed removed = P##_remove(&map, &env, &key);                     \
    require(removed.found && removed.owner.key == key                       \
            && B##_identity(&removed.owner.value) == 10, "owned removal"); \
    for (uint64_t i = 1; i < 3; ++i) {                                    \
        key = key_at(i); Lookup found = P##_lookup(&map, &env, &key, &digest); \
        require(found.found && found.identity == (i == 1 ? 99 : 12),       \
                "lookup past deletion and dense reverse repair");       \
    }                                                                      \
    B##_Put reused = P##_put(&map, &env, (B##_Pair){key_at(3), B##_make(13)}); \
    require(reused.kind == INSERTED, "tombstone reuse at ceiling");       \
    key = key_at(0); Lookup absent_edit = P##_edit(&map, &env, &key, &digest); \
    require(!absent_edit.found, "edit does not construct a missing value"); \
    key = key_at(3); Lookup edited = P##_edit(&map, &env, &key, &digest);   \
    require(edited.found && edited.identity == 14, "borrowed in-place edit"); \
    require(!P##_reserve(&map, &env, 4), "over-ceiling reserve refused"); \
    require(P##_rehash(&map, &env), "same-capacity rehash");              \
    require(P##_reserve(&map, &env, 1), "smaller reserve is a no-op");    \
    P##_free(map, &digest);                                                 \
    require(digest.count == 3, "final owner count");                      \
}

TRACE(word_sparse, word, uint64_t)
TRACE(record_sparse, record, Record)
TRACE(word_tagged, word, uint64_t)
TRACE(record_tagged, record, Record)
TRACE(word_compact, word, uint64_t)
TRACE(record_compact, record, Record)
TRACE(word_planned, word, uint64_t)
TRACE(record_planned, record, Record)
TRACE(word_repaired, word, uint64_t)
TRACE(record_repaired, record, Record)
TRACE(word_descending, word, uint64_t)
TRACE(record_descending, record, Record)
TRACE(word_single, word, uint64_t)
TRACE(record_single, record, Record)
TRACE(word_slot, word, uint64_t)
TRACE(record_slot, record, Record)
TRACE(word_staged, word, uint64_t)
TRACE(record_staged, record, Record)

POLICY_CHECK(word_sparse_small, word, uint64_t)
POLICY_CHECK(record_sparse_small, record, Record)
POLICY_CHECK(word_tagged_small, word, uint64_t)
POLICY_CHECK(record_tagged_small, record, Record)
POLICY_CHECK(word_compact_small, word, uint64_t)
POLICY_CHECK(record_compact_small, record, Record)
POLICY_CHECK(word_planned_small, word, uint64_t)
POLICY_CHECK(record_planned_small, record, Record)
POLICY_CHECK(word_repaired_small, word, uint64_t)
POLICY_CHECK(record_repaired_small, record, Record)

#include "map-oracle.h"

typedef uint64_t (*Trace)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
enum { WF_VARIANT = 1, REBUILD_VARIANT = 2, ZERO_REHASH_NOOP = 4,
       LIBRARY_VARIANT = 8, INACTIVE_CONTROL_VARIANT = 16 };
typedef struct {
    const char *name;
    bool wide, dense, planned;
    size_t cell_stride, index_stride;
    Trace trace;
    void (*policy_check)(void);
    unsigned flags;
} Variant;
static const Variant variants[] = {
    {"word-sparse-direct", false, false, false, sizeof(word_sparse_Cell), 0, word_sparse_trace, word_sparse_small_policy_check, INACTIVE_CONTROL_VARIANT},
    {"record-sparse-direct", true, false, false, sizeof(record_sparse_Cell), 0, record_sparse_trace, record_sparse_small_policy_check, INACTIVE_CONTROL_VARIANT},
    {"word-dense-tagged", false, true, false, sizeof(word_tagged_Entry), sizeof(TaggedIndex), word_tagged_trace, word_tagged_small_policy_check, 0},
    {"record-dense-tagged", true, true, false, sizeof(record_tagged_Entry), sizeof(TaggedIndex), record_tagged_trace, record_tagged_small_policy_check, 0},
    {"word-dense-compact", false, true, false, sizeof(word_compact_Entry), sizeof(uint64_t), word_compact_trace, word_compact_small_policy_check, 0},
    {"record-dense-compact", true, true, false, sizeof(record_compact_Entry), sizeof(uint64_t), record_compact_trace, record_compact_small_policy_check, 0},
    {"word-sparse-planned", false, false, true, sizeof(word_planned_Cell), 0, word_planned_trace, word_planned_small_policy_check, 0},
    {"record-sparse-planned", true, false, true, sizeof(record_planned_Cell), 0, record_planned_trace, record_planned_small_policy_check, 0},
    {"word-dense-repaired", false, true, true, sizeof(word_repaired_Entry), sizeof(TaggedIndex), word_repaired_trace, word_repaired_small_policy_check, 0},
    {"record-dense-repaired", true, true, true, sizeof(record_repaired_Entry), sizeof(TaggedIndex), record_repaired_trace, record_repaired_small_policy_check, 0},
#if defined(WITH_WF)
    {"word-wf-sparse", false, false, true, sizeof(word_planned_Cell), 0, wf_map_cost_sparse_word_trace, NULL, WF_VARIANT},
    {"record-wf-sparse", true, false, true, sizeof(record_planned_Cell), 0, wf_map_cost_sparse_record_trace, NULL, WF_VARIANT},
    {"word-wf-dense", false, true, true, sizeof(word_repaired_Entry), sizeof(TaggedIndex), wf_map_cost_dense_word_trace, NULL, WF_VARIANT},
    {"record-wf-dense", true, true, true, sizeof(record_repaired_Entry), sizeof(TaggedIndex), wf_map_cost_dense_record_trace, NULL, WF_VARIANT},
#endif
    {"word-sparse-descending", false, false, false, sizeof(word_descending_Cell), 0, word_descending_trace, NULL, REBUILD_VARIANT},
    {"record-sparse-descending", true, false, false, sizeof(record_descending_Cell), 0, record_descending_trace, NULL, REBUILD_VARIANT},
    {"word-slot-direct", false, false, false, sizeof(word_single_Cell), 0, word_single_trace, NULL, REBUILD_VARIANT | ZERO_REHASH_NOOP},
    {"record-slot-direct", true, false, false, sizeof(record_single_Cell), 0, record_single_trace, NULL, REBUILD_VARIANT | ZERO_REHASH_NOOP},
    {"word-slot-matched", false, false, false, sizeof(word_slot_Cell), 0, word_slot_trace, NULL, REBUILD_VARIANT | ZERO_REHASH_NOOP},
    {"record-slot-matched", true, false, false, sizeof(record_slot_Cell), 0, record_slot_trace, NULL, REBUILD_VARIANT | ZERO_REHASH_NOOP},
    {"word-staged-matched", false, false, false, sizeof(word_staged_Cell), 0, word_staged_trace, NULL, REBUILD_VARIANT | ZERO_REHASH_NOOP | INACTIVE_CONTROL_VARIANT},
    {"record-staged-matched", true, false, false, sizeof(record_staged_Cell), 0, record_staged_trace, NULL, REBUILD_VARIANT | ZERO_REHASH_NOOP | INACTIVE_CONTROL_VARIANT},
#if defined(WITH_WF)
    {"word-wf-slot", false, false, false, sizeof(word_slot_Cell), 0, wf_map_cost_slot_word_trace, NULL, WF_VARIANT | REBUILD_VARIANT | ZERO_REHASH_NOOP},
    {"record-wf-slot", true, false, false, sizeof(record_slot_Cell), 0, wf_map_cost_slot_record_trace, NULL, WF_VARIANT | REBUILD_VARIANT | ZERO_REHASH_NOOP},
    {"word-wf-staged", false, false, false, sizeof(word_staged_Cell), 0, wf_map_cost_staged_word_trace, NULL, WF_VARIANT | REBUILD_VARIANT | ZERO_REHASH_NOOP},
    {"record-wf-staged", true, false, false, sizeof(record_staged_Cell), 0, wf_map_cost_staged_record_trace, NULL, WF_VARIANT | REBUILD_VARIANT | ZERO_REHASH_NOOP},
    {"word-wf-library", false, false, false, sizeof(word_staged_Cell), 0, wf_map_cost_library_word_trace, NULL, WF_VARIANT | LIBRARY_VARIANT | ZERO_REHASH_NOOP},
    {"record-wf-library", true, false, false, sizeof(record_staged_Cell), 0, wf_map_cost_library_record_trace, NULL, WF_VARIANT | LIBRARY_VARIANT | ZERO_REHASH_NOOP},
#endif
};

static Ledger expected_ledger(const Variant *variant, uint64_t capacity,
                               uint64_t rounds, unsigned path) {
    size_t initial = 16 + capacity * variant->cell_stride;
    if (variant->dense) initial += 8 + capacity * variant->index_stride;
    size_t allocations = variant->dense ? 2 : 1;
    Ledger expected = {allocations, allocations, initial, 0, initial};
    if (path == GROW && rounds != 0) {
        uint64_t target = capacity ? capacity * 2 : 1;
        size_t next = 16 + target * variant->cell_stride;
        if (variant->dense) next += 8 + target * variant->index_stride;
        if (variant->planned && !variant->dense) {
            size_t plan = 8 + target * sizeof(uint64_t), used = 8 + target;
            expected.requests += 3; expected.releases += 3;
            expected.bytes += plan + used + next;
            expected.peak = initial + plan + (used > next ? used : next);
        } else {
            expected.requests += allocations; expected.releases += allocations;
            expected.bytes += next; expected.peak += next;
        }
    } else if (path == REHASH && rounds != 0
               && !(capacity == 0 && (variant->flags & ZERO_REHASH_NOOP))) {
        size_t per_round;
        if (variant->dense) {
            per_round = 8 + capacity * variant->index_stride;
            expected.requests += rounds; expected.releases += rounds;
        } else if (variant->planned) {
            per_round = 16 + capacity * (sizeof(uint64_t) + 1);
            expected.requests += 2 * rounds; expected.releases += 2 * rounds;
        } else {
            per_round = initial;
            expected.requests += rounds; expected.releases += rounds;
        }
        expected.bytes += rounds * per_round; expected.peak += per_round;
    }
    return expected;
}

static void check_ledger(Ledger expected) {
    require(ledger.requests == expected.requests && ledger.releases == expected.releases
            && ledger.bytes == expected.bytes && ledger.live == 0
            && ledger.peak == expected.peak, "backing counts, bytes, peak and cleanup");
}

#if defined(ECO_MAP_CONTROL)
__attribute__((unused))
#endif
static void check(void) {
    const struct { uint64_t capacity, count; } shapes[] = {
        {0, 0}, {1, 0}, {1, 1}, {3, 2}, {3, 3}, {63, 55}, {64, 32}, {64, 56}, {64, 64}
    };
    const uint64_t rounds[] = {0, 1, 3}, seeds[] = {0, 17, UINT64_MAX};
    size_t executions = 0, policies = 0;
    for (size_t v = 0; v < sizeof variants / sizeof variants[0]; ++v) {
        const Variant *variant = &variants[v];
        if (variant->policy_check) {
            reset_ledger(); variant->policy_check();
            require(ledger.live == 0 && ledger.requests == ledger.releases, "policy cleanup");
            ++policies;
        }
        for (size_t n = 0; n < sizeof shapes / sizeof shapes[0]; ++n)
            for (unsigned path = 0; path < PATH_COUNT; ++path)
                for (size_t r = 0; r < sizeof rounds / sizeof rounds[0]; ++r)
                    for (size_t s = 0; s < sizeof seeds / sizeof seeds[0]; ++s)
                        for (unsigned collide = 0; collide < 2; ++collide) {
                            if ((variant->flags & REBUILD_VARIANT)
                                && ((path != GROW && path != REHASH)
                                    || (n != 0 && n != 2 && n != 3 && n != 4 && n != 5)))
                                continue;
                            uint64_t expected = oracle(variant->wide, shapes[n].count, rounds[r], seeds[s], path);
                            reset_ledger();
                            uint64_t actual = variant->trace(shapes[n].capacity, shapes[n].count, rounds[r], seeds[s], path, collide != 0);
                            require(actual == expected, "independent key-ID/content/outcome oracle");
                            check_ledger(expected_ledger(variant, shapes[n].capacity, rounds[r], path));
                            ++executions;
                        }
    }
    printf("map costs: %zu executions and %zu C policy chains passed\n", executions, policies);
}

#if defined(WITH_WF)
static uint64_t nanoseconds(void) {
#if defined(_WIN32)
    LARGE_INTEGER count, frequency;
    QueryPerformanceCounter(&count); QueryPerformanceFrequency(&frequency);
    return (uint64_t)((long double)count.QuadPart * 1000000000.0L / frequency.QuadPart);
#else
    struct timespec value;
    require(clock_gettime(CLOCK_MONOTONIC, &value) == 0, "monotonic clock");
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
#endif
}

static uint64_t clock_quantum(void) {
    uint64_t quantum = UINT64_MAX, previous = nanoseconds();
    for (unsigned probe = 0; probe < 10000; ++probe) {
        uint64_t current = nanoseconds();
        require(current >= previous, "monotonic clock order");
        uint64_t delta = current - previous;
        if (delta != 0 && delta < quantum) quantum = delta;
        previous = current;
    }
    require(quantum != UINT64_MAX, "positive clock delta within 10000 probes");
    return quantum;
}

/* Timings are complete, independently checked traces. Growth is one real
 * reserve; rehash includes deletion, verification and reinsertion. No setup
 * baseline is subtracted to invent an isolated operation latency. */
enum { PRIMARY_SET, BOUNDARY_SET, EDIT_SET, REBUILD_SET, LIBRARY_SET, INACTIVE_SET };
static bool selected_cohort(unsigned set, bool wide, uint64_t capacity,
                            unsigned occupancy, bool collide, unsigned path) {
    if (set == INACTIVE_SET)
        return occupancy == 1 && !collide
            && ((capacity == 4096 && (path == GROW || path == REHASH || path == SETUP))
                || (capacity == 64 && (path == REPLACE || path == CHURN || path == HIT || path == EDIT)));
    if (set == LIBRARY_SET)
        return occupancy == 1 && !collide
            && ((capacity == 4096 && (path == GROW || path == REHASH))
                || (capacity == 64 && (path == REPLACE || path == CHURN)));
    if (set == EDIT_SET) return capacity == 64 && !collide && path == EDIT;
    if (set == REBUILD_SET)
        return capacity == 4096 && occupancy == 1 && !collide
            && (path == GROW || path == REHASH);
    if (set == BOUNDARY_SET)
        return capacity == 64 && occupancy == 1 && !collide
            && (path == REPLACE || path == CHURN);
    if (path == EDIT) return false;
    if (collide)
        return !wide && capacity == 64 && occupancy == 1
            && (path == MISS || path == CHURN || path == REHASH);
    if (capacity == 64 && occupancy == 1) return true;
    if (capacity == 4096 && occupancy == 1)
        return path == HIT || path == MISS || path == GROW || path == REHASH || path == SETUP;
    return capacity == 64 && occupancy == 0 && (path == MISS || path == CHURN);
}

static bool selected_variant(unsigned set, unsigned path, const Variant *variant) {
    if (set == INACTIVE_SET)
        return (variant->flags & (LIBRARY_VARIANT | INACTIVE_CONTROL_VARIANT))
            && (!(variant->flags & REBUILD_VARIANT) || path == GROW || path == REHASH);
    if (variant->flags & LIBRARY_VARIANT) return set == LIBRARY_SET;
    if (set == LIBRARY_SET)
        return !(variant->flags & REBUILD_VARIANT) || path == GROW || path == REHASH;
    return set == REBUILD_SET || !(variant->flags & REBUILD_VARIANT);
}

static void measure(unsigned cohort, unsigned set, const char *source_shape) {
    const uint64_t capacities[] = {64, 4096};
    const char *paths[] = {"hit", "miss", "replace", "churn", "grow", "rehash", "setup-cleanup", "edit-first-word"};
    puts("contract,cohort,element_bytes,path,capacity,count,hash,variant,source_shape,sample,rounds,traces,elapsed_ns,checksum,requests,requested_bytes,peak_bytes");
    for (unsigned wide = 0; wide < 2; ++wide)
            for (size_t n = 0; n < sizeof capacities / sizeof capacities[0]; ++n)
                for (unsigned occupancy = 0; occupancy < 2; ++occupancy)
                    for (unsigned collide = 0; collide < 2; ++collide) {
                        uint64_t capacity = capacities[n];
                        uint64_t count = occupancy ? capacity * 7 / 8 : capacity / 2;
                        for (unsigned path = 0; path < PATH_COUNT; ++path) {
                            if (!selected_cohort(set, wide != 0, capacity, occupancy, collide != 0, path)) continue;
                            size_t active[sizeof variants / sizeof variants[0]], variant_count = 0;
                            for (size_t v = 0; v < sizeof variants / sizeof variants[0]; ++v)
                                if (selected_variant(set, path, &variants[v]))
                                    active[variant_count++] = v;
                            uint64_t rounds = path == SETUP ? 0
                                : (path == GROW ? 1 : UINT64_C(8192) / count);
                            uint64_t traces = path == SETUP || path == GROW
                                ? UINT64_C(8192) / count : 1;
                            for (unsigned sample = 0; sample < 11; ++sample) {
                                uint64_t seed = 101 + sample, expected = 0;
                                for (uint64_t trace = 0; trace < traces; ++trace)
                                    expected = expected * UINT64_C(257)
                                        + oracle(wide != 0, count, rounds, seed + trace, path);
                                for (size_t offset = 0; offset < variant_count; ++offset) {
                                    size_t position = (sample + offset) % variant_count;
                                    size_t v = active[cohort ? variant_count - 1 - position : position];
                                    const Variant *variant = &variants[v];
                                    if (variant->wide != (wide != 0)) continue;
                                    reset_ledger();
                                    uint64_t checksum = 0, start = nanoseconds();
                                    for (uint64_t trace = 0; trace < traces; ++trace)
                                        checksum = checksum * UINT64_C(257)
                                            + variant->trace(capacity, count, rounds, seed + trace, path, collide);
                                    uint64_t elapsed = nanoseconds() - start;
                                    require(checksum == expected, "timed independent content/outcome oracle");
                                    Ledger accounting = expected_ledger(variant, capacity, rounds, path);
                                    accounting.requests *= traces; accounting.releases *= traces;
                                    accounting.bytes *= traces;
                                    check_ledger(accounting);
                                    observed ^= checksum;
                                    const char *shape = (variant->flags & LIBRARY_VARIANT) ? "library-compact"
                                        : (variant->flags & WF_VARIANT) ? ((variant->flags & REBUILD_VARIANT) ? "original" : source_shape)
                                        : "c-shared";
                                    printf("%s,%u,%zu,%s,%" PRIu64 ",%" PRIu64 ",%s,%s,%s,%u,%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%zu,%zu,%zu\n",
                                           CONTRACT, cohort, wide ? sizeof(Record) : sizeof(uint64_t), paths[path],
                                           capacity, count, collide ? "colliding" : "mixed", variant->name,
                                           shape, sample,
                                           rounds, traces, elapsed, checksum, ledger.requests, ledger.bytes, ledger.peak);
                                }
                            }
                        }
                    }
}
#endif

#if defined(ECO_MAP_CONTROL)
/* Reuse the existing ordinary sparse implementation and its complete trace.
 * Only this C ABI boundary remains visible to the ecosystem driver. */
uint64_t eco_c_map_word(uint64_t capacity, uint64_t count, uint64_t rounds,
                        uint64_t seed, uint64_t path, uint64_t collide) {
    return word_sparse_trace(capacity, count, rounds, seed, path, collide);
}
uint64_t eco_c_map_record(uint64_t capacity, uint64_t count, uint64_t rounds,
                          uint64_t seed, uint64_t path, uint64_t collide) {
    return record_sparse_trace(capacity, count, rounds, seed, path, collide);
}

/* Isolated running-index lookup attribution. The caller owns the context;
 * the existing sparse backing retains its 16-byte length/capacity header and
 * ordinary 24/272-byte enum cells. Unlike the insertion probe, this lookup
 * tracks no vacancy: it follows the WF lookup's bounded live/deleted/empty
 * decisions, one initial remainder and increment with conditional wrap.
 * Retire these exports with the isolated lookup comparison. */
#define ECO_LOOKUP(WIDTH, P, B)                                           \
typedef struct { P##_Map map; HashEnv env; uint64_t seed; } WIDTH##_LookupOwner; \
_Static_assert(sizeof(WIDTH##_LookupOwner) <= 128, "lookup context extent"); \
_Static_assert(_Alignof(WIDTH##_LookupOwner) <= 16, "lookup context alignment"); \
static HELPER Lookup WIDTH##_lookup_one(const WIDTH##_LookupOwner *owner, uint64_t key) { \
    uint64_t hash = key_hash(&owner->env, &key);                          \
    uint64_t count = owner->map.slots->capacity;                          \
    if (count == 0) return (Lookup){false, 0};                            \
    uint64_t index = hash % count;                                      \
    for (uint64_t step = 0; step < count; ++step) {                       \
        const P##_Cell *slot = &owner->map.slots->cells[index];           \
        if (slot->tag == LIVE) {                                         \
            if (key_equal(&owner->env, &slot->pair.key, &key))            \
                return (Lookup){true, B##_identity(&slot->pair.value)};  \
        } else if (slot->tag == EMPTY) {                                 \
            return (Lookup){false, 0};                                   \
        }                                                               \
        uint64_t next = index + 1;                                       \
        index = next < count ? next : 0;                                 \
    }                                                                   \
    return (Lookup){false, 0};                                           \
}                                                                       \
void eco_c_map_lookup_##WIDTH##_prepare(void *storage, uint64_t slots, uint64_t count, uint64_t seed) { \
    require(count <= slots && slots <= CEILING, "isolated lookup capacity"); \
    WIDTH##_LookupOwner *owner = storage;                                \
    *owner = (WIDTH##_LookupOwner){P##_new(slots),                        \
        {seed ^ UINT64_C(0x9e3779b97f4a7c15), false}, seed};              \
    for (uint64_t i = 0; i < count; ++i) {                               \
        B##_Put result = P##_put(&owner->map, &owner->env,                \
            (B##_Pair){key_at(i), B##_make(seed + i)});                   \
        require(result.kind == INSERTED, "isolated lookup population"); \
    }                                                                   \
}                                                                       \
uint64_t eco_c_map_lookup_##WIDTH##_query(const void *storage, uint64_t rounds, uint64_t miss) { \
    const WIDTH##_LookupOwner *owner = storage;                          \
    Digest digest = {owner->seed, 0, 0, 0};                              \
    for (uint64_t round = 0; round < rounds; ++round)                     \
        for (uint64_t i = 0; i < owner->map.count; ++i) {                  \
            uint64_t key = key_at(i) + (miss != 0);                      \
            Lookup found = WIDTH##_lookup_one(owner, key);               \
            ordered(&digest, found.found);                               \
            if (found.found) ordered(&digest, found.identity);            \
        }                                                               \
    return finish(digest);                                              \
}                                                                       \
uint64_t eco_c_map_lookup_##WIDTH##_geometry(const void *storage) {       \
    const WIDTH##_LookupOwner *owner = storage;                          \
    return (owner->map.slots->capacity << 32) | owner->map.count;          \
}                                                                       \
uint64_t eco_c_map_lookup_##WIDTH##_finish(void *storage) {               \
    WIDTH##_LookupOwner *owner = storage;                                \
    Digest digest = {owner->seed, 0, 0, 0};                              \
    P##_free(owner->map, &digest);                                       \
    *owner = (WIDTH##_LookupOwner){0};                                   \
    return finish(digest);                                              \
}
ECO_LOOKUP(word, word_sparse, word)
ECO_LOOKUP(record, record_sparse, record)
#undef ECO_LOOKUP
#if defined(ACCOUNT_ONLY)
/* A separate filled-map witness keeps geometry reads out of the timed trace.
 * The low/high halves encode count/capacity, combined with the setup digest. */
#define ECO_GEOMETRY(NAME, P, B) \
uint64_t NAME(uint64_t capacity, uint64_t count, uint64_t seed) { \
    HashEnv env = {seed ^ UINT64_C(0x9e3779b97f4a7c15), false}; \
    P##_Map map = P##_new(capacity); \
    Digest digest = {seed, 0, 0, 0}; \
    for (uint64_t i = 0; i < count; ++i) { \
        B##_Put result = P##_put(&map, &env, (B##_Pair){key_at(i), B##_make(seed + i)}); \
        ordered(&digest, result.kind); \
        if (result.kind != INSERTED) \
            ordered(&digest, B##_content(result.owner.key, result.owner.value)); \
    } \
    uint64_t geometry = (map.slots->capacity << 32) | map.count; \
    P##_free(map, &digest); \
    return finish(digest) ^ geometry; \
}
ECO_GEOMETRY(eco_c_map_word_geometry, word_sparse, word)
ECO_GEOMETRY(eco_c_map_record_geometry, record_sparse, record)
#undef ECO_GEOMETRY
#endif
#else
int main(int argc, char **argv) {
    require(argc >= 2, "usage: map-costs check | clock-quantum | measure 0|1 primary|boundary|edit|rebuild|library|inactive original|compact");
    if (strcmp(argv[1], "check") == 0) {
        require(argc == 2, "check takes no arguments"); check();
    }
#if defined(WITH_WF)
    else if (strcmp(argv[1], "clock-quantum") == 0) {
        require(argc == 2, "clock-quantum takes no arguments");
        printf("map costs: %s minimum positive monotonic delta over 10000 probes: %" PRIu64 " ns\n",
               CONTRACT, clock_quantum());
    }
    else if (strcmp(argv[1], "measure") == 0) {
        require(argc == 5 && (strcmp(argv[2], "0") == 0 || strcmp(argv[2], "1") == 0),
                "measure requires cohort 0 or 1, a named set and source shape");
        unsigned set = PRIMARY_SET;
        if (strcmp(argv[3], "boundary") == 0) set = BOUNDARY_SET;
        else if (strcmp(argv[3], "edit") == 0) set = EDIT_SET;
        else if (strcmp(argv[3], "rebuild") == 0) set = REBUILD_SET;
        else if (strcmp(argv[3], "library") == 0) set = LIBRARY_SET;
        else if (strcmp(argv[3], "inactive") == 0) set = INACTIVE_SET;
        else require(strcmp(argv[3], "primary") == 0, "unknown measurement set");
        require(strcmp(argv[4], "original") == 0 || strcmp(argv[4], "compact") == 0,
                "unknown source shape");
        require(set != LIBRARY_SET || strcmp(argv[4], "original") == 0,
                "library set uses the original comparison sources");
        require(set != INACTIVE_SET || strcmp(argv[4], "original") == 0,
                "inactive set uses the original comparison sources");
        measure((unsigned)(argv[2][0] - '0'), set, argv[4]);
    }
#endif
    else require(false, "unknown mode");
    return 0;
}
#endif
