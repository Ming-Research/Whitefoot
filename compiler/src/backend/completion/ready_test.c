/* Ready-queue wake probe: include the private bridge, as cancel_test.c does,
 * to step drivers through a fixed schedule without native threads or sleeps.
 * Real launch, wake epochs, search bookkeeping and stealing are exercised;
 * driver startup and coroutine execution are outside this test's scope.
 * No test-specific path is compiled into the runtime. */
#if defined(__linux__)
#define _GNU_SOURCE
#endif
#include "bridge.c"

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "ready-test:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

static wf_driver helper, spare;
static uint64_t group[2];

static void running(wf_driver *driver, wf_context *context) {
    wf_driver_self = driver;
    wf_context_current = context;
}

/* The idle announcement from wf_context_drive, with no native park. */
static void idle(wf_driver *driver) {
    CHECK(atomic_load(&driver->idle) == 0u);
    CHECK(atomic_load(&driver->searcher) == 0u);
    CHECK(atomic_load(&driver->run_count) == 0u);
    atomic_fetch_add(&wf_drivers_changes, 1u);
    atomic_store(&driver->idle, 1u);
    atomic_fetch_add(&wf_drivers_idle, 1u);
}

/* A newly constructed, suspended frame is only a marker: never resumed or
 * destroyed. Stack fixtures stand in for prepare's zeroed context storage. */
static void *suspended_frame(void *arguments) {
    CHECK(wf_context_current == arguments);
    return arguments;
}

static void launch(wf_context *child) {
    CHECK(wf_context_current == &wf_context_root);
    wf_context_prepared = child;
    wf__context_launch(group, child, suspended_frame);
    CHECK(wf_context_current == &wf_context_root);
    CHECK(child->root == child && child->resume == child);
}

/* No coroutine owns storage in these fixtures; retire just launch's counts. */
static void retire(wf_context *child) {
    atomic_fetch_sub(&wf_context_live, 1u);
    wf_group_finish(child->group);
}

static void busy_launch(void) {
    wf_context child = {0};
    idle(&helper);
    running(&wf_driver_root, &wf_context_root);
    uint64_t epoch = wf_completion_wake_epoch(helper.runtime);
    launch(&child);
    CHECK(atomic_load(&wf_driver_root.run_count) == 1u);
    /* With the old run_count > 1 condition this epoch does not advance. */
    CHECK(wf_completion_wake_epoch(helper.runtime) == epoch + 1u);
    CHECK(atomic_load(&helper.idle) == 0u);
    CHECK(atomic_load(&wf_drivers_idle) == 0u);
    CHECK(atomic_load(&helper.searcher) == 1u);
    CHECK(atomic_load(&wf_drivers_searching) == 1u);
    CHECK(wf_completion_park_if_unchanged(helper.runtime, epoch, 0)
          == WF_COMPLETION_PARK_EPOCH_CHANGED);

    running(&helper, NULL);
    CHECK(wf_driver_steal(&helper) == 1);
    wf_driver_end_search(&helper, 1);
    CHECK(wf_run_take(&helper) == &child);
    CHECK(child.driver == &helper);
    CHECK(wf__driver_index() == 1u);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    retire(&child);

    /* Stealing still excludes the entry frame, even as the only ready one. */
    wf_run_push(&wf_driver_root, &wf_context_root);
    CHECK(wf_driver_steal(&helper) == 0);
    CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);

    /* The starter wins the next race and drains the queue before the helper
     * looks. An unsuccessful search clears its ticket and can idle again. */
    idle(&helper);
    running(&wf_driver_root, &wf_context_root);
    epoch = wf_completion_wake_epoch(helper.runtime);
    launch(&child);
    CHECK(wf_completion_wake_epoch(helper.runtime) == epoch + 1u);
    CHECK(wf_run_take(&wf_driver_root) == &child);
    retire(&child);
    running(&helper, NULL);
    CHECK(wf_driver_steal(&helper) == 0);
    wf_driver_end_search(&helper, 0);
    CHECK(atomic_load(&wf_drivers_searching) == 0u);
    idle(&helper);
    CHECK(wf_contexts_stuck(&helper) == 0); /* The starter is still running. */

    running(&wf_driver_root, &wf_context_root);
    epoch = wf_completion_wake_epoch(helper.runtime);
    launch(&child);
    CHECK(wf_completion_wake_epoch(helper.runtime) == epoch + 1u);
    CHECK(wf_run_take(&wf_driver_root) == &child);
    retire(&child);
    running(&helper, NULL);
    wf_driver_end_search(&helper, 0);
}

static void wake_boundaries(void) {
    wf_context first = {0}, second = {0};
    first.driver = second.driver = &wf_driver_root;
    idle(&helper);
    running(&wf_driver_root, NULL);
    uint64_t epoch = wf_completion_wake_epoch(helper.runtime);
    wf_context_ready(&first);
    CHECK(wf_completion_wake_epoch(helper.runtime) == epoch);
    wf_context_ready(&second);
    CHECK(wf_completion_wake_epoch(helper.runtime) == epoch + 1u);
    CHECK(wf_run_take(&wf_driver_root) == &first);
    CHECK(wf_run_take(&wf_driver_root) == &second);
    running(&helper, NULL);
    wf_driver_end_search(&helper, 0);

    /* A foreign target retains its direct wake, with no search ticket. */
    running(&wf_driver_root, &wf_context_root);
    first.driver = &helper;
    epoch = wf_completion_wake_epoch(helper.runtime);
    wf_context_ready(&first);
    CHECK(wf_completion_wake_epoch(helper.runtime) == epoch + 1u);
    CHECK(atomic_load(&wf_drivers_searching) == 0u);
    CHECK(wf_run_take(&helper) == &first);

    /* With one driver there is nobody idle to wake; the child stays local. */
    atomic_store(&wf_drivers[1], NULL);
    atomic_store(&wf_driver_count, 1u);
    epoch = wf_completion_wake_epoch(wf_driver_root.runtime);
    launch(&first);
    CHECK(wf_completion_wake_epoch(wf_driver_root.runtime) == epoch);
    CHECK(atomic_load(&wf_drivers_searching) == 0u);
    CHECK(wf_run_take(&wf_driver_root) == &first);
    CHECK(first.driver == &wf_driver_root);
    retire(&first);
    atomic_store(&wf_drivers[1], &helper);
    atomic_store(&wf_driver_count, 2u);
}

static void coalesced_wakes(void) {
    wf_context first = {0}, second = {0};
    atomic_store(&wf_drivers[2], &spare);
    atomic_store(&wf_driver_count, 3u);
    idle(&helper);
    idle(&spare);
    running(&wf_driver_root, &wf_context_root);
    uint64_t helper_epoch = wf_completion_wake_epoch(helper.runtime);
    uint64_t spare_epoch = wf_completion_wake_epoch(spare.runtime);
    launch(&first);
    CHECK(wf_completion_wake_epoch(helper.runtime) == helper_epoch + 1u);
    launch(&second);
    CHECK(wf_completion_wake_epoch(helper.runtime) == helper_epoch + 1u);
    CHECK(wf_completion_wake_epoch(spare.runtime) == spare_epoch);
    CHECK(atomic_load(&wf_drivers_idle) == 1u);
    CHECK(atomic_load(&wf_drivers_searching) == 1u);

    /* Finding work releases the search ticket and allows the existing
     * cascade to wake the remaining idle driver for the remaining child. */
    running(&helper, NULL);
    CHECK(wf_driver_steal(&helper) == 1);
    wf_driver_end_search(&helper, 1);
    CHECK(wf_completion_wake_epoch(spare.runtime) == spare_epoch + 1u);
    CHECK(wf_run_take(&helper) == &first);
    retire(&first);
    running(&spare, NULL);
    CHECK(wf_driver_steal(&spare) == 1);
    wf_driver_end_search(&spare, 1);
    CHECK(wf_run_take(&spare) == &second);
    retire(&second);
    CHECK(atomic_load(&wf_drivers_idle) == 0u);
    CHECK(atomic_load(&wf_drivers_searching) == 0u);
}

int main(void) {
    wf_driver *drivers[] = {&wf_driver_root, &helper, &spare};
    for (unsigned index = 0; index < 3u; ++index) {
        wf_driver *driver = drivers[index];
        driver->index = index;
        atomic_flag_clear(&driver->run_lock);
        driver->runtime = &driver->own_runtime;
        CHECK(wf_completion_runtime_init(driver->runtime) == 0);
    }
    wf_context_root.driver = &wf_driver_root;
    atomic_store(&wf_drivers[0], &wf_driver_root);
    atomic_store(&wf_drivers[1], &helper);
    atomic_store(&wf_driver_count, 2u);
    /* Model an established driver pool: launch must not start host threads
     * or initialize I/O helpers. These are the once-initializer's done value. */
    wf_drivers_once = wf_bridge_contexts_once = 2u;
    busy_launch();
    wake_boundaries();
    coalesced_wakes();
    CHECK(group[0] == 0u && group[1] == 0u);
    CHECK(atomic_load(&wf_context_live) == 0u);
    for (unsigned index = 0; index < 3u; ++index) {
        CHECK(atomic_load(&drivers[index]->run_count) == 0u);
        atomic_store(&wf_drivers[index], NULL);
        CHECK(wf_completion_runtime_destroy(drivers[index]->runtime) == 0);
    }
    return 0;
}
