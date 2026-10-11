#define _GNU_SOURCE
#include "snapshot_export.h"
#include "fork_capsule.h"
#include "bridge.h"
#include "contract.h"
#include "../concurrent_map.h"

#if defined(__linux__) && defined(__x86_64__) && defined(__LP64__) && defined(__GLIBC__)
#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "../runtime_test_guard.h"

#define CHECK(value) do { if (!(value)) { \
    fprintf(stderr, "snapshot export: %s:%d: %s\n", __FILE__, __LINE__, #value); \
    exit(1); } } while (0)

typedef struct test_slot { uint32_t tag; uint64_t value; } test_slot;
static const wf_snapshot_type integer = {
    .shape = WF_SNAPSHOT_SCALAR, .tag = 8, .width = 8, .size = 8, .alignment = 8
};
static const wf_snapshot_type octet = {
    .shape = WF_SNAPSHOT_SCALAR, .tag = 5, .width = 1, .size = 1, .alignment = 1
};
static const wf_snapshot_type floating = {
    .shape = WF_SNAPSHOT_SCALAR, .tag = 10, .width = 8, .size = 8, .alignment = 8
};
static const wf_snapshot_variant boolean_variants[] = {{0, NULL}, {0, NULL}};
static const wf_snapshot_type boolean_type = {
    .shape = WF_SNAPSHOT_ENUM, .tag = 0x11, .width = 1, .size = 1, .alignment = 1,
    .count = 2, .variants = boolean_variants
};
static const wf_snapshot_type unit_type = {
    .shape = WF_SNAPSHOT_SCALAR, .tag = 0, .size = 1, .alignment = 1
};
static const wf_snapshot_type signed_type = {
    .shape = WF_SNAPSHOT_SCALAR, .tag = 2, .width = 2, .size = 2, .alignment = 2
};
static const wf_snapshot_type float32_type = {
    .shape = WF_SNAPSHOT_SCALAR, .tag = 9, .width = 4, .size = 4, .alignment = 4
};
typedef struct byte_slots { uint64_t len, cap; unsigned char data[]; } byte_slots;
typedef struct byte_ring { uint64_t len, head; unsigned char data[4]; } byte_ring;
typedef struct bucket { uint32_t tag; uint64_t key, value; } bucket;
typedef struct bucket_slots { uint64_t len, cap; bucket data[4]; } bucket_slots;
typedef struct hash_value { bucket_slots *cells; uint64_t length, vacated; } hash_value;
typedef struct tree_node tree_node;
typedef struct child_slot { uint64_t len; tree_node *data[1]; } child_slot;
typedef struct pair { uint64_t key, value; } pair;
typedef struct tree_entry { pair pair; child_slot after; } tree_entry;
typedef struct tree_entries { uint64_t len; tree_entry data[15]; } tree_entries;
struct tree_node { child_slot leading; tree_entries entries; };
typedef struct ordered_value { child_slot root; uint64_t length; } ordered_value;
typedef struct paged_bytes { uint64_t len, cap, pages; unsigned char *data[2]; } paged_bytes;
typedef struct closure {
    byte_slots *bytes;
    byte_ring ring;
    hash_value hash;
    ordered_value ordered;
    unsigned char *segments;
    paged_bytes *paged;
    unsigned char array[2];
    uint64_t nan;
    unsigned char boolean, unit;
    uint16_t signed_bits;
    uint32_t nan32;
} closure;
typedef struct closure_slot { uint32_t tag; closure value; } closure_slot;

static const wf_snapshot_type bytes_type = {
    .shape = WF_SNAPSHOT_SLOTS, .tag = 0x21, .size = 16, .alignment = 8,
    .element = &octet, .length_offset = 0, .capacity_offset = 8, .data_offset = 16
};
static const wf_snapshot_type boxed_bytes = {
    .shape = WF_SNAPSHOT_BOX, .tag = 0x25, .size = 8, .alignment = 8, .element = &bytes_type
};
static const wf_snapshot_type ring_type = {
    .shape = WF_SNAPSHOT_RING, .tag = 0x22, .size = sizeof(byte_ring), .alignment = 8,
    .element = &octet, .length_offset = 0, .capacity_offset = UINT64_MAX,
    .head_offset = 8, .data_offset = 16, .fixed_capacity = 4
};
static const wf_snapshot_field bucket_fields[] = {
    {offsetof(bucket, key), &integer}, {offsetof(bucket, value), &integer}
};
static const wf_snapshot_variant bucket_variants[] = {
    {0, NULL}, {0, NULL}, {2, bucket_fields}
};
static const wf_snapshot_type bucket_type = {
    .shape = WF_SNAPSHOT_ENUM, .tag = 0x11, .width = 4, .size = sizeof(bucket),
    .alignment = 8, .count = 3, .variants = bucket_variants
};
static const wf_snapshot_type buckets_type = {
    .shape = WF_SNAPSHOT_SLOTS, .tag = 0x21, .size = 16, .alignment = 8,
    .element = &bucket_type, .length_offset = 0, .capacity_offset = 8, .data_offset = 16
};
static const wf_snapshot_type boxed_buckets = {
    .shape = WF_SNAPSHOT_BOX, .tag = 0x25, .size = 8, .alignment = 8, .element = &buckets_type
};
static const wf_snapshot_field hash_fields[] = {
    {offsetof(hash_value, cells), &boxed_buckets}, {offsetof(hash_value, length), &integer}
};
static const wf_snapshot_type hash_type = {
    .shape = WF_SNAPSHOT_HASH, .tag = 0x30, .size = sizeof(hash_value), .alignment = 8,
    .count = 2, .fields = hash_fields, .element = &bucket_type
};
static const wf_snapshot_type node_type;
static const wf_snapshot_type boxed_node = {
    .shape = WF_SNAPSHOT_BOX, .tag = 0x25, .size = 8, .alignment = 8, .element = &node_type
};
static const wf_snapshot_type children_type = {
    .shape = WF_SNAPSHOT_SLOTS, .tag = 0x21, .size = sizeof(child_slot), .alignment = 8,
    .element = &boxed_node, .length_offset = 0, .capacity_offset = UINT64_MAX,
    .data_offset = 8, .fixed_capacity = 1
};
static const wf_snapshot_field pair_fields[] = {
    {offsetof(pair, key), &integer}, {offsetof(pair, value), &floating}
};
static const wf_snapshot_type pair_type = {
    .shape = WF_SNAPSHOT_STRUCT, .tag = 0x10, .size = sizeof(pair), .alignment = 8,
    .count = 2, .fields = pair_fields
};
static const wf_snapshot_field entry_fields[] = {
    {offsetof(tree_entry, pair), &pair_type}, {offsetof(tree_entry, after), &children_type}
};
static const wf_snapshot_type entry_type = {
    .shape = WF_SNAPSHOT_STRUCT, .tag = 0x10, .size = sizeof(tree_entry), .alignment = 8,
    .count = 2, .fields = entry_fields
};
static const wf_snapshot_type entries_type = {
    .shape = WF_SNAPSHOT_SLOTS, .tag = 0x21, .size = sizeof(tree_entries), .alignment = 8,
    .element = &entry_type, .length_offset = 0, .capacity_offset = UINT64_MAX,
    .data_offset = 8, .fixed_capacity = 15
};
static const wf_snapshot_field node_fields[] = {
    {offsetof(tree_node, leading), &children_type}, {offsetof(tree_node, entries), &entries_type}
};
static const wf_snapshot_type node_type = {
    .shape = WF_SNAPSHOT_STRUCT, .tag = 0x10, .size = sizeof(tree_node), .alignment = 8,
    .count = 2, .fields = node_fields
};
static const wf_snapshot_field ordered_fields[] = {
    {offsetof(ordered_value, root), &children_type}, {offsetof(ordered_value, length), &integer}
};
static const wf_snapshot_type ordered_type = {
    .shape = WF_SNAPSHOT_ORDERED, .tag = 0x31, .size = sizeof(ordered_value), .alignment = 8,
    .count = 2, .fields = ordered_fields, .element = &node_type
};
static const wf_snapshot_type segments_type = {
    .shape = WF_SNAPSHOT_SEGMENTS, .tag = 0x23, .size = 8, .alignment = 8, .element = &octet
};
static const wf_snapshot_type boxed_segments = {
    .shape = WF_SNAPSHOT_BOX, .tag = 0x25, .size = 8, .alignment = 8, .element = &segments_type
};
static const wf_snapshot_type paged_type = {
    .shape = WF_SNAPSHOT_PAGED, .tag = 0x24, .size = sizeof(paged_bytes), .alignment = 8,
    .element = &octet, .length_offset = 0, .data_offset = 24, .page_elements = 2
};
static const wf_snapshot_type boxed_paged = {
    .shape = WF_SNAPSHOT_BOX, .tag = 0x25, .size = 8, .alignment = 8, .element = &paged_type
};
static const wf_snapshot_type array_type = {
    .shape = WF_SNAPSHOT_ARRAY, .tag = 0x20, .size = 2, .alignment = 1,
    .element = &octet, .length_offset = UINT64_MAX, .capacity_offset = UINT64_MAX,
    .fixed_capacity = 2
};
static const wf_snapshot_field closure_fields[] = {
    {offsetof(closure, bytes), &boxed_bytes}, {offsetof(closure, ring), &ring_type},
    {offsetof(closure, hash), &hash_type}, {offsetof(closure, ordered), &ordered_type},
    {offsetof(closure, segments), &boxed_segments}, {offsetof(closure, paged), &boxed_paged},
    {offsetof(closure, array), &array_type}, {offsetof(closure, nan), &floating},
    {offsetof(closure, boolean), &boolean_type}, {offsetof(closure, unit), &unit_type},
    {offsetof(closure, signed_bits), &signed_type}, {offsetof(closure, nan32), &float32_type}
};
static const wf_snapshot_type closure_type = {
    .shape = WF_SNAPSHOT_STRUCT, .tag = 0x10, .size = sizeof(closure), .alignment = 8,
    .count = 12, .fields = closure_fields
};

static uint64_t word(const unsigned char *p, unsigned n) {
    uint64_t value = 0;
    for (unsigned i = 0; i < n; ++i) value |= (uint64_t)p[i] << (8u * i);
    return value;
}
static wf_cmap *map_new(void) {
    wf_cmap *map = wf_cmap_create_entries(sizeof(test_slot), _Alignof(test_slot), 16);
    CHECK(map != NULL);
    return map;
}
static void put(wf_cmap *map, unsigned char key, uint64_t value) {
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    wf_cmap_entry entry;
    test_slot *slot = wf_cmap_lock_entry(user, &key, 1, 0, &entry);
    slot->tag = 1;
    slot->value = value;
    wf_cmap_unlock_entry(user, &entry, 0, 1);
}
static wf_snapshot_owner *capture(wf_cmap *map, const wf_snapshot_type *type,
                                  uint64_t scratch, uint64_t quota) {
    uint32_t error;
    wf_snapshot_owner *reserve = wf_snapshot_prepare(scratch, quota, &error);
    CHECK(reserve != NULL && error == 0);
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    uint64_t cut = 7;
    wf_snapshot_atomic_begin();
    /* Ordinary retries may call begin twice; activation still ends once. */
    wf_snapshot_atomic_begin();
    wf_cmap_hold(user);
    wf_snapshot_owner *job = wf_snapshot_capture(reserve, map, &cut, type,
        offsetof(test_slot, tag), sizeof(uint32_t), 0, offsetof(test_slot, value));
    wf_cmap_unhold(user);
    wf_snapshot_atomic_end();
    CHECK(job != NULL);
    return job;
}
static size_t collect(wf_snapshot_owner *job, unsigned char *bytes, size_t capacity) {
    size_t length = 0;
    void *cancel = wf__cancel_never();
    for (;;) {
        wf_completion_record record;
        wf_snapshot_read_result result;
        unsigned char window[23];
        int submitted = wf_snapshot_read_start(job, window, sizeof(window), 0, 0,
                                                cancel, &record, &result);
        if (submitted != 0) wf_snapshot_read_finish(job, window, &record, &result);
        CHECK(result.error == 0);
        if (result.count == 0) break;
        CHECK(result.count <= capacity - length);
        memcpy(bytes + length, window, (size_t)result.count);
        length += (size_t)result.count;
    }
    CHECK(!wf__shared_release(cancel));
    return length;
}
static wf_snapshot_outcome finish(wf_snapshot_owner *job, uint32_t stop) {
    wf_snapshot_outcome result;
    wf_snapshot_run_finish(job, stop);
    wf_snapshot_take_outcome(job, &result);
    return result;
}

/* Independent decoder of this scalar witness's exact wire grammar; record
 * order is unconstrained. A live-table a=99 control must fail the same oracle. */
static int replay_matches(const unsigned char *bytes, size_t size, uint64_t expected_a) {
    if (size < 25 || memcmp(bytes, "WFSNAP01", 8) != 0 || word(bytes + 8, 8) != 7) return 0;
    size_t at = 16;
    unsigned seen = 0;
    uint64_t records = 0;
    while (at < size && bytes[at] == 1) {
        if (size - at < 17 || word(bytes + at + 1, 8) != 1
            || word(bytes + at + 9, 8) != 7) return 0;
        at += 17;
        unsigned char body[10];
        size_t body_size = 0;
        while (at < size && bytes[at] == 2) {
            if (size - at < 5) return 0;
            uint64_t n = word(bytes + at + 1, 4);
            at += 5;
            if (n == 0 || n > 4096 || n > size - at || n > sizeof(body) - body_size) return 0;
            memcpy(body + body_size, bytes + at, (size_t)n);
            body_size += (size_t)n;
            at += (size_t)n;
        }
        if (at == size || bytes[at++] != 3 || body_size != 10 || body[1] != 8) return 0;
        if (body[0] == 'a') {
            if ((seen & 1) || word(body + 2, 8) != expected_a) return 0;
            seen |= 1;
        } else if (body[0] == 'b') {
            if ((seen & 2) || word(body + 2, 8) != 22) return 0;
            seen |= 2;
        } else return 0;
        ++records;
    }
    return seen == 3 && records == 2 && size - at == 9 && bytes[at] == 255
        && word(bytes + at + 1, 8) == records;
}

static void captured_image_and_control(void) {
    wf_cmap *map = map_new();
    put(map, 'a', 11);
    put(map, 'b', 22);
    put(map, 'c', 33);
    wf_cmap_entry removed;
    unsigned char removed_key = 'c';
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    test_slot *old = wf_cmap_lock_entry(user, &removed_key, 1, 0, &removed);
    old->tag = 0;
    wf_cmap_unlock_entry(user, &removed, 0, 0);
    wf_snapshot_owner *job = capture(map, &integer, 65536, 65536);
    put(map, 'a', 99);
    unsigned char bytes[128];
    size_t n = collect(job, bytes, sizeof(bytes));
    wf_snapshot_outcome result = finish(job, 0);
    CHECK(result.child == 0 && result.collection == 0 && result.abort_requested == 0);
    CHECK(result.bytes == n && result.records == 2 && result.delivered == n);
    CHECK(replay_matches(bytes, n, 11));
    CHECK(!replay_matches(bytes, n, 99));
    /* Exporting the post-cut live map fails the identical replay oracle. */
    job = capture(map, &integer, 65536, 65536);
    n = collect(job, bytes, sizeof(bytes));
    result = finish(job, 0);
    CHECK(result.child == 0);
    CHECK(!replay_matches(bytes, n, 11));
    wf_cmap_destroy(map);
}

static void exhaustion_and_discard(void) {
    uint32_t error;
    wf_snapshot_owner *reserve = wf_snapshot_prepare(0, 0, &error);
    CHECK(reserve != NULL && error == 0);
    wf_snapshot_discard(reserve);
    wf_cmap *map = map_new();
    wf_snapshot_owner *job = capture(map, &integer, 65536, 65536);
    unsigned char empty[32];
    size_t n = collect(job, empty, sizeof(empty));
    wf_snapshot_outcome result = finish(job, 0);
    CHECK(n == 25 && empty[16] == 255 && word(empty + 17, 8) == 0
          && result.child == 0 && result.collection == 0 && result.records == 0);
    put(map, 'a', 11);
    job = capture(map, &integer, 0, 65536);
    result = finish(job, 0);
    CHECK(result.child == 1 && result.detail == 0); /* ScratchExhausted */
    job = capture(map, &integer, 65536, 0);
    result = finish(job, 0);
    CHECK(result.child == 1 && result.detail == 1 && result.discarded == 0);
    job = capture(map, &integer, 65536, 17);
    result = finish(job, 0);
    CHECK(result.child == 1 && result.detail == 1 && result.discarded <= 17);
    job = capture(map, &integer, 65536, 65536);
    result = finish(job, 0);
    CHECK(result.child == 0 && result.collection == 1 && result.delivered == 0
          && result.discarded == result.bytes && result.records == 1);
    job = capture(map, &integer, 65536, 65536);
    result = finish(job, 1);
    CHECK(result.abort_requested == 1 && (result.child == 0 || result.child == 3));
    wf_cmap_destroy(map);
}

static void read_interruptions_retain_job(void) {
    wf_cmap *map = map_new();
    put(map, 'a', 11);
    put(map, 'b', 22);
    wf_snapshot_owner *job = capture(map, &integer, 65536, 65536);
    unsigned char window[23], expected[23];
    memset(window, 0x7e, sizeof(window));
    memcpy(expected, window, sizeof(window));
    wf_completion_record record;
    wf_snapshot_read_result result;
    void *never = wf__cancel_never();
    int started = wf_snapshot_read_start(job, window, sizeof(window), 0, 1,
                                        never, &record, &result);
    if (started != 0) wf_snapshot_read_finish(job, window, &record, &result);
    CHECK(result.error == 2 && result.count == 0);
    CHECK(memcmp(window, expected, sizeof(window)) == 0);
    CHECK(!wf__shared_release(never));
    void *cancel = wf__cancel_new();
    wf__shared_take(cancel, 1);
    wf__cancel_fire_held(cancel);
    started = wf_snapshot_read_start(job, window, sizeof(window), 0, 0,
                                    cancel, &record, &result);
    if (started != 0) wf_snapshot_read_finish(job, window, &record, &result);
    CHECK(result.error == 3 && result.count == 0);
    CHECK(memcmp(window, expected, sizeof(window)) == 0);
    CHECK(wf__shared_release(cancel));
    wf__shared_free(cancel);
    unsigned char bytes[128];
    size_t n = collect(job, bytes, sizeof(bytes));
    wf_snapshot_outcome outcome = finish(job, 0);
    CHECK(outcome.child == 0 && outcome.collection == 0);
    CHECK(replay_matches(bytes, n, 11));
    wf_cmap_destroy(map);
}

typedef struct expected_body { unsigned char bytes[15000]; size_t length; } expected_body;
static void expected_word(expected_body *body, uint64_t n, unsigned width) {
    CHECK(width <= sizeof(body->bytes) - body->length);
    for (unsigned i = 0; i < width; ++i)
        body->bytes[body->length++] = (unsigned char)(n >> (8u * i));
}
static void expected_scalar(expected_body *body, unsigned tag, uint64_t n, unsigned width) {
    expected_word(body, tag, 1);
    expected_word(body, n, width);
}
static void expected_count(expected_body *body, unsigned tag, uint64_t n) {
    expected_word(body, tag, 1);
    expected_word(body, n, 8);
}

/* An independently constructed logical body covers every storage shape,
 * large binary segmentation, deleted buckets, structural recursive order,
 * parent frees, infinities, signed zero and a signed NaN payload. Collection
 * order for a hash remains unconstrained; both orders of its two pairs pass. */
static void complete_owned_closure(void) {
    byte_slots *bytes = malloc(16 + 5000);
    CHECK(bytes != NULL);
    bytes->len = bytes->cap = 5000;
    for (unsigned i = 0; i < 5000; ++i) bytes->data[i] = (unsigned char)i;
    bucket_slots buckets = {4, 4, {{2, 7, 70}, {1, 0, 0}, {0, 0, 0}, {2, 9, 90}}};
    tree_node *leading = calloc(1, sizeof(*leading));
    tree_node *root = calloc(1, sizeof(*root));
    tree_node *after = calloc(1, sizeof(*after));
    CHECK(leading != NULL && root != NULL && after != NULL);
    leading->entries.len = 1;
    leading->entries.data[0].pair = (pair){1, UINT64_C(0x3fd0000000000000)};
    root->leading = (child_slot){1, {leading}};
    root->entries.len = 1;
    root->entries.data[0].pair = (pair){2, UINT64_C(0x7ff0000000000000)};
    root->entries.data[0].after = (child_slot){1, {after}};
    after->entries.len = 2;
    after->entries.data[0].pair = (pair){3, UINT64_C(0xfff0000000000000)};
    after->entries.data[1].pair = (pair){4, UINT64_C(0x8000000000000000)};
    _Alignas(uint64_t) unsigned char segments[35] = {0};
    uint64_t segment_header[] = {2, 0, 2, 3};
    memcpy(segments, segment_header, sizeof(segment_header));
    segments[32] = 's'; segments[33] = 0; segments[34] = 't';
    unsigned char page0[] = {'p', 0}, page1[] = {'q', 'x'};
    paged_bytes paged = {3, 4, 2, {page0, page1}};
    closure value = {
        bytes, {3, 3, {'b', 'c', 'x', 'a'}}, {&buckets, 99, 99},
        {{1, {root}}, 99}, segments, &paged, {0, 255}, UINT64_C(0xfff8000000000042),
        255, 0xad, UINT16_C(0xfffe), UINT32_C(0x7fc01234)
    };
    expected_body expected = {{0}, 0};
    expected_word(&expected, 'k', 1);
    expected_count(&expected, 0x10, 12);
    expected_word(&expected, 0x25, 1);
    expected_count(&expected, 0x21, 5000);
    for (unsigned i = 0; i < 5000; ++i) expected_scalar(&expected, 5, (unsigned char)i, 1);
    expected_count(&expected, 0x22, 3);
    for (unsigned i = 0; i < 3; ++i) expected_scalar(&expected, 5, (uint64_t)('a' + i), 1);
    expected_count(&expected, 0x30, 2);
    size_t hash_pairs = expected.length;
    expected_scalar(&expected, 8, 7, 8); expected_scalar(&expected, 8, 70, 8);
    expected_scalar(&expected, 8, 9, 8); expected_scalar(&expected, 8, 90, 8);
    expected_count(&expected, 0x31, 4);
    const uint64_t scores[] = {
        UINT64_C(0x3fd0000000000000), UINT64_C(0x7ff0000000000000),
        UINT64_C(0xfff0000000000000), UINT64_C(0x8000000000000000)
    };
    for (unsigned i = 0; i < 4; ++i) {
        expected_scalar(&expected, 8, i + 1, 8);
        expected_scalar(&expected, 10, scores[i], 8);
    }
    expected_word(&expected, 0x25, 1);
    expected_count(&expected, 0x23, 3);
    expected_scalar(&expected, 5, 's', 1); expected_scalar(&expected, 5, 0, 1);
    expected_scalar(&expected, 5, 't', 1);
    expected_word(&expected, 0x25, 1);
    expected_count(&expected, 0x24, 3);
    expected_scalar(&expected, 5, 'p', 1); expected_scalar(&expected, 5, 0, 1);
    expected_scalar(&expected, 5, 'q', 1);
    expected_count(&expected, 0x20, 2);
    expected_scalar(&expected, 5, 0, 1); expected_scalar(&expected, 5, 255, 1);
    expected_scalar(&expected, 10, UINT64_C(0xfff8000000000042), 8);
    expected_count(&expected, 0x11, 1);
    expected_word(&expected, 0, 8);
    expected_word(&expected, 0, 1);
    expected_scalar(&expected, 2, UINT16_C(0xfffe), 2);
    expected_scalar(&expected, 9, UINT32_C(0x7fc01234), 4);

    wf_cmap *map = wf_cmap_create_entries(sizeof(closure_slot), _Alignof(closure_slot), 16);
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    wf_cmap_entry entry;
    unsigned char key = 'k';
    closure_slot *slot = wf_cmap_lock_entry(user, &key, 1, 0, &entry);
    slot->tag = 1; slot->value = value;
    wf_cmap_unlock_entry(user, &entry, 0, 1);
    uint32_t error;
    wf_snapshot_owner *reserve = wf_snapshot_prepare(65536, 20000, &error);
    CHECK(reserve != NULL && error == 0);
    uint64_t cut = 7;
    wf_snapshot_atomic_begin();
    wf_cmap_hold(user);
    wf_snapshot_owner *job = wf_snapshot_capture(reserve, map, &cut, &closure_type,
        offsetof(closure_slot, tag), 4, 0, offsetof(closure_slot, value));
    wf_cmap_unhold(user);
    wf_snapshot_atomic_end();
    CHECK(job != NULL);
    free(bytes); free(leading); free(root); free(after);
    memset(&buckets, 0, sizeof(buckets));
    memset(segments, 0, sizeof(segments));
    memset(page0, 0, sizeof(page0)); memset(page1, 0, sizeof(page1));
    unsigned char stream[15000], body[15000];
    size_t n = collect(job, stream, sizeof(stream));
    wf_snapshot_outcome outcome = finish(job, 0);
    CHECK(outcome.child == 0 && outcome.collection == 0 && outcome.records == 1);
    CHECK(n > 33 && memcmp(stream, "WFSNAP01", 8) == 0
          && word(stream + 8, 8) == 7 && stream[16] == 1
          && word(stream + 17, 8) == 1 && word(stream + 25, 8) == 7);
    size_t at = 33, used = 0;
    unsigned chunks = 0;
    while (at < n && stream[at] == 2) {
        CHECK(n - at >= 5);
        uint64_t count = word(stream + at + 1, 4);
        at += 5;
        CHECK(count > 0 && count <= 4096 && count <= n - at
              && count <= sizeof(body) - used);
        if (chunks < 2) CHECK(count == 4096);
        memcpy(body + used, stream + at, (size_t)count);
        used += (size_t)count; at += (size_t)count; ++chunks;
    }
    CHECK(chunks == 3 && n - at == 10 && stream[at] == 3 && stream[at + 1] == 255
          && word(stream + at + 2, 8) == 1 && used == expected.length);
    if (memcmp(body, expected.bytes, used) != 0) {
        unsigned char pair_copy[18];
        memcpy(pair_copy, expected.bytes + hash_pairs, 18);
        memcpy(expected.bytes + hash_pairs, expected.bytes + hash_pairs + 18, 18);
        memcpy(expected.bytes + hash_pairs + 18, pair_copy, 18);
        CHECK(memcmp(body, expected.bytes, used) == 0);
    }
    wf_cmap_destroy(map);
}

/* Native fault control between settled host input and schema validation:
 * change fixed Array length 2 to 3. The child still supplies a complete
 * terminal and matching byte counts, so this must be malformed, not success
 * or truncation. The same production parser sees every other byte normally. */
static void malformed_fixed_length(void) {
    wf_cmap *map = map_new();
    put(map, 'a', UINT64_C(0xff00));
    wf_snapshot_owner *job = capture(map, &array_type, 65536, 65536);
    void *cancel = wf__cancel_never();
    uint64_t seen = 0;
    unsigned changed = 0;
    for (;;) {
        unsigned char window[23];
        wf_completion_record record;
        wf_snapshot_read_result result;
        int submitted = wf_snapshot_read_start(job, window, sizeof(window), 0, 0,
                                                cancel, &record, &result);
        if (submitted != 0) {
            int64_t count;
            int error;
            wf__completion_file_join(&record, &count, &error);
            CHECK(count >= 0 && error == 0);
            if (seen <= 40 && (uint64_t)count > 40 - seen) {
                CHECK(window[40 - seen] == 2);
                window[40 - seen] = 3;
                changed = 1;
            }
            wf_snapshot_read_finish(job, window, &record, &result);
        }
        CHECK(result.error == 0);
        if (result.count == 0) break;
        seen += result.count;
    }
    CHECK(changed && !wf__shared_release(cancel));
    wf_snapshot_outcome result = finish(job, 0);
    CHECK(result.child == 4 && result.detail == 3 && result.collection == 0
          && result.delivered == seen);
    wf_cmap_destroy(map);
}

int main(void) {
    wf_test_guard_start(30);
    captured_image_and_control();
    exhaustion_and_discard();
    read_interruptions_retain_job();
    complete_owned_closure();
    malformed_fixed_length();
    wf_test_guard_finish();
    return 0;
}
#else
int main(void) { return 77; }
#endif
