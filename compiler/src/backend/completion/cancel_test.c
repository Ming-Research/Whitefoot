/* Cancellation runtime probe: includes the private bridge so a deterministic
 * schedule can observe registration, park, notification-before-sleep, and
 * exactly one ready-queue insertion. The other pthread stands for another
 * driver's firing context, as in shared_object_test.c. Socketpairs carry
 * real receives; the silent peer remains open throughout cancellation.
 * No test-specific branch is compiled into the runtime. */
#define _GNU_SOURCE
#include "bridge.c"
#include "../ordinary_values.c"
#include "../runtime_test_guard.h"
#include <pthread.h>
#include <netinet/in.h>
#include <sys/socket.h>

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "cancel-test:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

static wf_driver firing_driver;
static int frame;

static void *fire_on_other_driver(void *source) {
    wf_driver_self = &firing_driver;
    wf__body_cancel_fire(source);
    wf__body_cancel_fire(source); /* idempotent */
    wf_driver_self = NULL;
    return NULL;
}

static void fire_elsewhere(wf_value *source) {
    pthread_t thread;
    CHECK(pthread_create(&thread, NULL, fire_on_other_driver, source) == 0);
    CHECK(pthread_join(thread, NULL) == 0);
}

static wf_host_operation *operation(void) {
    return wf__context_operation();
}

static void parked(int state) {
    CHECK(state == 2);
    CHECK(wf__context_wait(operation(), &frame) == 1);
    CHECK(wf_context_root.record == &operation()->record);
    CHECK(atomic_load(&wf_driver_root.host_waits) == 1u);
}

static void finish_park(void) {
    /* The test watchdog fails a lost wake/cancellation; no test timeout is
     * passed as the operation's deadline, so it cannot make a broken fire pass. */
    while (wf_context_root.record != NULL) {
        wf_driver_reap(&wf_driver_root);
        if (wf_context_root.record == NULL) break;
        uint64_t epoch = wf_completion_wake_epoch(wf_driver_root.runtime);
        if (!wf_driver_find_cancelled(&wf_driver_root)
            && !wf_context_harvest(&wf_driver_root) && wf_context_root.record != NULL)
            wf_bridge_park(epoch, wf_timer_wait_ms(&wf_driver_root));
    }
    CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    CHECK(wf_driver_root.cancel_waits == NULL);
    CHECK(wf_context_root.timer_slot == 0);
    CHECK(wf_driver_root.timer_count == 0);
    CHECK(atomic_load(&wf_driver_root.host_waits) == 0u);
}

static void stopped_result(const wf_read_result *result, unsigned char byte, unsigned reason) {
    CHECK(result->tag == 1);
    CHECK(result->err.error.tag == 1);
    CHECK(result->err.error.error.tag == reason);
    CHECK(byte == 0xcc);
}

int main(void) {
    wf_test_guard_start(30);
    wf_test_guard_phase("find-at-fire cancellation");
    wf__context_root_begin();
    firing_driver.runtime = &firing_driver.own_runtime;
    CHECK(wf_completion_runtime_init(firing_driver.runtime) == 0);
    atomic_store(&wf_drivers[1], &firing_driver);
    atomic_store(&wf_driver_count, 2u);

    int pair[2];
    CHECK(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);
    wf_value receive, source, shared, watch, never;
    wf_descriptor_value(&receive, pair[0]);
    wf__body_cancel_source(&source);
    wf__body_cancel_share(&shared, &source);
    wf__body_cancel_watch(&watch, &source);
    wf__body_close_cancel_source(&source); /* watch and shared retain it */
    wf__body_cancel_never(&never);
    unsigned char byte = 0xcc;
    wf_view destination = {&byte, 1};
    wf_read_result result;

    parked(wf__body_receive_next_start(&result, &receive, &destination,
                                             0, 1, NULL, &watch, operation()));
    CHECK(wf_driver_root.cancel_waits == &wf_context_root);
    CHECK(wf_driver_root.timer_count == 0); /* no clock deadline heap work */
    uint64_t before = wf_completion_wake_epoch(wf_driver_root.runtime);
    fire_elsewhere(&shared);
    CHECK(wf_completion_wake_epoch(wf_driver_root.runtime) != before);
    /* A notification arriving before the native park cannot be lost. */
    CHECK(wf_completion_park_if_unchanged(wf_driver_root.runtime, before, 0)
          == WF_COMPLETION_PARK_EPOCH_CHANGED);
    finish_park();
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, NULL, &watch, operation());
    stopped_result(&result, byte, WF_IO_CANCELLED);
    fire_elsewhere(&shared);
    wf_driver_reap(&wf_driver_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);

    /* A watch already fired before submission answers without touching the
     * socket, even when a byte is ready, and remains valid after all sources
     * have closed. The never watch must then receive that very byte. */
    wf__body_close_cancel_source(&shared);
    CHECK(send(pair[1], "N", 1, 0) == 1);
    uint64_t publications = atomic_load(&wf_bridge_publications);
    CHECK(wf__body_receive_next_start(&result, &receive, &destination,
                  0, 1, NULL, &watch, operation()) == 1);
    CHECK(atomic_load(&wf_bridge_publications) == publications + 1u);
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, NULL, &watch, operation());
    stopped_result(&result, byte, WF_IO_CANCELLED);
    wf__body_close_cancel_watch(&watch);
    int state = wf__body_receive_next_start(&result, &receive, &destination,
                                                 0, 1, NULL, &never, operation());
    CHECK(wf_driver_root.cancel_waits == NULL);
    if (state == 2) {
        if (wf__context_wait(operation(), &frame)) finish_park();
    } else CHECK(state == 1);
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, NULL, &never, operation());
    CHECK(result.tag == 0 && result.ok.value == 1 && byte == 'N');

    /* Host completion of a real watch unlinks before its context can move;
     * firing that source afterwards cannot cancel a later operation. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    wf_deadline later = {0};
    later.tag = WF_OPTION_SOME;
    later.value.words[0] = wf_file_monotonic_ns() + UINT64_C(60000000000);
    parked(wf__body_receive_next_start(&result, &receive, &destination,
                                             0, 1, &later, &watch, operation()));
    CHECK(wf_driver_root.timer_count == 1);
    CHECK(send(pair[1], "R", 1, 0) == 1);
    finish_park();
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, NULL, &watch, operation());
    CHECK(result.tag == 0 && byte == 'R');
    fire_elsewhere(&source);
    wf_driver_reap(&wf_driver_root);
    CHECK(wf_run_take(&wf_driver_root) == NULL);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    /* Fire after linking but before context_wait, with the driver's epoch
     * already advanced: the driver's post-snapshot recheck still finds it. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    byte = 0xcc;
    state = wf__body_receive_next_start(&result, &receive, &destination,
                                             0, 1, NULL, &watch, operation());
    fire_elsewhere(&source);
    parked(state);
    CHECK(wf_driver_find_cancelled(&wf_driver_root));
    finish_park();
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, NULL, &watch, operation());
    stopped_result(&result, byte, WF_IO_CANCELLED);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    /* Clock expiry removes the same watch even when the source never fires. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    later.value.words[0] = 1;
    parked(wf__body_receive_next_start(&result, &receive, &destination,
                                             0, 1, &later, &watch, operation()));
    finish_park();
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 1, &later, &watch, operation());
    stopped_result(&result, byte, WF_IO_DEADLINE_PASSED);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    /* The other watched public operation returns its factory credit when
     * cancellation ends an accept, without manufacturing a connection. */
    int listener_fd = socket(AF_INET, SOCK_STREAM, 0);
    CHECK(listener_fd >= 0);
    struct sockaddr_in address;
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    CHECK(bind(listener_fd, (struct sockaddr *)&address, sizeof(address)) == 0);
    CHECK(listen(listener_fd, 1) == 0);
    wf_value listener, factory = {{8, 0, 0, 0}};
    wf_descriptor_value(&listener, listener_fd);
    wf_accept_result accepted;
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    parked(wf__body_tcp_accept_start(&accepted, &factory, &listener,
                                           NULL, &watch, operation()));
    CHECK(factory.words[0] == 7);
    fire_elsewhere(&source);
    finish_park();
    wf__body_tcp_accept_finish(&accepted, &factory, &listener, NULL, &watch, operation());
    CHECK(accepted.tag == 1 && accepted.err.error.tag == WF_IO_CANCELLED);
    CHECK(factory.words[0] == 8);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    CHECK(close(listener_fd) == 0);

    /* Empty completion between watched start and park removes registration
     * before the ordinary fairness pass can migrate the context. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    CHECK(wf__body_receive_next_start(&result, &receive, &destination,
                  0, 0, NULL, &watch, operation()) == 2);
    CHECK(wf__context_wait(operation(), &frame) == 0);
    CHECK(wf_driver_root.cancel_waits == NULL);
    CHECK(wf_context_root.timer_slot == 0);
    wf__body_receive_next_finish(&result, &receive, &destination, 0, 0, NULL, &watch, operation());
    CHECK(result.tag == 0 && result.ok.value == 0);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    wf__body_close_cancel_watch(&never);

    /* A timer uses the same watched list but completes in its own driver.
     * No host timeout is allowed to stand in for the firing. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    wf_value future = {{wf_file_monotonic_ns() + UINT64_C(60000000000), 0, 0, 0}};
    wf_sleep_result slept;
    parked(wf__body_sleep_until_start(&slept, &future, &watch, operation()));
    fire_elsewhere(&source);
    finish_park();
    wf__body_sleep_until_finish(&slept, &future, &watch, operation());
    CHECK(slept.tag == 1);
    CHECK(wf_file_monotonic_ns() < future.words[0]);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    /* Every other watched route rejects a fired-before-wait operation with
     * its own result shape, leaving the transfer and handle credit intact. */
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    fire_elsewhere(&source);
    byte = 0xcc;
    CHECK(wf__body_read_next_start(&result, &factory, &receive, &destination,
                                   0, 1, NULL, &watch, operation()) == 1);
    wf__body_read_next_finish(&result, &factory, &receive, &destination,
                              0, 1, NULL, &watch, operation());
    stopped_result(&result, byte, WF_IO_CANCELLED);
    wf_write_result written;
    CHECK(wf__body_write_once_start(&written, &factory, &receive, &destination,
                                    0, 1, NULL, &watch, operation()) == 1);
    wf__body_write_once_finish(&written, &factory, &receive, &destination,
                               0, 1, NULL, &watch, operation());
    CHECK(written.tag == 1 && written.err.error.tag == WF_IO_CANCELLED);
    CHECK(wf__body_send_once_start(&written, &receive, &destination,
                                   0, 1, NULL, &watch, operation()) == 1);
    wf__body_send_once_finish(&written, &receive, &destination,
                              0, 1, NULL, &watch, operation());
    CHECK(written.tag == 1 && written.err.error.tag == WF_IO_CANCELLED);
    wf_connect_result connected;
    wf_value address_value = {{UINT64_C(0x0100007f), 0, 1, 0}};
    CHECK(wf__body_tcp_connect_start(&connected, &factory, &address_value,
                                     NULL, &watch, operation()) == 1);
    wf__body_tcp_connect_finish(&connected, &factory, &address_value,
                                NULL, &watch, operation());
    CHECK(connected.tag == 1 && connected.err.error.tag == WF_IO_CANCELLED);
    CHECK(factory.words[0] == 8);
    CHECK(wf__body_sleep_until_start(&slept, &future, &watch, operation()) == 1);
    wf__body_sleep_until_finish(&slept, &future, &watch, operation());
    CHECK(slept.tag == 1);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    CHECK(close(pair[0]) == 0 && close(pair[1]) == 0);
    atomic_store(&wf_driver_count, 1u);
    while (atomic_load(&wf_drivers_notifying) != 0u) wf_prim_yield();
    atomic_store(&wf_drivers[1], NULL);
    CHECK(wf_completion_runtime_destroy(firing_driver.runtime) == 0);
    wf_test_guard_finish();
    return 0;
}
