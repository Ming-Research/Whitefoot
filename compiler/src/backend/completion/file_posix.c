#if !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif
/* F_NOCACHE, the Darwin half of the WF_IO_NOCACHE target policy, is a BSD
 * extension the POSIX macro above hides. */
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE 1
#endif

/*
 * The POSIX host leaf of the file adapter: one host call per request kind, and
 * nothing else.
 *
 * `file_adapter.c` owns the queue, the helpers, the claim and the progress
 * pass, and is one implementation for every platform.  This unit is the one
 * thing that cannot be: the switch below is exactly the set of host calls a
 * POSIX target makes, and `file_windows.c` is its twin.  Nothing here knows
 * that a queue exists.
 */

#include "file_adapter.h"
#include "file_posix.h"

#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <netinet/tcp.h>
#include <poll.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#if !defined(WF_COMPLETION_SHUTDOWN)
#define WF_COMPLETION_SHUTDOWN shutdown
#else
extern int WF_COMPLETION_SHUTDOWN(int, int);
#endif

#if !defined(WF_COMPLETION_POLL)
#define WF_COMPLETION_POLL poll
#else
extern int WF_COMPLETION_POLL(struct pollfd *, nfds_t, int);
#endif

#if !defined(WF_COMPLETION_PREAD)
#define WF_COMPLETION_PREAD pread
#else
extern ssize_t WF_COMPLETION_PREAD(int, void *, size_t, off_t);
#endif

/* The one host call an adapter open makes, named so a build may observe it.
 * The observing build is expected to perform the same host call; nothing else
 * about the open changes, and the default build calls `openat` directly. */
#if !defined(WF_FILE_OPENAT)
#define WF_FILE_OPENAT openat
#else
extern int WF_FILE_OPENAT(int, const char *, int, ...);
#endif

#if defined(__APPLE__)
/* libSystem exports the exact facility used by the qualified Darwin target;
 * it is intentionally not replaced with an opendir/readdir loop. */
#if !defined(WF_COMPLETION_GETDIRENTRIES64)
#define WF_COMPLETION_GETDIRENTRIES64 __getdirentries64
#endif
extern ssize_t WF_COMPLETION_GETDIRENTRIES64(
    int,
    void *,
    size_t,
    int64_t *
);
#elif defined(__linux__)
/* glibc exports the batch facility used by the Linux native library. It is
 * declared here rather than reached through <dirent.h> for the same reason
 * the Darwin entry above is: the declaration is behind _GNU_SOURCE, which
 * this unit does not ask for, and the prototype is fixed by the ABI.  It is
 * used directly so one engine request returns one bounded native batch. */
#if !defined(WF_COMPLETION_GETDENTS64)
#define WF_COMPLETION_GETDENTS64 getdents64
#endif
extern ssize_t WF_COMPLETION_GETDENTS64(int, void *, size_t);
/* `accept4` is declared here rather than reached through <sys/socket.h> for
 * exactly the reason the enumeration facility above is: the declaration is
 * behind _GNU_SOURCE, which this unit does not ask for, and the prototype is
 * fixed by the ABI.  It is the one call that takes a connection and marks it
 * closed on exec in the same step; a family without it does the two steps
 * separately below. */
extern int accept4(int, struct sockaddr *, socklen_t *, int);
#define WF_SOCKET_HAS_ACCEPT4 1
#endif

/* One endpoint operation's two preparations: the host's own address record,
 * and one socket of that address's family.
 *
 * Both endpoint kinds do exactly these two things before their own host call,
 * and both dispose of the socket on any refusal. The ordinary linked caller
 * restores the factory's reserved credit on failure. Returns -1
 * with `errno` set when the host refused the socket. */
static int wf_socket_endpoint(
    const wf_file_request *request,
    wf_socket_native_address *native,
    unsigned *length
) {
    *length = wf_socket_native_from_address(
        &request->operation.endpoint.address.portable,
        native
    );
    return wf_socket_open(&request->operation.endpoint.address.portable);
}

/* TCP_NODELAY on every socket this leaf creates or accepts, right after the
 * host hands it over.  Nagle's coalescing measured an 11.78x-14.98x
 * throughput loss and a p99 tail of 41 ms against 2.4 ms without it
 * (research/investigations/io-model/SCHEDULER-FINDINGS.md, experiment 17).
 * A refusal leaves an ordinary, working socket -- just one that may coalesce
 * small writes -- so it is never folded into the operation's own outcome. */
static void wf_socket_disable_nagle(int descriptor) {
    int one = 1;
    (void)setsockopt(descriptor, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
}

static wf_file_result wf_file_execute_once(wf_file_request *request) {
    wf_file_result result;
    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;

    switch (request->kind) {
    case WF_FILE_READ:
        if (request->operation.read.count > (size_t)SSIZE_MAX) {
            result.head.error_code = EINVAL;
            return result;
        }
        break;
    case WF_FILE_WRITE:
        if (request->operation.write.count > (size_t)SSIZE_MAX) {
            result.head.error_code = EINVAL;
            return result;
        }
        break;
    case WF_FILE_PREAD:
        /* An empty transfer has no external action.  This must be decided
         * before descriptor and offset validation reaches the host: callers
         * rely on zero length completing without touching
         * either the destination or the file facility. */
        if (request->operation.pread.count == 0) {
            result.head.value = 0;
            return result;
        }
        if (request->operation.pread.count > (size_t)SSIZE_MAX
            || (int64_t)(off_t)request->operation.pread.offset
                != request->operation.pread.offset) {
            result.head.error_code = EINVAL;
            return result;
        }
        break;
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
    case WF_FILE_DIRECTORY_NEXT:
        if (request->operation.directory_next.count > (size_t)SSIZE_MAX) {
            result.head.error_code = EINVAL;
            return result;
        }
        break;
#endif
    case WF_FILE_SOCKET_RECEIVE:
        if (request->operation.receive.count > (size_t)SSIZE_MAX) {
            result.head.error_code = EINVAL;
            return result;
        }
        break;
    case WF_FILE_SOCKET_SEND:
        if (request->operation.send.count > (size_t)SSIZE_MAX) {
            result.head.error_code = EINVAL;
            return result;
        }
        break;
    default:
        break;
    }

    /* Each pass is one qualified host attempt. No-progress interruption and
     * readiness refusal are absorbed by wf_file_execute_direct below and
     * never become writer-visible outcomes. */
    switch (request->kind) {
    case WF_FILE_OPEN_AT:
        if (request->operation.open_at.expected_kind
            > WF_FILE_EXPECT_DIRECTORY) {
            result.head.error_code = EINVAL;
            result.head.open_outcome = WF_FILE_OPEN_FAILED;
            return result;
        }
        if (request->operation.open_at.has_mode != 0) {
            result.head.value = WF_FILE_OPENAT(
                request->operation.open_at.directory,
                request->operation.open_at.path,
                request->operation.open_at.flags
                    | wf_file_open_kind_flags(
                          request->operation.open_at.expected_kind
                      ),
                (mode_t)request->operation.open_at.mode
            );
        } else {
            result.head.value = WF_FILE_OPENAT(
                request->operation.open_at.directory,
                request->operation.open_at.path,
                request->operation.open_at.flags
                    | wf_file_open_kind_flags(
                          request->operation.open_at.expected_kind
                      )
            );
        }
        if (result.head.value < 0) {
            result.head.open_outcome = WF_FILE_OPEN_FAILED;
            break;
        }
        if (request->operation.open_at.expected_kind != WF_FILE_EXPECT_ANY) {
            struct stat status;
            int descriptor = (int)result.head.value;
            if (fstat(descriptor, &status) != 0) {
                int status_error = errno;
                (void)close(descriptor);
                result.head.error_code = status_error;
                result.head.open_outcome = WF_FILE_OPEN_STATUS_FAILED;
                return result;
            }
            result.head.open_outcome = wf_file_kind_outcome(
                request->operation.open_at.expected_kind,
                (unsigned int)status.st_mode
            );
            if (result.head.open_outcome != WF_FILE_OPEN_SUCCEEDED) {
                (void)close(descriptor);
                return result;
            }
        }
        /* The descriptor is the one this open hands back: the kind check has
         * either passed or was not asked for. WF_IO_NOCACHE is applied here,
         * once, so a refused descriptor never receives it. */
        wf_file_apply_uncached_reads((int)result.head.value);
        break;
    case WF_FILE_READ:
        result.head.value = read(
            request->operation.read.descriptor,
            request->operation.read.buffer,
            request->operation.read.count
        );
        break;
    case WF_FILE_WRITE:
        result.head.value = write(
            request->operation.write.descriptor,
            request->operation.write.buffer,
            request->operation.write.count
        );
        break;
    case WF_FILE_PREAD:
        result.head.value = WF_COMPLETION_PREAD(
            request->operation.pread.descriptor,
            request->operation.pread.buffer,
            request->operation.pread.count,
            (off_t)request->operation.pread.offset
        );
        break;
    case WF_FILE_CLOSE:
        result.head.value = close(request->operation.close.descriptor);
        break;
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
    case WF_FILE_DIRECTORY_NEXT:
#if defined(__APPLE__)
        result.head.value = WF_COMPLETION_GETDIRENTRIES64(
            request->operation.directory_next.descriptor,
            request->operation.directory_next.buffer,
            request->operation.directory_next.count,
            request->operation.directory_next.position
        );
#else
        /* Linux keeps the whole enumeration cursor in the descriptor, so the
         * facility takes no base-position argument and this cell is left as
         * the caller gave it. */
        result.head.value = WF_COMPLETION_GETDENTS64(
            request->operation.directory_next.descriptor,
            request->operation.directory_next.buffer,
            request->operation.directory_next.count
        );
#endif
        break;
#endif
    /* The six private socket request kinds perform their named host calls.
     * The ordinary linked caller maintains the factory's descriptor credit. */
    case WF_FILE_SOCKET_LISTEN: {
        wf_socket_native_address native;
        unsigned length = 0;
        int endpoint = wf_socket_endpoint(request, &native, &length);
        int refusal;
        if (endpoint < 0) {
            break;
        }
        wf_socket_disable_nagle(endpoint);
        if (bind(endpoint, (const struct sockaddr *)native.bytes,
                 (socklen_t)length)
                == 0
            && listen(endpoint, WF_SOCKET_BACKLOG) == 0) {
            result.head.value = endpoint;
            break;
        }
        /* One close attempt on a socket the program never received, its
         * diagnostic discarded, and the host's own refusal reported. */
        refusal = errno;
        (void)close(endpoint);
        errno = refusal;
        break;
    }
    case WF_FILE_SOCKET_ACCEPT: {
        socklen_t length =
            (socklen_t)sizeof(request->operation.accept.peer.native);
        struct sockaddr *peer =
            (struct sockaddr *)request->operation.accept.peer.native.bytes;
#if defined(WF_SOCKET_HAS_ACCEPT4)
        result.head.value = accept4(
            request->operation.accept.descriptor,
            peer,
            &length,
            SOCK_CLOEXEC
        );
#else
        result.head.value =
            accept(request->operation.accept.descriptor, peer, &length);
        if (result.head.value >= 0) {
            (void)fcntl((int)result.head.value, F_SETFD, FD_CLOEXEC);
        }
#endif
        if (result.head.value >= 0) {
            wf_socket_disable_nagle((int)result.head.value);
            request->operation.accept.peer_length = (unsigned)length;
            wf_socket_publish_peer(request);
        }
        break;
    }
    case WF_FILE_SOCKET_CONNECT: {
        wf_socket_native_address native;
        unsigned length = 0;
        int endpoint = wf_socket_endpoint(request, &native, &length);
        int refusal;
        if (endpoint < 0) {
            break;
        }
        wf_socket_disable_nagle(endpoint);
        if (connect(endpoint, (const struct sockaddr *)native.bytes,
                    (socklen_t)length)
            == 0) {
            result.head.value = endpoint;
            break;
        }
        refusal = errno;
        (void)close(endpoint);
        errno = refusal;
        break;
    }
    case WF_FILE_SOCKET_RECEIVE:
        result.head.value = recv(
            request->operation.receive.descriptor,
            request->operation.receive.buffer,
            request->operation.receive.count,
            0
        );
        break;
    case WF_FILE_SOCKET_SEND:
        result.head.value = send(
            request->operation.send.descriptor,
            request->operation.send.buffer,
            request->operation.send.count,
            WF_SOCKET_SEND_FLAGS
        );
        break;
    case WF_FILE_SOCKET_SHUTDOWN: {
        int descriptor = request->operation.shutdown.descriptor;
        int direction =
            request->operation.shutdown.direction == WF_SOCKET_DIRECTION_SEND
            ? SHUT_WR
            : SHUT_RD;
        /* A published release means this direction has finished using the
         * descriptor. Otherwise the other direction could close it before
         * this shutdown and the host could reuse its number. */
        (void)WF_COMPLETION_SHUTDOWN(descriptor, direction);
        int last = wf_file_connection_release(descriptor);
        if (last && close(descriptor) < 0) result.head.error_code = errno;
        // A linked close returns the released descriptor credit to its passed
        // factory. Preserve the last-half fact in this private native result.
        result.head.value = last ? 1 : 0;
        break;
    }
    default:
        result.head.error_code = EINVAL;
        return result;
    }
    if (result.head.value < 0) {
        result.head.error_code = errno;
    }
    return result;
}

static int wf_file_wait_ready(const wf_file_request *request) {
    struct pollfd descriptor;
    int waited;
    memset(&descriptor, 0, sizeof(descriptor));
    switch (request->kind) {
    case WF_FILE_READ:
        descriptor.fd = request->operation.read.descriptor;
        descriptor.events = POLLIN;
        break;
    case WF_FILE_PREAD:
        descriptor.fd = request->operation.pread.descriptor;
        descriptor.events = POLLIN;
        break;
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
    case WF_FILE_DIRECTORY_NEXT:
        descriptor.fd = request->operation.directory_next.descriptor;
        descriptor.events = POLLIN;
        break;
#endif
    case WF_FILE_SOCKET_RECEIVE:
        descriptor.fd = request->operation.receive.descriptor;
        descriptor.events = POLLIN;
        break;
    /* A listener becomes readable when a connection is waiting, which is the
     * same readiness a receive waits for. */
    case WF_FILE_SOCKET_ACCEPT:
        descriptor.fd = request->operation.accept.descriptor;
        descriptor.events = POLLIN;
        break;
    case WF_FILE_WRITE:
        descriptor.fd = request->operation.write.descriptor;
        descriptor.events = POLLOUT;
        break;
    case WF_FILE_SOCKET_SEND:
        descriptor.fd = request->operation.send.descriptor;
        descriptor.events = POLLOUT;
        break;
    default:
        return EINVAL;
    }
    do {
        waited = WF_COMPLETION_POLL(&descriptor, 1, -1);
    } while (waited < 0 && errno == EINTR);
    return waited > 0 ? 0 : errno;
}

wf_file_result wf_file_execute_direct(wf_file_request *request) {
    wf_file_result result;
    if (!wf_file_request_valid(request)) {
        memset(&result, 0, sizeof(result));
        result.head.kind = request == NULL ? 0 : request->kind;
        result.head.value = -1;
        result.head.error_code = EINVAL;
        return result;
    }
    for (;;) {
        int readiness_error;
        result = wf_file_execute_once(request);
        if (result.head.value >= 0 || result.head.error_code == 0) {
            return result;
        }
        switch (request->kind) {
        case WF_FILE_READ:
        case WF_FILE_WRITE:
        case WF_FILE_PREAD:
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
        case WF_FILE_DIRECTORY_NEXT:
#endif
        /* A socket transfer absorbs interruption and readiness refusal the
         * same way a file transfer does, and so does an accept: retrying it
         * takes whichever connection is waiting and has no effect of its own.
         * A connect does not, because an interrupted connect continues in the
         * host and a second attempt would be answered `EALREADY` rather than
         * the outcome the first one is about to produce; a listen does not,
         * because retrying it would create a second socket. */
        case WF_FILE_SOCKET_RECEIVE:
        case WF_FILE_SOCKET_SEND:
        case WF_FILE_SOCKET_ACCEPT:
            break;
        default:
            return result;
        }
        if (result.head.error_code == EINTR) {
            continue;
        }
        if (result.head.error_code != EAGAIN
#if defined(EWOULDBLOCK) && EWOULDBLOCK != EAGAIN
            && result.head.error_code != EWOULDBLOCK
#endif
        ) {
            return result;
        }
        readiness_error = wf_file_wait_ready(request);
        if (readiness_error != 0) {
            result.head.error_code = readiness_error;
            return result;
        }
    }
}


/* The monotonic clock, which is a host call and therefore this leaf's.
 *
 * Two readers: the helper policy's measurement of what one host call cost, and
 * the bridge's bounded look at its own record before it announces sleep. Both
 * are shared code, so the clock is declared in `file_adapter.h` beside the
 * execution leaf and answered here.
 *
 * WF_FILE_MONOTONIC_NS is a named seam of the same class as WF_COMPLETION_PREAD
 * and WF_COMPLETION_POLL: a build may answer it with a scripted clock so that
 * the growth rule can be tested for what it decides rather than for how fast
 * the machine running the test happened to be.  The shipped build reads the
 * host monotonic clock and nothing else. */
#if !defined(WF_FILE_MONOTONIC_NS)
static uint64_t wf_file_monotonic_ns_host(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
        return 0;
    }
    return (uint64_t)now.tv_sec * 1000000000u + (uint64_t)now.tv_nsec;
}
#define WF_FILE_MONOTONIC_NS wf_file_monotonic_ns_host
#else
extern uint64_t WF_FILE_MONOTONIC_NS(void);
#endif

uint64_t wf_file_monotonic_ns(void) {
    return WF_FILE_MONOTONIC_NS();
}

/* One transfer attempted without waiting: the host is asked once with
 * MSG_DONTWAIT, and the answer is either the operation's own outcome -- bytes
 * moved, the peer's end, or a definite refusal -- or the one answer that says
 * the host would have to wait, EAGAIN, which leaves the record for an engine
 * that can wait on it.  A send on a loopback whose window has room and a
 * receive whose bytes have already arrived complete here, with no ring, no
 * park and no wake, which is the same rule the bounded adapter applies to a
 * positioned read the submitting thread would run itself. */
int wf_file_readiness_supported(void) {
    return 1;
}

/* One `poll` over the descriptors the parked contexts name.  Every driver
 * thread may call it for its own contexts at once, so the host array is the
 * caller's: 32 KiB of a driver's stack, which is the floor's reservation. */
int wf_file_wait_readiness(wf_file_readiness *entries, size_t count, int timeout_ms) {
    struct pollfd polled[WF_FILE_READINESS_BATCH];
    int answered;
    size_t index;
    if (entries == NULL || count == 0 || count > WF_FILE_READINESS_BATCH) {
        return -1;
    }
    for (index = 0; index < count; index++) {
        polled[index].fd = entries[index].descriptor;
        polled[index].events = (short)(
            ((entries[index].events & WF_FILE_READABLE) != 0 ? POLLIN : 0)
            | ((entries[index].events & WF_FILE_WRITABLE) != 0 ? POLLOUT : 0)
        );
        polled[index].revents = 0;
        entries[index].ready = 0;
    }
    do {
        answered = poll(polled, (nfds_t)count, timeout_ms);
    } while (answered < 0 && errno == EINTR);
    if (answered < 0) {
        return -1;
    }
    for (index = 0; index < count; index++) {
        short seen = polled[index].revents;
        if ((seen & (POLLERR | POLLHUP | POLLNVAL)) != 0) {
            entries[index].ready = WF_FILE_READABLE | WF_FILE_WRITABLE;
            continue;
        }
        if ((seen & POLLIN) != 0) {
            entries[index].ready |= WF_FILE_READABLE;
        }
        if ((seen & POLLOUT) != 0) {
            entries[index].ready |= WF_FILE_WRITABLE;
        }
    }
    return answered;
}

int wf_file_transfer_now(const wf_file_request *request, wf_file_result *result) {
    ssize_t moved;
    memset(result, 0, sizeof(*result));
    result->head.kind = request->kind;
    result->head.value = -1;
    switch (request->kind) {
    case WF_FILE_SOCKET_SEND:
        if (request->operation.send.count > (size_t)SSIZE_MAX) {
            return 0;
        }
        moved = send(
            request->operation.send.descriptor,
            request->operation.send.buffer,
            request->operation.send.count,
            WF_SOCKET_SEND_FLAGS | MSG_DONTWAIT
        );
        break;
    case WF_FILE_SOCKET_RECEIVE:
        if (request->operation.receive.count > (size_t)SSIZE_MAX) {
            return 0;
        }
        moved = recv(
            request->operation.receive.descriptor,
            request->operation.receive.buffer,
            request->operation.receive.count,
            MSG_DONTWAIT
        );
        break;
    default:
        return 0;
    }
    if (moved >= 0) {
        result->head.value = moved;
        return 1;
    }
    if (errno == EAGAIN || errno == EINTR
#if defined(EWOULDBLOCK) && EWOULDBLOCK != EAGAIN
        || errno == EWOULDBLOCK
#endif
    ) {
        return 0;
    }
    result->head.error_code = errno;
    return 1;
}
