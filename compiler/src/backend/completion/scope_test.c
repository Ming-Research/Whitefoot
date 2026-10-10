/* Native scope-table/lifecycle probes. Include the maintained units to set
 * the generation boundary and deterministic deque schedules without adding
 * test branches to production. Kept by completion-test while these units
 * own scope retirement and logical-work attribution. Exact inventories
 * below also cover storage retained after leave and released elsewhere. */
#define _GNU_SOURCE
#include "bridge.c"
#include "../ordinary_values.c"
#include "../sched/core.c"
#include "../keyed_table.c"
#include "../runtime_test_guard.h"
#include <pthread.h>

_Static_assert(WF_SCOPE_CAPACITY == 64u, "target capacity includes one default slot");

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "scope-test:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

static wf_value meter;

static wf_value open_scope(const wf_value *parent) {
    wf_scope_open_result result;
    if (parent == NULL) wf__body_scope_open(&result, &meter);
    else wf__body_scope_open_child(&result, &meter, parent);
    CHECK(result.tag == 0u);
    return result.ok.value;
}

static wf_value view_of(const wf_value *owner) {
    wf_value view;
    wf__body_scope_view(&view, owner);
    CHECK(memcmp(&view, owner, sizeof(view)) == 0);
    return view;
}

static void reading(const wf_value *view, unsigned present, uint64_t bytes) {
    wf_optional_bytes result;
    wf__body_scope_bytes(&result, view);
    CHECK(result.tag == present);
    if (present) CHECK(result.value == bytes);
}

static void close_scope(const wf_value *owner, unsigned busy) {
    wf_scope_close_result result;
    wf__body_scope_close(&result, &meter, owner);
    CHECK(result.tag == busy);
    if (busy) CHECK(memcmp(&result.err.error, owner, sizeof(*owner)) == 0);
}

static void table_and_readings(void) {
    uint64_t baseline = wf__heap_in_use();
    wf_value parent = open_scope(NULL), child = open_scope(&parent);
    wf_value grandchild = open_scope(&child);
    wf_value parent_view = view_of(&parent), child_view = view_of(&child);
    wf_value grandchild_view = view_of(&grandchild);
    CHECK(!wf__body_scope_enter(&child));
    CHECK(wf__scope_current() == 0u);
    CHECK(wf__body_scope_enter(&parent));
    wf_value other = open_scope(NULL), other_view = view_of(&other);
    /* A top-level open uses default even while another scope is current. */
    CHECK(!wf__body_scope_enter(&other));
    close_scope(&parent, 1u); /* Activity, even before any allocation. */
    void *a = wf__heap_take(7u);
    CHECK(a != NULL);
    CHECK(wf__body_scope_enter(&child));
    void *b = wf__heap_take(11u);
    CHECK(b != NULL);
    CHECK(wf__body_scope_enter(&grandchild));
    void *c = wf__heap_take(13u);
    CHECK(c != NULL);
    reading(&parent_view, 1u, 31u);
    reading(&child_view, 1u, 24u);
    reading(&grandchild_view, 1u, 13u);
    reading(&other_view, 1u, 0u);
    CHECK(wf__heap_in_use() == baseline + 31u);
    wf__heap_give(c, 13u);
    wf__body_scope_leave(&grandchild);
    CHECK(wf__scope_current() == child.words[0]);
    wf__heap_give(b, 11u);
    wf__body_scope_leave(&child);
    CHECK(wf__scope_current() == parent.words[0]);
    wf__heap_give(a, 7u);
    wf__body_scope_leave(&parent);
    CHECK(wf__scope_current() == 0u);
    reading(&parent_view, 1u, 0u);
    close_scope(&parent, 1u); /* Zero bytes still has open descendants. */
    close_scope(&child, 1u);
    close_scope(&grandchild, 0u);
    wf_value reused = open_scope(NULL), reused_view = view_of(&reused);
    CHECK(reused.words[0] == grandchild.words[0]);
    CHECK(reused.words[1] != grandchild.words[1]);
    CHECK(wf__body_scope_enter(&reused));
    wf__heap_change((unsigned)reused.words[0], 19);
    reading(&parent_view, 1u, 0u); /* Reused child slot has unrelated ancestry. */
    reading(&reused_view, 1u, 19u);
    reading(&grandchild_view, 0u, 0u);
    wf__heap_change((unsigned)reused.words[0], -19);
    wf__body_scope_leave(&reused);
    close_scope(&child, 0u);
    close_scope(&parent, 0u);
    close_scope(&other, 0u);
    close_scope(&reused, 0u);
    reading(&parent_view, 0u, 0u);
    CHECK(!wf__body_scope_enter(&grandchild));
    CHECK(wf__heap_in_use() == baseline);
}

static void storage_and_raw_zero(void) {
    wf_value owner = open_scope(NULL), view = view_of(&owner);
    uint64_t baseline = wf__heap_in_use();
    CHECK(wf__body_scope_enter(&owner));
    void *block = wf__runtime_take(1u, (unsigned)owner.words[0]);
    uint64_t granted = wf__runtime_granted(1u);
    CHECK(granted == 512u); /* The pool's smallest granted block. */
    reading(&view, 1u, granted);
    CHECK(wf__heap_in_use() == baseline + granted); /* No double pool charge. */
    wf__body_scope_leave(&owner);
    close_scope(&owner, 1u); /* Inactive, but still has attributed bytes. */
    CHECK(wf__body_scope_enter(&owner));
    wf__runtime_give(block, 1u, (unsigned)owner.words[0]);
    /* Script a negative ledger: a clamped observation is not a close test. */
    wf__heap_change((unsigned)owner.words[0], -9);
    wf__body_scope_leave(&owner);
    reading(&view, 1u, 0u);
    close_scope(&owner, 1u);
    CHECK(wf__body_scope_enter(&owner));
    wf__heap_change((unsigned)owner.words[0], 9);
    wf__body_scope_leave(&owner);
    close_scope(&owner, 0u);
    CHECK(wf__heap_in_use() == baseline);
}

static void *allocate_on_peer(void *opaque) {
    wf_value *owner = opaque;
    unsigned previous = wf__scope_swap((unsigned)owner->words[0]);
    void *block = wf__heap_take(17u);
    (void)wf__scope_swap(previous);
    return block;
}

static void rows_survive_thread_exit_and_reuse(void) {
    wf_value owner = open_scope(NULL), view = view_of(&owner);
    pthread_t peer;
    void *block;
    CHECK(wf__body_scope_enter(&owner));
    CHECK(pthread_create(&peer, NULL, allocate_on_peer, &owner) == 0);
    CHECK(pthread_join(peer, &block) == 0 && block != NULL);
    reading(&view, 1u, 17u);
    wf__body_scope_leave(&owner);
    close_scope(&owner, 1u);
    /* Both a different thread and a different current scope. */
    wf_value freeing = open_scope(NULL), freeing_view = view_of(&freeing);
    CHECK(wf__body_scope_enter(&freeing));
    wf__heap_give(block, 17u);
    reading(&view, 1u, 0u);
    reading(&freeing_view, 1u, 0u);
    wf__body_scope_leave(&freeing);
    close_scope(&freeing, 0u);
    close_scope(&owner, 0u);
    wf_value next = open_scope(NULL), next_view = view_of(&next);
    CHECK(next.words[0] == owner.words[0]);
    reading(&next_view, 1u, 0u);
    CHECK(wf__body_scope_enter(&next));
    wf__heap_change((unsigned)next.words[0], 5);
    reading(&next_view, 1u, 5u); /* Old row cancellation survives reuse. */
    wf__heap_change((unsigned)next.words[0], -5);
    wf__body_scope_leave(&next);
    close_scope(&next, 0u);
    reading(&view, 0u, 0u);
}

static void resize_and_empty_origins(void) {
    uint64_t baseline = wf__heap_in_use();
    wf_value parent = open_scope(NULL), owner = open_scope(&parent);
    wf_value view = view_of(&owner), parent_view = view_of(&parent);
    wf_value other = open_scope(NULL), other_view = view_of(&other);
    CHECK(wf__body_scope_enter(&parent));
    CHECK(wf__body_scope_enter(&owner));
    unsigned char *block = wf__heap_take(8u);
    void *empty = wf__heap_take(0u);
    CHECK(block != NULL && empty != NULL);
    CHECK((uintptr_t)block % 16u == 0u && (uintptr_t)empty % 16u == 0u);
    memset(block, 0x53, 8u);
    wf__body_scope_leave(&owner);
    wf__body_scope_leave(&parent);
    close_scope(&owner, 1u);
    close_scope(&parent, 1u);
    CHECK(wf__body_scope_enter(&other));
    block = wf__heap_retake(block, 8u, 73u);
    CHECK(block != NULL && (uintptr_t)block % 16u == 0u);
    for (unsigned i = 0; i < 8u; ++i) CHECK(block[i] == 0x53);
    reading(&view, 1u, 73u);
    reading(&parent_view, 1u, 73u);
    reading(&other_view, 1u, 0u);
    CHECK(wf__heap_in_use() == baseline + 73u);
    CHECK(wf__heap_retake(block, 73u, UINT64_MAX) == NULL);
    reading(&view, 1u, 73u); /* Failed resize leaves old storage and charge. */
    block = wf__heap_retake(block, 73u, 0u);
    CHECK(block != NULL && (uintptr_t)block % 16u == 0u);
    reading(&view, 1u, 0u);
    close_scope(&owner, 1u); /* Zero requested bytes still pin both blocks. */
    wf__heap_give(empty, 0u);
    close_scope(&owner, 1u); /* The other zero block can still grow. */
    block = wf__heap_retake(block, 0u, 1u);
    CHECK(block != NULL);
    reading(&view, 1u, 1u);
    close_scope(&owner, 1u);
    wf__heap_give(block, 1u);
    wf__heap_give(NULL, 0u);
    reading(&view, 1u, 0u);
    reading(&other_view, 1u, 0u);
    close_scope(&owner, 0u);
    close_scope(&parent, 0u);
    wf__body_scope_leave(&other);
    close_scope(&other, 0u);
    CHECK(wf__heap_in_use() == baseline);
}

static void destroy_cleared(void *map) { wf_cmap_destroy(map); }

static void map_origins(void) {
    uint64_t baseline = wf__heap_in_use();
    wf_value a = open_scope(NULL), b = open_scope(NULL);
    wf_value av = view_of(&a), bv = view_of(&b);
    CHECK(wf__body_scope_enter(&a));
    wf_cmap *left = wf_cmap_create_entries(8u, 8u, 1u);
    wf_cmap_user *user = wf_cmap_user_at(left, 0);
    wf_cmap_entry entry;
    uint64_t *value = wf_cmap_lock_entry(user, (const unsigned char *)"x", 1u, 0, &entry);
    *value = 1u;
    wf_cmap_unlock_entry(user, &entry, 0, 1);
    uint64_t control = wf__runtime_granted(sizeof(wf_cmap) + 64u) + wf__runtime_granted(16u);
    uint64_t table_bytes = wf__runtime_granted(sizeof(table)) + 512u;
    /* One 32-byte small node; unused chunk space is not a live request. */
    reading(&av, 1u, control + table_bytes + 32u);
    table *reserve = new_table(NULL, 16u);
    atomic_store(&left->retired, reserve);
    reclaim(left);
    reading(&av, 1u, control + table_bytes + 32u + 512u);
    wf__body_scope_leave(&a);
    CHECK(wf__body_scope_enter(&b));
    table *reused = new_table(left, 16u);
    reading(&av, 1u, control + table_bytes + 32u + 512u);
    reading(&bv, 1u, wf__runtime_granted(sizeof(table)));
    free_table(reused); /* Descriptor belongs to B, reused cells to A. */
    reading(&av, 1u, control + table_bytes + 32u);
    reading(&bv, 1u, 0u);
    wf_cmap *right = wf_cmap_create_entries(8u, 8u, 1u);
    wf_cmap_swap(left, right, 0u, 8u, 0u);
    reading(&av, 1u, control + table_bytes + 32u);
    reading(&bv, 1u, control + table_bytes);
    wf_cmap_holding hold;
    wf_cmap_hold_begin(&hold, right);
    wf_cmap_hold_whole(&hold);
    wf_cmap_hold_take(wf_cmap_user_at(right, 0), &hold);
    wf_cmap_clear(right, 0u, 8u, 0u, destroy_cleared);
    wf_cmap *cleared = wf_cmap_take_cleared(right);
    (void)wf_cmap_hold_release(&hold, 0u, 8u, 0u);
    wf_cmap_release_cleared(cleared);
    reading(&av, 1u, control); /* Clear drained A's node and table. */
    wf_cmap_destroy(left);
    wf_cmap_destroy(right);
    reading(&av, 1u, 0u);
    reading(&bv, 1u, 0u);
    wf__body_scope_leave(&b);
    close_scope(&a, 0u);
    close_scope(&b, 0u);
    CHECK(wf__heap_in_use() == baseline);
}

static void mapped_reserve_and_large_nodes(void) {
    uint64_t baseline = wf__heap_in_use();
    wf_value a = open_scope(NULL), av = view_of(&a);
    CHECK(wf__body_scope_enter(&a));
    wf_cmap *map = wf_cmap_create_entries(1024u, 16u, 65536u);
    wf_cmap_user *user = wf_cmap_user_at(map, 0);
    wf_cmap_entry entry;
    uint64_t *value = wf_cmap_lock_entry(user, (const unsigned char *)"big", 3u, 0, &entry);
    *value = 1u;
    wf_cmap_unlock_entry(user, &entry, 0, 1);
    uint64_t control = wf__runtime_granted(sizeof(wf_cmap) + 64u) + wf__runtime_granted(1024u);
    uint64_t descriptor = wf__runtime_granted(sizeof(table));
    uint64_t node_grant = wf__runtime_granted(1040u + 16u);
    reading(&av, 1u, control + descriptor + HUGE_BYTES + node_grant);
    table *reserve = new_table(NULL, HUGE_BYTES / sizeof(cell));
    atomic_store(&map->retired, reserve);
    reclaim(map);
    wf__body_scope_leave(&a);
    CHECK(wf_cmap_release_reserve(map) == HUGE_BYTES);
    reading(&av, 1u, control + descriptor + HUGE_BYTES + node_grant);
    CHECK(wf_cmap_drain(map) != NULL);
    reading(&av, 1u, control + descriptor + HUGE_BYTES + node_grant);
    CHECK(wf_cmap_drain(map) == NULL);
    reading(&av, 1u, control + descriptor + HUGE_BYTES);
    close_scope(&a, 1u);
    wf_cmap_destroy(map);
    reading(&av, 1u, 0u);
    close_scope(&a, 0u);
    CHECK(wf__heap_in_use() == baseline);
}

static void context_and_shared_storage_origins(void) {
    uint64_t baseline = wf__heap_in_use();
    wf_value a = open_scope(NULL), av = view_of(&a);
    wf_value b = open_scope(NULL), bv = view_of(&b);
    CHECK(wf__body_scope_enter(&a));
    (void)wf__context_prepare(16u);
    wf_context *context = wf_context_prepared;
    wf_context_prepared = NULL;
    void *shared = wf__shared_new(8u);
    uint64_t a_bytes = wf__runtime_granted(sizeof(wf_context)) + 1024u + 512u;
    wf__body_scope_leave(&a);
    CHECK(wf__body_scope_enter(&b));
    void *frame = wf_context_allocate(context, 8192u);
    uint64_t b_bytes = wf__runtime_granted(WF_CONTEXT_CHUNK_HEADER + 8192u + WF_CONTEXT_CHUNK_SLACK);
    wf_context_release(context, frame); /* Empty spare retains B's origin. */
    wf__body_scope_leave(&b);
    reading(&av, 1u, a_bytes);
    reading(&bv, 1u, b_bytes);
    close_scope(&a, 1u);
    close_scope(&b, 1u);
    /* Driver cleanup runs with neither allocation origin current. */
    wf_context_release_arena(context);
    wf_pool_give_for(context, context->pool_bytes, context->origin);
    reading(&av, 1u, 512u);
    reading(&bv, 1u, 0u);
    CHECK(wf__shared_release(shared));
    wf__shared_free(shared);
    close_scope(&a, 0u);
    close_scope(&b, 0u);
    CHECK(wf__heap_in_use() == baseline);
}

static void key_store_growth_and_release(void) {
    wf_cmap_key_set_drop_spare();
    uint64_t baseline = wf__heap_in_use();
    wf_value a = open_scope(NULL), av = view_of(&a);
    wf_value b = open_scope(NULL), bv = view_of(&b);
    CHECK(wf__body_scope_enter(&a));
    wf_key_set set;
    wf_cmap_key_set_new(&set, 1u);
    CHECK(wf_cmap_key_set_insert(&set, (const unsigned char *)"a", 1u) == 0u);
    reading(&av, 1u, wf__runtime_granted(store_bytes(8u)) + 512u);
    wf__body_scope_leave(&a);
    CHECK(wf__body_scope_enter(&b));
    for (unsigned char key = 0u; key < 32u; ++key)
        (void)wf_cmap_key_set_insert(&set, &key, 1u);
    unsigned char long_key[128] = {0};
    (void)wf_cmap_key_set_insert(&set, long_key, sizeof(long_key));
    /* Both replacement structures keep A, although growth runs in B. */
    reading(&av, 1u, wf__runtime_granted(store_bytes(64u)) + 512u);
    reading(&bv, 1u, 0u);
    wf_cmap_key_set_release(&set);
    CHECK(wf_key_set_spare == NULL);
    reading(&av, 1u, 0u);
    reading(&bv, 1u, 0u);
    wf__body_scope_leave(&b);
    close_scope(&a, 0u);
    close_scope(&b, 0u);
    /* A cached default store can acquire a scoped byte arena on reuse. */
    wf_cmap_key_set_new(&set, 1u);
    wf_cmap_key_set_release(&set);
    CHECK(wf_key_set_spare != NULL);
    wf_value c = open_scope(NULL), cv = view_of(&c);
    CHECK(wf__body_scope_enter(&c));
    wf_cmap_key_set_new(&set, 1u);
    CHECK(wf_cmap_key_set_insert(&set, (const unsigned char *)"c", 1u) == 0u);
    reading(&cv, 1u, 512u);
    wf_cmap_key_set_release(&set);
    CHECK(wf_key_set_spare == NULL);
    reading(&cv, 1u, 0u);
    wf__body_scope_leave(&c);
    close_scope(&c, 0u);
    CHECK(wf__heap_in_use() == baseline);
}

static void *default_pool_on_host_thread(void *unused) {
    (void)unused;
    return wf__runtime_take_default(1u);
}

static void default_pool_needs_no_writer_row(void) {
    unsigned rows = atomic_load(&wf_heap_counter_count);
    uint64_t baseline = wf__heap_in_use();
    pthread_t host;
    void *block;
    CHECK(pthread_create(&host, NULL, default_pool_on_host_thread, NULL) == 0);
    CHECK(pthread_join(host, &block) == 0 && block != NULL);
    CHECK(atomic_load(&wf_heap_counter_count) == rows);
    CHECK(wf__heap_in_use() == baseline + 512u);
    wf_value owner = open_scope(NULL), view = view_of(&owner);
    CHECK(wf__body_scope_enter(&owner));
    wf__runtime_give_default(block, 1u);
    reading(&view, 1u, 0u);
    wf__body_scope_leave(&owner);
    close_scope(&owner, 0u);
    CHECK(wf__heap_in_use() == baseline);
}

typedef struct {
    wf_value view;
    _Atomic unsigned started, closed, observed_closed, stop;
} observation_race;

static void *observe_while_closing(void *opaque) {
    observation_race *race = opaque;
    atomic_store(&race->started, 1u);
    while (!atomic_load(&race->stop)) {
        unsigned closed = atomic_load(&race->closed);
        wf_optional_bytes bytes;
        wf__body_scope_bytes(&bytes, &race->view);
        if (closed) {
            CHECK(bytes.tag == 0u);
            atomic_store(&race->observed_closed, 1u);
        } else {
            CHECK(bytes.tag == 0u || (bytes.tag == WF_OPTION_SOME && bytes.value == 0u));
        }
    }
    return NULL;
}

static void observation_and_retirement(void) {
    wf_value owner = open_scope(NULL);
    observation_race race = { .view = view_of(&owner) };
    pthread_t reader;
    CHECK(pthread_create(&reader, NULL, observe_while_closing, &race) == 0);
    while (!atomic_load(&race.started)) wf_prim_yield();
    close_scope(&owner, 0u);
    atomic_store(&race.closed, 1u);
    wf_value reused = open_scope(NULL);
    CHECK(reused.words[0] == owner.words[0]);
    CHECK(wf__body_scope_enter(&reused));
    wf__heap_change((unsigned)reused.words[0], 23);
    while (!atomic_load(&race.observed_closed)) wf_prim_yield();
    wf__heap_change((unsigned)reused.words[0], -23);
    wf__body_scope_leave(&reused);
    close_scope(&reused, 0u);
    atomic_store(&race.stop, 1u);
    CHECK(pthread_join(reader, NULL) == 0);
}

static unsigned compute_calls;
static void scoped_task(void *frame) {
    unsigned expected = *(unsigned *)frame;
    CHECK(wf__scope_current() == expected);
    void *block = wf__heap_take(3u);
    CHECK(block != NULL);
    wf__heap_give(block, 3u);
    compute_calls += 1u;
}

static void *publish_task(unsigned scope) {
    unsigned previous = wf__scope_swap(scope);
    unsigned *frame = wf__par_acquire_lane(sizeof(*frame));
    CHECK(frame != NULL);
    *frame = scope;
    wf__par_publish(frame, scoped_task);
    (void)wf__scope_swap(previous);
    return frame;
}

static void compute_paths(void) {
    wf_value owner = open_scope(NULL);
    CHECK(wf__body_scope_enter(&owner));
    wf__par_prepare(&wf__par_lanes[0], 0);
    wf__par_self = &wf__par_lanes[0];
    void *first = publish_task((unsigned)owner.words[0]);
    (void)wf__scope_swap(0u);
    wf__par_join(first); /* Immediate owner reclaim. */
    CHECK(wf__scope_current() == 0u);
    wf__par_release(first);
    first = publish_task((unsigned)owner.words[0]);
    void *second = publish_task(0u);
    wf__par_join(first); /* Help second, then reclaim first inside the loop. */
    CHECK(wf__scope_current() == 0u);
    wf__par_release(second);
    wf__par_release(first);
    first = publish_task((unsigned)owner.words[0]);
    struct wf__par_slot *stolen = wf__par_steal(&wf__par_lanes[0]);
    CHECK(stolen == first);
    wf__par_execute(stolen);
    CHECK(wf__scope_current() == 0u);
    wf__par_join(first);
    wf__par_release(first);
    CHECK(compute_calls == 4u);
    wf__par_self = NULL;
    (void)wf__scope_swap((unsigned)owner.words[0]);
    wf__body_scope_leave(&owner);
    close_scope(&owner, 0u);
}

static void capacity_and_generation_exhaustion(void) {
    wf_value owners[WF_SCOPE_CAPACITY - 1u], views[WF_SCOPE_CAPACITY - 1u];
    for (unsigned i = 0; i < WF_SCOPE_CAPACITY - 1u; ++i) {
        owners[i] = open_scope(NULL);
        views[i] = view_of(&owners[i]);
    }
    wf_scope_open_result refused;
    wf__body_scope_open(&refused, &meter);
    CHECK(refused.tag == 1u && refused.err.error == 0u);
    wf__body_scope_open_child(&refused, &meter, &owners[0]);
    CHECK(refused.tag == 1u && refused.err.error == 0u);
    for (unsigned i = 0; i < WF_SCOPE_CAPACITY - 1u; ++i) {
        reading(&views[i], 1u, 0u);
        close_scope(&owners[i], 0u);
    }
    /* Set a retired slot at the last assignable generation, not a smaller
     * test-only domain; the production overflow guard must reject wrapping. */
    wf_scopes[1].generation = UINT64_MAX - 1u;
    wf_value last = open_scope(NULL), last_view = view_of(&last);
    CHECK(last.words[0] == 1u && last.words[1] == UINT64_MAX);
    close_scope(&last, 0u);
    for (unsigned i = 0; i < WF_SCOPE_CAPACITY - 2u; ++i) {
        owners[i] = open_scope(NULL);
        CHECK(owners[i].words[0] != 1u);
    }
    wf__body_scope_open(&refused, &meter);
    CHECK(refused.tag == 1u && refused.err.error == 0u);
    reading(&last_view, 0u, 0u);
    for (unsigned i = 0; i < WF_SCOPE_CAPACITY - 2u; ++i) close_scope(&owners[i], 0u);
}

int main(void) {
    wf_test_guard_start(30);
    table_and_readings();
    storage_and_raw_zero();
    rows_survive_thread_exit_and_reuse();
    resize_and_empty_origins();
    map_origins();
    mapped_reserve_and_large_nodes();
    context_and_shared_storage_origins();
    key_store_growth_and_release();
    default_pool_needs_no_writer_row();
    observation_and_retirement();
    compute_paths();
    capacity_and_generation_exhaustion();
    wf_test_guard_finish();
    puts("scope-test: PASS");
    return 0;
}
