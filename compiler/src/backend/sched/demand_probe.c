/* Request posting, publication ordering and TLS retirement on native threads.
 * Hooks hold the real protocol between its pointer load and last TLS store;
 * no substitute request or cleanup implementation can make this test pass. */
#define WF_SCHED_REQUEST_TEST 1
#include "core.c"
#include "../runtime_test_guard.h"
#if defined(_WIN32)
#include <process.h>
#endif

static unsigned executed;
static unsigned inject_after_clear;
static unsigned requester_acquired;
static unsigned requester_release;
static unsigned owner_ready;
static unsigned owner_exit;
static unsigned owner_withdrawn;
static unsigned owner_released;
static _Thread_local int hold_request;
static _Thread_local int exiting_owner;
static struct wf__par_lane *exit_lane;

static void check(int condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "scheduler request probe: %s\n", message);
        abort();
    }
}
static void wf_sched_request_test_acquired(void) {
    if (!hold_request) return;
    __atomic_store_n(&requester_acquired, 1u, __ATOMIC_RELEASE);
    while (!__atomic_load_n(&requester_release, __ATOMIC_ACQUIRE)) wf_prim_yield();
}
static void wf_sched_request_test_withdrawn(void) {
    if (exiting_owner)
        __atomic_store_n(&owner_withdrawn, 1u, __ATOMIC_RELEASE);
}
static void wf_sched_request_test_released(void) {
    if (!exiting_owner) return;
    check(__atomic_load_n(&exit_lane->request_users, __ATOMIC_SEQ_CST) == 0,
          "TLS cleanup returned with an in-flight requester");
    __atomic_store_n(&owner_released, 1u, __ATOMIC_RELEASE);
}
static void wf_sched_request_test_cleared(void) {
    if (!inject_after_clear) return;
    check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) == 0,
          "publication did not clear its request first");
    check(wf__par_self->top == wf__par_self->bottom,
          "request was cleared after the deque push");
    wf__par_request(wf__par_self);
}
static void run(void *frame) { (void)frame; executed += 1; }

static void owner_body(void) {
    exiting_owner = 1;
    wf__par_register_word(exit_lane);
    check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) == 1,
          "new native owner has no self request");
    __atomic_store_n(&wf__par_demand_word, (uint64_t)0, __ATOMIC_RELAXED);
    __atomic_store_n(&owner_ready, 1u, __ATOMIC_RELEASE);
    while (!__atomic_load_n(&owner_exit, __ATOMIC_ACQUIRE)) wf_prim_yield();
    /* Returning from a native thread, outside wf_prim_thread_start, must run
     * the installed cleanup before the operating system frees this TLS. */
}
static void requester_body(void) {
    hold_request = 1;
    wf__par_request(exit_lane);
}
#if defined(_WIN32)
static unsigned __stdcall owner_main(void *unused) { (void)unused; owner_body(); return 0; }
static unsigned __stdcall requester_main(void *unused) { (void)unused; requester_body(); return 0; }
typedef HANDLE probe_thread;
static probe_thread start(unsigned (__stdcall *entry)(void *)) {
    uintptr_t thread = _beginthreadex(NULL, 0, entry, NULL, 0, NULL);
    check(thread != 0, "native thread creation failed");
    return (HANDLE)thread;
}
static void join(probe_thread thread) {
    check(WaitForSingleObject(thread, INFINITE) == WAIT_OBJECT_0, "native join failed");
    check(CloseHandle(thread) != 0, "native handle close failed");
}
#else
static void *owner_main(void *unused) { (void)unused; owner_body(); return NULL; }
static void *requester_main(void *unused) { (void)unused; requester_body(); return NULL; }
typedef pthread_t probe_thread;
static probe_thread start(void *(*entry)(void *)) {
    pthread_t thread;
    check(pthread_create(&thread, NULL, entry, NULL) == 0, "native thread creation failed");
    return thread;
}
static void join(probe_thread thread) {
    check(pthread_join(thread, NULL) == 0, "native join failed");
}
#endif

int main(void) {
    wf_test_guard_start(60);
    wf_test_guard_phase("request posting and publication");
    wf__par_prepare(&wf__par_lanes[0], 0);
    wf__par_prepare(&wf__par_lanes[1], 1);
    __atomic_store_n(&wf__par_lane_count, 2, __ATOMIC_RELAXED);
    struct wf__par_lane *idle = &wf__par_lanes[0];
    struct wf__par_lane *victim = &wf__par_lanes[1];
    wf__par_self = idle;
    check(wf__par_find(idle) == NULL, "empty lanes offered a task");
    check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) == 0,
          "a missing registration received a request");
    wf__par_register_word(victim);
    check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) == 1,
          "registration did not request itself");
    __atomic_store_n(&wf__par_demand_word, (uint64_t)0, __ATOMIC_RELAXED);
    check(wf__par_find(idle) == NULL, "requesting created a task");
    check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) == 1,
          "failed scan did not request a registered victim");
    __atomic_store_n(&wf__par_demand_word, (uint64_t)7, __ATOMIC_RELAXED);
    wf__par_request(victim);
    check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) == 7,
          "write-if-zero overwrote an outstanding request");
    wf__par_self = victim;
    void *frame = wf__par_acquire_lane(8);
    check(frame != NULL, "empty lane refused a frame");
    wf__par_publish(frame, run);
    check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) == 0,
          "publication retained an old request");
    wf__par_join(frame);
    wf__par_release(frame);
    frame = wf__par_acquire_lane(8);
    check(frame != NULL, "released frame was lost");
    inject_after_clear = 1;
    wf__par_publish(frame, run);
    check(__atomic_load_n(&wf__par_demand_word, __ATOMIC_RELAXED) == 1,
          "publication erased a later request");
    wf__par_join(frame);
    wf__par_release(frame);
    check(executed == 2, "a published task was lost or duplicated");
    inject_after_clear = 0;

    wf_test_guard_phase("native owner exit with an in-flight requester");
    exit_lane = idle;
    probe_thread owner = start(owner_main);
    while (!__atomic_load_n(&owner_ready, __ATOMIC_ACQUIRE)) wf_prim_yield();
    probe_thread requester = start(requester_main);
    while (!__atomic_load_n(&requester_acquired, __ATOMIC_ACQUIRE)) wf_prim_yield();
    __atomic_store_n(&owner_exit, 1u, __ATOMIC_RELEASE);
    while (!__atomic_load_n(&owner_withdrawn, __ATOMIC_ACQUIRE)) wf_prim_yield();
    check(__atomic_load_n(&exit_lane->request_word, __ATOMIC_SEQ_CST) == NULL,
          "exit did not withdraw its address");
    check(__atomic_load_n(&exit_lane->request_users, __ATOMIC_SEQ_CST) == 1,
          "requester did not protect the borrowed address");
    check(__atomic_load_n(&owner_released, __ATOMIC_ACQUIRE) == 0,
          "exit passed the in-flight requester");
    __atomic_store_n(&requester_release, 1u, __ATOMIC_RELEASE);
    join(requester);
    join(owner);
    check(__atomic_load_n(&owner_released, __ATOMIC_ACQUIRE) == 1,
          "thread exit omitted cleanup");
    wf__par_request(exit_lane);
    check(__atomic_load_n(&exit_lane->request_word, __ATOMIC_SEQ_CST) == NULL,
          "post-exit request revived the TLS address");
    check(__atomic_load_n(&exit_lane->request_users, __ATOMIC_SEQ_CST) == 0,
          "post-exit request leaked a user count");
    puts("scheduler request protocol: PASS");
    wf_test_guard_finish();
    return 0;
}
