/* Native scope-table/lifecycle probes. Include the maintained units to set
 * the generation boundary and deterministic deque schedules without adding
 * test branches to production. Kept by completion-test while these units
 * own scope retirement and logical-work attribution. This phase's probes
 * use same-scope releases; cross-scope origin storage belongs to the next
 * pass and is not asserted to work by these tests. */
#define _GNU_SOURCE
#include "bridge.c"
#include "../ordinary_values.c"
#include "../sched/core.c"
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
    wf__heap_change(19);
    reading(&parent_view, 1u, 0u); /* Reused child slot has unrelated ancestry. */
    reading(&reused_view, 1u, 19u);
    reading(&grandchild_view, 0u, 0u);
    wf__heap_change(-19);
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
    void *block = wf__runtime_take(1u);
    uint64_t granted = wf__runtime_granted(1u);
    CHECK(granted == 512u); /* The pool's smallest granted block. */
    reading(&view, 1u, granted);
    CHECK(wf__heap_in_use() == baseline + granted); /* No double pool charge. */
    wf__body_scope_leave(&owner);
    close_scope(&owner, 1u); /* Inactive, but still has attributed bytes. */
    CHECK(wf__body_scope_enter(&owner));
    wf__runtime_give(block, 1u);
    /* Script a negative ledger: a clamped observation is not a close test. */
    wf__heap_change(-9);
    wf__body_scope_leave(&owner);
    reading(&view, 1u, 0u);
    close_scope(&owner, 1u);
    CHECK(wf__body_scope_enter(&owner));
    wf__heap_change(9);
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
    wf__heap_give(block, 17u); /* Opposite deltas in two single-writer rows. */
    wf__body_scope_leave(&owner);
    close_scope(&owner, 0u);
    wf_value next = open_scope(NULL), next_view = view_of(&next);
    CHECK(next.words[0] == owner.words[0]);
    reading(&next_view, 1u, 0u);
    CHECK(wf__body_scope_enter(&next));
    wf__heap_change(5);
    reading(&next_view, 1u, 5u); /* Old row cancellation survives reuse. */
    wf__heap_change(-5);
    wf__body_scope_leave(&next);
    close_scope(&next, 0u);
    reading(&view, 0u, 0u);
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
    wf__heap_change(23);
    while (!atomic_load(&race.observed_closed)) wf_prim_yield();
    wf__heap_change(-23);
    wf__body_scope_leave(&reused);
    close_scope(&reused, 0u);
    atomic_store(&race.stop, 1u);
    CHECK(pthread_join(reader, NULL) == 0);
}

static unsigned compute_calls;
static void scoped_task(void *frame) {
    unsigned expected = *(unsigned *)frame;
    CHECK(wf__scope_current() == expected);
    wf__heap_change(3);
    wf__heap_change(-3);
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
    default_pool_needs_no_writer_row();
    observation_and_retirement();
    compute_paths();
    capacity_and_generation_exhaustion();
    wf_test_guard_finish();
    puts("scope-test: PASS");
    return 0;
}
