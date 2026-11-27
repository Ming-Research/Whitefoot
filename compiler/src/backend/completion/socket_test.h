#ifndef WHITEFOOT_COMPLETION_SOCKET_TEST_H
#define WHITEFOOT_COMPLETION_SOCKET_TEST_H

/* Shared bridge lifecycle case for scripted-policy and shipped-policy tests.
 * Retire this test support if those two callers no longer need the same host
 * and result observations. No production source includes it. */
#include "bridge.h"
#include "contract.h"
#include "socket_address.h"
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#if defined(_WIN32)
#include <winsock2.h>
#include <ws2tcpip.h>
#include "../windows_runtime.h"
#else
#include <arpa/inet.h>
#include <fcntl.h>
#include <sys/socket.h>
#endif

static inline int wf_test_socket_open(int descriptor) {
#if defined(_WIN32)
    return wf__windows_socket_handle(descriptor) != (uintptr_t)INVALID_SOCKET;
#else
    return fcntl(descriptor, F_GETFD) >= 0;
#endif
}

/* Capture while the CRT descriptor is valid. Querying _get_osfhandle after
 * its final close invokes the CRT invalid-parameter handler rather than
 * providing a liveness result. Winsock can safely inspect the saved socket;
 * this fixture creates no intervening socket that could reuse its value. */
static inline uintptr_t wf_test_socket_native(int descriptor) {
#if defined(_WIN32)
    return wf__windows_socket_handle(descriptor);
#else
    return (uintptr_t)descriptor;
#endif
}

static inline int wf_test_native_socket_open(uintptr_t native) {
#if defined(_WIN32)
    int type = 0, length = sizeof(type);
    return getsockopt((SOCKET)native, SOL_SOCKET, SO_TYPE,
                      (char *)&type, &length) == 0;
#else
    return fcntl((int)native, F_GETFD) >= 0;
#endif
}

static inline unsigned wf_test_socket_port(int descriptor) {
    struct sockaddr_in address;
    memset(&address, 0, sizeof(address));
#if defined(_WIN32)
    int length = sizeof(address);
    SOCKET socket = (SOCKET)wf__windows_socket_handle(descriptor);
#else
    socklen_t length = sizeof(address);
    int socket = descriptor;
#endif
    if (getsockname(socket, (struct sockaddr *)&address, &length)) return 0;
    return (unsigned)ntohs(address.sin_port);
}

/* The host-record to portable-address conversion every engine shares
 * (`socket_address.h`): the two admitted families convert, and every other
 * record answers a nonzero status and the all-zero address.  The output is
 * filled with ones first, so a conversion that left it untouched would read as
 * a failure of the zero rule and not as a pass. */
static inline int wf_test_socket_address_conversion(void) {
    wf_socket_native_address native;
    wf_socket_address portable, back;
    struct sockaddr_in host4;
    unsigned length, index;
    const char *phase = "ipv4";
    int failed = 1;
#define ADDRESS_REQUIRE(condition) do { if (!(condition)) goto cleanup; } while (0)
#define ADDRESS_FILL() memset(&back, 0xff, sizeof(back))
#define ADDRESS_ZERO() (back.words[0] == 0 && back.words[1] == 0 \
                        && back.port_and_family == 0)
    memset(&portable, 0, sizeof(portable));
    portable.words[0] = UINT64_C(0x0100007f);
    portable.port_and_family = 8080u;
    length = wf_socket_native_from_address(&portable, &native);
    ADDRESS_FILL();
    ADDRESS_REQUIRE(wf_socket_address_from_native(&native, length, &back) == 0);
    ADDRESS_REQUIRE(back.words[0] == portable.words[0] && back.words[1] == 0
                    && back.port_and_family == 8080u);
    phase = "ipv6";
    memset(&portable, 0, sizeof(portable));
    for (index = 0; index < 16u; index += 1u) {
        wf_socket_address_set_byte(&portable, index, (unsigned char)(index + 1u));
    }
    portable.port_and_family = 443u | WF_SOCKET_FAMILY_V6;
    length = wf_socket_native_from_address(&portable, &native);
    ADDRESS_FILL();
    ADDRESS_REQUIRE(wf_socket_address_from_native(&native, length, &back) == 0);
    ADDRESS_REQUIRE(back.words[0] == portable.words[0]
                    && back.words[1] == portable.words[1]
                    && back.port_and_family == portable.port_and_family);
    phase = "ipv6 record too short";
    ADDRESS_FILL();
    ADDRESS_REQUIRE(wf_socket_address_from_native(&native, length - 1u, &back) != 0);
    ADDRESS_REQUIRE(ADDRESS_ZERO());
    phase = "ipv4 record too short";
    memset(&portable, 0, sizeof(portable));
    portable.words[0] = UINT64_C(0x0100007f);
    length = wf_socket_native_from_address(&portable, &native);
    ADDRESS_FILL();
    ADDRESS_REQUIRE(
        wf_socket_address_from_native(&native, length - 1u, &back) != 0
    );
    ADDRESS_REQUIRE(ADDRESS_ZERO());
    phase = "no family byte";
    ADDRESS_FILL();
    ADDRESS_REQUIRE(wf_socket_address_from_native(&native, 0u, &back) != 0);
    ADDRESS_REQUIRE(ADDRESS_ZERO());
    phase = "unspecified family";
    memcpy(&host4, native.bytes, sizeof(host4));
    host4.sin_family = AF_UNSPEC;
    memcpy(native.bytes, &host4, sizeof(host4));
    ADDRESS_FILL();
    ADDRESS_REQUIRE(wf_socket_address_from_native(&native, length, &back) != 0);
    ADDRESS_REQUIRE(ADDRESS_ZERO());
#if defined(AF_UNIX)
    phase = "unix family";
    host4.sin_family = AF_UNIX;
    memcpy(native.bytes, &host4, sizeof(host4));
    ADDRESS_FILL();
    ADDRESS_REQUIRE(wf_socket_address_from_native(&native, length, &back) != 0);
    ADDRESS_REQUIRE(ADDRESS_ZERO());
#endif
    failed = 0;
cleanup:
    if (failed) fprintf(stderr, "socket address conversion: %s failed\n", phase);
#undef ADDRESS_ZERO
#undef ADDRESS_FILL
#undef ADDRESS_REQUIRE
    return failed;
}

static inline int wf_test_socket_lifecycle(unsigned *chosen_port) {
    _Alignas(WF_COMPLETION_RECORD_ALIGN) unsigned char record[WF_COMPLETION_RECORD_BYTES];
    const unsigned char sent[] = {3, 1, 4, 1, 5, 9, 2, 6};
    unsigned char received[sizeof(sent)] = {0};
    int listener = -1, endpoints[2] = {-1, -1};
    unsigned released[2] = {0, 0};
    int64_t value = -1;
    int error = 0, failed = 1;
    uint64_t low = 0, high = 0;
    uint32_t tag = 0;
    const char *phase = "listen";
#define SOCKET_REQUIRE(condition) do { if (!(condition)) goto cleanup; } while (0)
#define SOCKET_JOIN() wf__completion_file_join(record, &value, &error)
    *chosen_port = 0;
    wf__completion_socket_listen_submit(UINT64_C(0x0100007f), 0, 0, record);
    SOCKET_JOIN();
    SOCKET_REQUIRE(value >= 0 && error == 0);
    listener = (int)value;
    *chosen_port = wf_test_socket_port(listener);
    SOCKET_REQUIRE(*chosen_port != 0);
    phase = "connect";
    wf__completion_socket_connect_submit(UINT64_C(0x0100007f), 0, *chosen_port, record);
    SOCKET_JOIN();
    SOCKET_REQUIRE(value >= 0 && error == 0);
    endpoints[0] = (int)value;
    phase = "accept/peer";
    wf__completion_socket_accept_submit(listener, record);
    wf__completion_socket_accept_join(record, &value, &error, &low, &high, &tag);
    SOCKET_REQUIRE(value >= 0 && error == 0);
    endpoints[1] = (int)value;
    SOCKET_REQUIRE(low == UINT64_C(0x0100007f) && high == 0);
    SOCKET_REQUIRE(!(tag & WF_SOCKET_FAMILY_V6) && (tag & WF_SOCKET_PORT_MASK));
    phase = "send";
    for (size_t at = 0; at < sizeof(sent); at += (size_t)value) {
        wf__completion_socket_send_submit(endpoints[0], sent + at, sizeof(sent) - at, record);
        SOCKET_JOIN();
        SOCKET_REQUIRE(value > 0 && (uint64_t)value <= sizeof(sent) - at && error == 0);
    }
    phase = "receive";
    for (size_t at = 0; at < sizeof(received); at += (size_t)value) {
        wf__completion_socket_receive_submit(endpoints[1], received + at,
                                            sizeof(received) - at, record);
        SOCKET_JOIN();
        SOCKET_REQUIRE(value > 0 && (uint64_t)value <= sizeof(received) - at && error == 0);
    }
    SOCKET_REQUIRE(memcmp(received, sent, sizeof(sent)) == 0);
    phase = "empty receive";
    wf__completion_socket_receive_submit(endpoints[1], received, 0, record);
    SOCKET_JOIN();
    SOCKET_REQUIRE(value == 0 && error == 0);
    SOCKET_REQUIRE(memcmp(received, sent, sizeof(sent)) == 0);
    phase = "directional close/credit";
    for (unsigned endpoint = 0; endpoint < 2; ++endpoint) {
        uintptr_t native = wf_test_socket_native(endpoints[endpoint]);
        /* Both direction orders protect the private first/last-close values. */
        for (unsigned order = 0; order < 2; ++order) {
            unsigned direction = endpoint ^ order;
            wf__completion_socket_shutdown_submit(endpoints[endpoint], direction, record);
            SOCKET_JOIN();
            SOCKET_REQUIRE(value == (int64_t)order && error == 0);
            released[endpoint] |= 1u << direction;
            SOCKET_REQUIRE(wf_test_native_socket_open(native) == (order == 0));
        }
        endpoints[endpoint] = -1;
    }
    phase = "listener close";
    wf__completion_file_close_submit(listener, record);
    SOCKET_JOIN();
    SOCKET_REQUIRE(value == 0 && error == 0);
    listener = -1;
#if !defined(_WIN32)
    phase = "refused connect";
    wf__completion_socket_connect_submit(UINT64_C(0x0100007f), 0, *chosen_port, record);
    SOCKET_JOIN();
    SOCKET_REQUIRE(value < 0 && error == ECONNREFUSED);
#endif
    failed = 0;
cleanup:
    if (failed) fprintf(stderr, "bridge socket lifecycle: %s failed (value=%lld error=%d)\n",
                        phase, (long long)value, error);
    for (unsigned endpoint = 0; endpoint < 2; ++endpoint) {
        if (endpoints[endpoint] < 0) continue;
        for (unsigned direction = 0; direction < 2; ++direction) {
            if (released[endpoint] & (1u << direction)) continue;
            wf__completion_socket_shutdown_submit(endpoints[endpoint], direction, record);
            SOCKET_JOIN();
        }
    }
    if (listener >= 0) {
        wf__completion_file_close_submit(listener, record);
        SOCKET_JOIN();
    }
#undef SOCKET_JOIN
#undef SOCKET_REQUIRE
    return failed;
}
#endif
