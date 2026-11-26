/* Windows connect regression: a `tcp_connect` that carries a deadline or a
 * real cancellation watch ends at its bound.
 *
 * With the native ring off (`WF_IO_NO_NATIVE_RING=1`) such a connect runs on a
 * helper thread, and `CancelSynchronousIo`, which is all that can interrupt a
 * helper, is not documented to end a blocking Winsock connect. The target is
 * a loopback listener with `SO_CONDITIONAL_ACCEPT` that never calls
 * `WSAAccept`: the stack then holds each connection request until the
 * application decides, so a connect to it stays pending, where a listener
 * with a full queue would refuse. Before anything is measured, a raw
 * nonblocking connect to it must stay pending for three seconds.
 *
 * Cases, each timed from the firing or the deadline to the call's return and
 * required to end within one second:
 *   A  cancellation fires while the helper is inside the connect;
 *   B  a deadline passes (B1), a watch is already fired (B2) and a deadline
 *      has already passed (B3) when the call is made;
 *   C  a connect that succeeds and a firing that races it, repeated with the
 *      firing delayed from 0 to 2 ms: each call ends with a usable connection
 *      or Cancelled and nothing else;
 *   D  a connect with no bound at all returns a blocking-mode connection;
 *   E  a connect to a closed port returns the host's refusal, with and
 *      without a watch, and never Cancelled;
 *   F  a connect that carries a bound it does not reach (a watch, a far
 *      deadline, both) returns a blocking-mode connection at once;
 *   G  the firing arrives at every phase of the helper's connect (30
 *      firings, 0 to 29 ms after the submit, some before the helper has
 *      claimed it) and each ends Cancelled;
 *   H  a deadline and a watch together: the deadline reached first is
 *      DeadlinePassed, the watch fired first is Cancelled;
 *   I  deadlines of 1 to 99 ms, straddling the 50 ms slice, each end
 *      DeadlinePassed;
 *   J  a connect that can succeed raced by a deadline: a usable connection or
 *      DeadlinePassed and nothing else.
 * After a warm-up pass (helpers and the completion port are created on first
 * use and kept), the handle count, the factory credit, the driver's
 * cancellation list and its timers must be what they were.
 *
 * The same image runs on the default route as the control: its connect is a
 * ConnectEx the port cancels. The three arguments `control-bound`,
 * `control-leak` and `control-blocking` make a check fail on purpose, to show
 * that the one-second bound, the handle comparison and the blocking-mode
 * probe of case D can fail; each must exit nonzero through its own check.
 * Compatible production objects are shared with the other Windows probes. */
#include "bridge.c"
#include "../ordinary_values.c"
#include "../runtime_test_guard.h"
#include "socket_test.h"

#define WF_BOUNDED_TEST_NAME "windows-bounded-connect"
#include "windows_bounded_test.h"

#ifndef SO_CONDITIONAL_ACCEPT
#define SO_CONDITIONAL_ACCEPT 0x3002
#endif

#define SECOND_NS UINT64_C(1000000000)
#define MILLISECOND_NS UINT64_C(1000000)

static wf_value factory = {{8, 0, 0, 0}};
static wf_cancel_handle never;
static int helper_route;
static uint64_t bound_ns = SECOND_NS;
static int control_nonblocking;
static unsigned silent_port;
static unsigned ordinary_port;
static SOCKET silent_listener = INVALID_SOCKET;
static SOCKET ordinary_listener = INVALID_SOCKET;

static SOCKET raw_listener(int conditional, unsigned *port) {
    SOCKET listener = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    struct sockaddr_in address;
    int length = (int)sizeof(address);
    CHECK(listener != INVALID_SOCKET);
    if (conditional) {
        DWORD on = 1;
        CHECK(setsockopt(listener, SOL_SOCKET, SO_CONDITIONAL_ACCEPT,
                         (const char *)&on, sizeof(on)) == 0);
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    CHECK(bind(listener, (const struct sockaddr *)&address, sizeof(address)) == 0);
    CHECK(listen(listener, SOMAXCONN) == 0);
    CHECK(getsockname(listener, (struct sockaddr *)&address, &length) == 0);
    *port = (unsigned)ntohs(address.sin_port);
    return listener;
}

/* A port nothing listens on: bound by a listener that is then closed. */
static unsigned closed_port(void) {
    unsigned port = 0;
    SOCKET listener = raw_listener(0, &port);
    CHECK(closesocket(listener) == 0);
    return port;
}

/* The fixture must not answer: a raw nonblocking connect stays pending, with
 * no completion and no error, for three seconds. */
static void check_silent_target(void) {
    SOCKET probe = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    struct sockaddr_in address;
    u_long nonblocking = 1;
    fd_set writable, failed;
    struct timeval limit = {3, 0};
    int ready;
    CHECK(probe != INVALID_SOCKET);
    CHECK(ioctlsocket(probe, FIONBIO, &nonblocking) == 0);
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    address.sin_port = htons((u_short)silent_port);
    CHECK(connect(probe, (const struct sockaddr *)&address, sizeof(address)) == SOCKET_ERROR);
    CHECK(WSAGetLastError() == WSAEWOULDBLOCK);
    FD_ZERO(&writable);
    FD_ZERO(&failed);
    FD_SET(probe, &writable);
    FD_SET(probe, &failed);
    ready = select(0, NULL, &writable, &failed, &limit);
    if (ready != 0) {
        int error = 0, length = (int)sizeof(error);
        (void)getsockopt(probe, SOL_SOCKET, SO_ERROR, (char *)&error, &length);
        fprintf(stderr,
                "%s: the silent listener answered within 3 s: select %d, writable %d, failed %d, SO_ERROR %d\n",
                WF_BOUNDED_TEST_NAME, ready, FD_ISSET(probe, &writable) != 0,
                FD_ISSET(probe, &failed) != 0, error);
        exit(1);
    }
    CHECK(closesocket(probe) == 0);
}

static DWORD handle_count(void) {
    DWORD count = 0;
    CHECK(GetProcessHandleCount(GetCurrentProcess(), &count));
    return count;
}

static int connect_start(wf_connect_result *result, unsigned port,
                         const wf_deadline *deadline, const wf_cancel_handle *watch) {
    wf_value address;
    wf__body_socket_address_v4(&address, 127, 0, 0, 1, (uint16_t)port);
    return wf__body_tcp_connect_start(result, &factory, &address, deadline, watch, operation());
}

static void connect_finish(wf_connect_result *result, int state, unsigned port,
                           const wf_deadline *deadline, const wf_cancel_handle *watch) {
    wf_value address;
    wf__body_socket_address_v4(&address, 127, 0, 0, 1, (uint16_t)port);
    finish_operation(state);
    wf__body_tcp_connect_finish(result, &factory, &address, deadline, watch, operation());
}

static void expect_route(void) {
    if (helper_route) {
        CHECK(operation()->record.route == WF_COMPLETION_ROUTE_FILE_ADAPTER);
    } else {
        CHECK(operation()->record.route == WF_COMPLETION_ROUTE_WINDOWS_IOCP);
    }
}

static void expect_error(const wf_connect_result *result, unsigned tag, const char *what) {
    if (result->tag != 1 || result->err.error.tag != tag) {
        fprintf(stderr, "%s: %s: result tag %u, error tag %u, expected error tag %u\n",
                WF_BOUNDED_TEST_NAME, what, (unsigned)result->tag,
                result->tag == 1 ? (unsigned)result->err.error.tag : 0u, tag);
        exit(1);
    }
}

/* The same bound without the line per call, for the repeated cases. */
static void expect_quickly(const char *what, uint64_t elapsed) {
    if (elapsed > bound_ns) {
        fprintf(stderr, "%s: %s took %llu ms, over the %llu ms bound\n",
                WF_BOUNDED_TEST_NAME, what,
                (unsigned long long)(elapsed / MILLISECOND_NS),
                (unsigned long long)(bound_ns / MILLISECOND_NS));
        exit(1);
    }
}

static void expect_within(const char *what, uint64_t elapsed) {
    printf("%s: %s: %llu ms\n", WF_BOUNDED_TEST_NAME, what,
           (unsigned long long)(elapsed / MILLISECOND_NS));
    fflush(stdout);
    if (elapsed > bound_ns) {
        fprintf(stderr, "%s: %s took %llu ms, over the %llu ms bound\n",
                WF_BOUNDED_TEST_NAME, what,
                (unsigned long long)(elapsed / MILLISECOND_NS),
                (unsigned long long)(bound_ns / MILLISECOND_NS));
        exit(1);
    }
}

static void close_connection(wf_connection *connection) {
    wf_close_result closed;
    wf__body_close_send(&closed, &factory, &connection->send);
    CHECK(closed.tag == 0);
    wf__body_close_receive(&closed, &factory, &connection->receive);
    CHECK(closed.tag == 0);
}

/* Closes every connection the ordinary listener holds that no one accepted. */
static void drain_ordinary_listener(void) {
    u_long nonblocking = 1;
    CHECK(ioctlsocket(ordinary_listener, FIONBIO, &nonblocking) == 0);
    for (;;) {
        SOCKET taken = accept(ordinary_listener, NULL, NULL);
        if (taken == INVALID_SOCKET) {
            CHECK(WSAGetLastError() == WSAEWOULDBLOCK);
            break;
        }
        CHECK(closesocket(taken) == 0);
    }
    nonblocking = 0;
    CHECK(ioctlsocket(ordinary_listener, FIONBIO, &nonblocking) == 0);
}

/* A: the watch fires while a helper is inside the connect. */
static void case_cancel(void) {
    wf_cancel_handle source, watch;
    wf_connect_result result;
    uint64_t start;
    int state;
    wf_test_guard_phase("connect A: cancellation while the connect is pending");
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    state = connect_start(&result, silent_port, NULL, &watch);
    expect_route();
    if (helper_route) await_helper_claim();
    /* Long enough for the helper to be inside the connect, not still in its
     * queue; the connect itself cannot have completed. */
    Sleep(150);
    CHECK(wf_bridge_record_state(&operation()->record) != WF_COMPLETION_DONE);
    start = wf_file_monotonic_ns();
    end_bound(1, &source, NULL);
    connect_finish(&result, state, silent_port, NULL, &watch);
    expect_error(&result, WF_IO_CANCELLED, "A: cancellation");
    expect_within("A: cancellation to return", wf_file_monotonic_ns() - start);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
}

/* B1: the deadline passes while the helper is inside the connect. */
static void case_deadline(void) {
    wf_deadline deadline = {0};
    wf_connect_result result;
    uint64_t finished;
    int state;
    wf_test_guard_phase("connect B1: deadline while the connect is pending");
    deadline.tag = WF_OPTION_SOME;
    deadline.value.ticks = wf_file_monotonic_ns() + 300 * MILLISECOND_NS;
    state = connect_start(&result, silent_port, &deadline, &never);
    expect_route();
    if (helper_route) await_helper_claim();
    end_bound(0, NULL, &deadline);
    connect_finish(&result, state, silent_port, &deadline, &never);
    finished = wf_file_monotonic_ns();
    expect_error(&result, WF_IO_DEADLINE_PASSED, "B1: deadline");
    CHECK(finished >= deadline.value.ticks);
    expect_within("B1: deadline to return", finished - deadline.value.ticks);
}

/* B2 and B3: the bound is over before the call is made. */
static void case_bound_over_at_call(void) {
    wf_cancel_handle source, watch;
    wf_deadline deadline = {0};
    wf_connect_result result;
    uint64_t start;
    int state;
    wf_test_guard_phase("connect B2: watch fired before the call");
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    end_bound(1, &source, NULL);
    start = wf_file_monotonic_ns();
    state = connect_start(&result, silent_port, NULL, &watch);
    connect_finish(&result, state, silent_port, NULL, &watch);
    expect_error(&result, WF_IO_CANCELLED, "B2: watch fired before the call");
    expect_within("B2: fired watch to return", wf_file_monotonic_ns() - start);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    wf_test_guard_phase("connect B3: deadline passed before the call");
    deadline.tag = WF_OPTION_SOME;
    deadline.value.ticks = wf_file_monotonic_ns() - 1;
    start = wf_file_monotonic_ns();
    state = connect_start(&result, silent_port, &deadline, &never);
    connect_finish(&result, state, silent_port, &deadline, &never);
    expect_error(&result, WF_IO_DEADLINE_PASSED, "B3: deadline passed before the call");
    expect_within("B3: passed deadline to return", wf_file_monotonic_ns() - start);
}

/* C: a connect that can succeed, raced by a firing at 0 to 2 ms. */
static void case_race(void) {
    enum { ITERATIONS = 200 };
    unsigned connected = 0, cancelled = 0;
    wf_test_guard_phase("connect C: success racing a cancellation");
    for (unsigned i = 0; i < ITERATIONS; i++) {
        wf_cancel_handle source, watch;
        wf_connect_result result;
        uint64_t start, delay_until;
        int state;
        wf__body_cancel_source(&source);
        wf__body_cancel_watch(&watch, &source);
        state = connect_start(&result, ordinary_port, NULL, &watch);
        expect_route();
        delay_until = wf_file_monotonic_ns() + (uint64_t)(i % 100) * 20000u;
        while (wf_file_monotonic_ns() < delay_until) {
        }
        start = wf_file_monotonic_ns();
        end_bound(1, &source, NULL);
        connect_finish(&result, state, ordinary_port, NULL, &watch);
        if (result.tag == 0) {
            connected++;
            CHECK(wf_test_socket_open((int)result.ok.value.send.words[0]));
            close_connection(&result.ok.value);
        } else {
            expect_error(&result, WF_IO_CANCELLED, "C: raced connect");
            cancelled++;
        }
        expect_within("C: raced call to return", wf_file_monotonic_ns() - start);
        wf__body_close_cancel_watch(&watch);
        wf__body_close_cancel_source(&source);
    }
    printf("%s: C: %u connected, %u cancelled\n", WF_BOUNDED_TEST_NAME, connected, cancelled);
    fflush(stdout);
    drain_ordinary_listener();
}

/* What a connection a connect returned must be: connected to the ordinary
 * listener's next peer, in blocking mode, carrying data both ways; closes it
 * and the peer. */
static void probe_connection(wf_connect_result *result) {
    SOCKET peer, client;
    DWORD timeout = 300;
    char byte = 0;
    uint64_t start;
    int received;
    CHECK(result->tag == 0);
    peer = accept(ordinary_listener, NULL, NULL);
    CHECK(peer != INVALID_SOCKET);
    client = (SOCKET)wf__windows_socket_handle((int)result->ok.value.send.words[0]);
    if (control_nonblocking) {
        /* What a connect that forgot to restore blocking mode would hand out. */
        u_long on = 1;
        CHECK(ioctlsocket(client, FIONBIO, &on) == 0);
    }
    /* A nonblocking socket answers an empty receive at once; a blocking one
     * waits out the receive timeout. */
    CHECK(setsockopt(client, SOL_SOCKET, SO_RCVTIMEO, (const char *)&timeout, sizeof(timeout)) == 0);
    start = wf_file_monotonic_ns();
    received = recv(client, &byte, 1, 0);
    CHECK(received == SOCKET_ERROR && WSAGetLastError() == WSAETIMEDOUT);
    CHECK(wf_file_monotonic_ns() - start >= 200 * MILLISECOND_NS);
    timeout = 0;
    CHECK(setsockopt(client, SOL_SOCKET, SO_RCVTIMEO, (const char *)&timeout, sizeof(timeout)) == 0);
    CHECK(send(peer, "x", 1, 0) == 1);
    CHECK(recv(client, &byte, 1, 0) == 1 && byte == 'x');
    CHECK(send(client, "y", 1, 0) == 1);
    CHECK(recv(peer, &byte, 1, 0) == 1 && byte == 'y');
    CHECK(closesocket(peer) == 0);
    close_connection(&result->ok.value);
}

/* D: no bound at all returns a connection in blocking mode that carries data. */
static void case_unbounded(void) {
    wf_connect_result result;
    wf_value address;
    wf_test_guard_phase("connect D: no bound");
    wf__body_socket_address_v4(&address, 127, 0, 0, 1, (uint16_t)ordinary_port);
    wf__body_tcp_connect(&result, &factory, &address, NULL, &never);
    probe_connection(&result);
}

/* F: a bound that is never reached changes neither the outcome nor the mode
 * of the socket handed out, and costs no slice of waiting. */
static void case_bounded_success(void) {
    wf_cancel_handle source, watch;
    wf_deadline deadline = {0};
    wf_connect_result result;
    uint64_t start;
    int state;
    for (int variant = 0; variant < 3; variant++) {
        const wf_cancel_handle *used = variant == 1 ? &never : &watch;
        const wf_deadline *limit = variant == 0 ? NULL : &deadline;
        wf_test_guard_phase("connect F: bounded connect that succeeds");
        wf__body_cancel_source(&source);
        wf__body_cancel_watch(&watch, &source);
        deadline.tag = WF_OPTION_SOME;
        deadline.value.ticks = wf_file_monotonic_ns() + 60 * SECOND_NS;
        start = wf_file_monotonic_ns();
        state = connect_start(&result, ordinary_port, limit, used);
        connect_finish(&result, state, ordinary_port, limit, used);
        CHECK(result.tag == 0);
        expect_within(variant == 0 ? "F: watch only, success"
                      : variant == 1 ? "F: deadline only, success"
                                     : "F: watch and deadline, success",
                      wf_file_monotonic_ns() - start);
        probe_connection(&result);
        wf__body_close_cancel_watch(&watch);
        wf__body_close_cancel_source(&source);
    }
}

/* G: the firing meets the helper at each phase of its connect: not yet
 * claimed, creating the socket, inside the connect, inside the wait. */
static void case_cancel_phases(void) {
    enum { FIRINGS = 30 };
    wf_test_guard_phase("connect G: cancellation at every phase");
    for (unsigned i = 0; i < FIRINGS; i++) {
        wf_cancel_handle source, watch;
        wf_connect_result result;
        uint64_t start;
        int state;
        wf__body_cancel_source(&source);
        wf__body_cancel_watch(&watch, &source);
        state = connect_start(&result, silent_port, NULL, &watch);
        expect_route();
        if (helper_route && (i & 1u)) await_helper_claim();
        if (i != 0) Sleep(i);
        CHECK(wf_bridge_record_state(&operation()->record) != WF_COMPLETION_DONE);
        start = wf_file_monotonic_ns();
        end_bound(1, &source, NULL);
        connect_finish(&result, state, silent_port, NULL, &watch);
        expect_error(&result, WF_IO_CANCELLED, "G: cancellation at a phase");
        expect_quickly("G: cancellation to return", wf_file_monotonic_ns() - start);
        wf__body_close_cancel_watch(&watch);
        wf__body_close_cancel_source(&source);
    }
    printf("%s: G: %u firings ended Cancelled\n", WF_BOUNDED_TEST_NAME, (unsigned)FIRINGS);
    fflush(stdout);
}

/* H: a deadline and a real watch on one connect: whichever is reached first
 * decides, and the other changes nothing. */
static void case_both_bounds(void) {
    wf_cancel_handle source, watch;
    wf_deadline deadline = {0};
    wf_connect_result result;
    uint64_t start, finished;
    int state;
    wf_test_guard_phase("connect H1: deadline reached before the watch fires");
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    deadline.tag = WF_OPTION_SOME;
    deadline.value.ticks = wf_file_monotonic_ns() + 300 * MILLISECOND_NS;
    state = connect_start(&result, silent_port, &deadline, &watch);
    expect_route();
    if (helper_route) await_helper_claim();
    end_bound(0, NULL, &deadline);
    connect_finish(&result, state, silent_port, &deadline, &watch);
    finished = wf_file_monotonic_ns();
    expect_error(&result, WF_IO_DEADLINE_PASSED, "H1: deadline with a watch");
    CHECK(finished >= deadline.value.ticks);
    expect_within("H1: deadline to return", finished - deadline.value.ticks);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    wf_test_guard_phase("connect H2: watch fires before the deadline");
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    deadline.value.ticks = wf_file_monotonic_ns() + 60 * SECOND_NS;
    state = connect_start(&result, silent_port, &deadline, &watch);
    expect_route();
    if (helper_route) await_helper_claim();
    Sleep(100);
    start = wf_file_monotonic_ns();
    end_bound(1, &source, NULL);
    connect_finish(&result, state, silent_port, &deadline, &watch);
    finished = wf_file_monotonic_ns();
    expect_error(&result, WF_IO_CANCELLED, "H2: watch with a deadline");
    CHECK(finished < deadline.value.ticks);
    expect_within("H2: cancellation to return", finished - start);
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);
}

/* I: short deadlines on both sides of the 50 ms slice. */
static void case_deadline_slices(void) {
    static const unsigned milliseconds[] = {1, 2, 10, 49, 50, 51, 99};
    wf_test_guard_phase("connect I: deadlines around one slice");
    for (size_t i = 0; i < sizeof(milliseconds) / sizeof(milliseconds[0]); i++) {
        wf_deadline deadline = {0};
        wf_connect_result result;
        uint64_t finished;
        int state;
        deadline.tag = WF_OPTION_SOME;
        deadline.value.ticks = wf_file_monotonic_ns() + (uint64_t)milliseconds[i] * MILLISECOND_NS;
        state = connect_start(&result, silent_port, &deadline, &never);
        end_bound(0, NULL, &deadline);
        connect_finish(&result, state, silent_port, &deadline, &never);
        finished = wf_file_monotonic_ns();
        expect_error(&result, WF_IO_DEADLINE_PASSED, "I: short deadline");
        CHECK(finished >= deadline.value.ticks);
        expect_quickly("I: short deadline to return", finished - deadline.value.ticks);
    }
}

/* J: a connect that can succeed, raced by a deadline a few tens of
 * microseconds away: a connection or DeadlinePassed, nothing else. */
static void case_deadline_race(void) {
    enum { ITERATIONS = 100 };
    unsigned connected = 0, passed = 0;
    wf_test_guard_phase("connect J: success racing a deadline");
    for (unsigned i = 0; i < ITERATIONS; i++) {
        wf_deadline deadline = {0};
        wf_connect_result result;
        uint64_t finished;
        int state;
        deadline.tag = WF_OPTION_SOME;
        deadline.value.ticks = wf_file_monotonic_ns() + (uint64_t)(i % 50) * 40000u;
        state = connect_start(&result, ordinary_port, &deadline, &never);
        connect_finish(&result, state, ordinary_port, &deadline, &never);
        finished = wf_file_monotonic_ns();
        if (result.tag == 0) {
            connected++;
            CHECK(wf_test_socket_open((int)result.ok.value.send.words[0]));
            close_connection(&result.ok.value);
        } else {
            expect_error(&result, WF_IO_DEADLINE_PASSED, "J: raced connect");
            CHECK(finished >= deadline.value.ticks);
            passed++;
        }
        expect_quickly("J: raced call to return",
                       finished > deadline.value.ticks ? finished - deadline.value.ticks : 0);
    }
    printf("%s: J: %u connected, %u deadline passed\n", WF_BOUNDED_TEST_NAME, connected, passed);
    fflush(stdout);
    drain_ordinary_listener();
}

/* E: a connect to a closed port is the host's refusal. */
static void case_refused(void) {
    wf_cancel_handle source, watch;
    wf_connect_result result;
    wf_value address;
    unsigned port = closed_port();
    int state;
    wf_test_guard_phase("connect E: refused connect with a watch");
    wf__body_cancel_source(&source);
    wf__body_cancel_watch(&watch, &source);
    state = connect_start(&result, port, NULL, &watch);
    connect_finish(&result, state, port, NULL, &watch);
    expect_error(&result, WF_IO_CONNECTION_REFUSED, "E: refused connect with a watch");
    wf__body_close_cancel_watch(&watch);
    wf__body_close_cancel_source(&source);

    wf_test_guard_phase("connect E: refused connect with no bound");
    wf__body_socket_address_v4(&address, 127, 0, 0, 1, (uint16_t)port);
    wf__body_tcp_connect(&result, &factory, &address, NULL, &never);
    expect_error(&result, WF_IO_CONNECTION_REFUSED, "E: refused connect with no bound");
}

/* One of each case that creates a helper or a port or an event on first use. */
static void warm_up(void) {
    wf_test_guard_phase("connect warm-up");
    case_cancel();
    case_deadline();
    case_unbounded();
}

int main(int argc, char **argv) {
    const char *route = getenv("WF_IO_NO_NATIVE_RING");
    int control_leak = argc > 1 && strcmp(argv[1], "control-leak") == 0;
    SOCKET leaked = INVALID_SOCKET;
    WSADATA winsock;
    DWORD baseline, after;
    uint64_t settle;
    if (argc > 1 && strcmp(argv[1], "control-bound") == 0) bound_ns = 0;
    control_nonblocking = argc > 1 && strcmp(argv[1], "control-blocking") == 0;
    CHECK(WSAStartup(MAKEWORD(2, 2), &winsock) == 0);
    wf_test_guard_start(180);
    wf__context_root_begin();
    helper_route = route != NULL && route[0] == '1';
    wf__body_cancel_never(&never);
    printf("%s: %s route\n", WF_BOUNDED_TEST_NAME,
           helper_route ? "helper" : "completion port");

    wf_test_guard_phase("fixtures");
    silent_listener = raw_listener(1, &silent_port);
    ordinary_listener = raw_listener(0, &ordinary_port);
    check_silent_target();

    warm_up();
    baseline = handle_count();

    case_cancel();
    case_deadline();
    case_bound_over_at_call();
    case_cancel_phases();
    case_both_bounds();
    case_deadline_slices();
    case_race();
    case_deadline_race();
    case_unbounded();
    case_bounded_success();
    case_refused();
    if (control_leak) {
        leaked = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        CHECK(leaked != INVALID_SOCKET);
    }

    wf_test_guard_phase("leak check");
    CHECK(factory.words[0] == 8);
    CHECK(wf_driver_root.cancel_waits == NULL);
    CHECK(wf_driver_root.timer_count == 0);
    /* A closed socket's handle may be reaped a moment later. */
    settle = wf_file_monotonic_ns() + SECOND_NS;
    for (;;) {
        after = handle_count();
        if (after <= baseline || wf_file_monotonic_ns() >= settle) break;
        Sleep(10);
    }
    if (after != baseline) {
        fprintf(stderr, "%s: handle count %lu after the cases, %lu before them\n",
                WF_BOUNDED_TEST_NAME, (unsigned long)after, (unsigned long)baseline);
        exit(1);
    }
    if (leaked != INVALID_SOCKET) (void)closesocket(leaked);
    CHECK(closesocket(ordinary_listener) == 0);
    CHECK(closesocket(silent_listener) == 0);
    wf__body_close_cancel_watch(&never);
    wf_test_guard_finish();
    return 0;
}
