#ifndef WHITEFOOT_COMPLETION_SOCKET_ADDRESS_H
#define WHITEFOOT_COMPLETION_SOCKET_ADDRESS_H

/*
 * The address vocabulary every socket engine shares (ordinary native library).
 *
 * A `SocketAddress` crosses the ABI as the three scalars of its emitted
 * layout (`contract.h`, `wf_socket_address`), and every engine that makes a
 * host call has to turn that into the host's own address record and back
 * again: `file_posix.c` for the queued and inline routes, `linux_io_uring.c`
 * for the ring's connect and accept, `file_windows.c` for the Windows adapter
 * route, and `windows_iocp.c` for the completion port's connect.  So the
 * conversions live here, once, and one address states one contract whichever
 * engine ran the operation.
 *
 * The only thing that differs by platform is which header declares the host's
 * address records and its byte-order calls.  Their names, their fields and
 * their layouts are the same on both, so the rules below are one text: an
 * address is a value, and no host call is made here.
 *
 * This header was `file_posix.h`'s until the Windows routes landed.  It is not
 * a twin of that header: the POSIX leaf includes this one and keeps only what
 * `<fcntl.h>` and `<sys/stat.h>` give it.
 */

#include "contract.h"

#if defined(_WIN32)
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <winsock2.h>
#include <ws2tcpip.h>
#else
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>
#endif

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(__cplusplus)
extern "C" {
#endif

/* The record carries `unsigned` for a native address length, because the
 * record is one ABI on every platform and neither `<sys/socket.h>` nor
 * `<winsock2.h>` reaches it.  The assertion is what keeps that field and the
 * host's own length type one fact: the Linux ring hands the field straight to
 * the kernel as a `socklen_t *`, and the Windows leaves hand it to Winsock as
 * an `int *`. */
#if defined(_WIN32)
_Static_assert(
    sizeof(int) == sizeof(unsigned),
    "the record's address length must be the host's own length type"
);
#else
_Static_assert(
    sizeof(socklen_t) == sizeof(unsigned),
    "the record's address length must be the host's own socklen_t"
);
#endif
_Static_assert(
    sizeof(struct sockaddr_in6) <= sizeof(wf_socket_native_address),
    "the record's native address storage must hold the larger of the two families"
);
_Static_assert(
    sizeof(struct sockaddr_in) <= sizeof(wf_socket_native_address),
    "the record's native address storage must hold an IPv4 address"
);

/* The listen backlog, which is the target's own maximum.
 *
 * It is not a resource on this API: the accept queue is a kernel queue the
 * peer fills, and the program observes it only through accept's outcomes, so
 * a full backlog refuses the peer and never the program
 * (`research/investigations/io-model/NETWORK.md` §2).  Asking for the host's
 * own maximum is therefore the only answer that adds no bound of the
 * runtime's.  Both families spell it `SOMAXCONN`; on Windows that value is
 * the provider's "choose your own maximum" request rather than a count, which
 * is the same meaning. */
#if defined(SOMAXCONN)
#define WF_SOCKET_BACKLOG SOMAXCONN
#else
#define WF_SOCKET_BACKLOG 128
#endif

/* The flags a send carries.  A write to a connection whose peer has gone is
 * an ordinary `BrokenPipe` outcome, and native initialization already ignores
 * the write-to-closed-pipe signal once before invocation; where the
 * host offers the per-call form as well, it is used, so a runtime unit with no
 * bootstrap in front of it -- a probe -- answers the same way.  Windows raises
 * no such signal and offers no such flag, so it carries none. */
#if defined(MSG_NOSIGNAL)
#define WF_SOCKET_SEND_FLAGS MSG_NOSIGNAL
#else
#define WF_SOCKET_SEND_FLAGS 0
#endif

/* The host address family one portable address names.  The family is one bit
 * of the value, so there are exactly two answers. */
static inline int wf_socket_address_family(const wf_socket_address *address) {
    return (address->port_and_family & WF_SOCKET_FAMILY_V6) != 0u ? AF_INET6
                                                                  : AF_INET;
}

/* The byte of one portable address at `index`, by the rule the emitted
 * constructors write it with: byte `i` occupies bits `8 * (i % 8)` of word
 * `i / 8`, which reads the same on either endianness. */
static inline unsigned char wf_socket_address_byte(
    const wf_socket_address *address,
    unsigned index
) {
    return (unsigned char)((address->words[index / 8u] >> (8u * (index % 8u)))
                           & 0xffu);
}

static inline void wf_socket_address_set_byte(
    wf_socket_address *address,
    unsigned index,
    unsigned char value
) {
    unsigned shift = 8u * (index % 8u);
    address->words[index / 8u] &= ~((uint64_t)0xffu << shift);
    address->words[index / 8u] |= (uint64_t)value << shift;
}

/* Builds the host's own record from one portable address, and reports its
 * length.  The family is one bit of the value, so there are exactly two
 * records to build and every address builds one.
 *
 * The port is carried in host order in the portable value and put into
 * network order here, which is the one place either representation is
 * converted. */
static inline unsigned wf_socket_native_from_address(
    const wf_socket_address *address,
    wf_socket_native_address *native
) {
    unsigned port = address->port_and_family & WF_SOCKET_PORT_MASK;
    memset(native, 0, sizeof(*native));
    if ((address->port_and_family & WF_SOCKET_FAMILY_V6) != 0u) {
        struct sockaddr_in6 host;
        unsigned index;
        memset(&host, 0, sizeof(host));
        host.sin6_family = AF_INET6;
        host.sin6_port = htons((unsigned short)port);
        for (index = 0; index < 16u; index += 1u) {
            host.sin6_addr.s6_addr[index] =
                wf_socket_address_byte(address, index);
        }
        memcpy(native->bytes, &host, sizeof(host));
        return (unsigned)sizeof(host);
    } else {
        struct sockaddr_in host;
        uint32_t packed = 0;
        unsigned index;
        memset(&host, 0, sizeof(host));
        host.sin_family = AF_INET;
        host.sin_port = htons((unsigned short)port);
        /* The four bytes are the address in its conventional order, so byte 0
         * is the first written octet; `s_addr` is that sequence in network
         * order, which is the same sequence read big-endian. */
        for (index = 0; index < 4u; index += 1u) {
            packed = (packed << 8) | wf_socket_address_byte(address, index);
        }
        host.sin_addr.s_addr = htonl(packed);
        memcpy(native->bytes, &host, sizeof(host));
        return (unsigned)sizeof(host);
    }
}

/* The wildcard address of one family, with no port, in the host's own record.
 *
 * One reader: the Windows completion port's connect, whose `ConnectEx`
 * requires its socket to be bound before the request is issued.  It is here
 * rather than in that leaf because it is the same construction the two rules
 * above make and states the same layout fact; nothing about it is Windows's. */
static inline unsigned wf_socket_native_wildcard(
    int family,
    wf_socket_native_address *native
) {
    memset(native, 0, sizeof(*native));
    if (family == AF_INET6) {
        struct sockaddr_in6 host;
        memset(&host, 0, sizeof(host));
        host.sin6_family = AF_INET6;
        memcpy(native->bytes, &host, sizeof(host));
        return (unsigned)sizeof(host);
    } else {
        struct sockaddr_in host;
        memset(&host, 0, sizeof(host));
        host.sin_family = AF_INET;
        memcpy(native->bytes, &host, sizeof(host));
        return (unsigned)sizeof(host);
    }
}

/* The reverse: one portable address from the host's own record.  Answers 0
 * with the address written, and 1 when the record is of neither family, or is
 * shorter than its family needs, leaving the address all zero: the portable
 * form holds no third family, and an all-zero value would read as 0.0.0.0:0,
 * an address the host never reported.  The status is for a caller that has to
 * tell the two apart; the engines use `wf_socket_address_published` below,
 * because every socket a program holds was established from an address of one
 * of the two families, so a record of any other form means the host broke
 * what the runtime rests on. */
static inline int wf_socket_address_from_native(
    const wf_socket_native_address *native,
    unsigned length,
    wf_socket_address *address
) {
    struct sockaddr head;
    unsigned family;
    memset(address, 0, sizeof(*address));
    if (length < sizeof(head)) {
        return 1;
    }
    /* The family is read through `struct sockaddr`'s own member and never as
     * a number at the record's first byte: on Linux `sa_family` is a
     * sixteen-bit field at offset zero, on the BSD family and Darwin it is one
     * byte at offset one behind `sa_len`, and only the host's own record type
     * knows which. The copy is because the record's storage is bytes. */
    memcpy(&head, native->bytes, sizeof(head));
    family = head.sa_family;
    if (family == AF_INET6 && length >= sizeof(struct sockaddr_in6)) {
        struct sockaddr_in6 host;
        unsigned index;
        memcpy(&host, native->bytes, sizeof(host));
        for (index = 0; index < 16u; index += 1u) {
            wf_socket_address_set_byte(
                address,
                index,
                host.sin6_addr.s6_addr[index]
            );
        }
        address->port_and_family =
            (unsigned)ntohs(host.sin6_port) | WF_SOCKET_FAMILY_V6;
        return 0;
    }
    if (family == AF_INET && length >= sizeof(struct sockaddr_in)) {
        struct sockaddr_in host;
        uint32_t packed;
        unsigned index;
        memcpy(&host, native->bytes, sizeof(host));
        packed = ntohl(host.sin_addr.s_addr);
        for (index = 0; index < 4u; index += 1u) {
            wf_socket_address_set_byte(
                address,
                index,
                (unsigned char)((packed >> (8u * (3u - index))) & 0xffu)
            );
        }
        address->port_and_family = (unsigned)ntohs(host.sin_port);
        return 0;
    }
    return 1;
}

/* The runtime's stop for a host record of no admitted family: one line on
 * standard error naming the condition, then `abort`, the same fail-stop shape
 * as the bridge's own (`bridge.c`, `wf_bridge_fail`), which the engines cannot
 * call because it is private to that unit.  The program is stopped by the
 * trusted base's failure and is handed no address ([SCOPE-3]). */
#define WF_SOCKET_ADDRESS_UNREPORTED_FAMILY \
    "whitefoot completion: the host reported a socket address of no " \
    "admitted family"

static inline _Noreturn void wf_socket_address_stop_unreported(void) {
    (void)fprintf(stderr, "%s\n", WF_SOCKET_ADDRESS_UNREPORTED_FAMILY);
    (void)fflush(stderr);
    abort();
}

/* The conversion every engine makes: the address the host reported, or the
 * stop above.  A caller never holds an address the host did not report. */
static inline void wf_socket_address_published(
    const wf_socket_native_address *native,
    unsigned length,
    wf_socket_address *address
) {
    if (wf_socket_address_from_native(native, length, address) != 0) {
        wf_socket_address_stop_unreported();
    }
}

/* Rewrites one completed accept's peer record, in place, as the portable form
 * the accept join publishes.  Every engine calls it on the completion path,
 * so the join reads one form whichever of them ran the accept. */
static inline void wf_socket_publish_peer(wf_file_request *request) {
    wf_socket_native_address native = request->operation.accept.peer.native;
    wf_socket_address_published(
        &native,
        request->operation.accept.peer_length,
        &request->operation.accept.peer.portable
    );
}

#if defined(__cplusplus)
}
#endif

#endif
