/* Whole-role reassignment must reap the submitting driver's native ring.
 * The empty pipe and OUTSIDE barrier choose the schedule; the guard only
 * diagnoses a hang. Handwritten entries are shared with driver_service_test. */
#define _GNU_SOURCE
#include "bridge.c"
#include "../runtime_test_guard.h"
#include <pthread.h>

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "driver-service-native-test:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

#if defined(__linux__)

enum { FRAME_READ, FRAME_COMPUTE };
typedef struct native_service_frame {
    unsigned kind, stage;
    int done, descriptor;
    unsigned char before, bytes[4], after;
} native_service_frame;

static const unsigned char payload[4] = {'r', 'i', 'n', 'g'};
static _Atomic unsigned computing, release_compute, resumed, destroyed_read, destroyed_root;
static uint64_t group[2] = {1u, 0u};

void wf_driver_service_test_resume(void *opaque) {
    native_service_frame *frame = opaque;
    CHECK(wf_driver_service == NULL);
    CHECK((wf_context_outside & WF_DRIVER_PHASE_MASK) == WF_DRIVER_OUTSIDE);
    if (frame->kind == FRAME_COMPUTE) {
        CHECK(wf_executor_index == 0u);
        atomic_store(&computing, 1u);
        /* Keep the submitter in userspace: even sched_yield would let it
         * enter the kernel and run deferred task work during the hold. */
        while (!atomic_load(&release_compute)) wf_prim_spin_hint();
        CHECK(wf_driver_service == NULL);
        CHECK(group[0] == 0u && atomic_load(&resumed) == 1u);
        frame->done = 1;
    } else if (frame->stage == 0u) {
        CHECK(wf_executor_index == 0u);
        wf_completion_record *record = (wf_completion_record *)wf__context_operation();
        wf__completion_file_read_submit(frame->descriptor, frame->bytes, sizeof(frame->bytes), record);
        CHECK(record->route == WF_COMPLETION_ROUTE_LINUX_IO_URING);
        CHECK(wf_bridge_record_state(record) == WF_COMPLETION_PENDING);
        frame->stage = 1u;
        CHECK(wf__context_wait(record, frame));
    } else {
        CHECK(wf_executor_index == 1u && wf_driver_self == &wf_driver_root);
        CHECK(atomic_load(&computing) && !atomic_load(&release_compute));
        wf_completion_record *record = (wf_completion_record *)wf__context_operation();
        CHECK(wf_bridge_record_state(record) == WF_COMPLETION_DONE);
        CHECK(record->result.kind == WF_FILE_READ && record->result.error_code == 0);
        CHECK(record->result.value == (int64_t)sizeof(payload));
        CHECK(memcmp(frame->bytes, payload, sizeof(payload)) == 0);
        CHECK(frame->before == 0xa5u && frame->after == 0x5au);
        CHECK(atomic_fetch_add(&resumed, 1u) == 0u);
        frame->done = 1;
    }
}

int wf_driver_service_test_done(void *opaque) {
    return ((native_service_frame *)opaque)->done;
}

void wf_driver_service_test_destroy(void *opaque) {
    native_service_frame *frame = opaque;
    if (frame->kind == FRAME_READ) {
        CHECK(wf_executor_index == 1u && atomic_load(&resumed) == 1u);
        CHECK(atomic_fetch_add(&destroyed_read, 1u) == 0u);
    } else {
        CHECK(wf_executor_index == 0u && atomic_load(&wf_context_root_done));
        CHECK(atomic_fetch_add(&destroyed_root, 1u) == 0u);
    }
}

typedef struct native_owner {
    wf_context *reader;
    wf_context timer;
} native_owner;

static void *owner_main(void *opaque) {
    native_owner *owner = opaque;
    wf_executor_index = 0u;
    wf_executor_bind(&wf_driver_root);
    CHECK(wf_context_run(&wf_driver_root, owner->reader));
    CHECK(wf_driver_root.parked == owner->reader);
    /* A separate due sleep triggers reassignment without cancelling the
     * pending read. The monitor's instant is explicit fixture data. */
    wf_completion_record *timer = wf_bridge_begin(owner->timer.operation.bytes);
    timer->request.kind = WF_FILE_SLEEP;
    timer->route = WF_COMPLETION_ROUTE_TIMER;
    atomic_store(&timer->deadline, 1u);
    owner->timer.driver = &wf_driver_root;
    owner->timer.record = timer;
    wf_context_adopt_wait(&wf_driver_root, &owner->timer);
    /* Submit on the original physical thread before it enters the barrier.
     * It performs no ring entry, progress or park until released. */
    wf_bridge_ring_flush();
    CHECK(wf_linux_io_uring_in_flight(wf_bridge_linux_current()) == 1u);
    CHECK(!wf_context_run(&wf_driver_root, &wf_context_root));
    wf_executor_bind(NULL);
    /* The displaced executor really joins the spare pool. Root ingress
     * adoption wakes it through the production executor-zero endpoint. */
    wf_executor_loop(&wf_executors[0], NULL);
    atomic_store(&wf_executors[0].exited, 1u);
    wf__coro_destroy(wf_context_root.root);
    return NULL;
}

int main(void) {
    wf_test_guard_start(30);
    wf_bridge_require();
    if (!wf_bridge_ring_ready()) {
        const char *required = getenv("WF_REQUIRE_LINUX_IO_URING");
        if (required != NULL && strcmp(required, "1") == 0) CHECK(wf_bridge_ring_ready());
        fputs("driver-service-native-test: io_uring qualification unavailable\n", stderr);
        wf_test_guard_finish();
        return 77;
    }
    CHECK(wf_bridge_linux_adapter.initialized);
    CHECK(wf_file_adapter_helper_count(&wf_bridge_adapter) == 0u);
    int descriptors[2];
    CHECK(pipe(descriptors) == 0);
    native_service_frame reader_frame = {
        .kind = FRAME_READ, .descriptor = descriptors[0],
        .before = 0xa5u, .bytes = {0xccu, 0xccu, 0xccu, 0xccu}, .after = 0x5au
    };
    native_service_frame compute_frame = {.kind = FRAME_COMPUTE};
    wf__context_root_begin();
    wf_context_root.root = wf_context_root.resume = &compute_frame;
    size_t granted;
    wf_context *reader = wf_pool_take(sizeof(*reader), &granted);
    memset(reader, 0, sizeof(*reader));
    reader->pool_bytes = granted;
    reader->driver = &wf_driver_root;
    reader->root = reader->resume = &reader_frame;
    reader->group = group;
    atomic_store(&wf_context_live, 1u);
    wf_executor_init(1u, NULL);
    wf_executor_count = 2u;
    wf_executors[1].available = 1u;
    atomic_store(&wf_driver_stats_count, 2u);
    /* Observe a fresh publication when the displaced executor joins spares. */
    atomic_store(&wf_executors[0].ready, 0u);
    wf_context_current = NULL;
    wf_executor_bind(NULL);
    native_owner owner = {.reader = reader};
    pthread_t thread;
    wf_test_guard_phase("submit pipe read, flush on owner, hold owner OUTSIDE");
    CHECK(pthread_create(&thread, NULL, owner_main, &owner) == 0);
    while (!atomic_load(&computing)) wf_prim_yield();
    wf_completion_record *record = (wf_completion_record *)reader->operation.bytes;
    CHECK(record->route == WF_COMPLETION_ROUTE_LINUX_IO_URING);
    CHECK(atomic_load(&record->issued) == 1u);
    CHECK(wf_bridge_record_state(record) == WF_COMPLETION_PENDING);
    CHECK(__atomic_load_n(wf_bridge_linux_adapter.submission_head, __ATOMIC_ACQUIRE)
        == __atomic_load_n(wf_bridge_linux_adapter.submission_tail, __ATOMIC_ACQUIRE));
    CHECK(wf_driver_root.timer_count == 1u);
    CHECK(wf_driver_root.timers[0] == &owner.timer);
    wf_test_guard_phase("reserve and claim the submitting driver's whole role");
    CHECK(wf_driver_reassign(&wf_driver_root, WF_HANDOFF_TAU_NS + 2u));
    wf_spin_lock(&wf_executor_lock);
    CHECK(wf_executors[1].assignment == &wf_driver_root);
    wf_executors[1].assignment = NULL;
    wf_spin_unlock(&wf_executor_lock);
    wf_executor_index = 1u;
    wf_driver_acquire_reserved(&wf_driver_root);
    CHECK(wf_bridge_linux_current() == &wf_bridge_linux_adapter);
    CHECK(wf_driver_service == &wf_driver_root && !atomic_load(&wf_drivers_moving));
    CHECK(wf_driver_expire_wait(&wf_driver_root, &owner.timer, WF_HANDOFF_TAU_NS + 2u, 0));
    CHECK(wf_run_take(&wf_driver_root) == &owner.timer);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    CHECK(wf_driver_root.parked == reader && !atomic_load(&resumed));

    wf_test_guard_phase("replacement publishes native read while submitter remains OUTSIDE");
    CHECK(write(descriptors[1], payload, sizeof(payload)) == (ssize_t)sizeof(payload));
    /* Wait for the real CQE without entering the ring. The negative image
     * sees the same completed kernel read, then omits only publication. */
    wf_linux_io_uring_adapter *adapter = wf_bridge_linux_current();
    unsigned head = __atomic_load_n(adapter->completion_head, __ATOMIC_ACQUIRE);
    while (head == __atomic_load_n(adapter->completion_tail, __ATOMIC_ACQUIRE)) wf_prim_yield();
    struct io_uring_cqe completion = adapter->completion_entries[head & *adapter->completion_mask];
    CHECK(completion.user_data == (uint64_t)(uintptr_t)record);
    CHECK(completion.res == (int)sizeof(payload));
    CHECK(wf_bridge_record_state(record) == WF_COMPLETION_PENDING);
#if !defined(WF_DRIVER_SERVICE_TEST_REPLACEMENT_NO_REAP)
    /* Test-only omission in the control image; bridge.c and every production
     * object retain their ordinary bytes and have no conditional hook. */
    (void)wf_bridge_progress();
#endif
    CHECK(wf_bridge_record_state(record) == WF_COMPLETION_DONE);
    CHECK(record->result.kind == WF_FILE_READ && record->result.error_code == 0);
    CHECK(record->result.value == (int64_t)sizeof(payload));
    CHECK(record->request.operation.read.buffer == reader_frame.bytes);
    CHECK(memcmp(reader_frame.bytes, payload, sizeof(payload)) == 0);
    CHECK(reader_frame.before == 0xa5u && reader_frame.after == 0x5au);
    CHECK(!atomic_load(&destroyed_read) && !atomic_load(&release_compute));
    CHECK(wf_linux_io_uring_in_flight(adapter) == 0u);
    CHECK(wf_run_take(&wf_driver_root) == reader);
    CHECK(wf_context_run(&wf_driver_root, reader));
    CHECK(atomic_load(&resumed) == 1u && atomic_load(&destroyed_read) == 1u);
    CHECK(group[0] == 0u && !atomic_load(&wf_context_live));
    CHECK(wf_run_take(&wf_driver_root) == NULL && wf_driver_root.parked == NULL);

    wf_test_guard_phase("displaced executor commits ingress and joins spares once");
    atomic_store(&release_compute, 1u);
    while (!atomic_load(&wf_executors[0].ready)) wf_prim_yield();
    CHECK(atomic_load(&wf_driver_root.ingress_count) == 1u);
    CHECK(atomic_load(&wf_driver_stats[0].ingress_commits) == 1u);
    CHECK(!atomic_load(&wf_active_invocations) && !atomic_load(&destroyed_root));
    CHECK(wf_driver_adopt_ingress(&wf_driver_root));
    CHECK(!wf_driver_adopt_ingress(&wf_driver_root));
    CHECK(pthread_join(thread, NULL) == 0);
    CHECK(atomic_load(&wf_executors[0].exited) && atomic_load(&destroyed_root) == 1u);
    CHECK(atomic_load(&wf_driver_stats[0].reassignments) == 1u);
    CHECK(atomic_load(&resumed) == 1u && atomic_load(&destroyed_read) == 1u);
    CHECK(wf_run_take(&wf_driver_root) == NULL && wf_driver_root.parked == NULL);
    CHECK(!wf_driver_root.timer_count && !atomic_load(&wf_driver_root.host_waits));
    CHECK(!atomic_load(&wf_driver_root.ingress_count) && wf_drivers_quiescent());
    CHECK(close(descriptors[0]) == 0 && close(descriptors[1]) == 0);
    CHECK(wf_completion_runtime_destroy(&wf_executors[0].wake) == 0);
    CHECK(wf_completion_runtime_destroy(&wf_executors[1].wake) == 0);
    wf_pool_give(wf_driver_root.timers, wf_driver_root.timer_bytes);
    wf_executor_bind(NULL);
    atomic_store(&wf_driver_count, 0u);
    atomic_store(&wf_drivers[0], NULL);
    wf_test_guard_finish();
    return 0;
}

#else

void wf_driver_service_test_resume(void *opaque) { (void)opaque; }
int wf_driver_service_test_done(void *opaque) { (void)opaque; return 1; }
void wf_driver_service_test_destroy(void *opaque) { (void)opaque; }

int main(void) {
    fputs("driver-service-native-test: io_uring qualification unavailable (needs Linux)\n", stderr);
    return 77;
}

#endif
