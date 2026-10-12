/* Driver selection is thread-free, fixed before entry, and independent of
 * the count of roles successfully started later. This Linux probe counts
 * /proc tasks and successful executor starts, and changes both environment
 * and affinity between selection and startup. Today's code has no early
 * count, rereads those inputs at startup, and silently keeps fewer roles
 * after a secondary ring/wake initialization refusal. The wrappers below
 * substitute host calls only in this probe's bridge translation unit. */
#define _GNU_SOURCE
#include "contract.h"
#include "bridge.h"
#include "file_adapter.h"
#include "linux_io_uring.h"
#include "../sched/prim.h"
#include <dirent.h>
#include <errno.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int counted_thread_start(wf_prim_thread *, void (*)(void *), void *, size_t);
static int selected_runtime_init(wf_completion_runtime *);
static int selected_ring_init(wf_linux_io_uring_adapter *, wf_completion_runtime *, size_t, size_t);
static int selected_wake_callback(wf_completion_runtime *, wf_completion_wake_callback, void *);
static unsigned selected_online_cpus(void);
static int unknown_cpu_count;

#define wf_prim_thread_start counted_thread_start
#define wf_completion_runtime_init selected_runtime_init
#define wf_linux_io_uring_init selected_ring_init
#define wf_completion_set_wake_callback selected_wake_callback
#define wf_prim_online_cpus selected_online_cpus
#include "bridge.c"
#undef wf_prim_thread_start
#undef wf_completion_runtime_init
#undef wf_linux_io_uring_init
#undef wf_completion_set_wake_callback
#undef wf_prim_online_cpus
#include "../runtime_test_guard.h"

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "driver-count-test:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

enum { REFUSE_NONE, REFUSE_WAKE, REFUSE_RING, REFUSE_CALLBACK };
static unsigned refusal;
static unsigned started_secondary_drivers;
static unsigned started_bridge_threads;

static unsigned selected_online_cpus(void) {
    return unknown_cpu_count ? 0u : wf_prim_online_cpus();
}

static int counted_thread_start(wf_prim_thread *thread, void (*entry)(void *),
                                void *argument, size_t stack_bytes) {
    int secondary = entry == wf_executor_main && ((wf_executor *)argument)->assignment != NULL;
    int result = wf_prim_thread_start(thread, entry, argument, stack_bytes);
    if (result == 0) {
        ++started_bridge_threads;
        if (secondary) ++started_secondary_drivers;
    }
    return result;
}

static int selected_runtime_init(wf_completion_runtime *runtime) {
    if (refusal == REFUSE_WAKE && runtime != &wf_bridge_runtime)
        return EAGAIN;
    return wf_completion_runtime_init(runtime);
}

static int selected_ring_init(wf_linux_io_uring_adapter *adapter, wf_completion_runtime *runtime,
                              size_t depth, size_t completions) {
    if (refusal == REFUSE_RING && adapter != &wf_bridge_linux_adapter)
        return EAGAIN;
    return wf_linux_io_uring_init(adapter, runtime, depth, completions);
}

static int selected_wake_callback(wf_completion_runtime *runtime,
                                  wf_completion_wake_callback wake, void *context) {
    if (refusal == REFUSE_CALLBACK && runtime != &wf_bridge_runtime)
        return EAGAIN;
    return wf_completion_set_wake_callback(runtime, wake, context);
}

static unsigned task_count(void) {
    DIR *tasks = opendir("/proc/self/task");
    CHECK(tasks != NULL);
    unsigned count = 0;
    struct dirent *entry;
    while ((entry = readdir(tasks)) != NULL)
        if (entry->d_name[0] >= '0' && entry->d_name[0] <= '9') ++count;
    CHECK(closedir(tasks) == 0);
    return count;
}

int main(int argc, char **argv) {
    CHECK(argc == 2);
    unsigned fail = REFUSE_NONE;
    int singleton = strcmp(argv[1], "singleton") == 0;
    int unknown = strcmp(argv[1], "unknown-count") == 0;
    if (strcmp(argv[1], "wake-refused") == 0) fail = REFUSE_WAKE;
    else if (strcmp(argv[1], "ring-refused") == 0) fail = REFUSE_RING;
    else if (strcmp(argv[1], "callback-refused") == 0) fail = REFUSE_CALLBACK;
    else CHECK(singleton || unknown || strcmp(argv[1], "frozen") == 0);
    CHECK(setenv("WF_DRIVERS", unknown ? "0" : "2", 1) == 0);
    CHECK(setenv("WF_IO_HELPERS", "2", 1) == 0);
    if (singleton) CHECK(setenv("WF_IO_NO_NATIVE_RING", "1", 1) == 0);
    else CHECK(unsetenv("WF_IO_NO_NATIVE_RING") == 0);
    unsigned before = task_count();
    /* A ready-ring default cannot publish one driver as though it knew the
     * allowed CPU count. The old zero-to-one fallback reaches the failure
     * below instead of stopping before entry with the runtime diagnostic. */
    unknown_cpu_count = unknown;
    uint64_t selected = wf__context_driver_count();
    CHECK(task_count() == before);
    CHECK(started_bridge_threads == 0u);
    CHECK(atomic_load(&wf_driver_count) == 0u);
    CHECK(!wf_bridge_file_ready && !wf_bridge_ready);
    if (!singleton && !wf_bridge_ring_ready()) {
        const char *required = getenv("WF_REQUIRE_LINUX_IO_URING");
        if (required != NULL && strcmp(required, "1") == 0) CHECK(wf_bridge_ring_ready());
        fputs("driver-count-test: io_uring qualification unavailable\n", stderr);
        return 77;
    }
    if (unknown) {
        fputs("driver-count-test: unknown CPU count did not stop selection\n", stderr);
        return 2;
    }
    CHECK(selected == (singleton ? 1u : 2u));
    /* Entry could observe a later host change; lazy driver startup must use
     * the execution input already supplied, including its root-ring result. */
    CHECK(setenv("WF_DRIVERS", singleton ? "64" : "1", 1) == 0);
    CHECK(setenv("WF_IO_NO_NATIVE_RING", singleton ? "0" : "1", 1) == 0);
    cpu_set_t previous, one;
    CHECK(sched_getaffinity(0, sizeof previous, &previous) == 0);
    CPU_ZERO(&one);
    for (unsigned cpu = 0; cpu < CPU_SETSIZE; ++cpu) {
        if (CPU_ISSET(cpu, &previous)) { CPU_SET(cpu, &one); break; }
    }
    CHECK(CPU_COUNT(&one) == 1);
    CHECK(sched_setaffinity(0, sizeof one, &one) == 0);
    CHECK(wf__context_driver_count() == selected);
    CHECK(task_count() == before);
    wf_test_guard_start(30);
    wf__context_root_begin();
    CHECK(wf_file_adapter_helper_count(&wf_bridge_adapter) == (singleton ? 2u : 0u));
    CHECK(atomic_load(&wf_driver_count) == 1u && started_secondary_drivers == 0u);
    refusal = fail;
    wf__sched_once(&wf_drivers_once, wf_drivers_begin);
    if (fail != REFUSE_NONE) {
        fputs("driver-count-test: selected driver initialization refusal did not stop startup\n", stderr);
        return 2;
    }
    CHECK(1u + started_secondary_drivers == selected);
    CHECK(started_bridge_threads == 2u * selected);
    CHECK(atomic_load(&wf_driver_count) == selected);
    CHECK(wf__context_driver_count() == selected);
    atomic_store(&wf_context_root_done, 1u);
    wf_drivers_end();
    CHECK(wf__context_driver_count() == selected);
    CHECK(sched_setaffinity(0, sizeof previous, &previous) == 0);
    wf_test_guard_finish();
    return 0;
}
