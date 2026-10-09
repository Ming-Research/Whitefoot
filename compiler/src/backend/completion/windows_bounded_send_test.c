/* Windows helper-route regression. The private bridge lets the probe fire a
 * real watch after bytes reach a silent peer but before harvesting the send.
 * The peer never reads until the bounded call returns. A blocking whole-buffer
 * send therefore hits the watchdog, even after cancellation/deadline expiry.
 * Compatible production objects are shared with the other Windows probes. */
#include "bridge.c"
#include "../ordinary_values.c"
#include "../runtime_test_guard.h"
#include "socket_test.h"

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "windows-bounded-send:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

static int frame;

static wf_host_operation *operation(void) {
    return wf__context_operation();
}

static void finish_operation(int state) {
    CHECK(state == 1 || state == 2);
    if (state == 2 && wf__context_wait(operation(), &frame)) {
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
    }
    CHECK(wf_driver_root.cancel_waits == NULL);
    CHECK(wf_context_root.timer_slot == 0);
    CHECK(wf_driver_root.timer_count == 0);
}

static void end_bound(int cancelled, wf_value *source, const wf_deadline *deadline) {
    if (cancelled) wf__body_cancel_fire(source);
    else while (wf_file_monotonic_ns() < deadline->value.words[0]) Sleep(1);
}

static void drain_prefix(SOCKET peer, uint64_t count) {
    unsigned char bytes[4096];
    while (count != 0) {
        int take = count > sizeof(bytes) ? (int)sizeof(bytes) : (int)count;
        int received = recv(peer, (char *)bytes, take, 0);
        CHECK(received > 0 && received <= take);
        for (int i = 0; i < received; i++) CHECK(bytes[i] == 0x5a);
        count -= (uint64_t)received;
    }
}

static void bounded_send(int cancelled) {
    enum { COUNT = 8 * 1024 * 1024 };
    unsigned char *bytes = malloc(COUNT);
    CHECK(bytes != NULL);
    memset(bytes, 0x5a, COUNT);
    wf_view buffer = {bytes, COUNT};
    wf_value factory = {{8, 0, 0, 0}}, address, never, source, watch;
    wf_open_result listener;
    wf_connect_result client;
    wf_accept_result server;
    wf_close_result closed;
    wf_write_result sent;
    wf__body_cancel_never(&never);
    wf__body_socket_address_v4(&address, 127, 0, 0, 1, 0);
    wf__body_tcp_listen(&listener, &factory, &address);
    CHECK(listener.tag == 0);
    SOCKET listening = (SOCKET)wf__windows_socket_handle((int)listener.ok.value.words[0]);
    int capacity = 4096;
    CHECK(setsockopt(listening, SOL_SOCKET, SO_RCVBUF,
                     (const char *)&capacity, sizeof(capacity)) == 0);
    unsigned port = wf_test_socket_port((int)listener.ok.value.words[0]);
    CHECK(port != 0);
    wf__body_socket_address_v4(&address, 127, 0, 0, 1, (uint16_t)port);
    wf__body_tcp_connect(&client, &factory, &address, NULL, &never);
    CHECK(client.tag == 0);
    wf__body_tcp_accept(&server, &factory, &listener.ok.value, NULL, &never);
    CHECK(server.tag == 0);
    SOCKET connection = (SOCKET)wf__windows_socket_handle((int)client.ok.value.send.words[0]);
    SOCKET peer = (SOCKET)wf__windows_socket_handle((int)server.ok.value.connection.receive.words[0]);
    CHECK(setsockopt(connection, SOL_SOCKET, SO_SNDBUF,
                     (const char *)&capacity, sizeof(capacity)) == 0);

    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    const wf_value *bound_watch = cancelled ? &watch : &never;
    wf_deadline deadline = {0};
    deadline.tag = WF_OPTION_SOME;
    deadline.value.words[0] = wf_file_monotonic_ns() + UINT64_C(1000000000);
    const wf_deadline *bound_deadline = cancelled ? NULL : &deadline;
    int state = wf__body_send_once_start(&sent, &client.ok.value.send,
        &buffer, 0, COUNT, bound_deadline, bound_watch, operation());
    CHECK(operation()->record.route == WF_COMPLETION_ROUTE_FILE_ADAPTER);
    /* Establish actual transmission before ending the bound. Do not read:
     * recv here would let the old blocking send finish and hide the defect. */
    u_long available = 0;
    while (available == 0) {
        CHECK(ioctlsocket(peer, FIONREAD, &available) == 0);
        CHECK(wf_bridge_record_state(&operation()->record) != WF_COMPLETION_DONE
              || operation()->record.result.value > 0);
        if (available == 0) Sleep(1);
    }
    end_bound(cancelled, &source, &deadline);
    finish_operation(state);
    wf__body_send_once_finish(&sent, &client.ok.value.send,
        &buffer, 0, COUNT, bound_deadline, bound_watch, operation());
    /* send_once's Ok carries the index after the last byte sent, at most the
     * range's end: Winsock may buffer the whole range in one nonblocking
     * send, so a full count is as ordinary as a shorter prefix. */
    if (sent.tag != 0 || sent.ok.value == 0 || sent.ok.value > COUNT) {
        fprintf(stderr, "windows-bounded-send:%d: tag %u value %llu error %u\n",
                __LINE__, (unsigned)sent.tag, (unsigned long long)sent.ok.value,
                (unsigned)sent.err.error.tag);
        exit(1);
    }
    printf("windows-bounded-send: %s bound kept %llu of %d bytes sent\n",
           cancelled ? "cancellation" : "deadline",
           (unsigned long long)sent.ok.value, (int)COUNT);
    drain_prefix(peer, sent.ok.value);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    /* Fill both buffers, then submit a fresh bounded operation with no space.
     * A quiet full window has no peer read that could free it. The raw fixture
     * fill is counted so a cancelled transfer cannot hide any extra bytes. */
    u_long nonblocking = 1;
    CHECK(ioctlsocket(connection, FIONBIO, &nonblocking) == 0);
    uint64_t filled = 0;
    for (;;) {
        int count = send(connection, (const char *)bytes, COUNT, 0);
        if (count > 0) {
            filled += (uint64_t)count;
            continue;
        }
        CHECK(count == SOCKET_ERROR && WSAGetLastError() == WSAEWOULDBLOCK);
        WSAPOLLFD poll = {connection, POLLWRNORM, 0};
        int ready = WSAPoll(&poll, 1, 100);
        CHECK(ready >= 0);
        if (ready == 0) break;
    }
    nonblocking = 0;
    CHECK(ioctlsocket(connection, FIONBIO, &nonblocking) == 0);
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    deadline.value.words[0] = wf_file_monotonic_ns() + UINT64_C(1000000000);
    state = wf__body_send_once_start(&sent, &client.ok.value.send,
        &buffer, 0, COUNT, bound_deadline, bound_watch, operation());
    CHECK(operation()->record.route == WF_COMPLETION_ROUTE_FILE_ADAPTER);
    /* Let the helper claim the operation before firing, so this tests the
     * running helper, not cancellation of a record still in its queue. */
    for (;;) {
        int executing = 0;
        wf_completion_wait_lock(&wf_bridge_adapter.queue_wait);
        for (size_t i = 0; i < wf_bridge_adapter.helper_slots; i++)
            executing |= wf_bridge_adapter.executing[i] == &operation()->record;
        wf_completion_wait_unlock(&wf_bridge_adapter.queue_wait);
        if (executing) break;
        CHECK(wf_bridge_record_state(&operation()->record) != WF_COMPLETION_DONE);
        Sleep(1);
    }
    end_bound(cancelled, &source, &deadline);
    finish_operation(state);
    wf__body_send_once_finish(&sent, &client.ok.value.send,
        &buffer, 0, COUNT, bound_deadline, bound_watch, operation());
    CHECK(sent.tag == 1);
    CHECK(sent.err.error.tag == (cancelled ? WF_IO_CANCELLED : WF_IO_DEADLINE_PASSED));
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
    wf__body_close_send(&closed, &factory, &client.ok.value.send);
    CHECK(closed.tag == 0);
    drain_prefix(peer, filled);
    char extra;
    CHECK(recv(peer, &extra, 1, 0) == 0);
    wf__body_close_receive(&closed, &factory, &client.ok.value.receive);
    CHECK(closed.tag == 0);
    wf__body_close_receive(&closed, &factory, &server.ok.value.connection.receive);
    CHECK(closed.tag == 0);
    wf__body_close_send(&closed, &factory, &server.ok.value.connection.send);
    CHECK(closed.tag == 0);
    wf__body_close_listener(&closed, &factory, &listener.ok.value);
    CHECK(closed.tag == 0 && factory.words[0] == 8);
    wf__body_close_cancel_watch(&never);
    free(bytes);
}

int main(void) {
    wf_test_guard_start(30);
    wf__context_root_begin();
    wf_test_guard_phase("Windows helper send: cancellation, prefix and no-space");
    bounded_send(1);
    wf_test_guard_phase("Windows helper send: deadline, prefix and no-space");
    bounded_send(0);
    wf_test_guard_finish();
    return 0;
}
