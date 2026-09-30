#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0602
#endif

/* Winsock ahead of `<windows.h>`, which `windows_iocp.h` includes: the address
 * vocabulary this ring's socket requests are written in is the shared one, and
 * it is the header that names the platform's socket declarations. */
#include "socket_address.h"

#include "windows_iocp.h"

#if defined(_WIN32)

#include "../windows_runtime.h"

#include <mswsock.h>

#include <errno.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(WF_WINDOWS_IOCP_READ_CALL)
extern BOOL WINAPI WF_WINDOWS_IOCP_READ_CALL(
    HANDLE handle,
    LPVOID buffer,
    DWORD count,
    LPDWORD transferred,
    LPOVERLAPPED overlapped
);
#else
#define WF_WINDOWS_IOCP_READ_CALL ReadFile
#endif

#if defined(WF_WINDOWS_IOCP_RESULT_CALL)
extern BOOL WINAPI WF_WINDOWS_IOCP_RESULT_CALL(
    HANDLE handle,
    LPOVERLAPPED overlapped,
    LPDWORD transferred,
    BOOL wait
);
#else
#define WF_WINDOWS_IOCP_RESULT_CALL GetOverlappedResult
#endif

#if defined(WF_WINDOWS_IOCP_NOTIFICATION_CALL)
extern BOOL WINAPI WF_WINDOWS_IOCP_NOTIFICATION_CALL(
    HANDLE handle,
    UCHAR flags
);
#else
#define WF_WINDOWS_IOCP_NOTIFICATION_CALL SetFileCompletionNotificationModes
#endif

/* The ring's own state inside the record.
 *
 * The `OVERLAPPED` is first, so the completion packet's `OVERLAPPED` address
 * minus the record's ring offset is the record, and there is no side table and
 * no allocation between a submission and its completion.  The handle beside it
 * is what the synchronous-success path asks `GetOverlappedResult` about; the
 * record is the only place it can live, because there is no entry pool left to
 * keep it in (design section 7). */
typedef struct wf_windows_iocp_state {
    OVERLAPPED overlapped;
    HANDLE handle;
} wf_windows_iocp_state;

_Static_assert(
    offsetof(wf_windows_iocp_state, overlapped) == 0,
    "a completion packet's OVERLAPPED must identify its record by subtraction"
);
_Static_assert(
    sizeof(wf_windows_iocp_state) <= sizeof(wf_completion_ring_state),
    "the IOCP ring state must fit the record's ring block"
);
_Static_assert(
    _Alignof(wf_windows_iocp_state) <= _Alignof(wf_completion_ring_state),
    "the IOCP ring state must not out-align the record's ring block"
);

/* This leaf's one fail-stop.
 *
 * Each `abort` below is a trusted-computing-base defect, not an operation
 * outcome, and a fail-stop that writes nothing cannot be diagnosed from a
 * crash log: the release UCRT ends such a process through the fast-fail path,
 * which a shell reports as a bare status with no message at all. This writes
 * one line and then aborts exactly where the bare call did, and changes no
 * control flow. */
static _Noreturn void wf_windows_iocp_fail(const char *reason) {
    (void)fprintf(stderr, "whitefoot completion port: %s\n", reason);
    (void)fflush(stderr);
    abort();
}

static wf_windows_iocp_state *wf_windows_state_of(
    wf_completion_record *record
) {
    return (wf_windows_iocp_state *)(void *)&record->ring;
}

void wf_windows_iocp_cancel(wf_completion_record *record) {
    wf_windows_iocp_state *state = wf_windows_state_of(record);
    /* A refusal means the operation has already completed, and its packet
     * reports that. */
    (void)CancelIoEx(state->handle, &state->overlapped);
}

static wf_completion_record *wf_windows_record_of(OVERLAPPED *overlapped) {
    unsigned char *bytes = (unsigned char *)(void *)overlapped;
    return (wf_completion_record *)(void *)(
        bytes - offsetof(wf_completion_record, ring)
    );
}

static void wf_windows_record_progress_error(
    wf_windows_iocp_adapter *adapter,
    int error
) {
    int expected = 0;
    (void)atomic_compare_exchange_strong_explicit(
        &adapter->progress_error,
        &expected,
        error,
        memory_order_acq_rel,
        memory_order_acquire
    );
}

int wf_windows_iocp_progress_error(const wf_windows_iocp_adapter *adapter) {
    if (adapter == NULL) {
        return EINVAL;
    }
    return atomic_load_explicit(&adapter->progress_error, memory_order_acquire);
}

int wf_windows_iocp_init(
    wf_windows_iocp_adapter *adapter,
    wf_completion_runtime *runtime,
    DWORD concurrency
) {
    HANDLE port;
    if (adapter == NULL || runtime == NULL) {
        return EINVAL;
    }
    memset(adapter, 0, sizeof(*adapter));
    port = CreateIoCompletionPort(INVALID_HANDLE_VALUE, NULL, 0, concurrency);
    if (port == NULL) {
        return (int)GetLastError();
    }
    adapter->runtime = runtime;
    adapter->port = port;
    /* Two keys, and they must differ: a packet with no `OVERLAPPED` is a wake
     * and a packet with one is a completion, and the key is what says which
     * queue a packet came from when a protocol failure has to be named. */
    adapter->wake_key = (ULONG_PTR)(void *)runtime;
    adapter->file_key = (ULONG_PTR)(void *)adapter;
    if (adapter->wake_key == adapter->file_key) {
        (void)CloseHandle(port);
        return EINVAL;
    }
    atomic_init(&adapter->in_flight, 0);
    atomic_init(&adapter->progress_error, 0);
    atomic_init(&adapter->stat_submissions, 0);
    atomic_init(&adapter->stat_inline_completions, 0);
    atomic_init(&adapter->stat_dequeued_completions, 0);
    atomic_init(&adapter->stat_completions, 0);
    atomic_init(&adapter->stat_kernel_waits, 0);
    atomic_init(&adapter->stat_host_wake_posts, 0);
    adapter->initialized = 1;
    return 0;
}

int wf_windows_iocp_associate(
    wf_windows_iocp_adapter *adapter,
    HANDLE handle
) {
    HANDLE associated;
    if (adapter == NULL || adapter->initialized == 0 || handle == NULL
        || handle == INVALID_HANDLE_VALUE) {
        return EINVAL;
    }
    /* A request the kernel answers synchronously is already complete, so the
     * submitting thread publishes it and the port is spared a round trip that
     * carries no news.  A pending request still has exactly one packet.
     *
     * This is asked *before* the port takes the handle, and the order is the
     * whole of this function's failure story.  A handle cannot be taken off a
     * completion port: there is no call for it, and the association lasts as
     * long as the handle does.  Asking second would mean that a host which
     * refuses these modes -- a file system that does not carry them is the
     * documented case -- leaves the handle bound to this port with the skip it
     * needs unset, and every later transfer on it belongs to no engine: the
     * ring is refused for it here, so it goes to the bounded adapter, whose
     * overlapped read would then post its completion to this port instead of
     * signalling its own event, wedging that read and handing the reaper an
     * `OVERLAPPED` that is not a record.  Asking first leaves a refused handle
     * exactly as it was found, which is what makes `file_windows.c`'s
     * "the handle it reads is one the port never took" true rather than
     * hopeful. */
    if (WF_WINDOWS_IOCP_NOTIFICATION_CALL(
            handle,
            (UCHAR)(FILE_SKIP_COMPLETION_PORT_ON_SUCCESS
                | FILE_SKIP_SET_EVENT_ON_HANDLE)
        ) == FALSE) {
        return (int)GetLastError();
    }
    associated = CreateIoCompletionPort(
        handle,
        adapter->port,
        adapter->file_key,
        0
    );
    if (associated != adapter->port) {
        return (int)GetLastError();
    }
    return 0;
}

/* The extension entries this ring issues, obtained once per process.
 *
 * `ConnectEx` is not exported by any import library: it is asked of the
 * provider behind the socket with `WSAIoctl(SIO_GET_EXTENSION_FUNCTION_POINTER)`,
 * and the answer is the same for every socket of the same provider, so it is
 * asked once.  The once is an `INIT_ONCE`, the path this platform already uses
 * for a fact the whole process shares (`../windows_runtime.h`,
 * `wf__windows_socket_startup`; `../wf_floor_windows.c`, the exception
 * handler's install).
 *
 * `AcceptEx` is deliberately absent, and the reason is a measurement rather
 * than a preference: it writes the local and the remote address into a caller
 * buffer that must live until the operation completes, and its two length
 * arguments must each be sixteen bytes past the largest address of the
 * transport, which for the two families (ordinary native library) admits is
 * `2 * (sizeof(struct sockaddr_in6) + 16)` = 88 bytes.  The completion record
 * is 160 bytes on this platform and holds exactly that today: the accept's own
 * union arm is 40 bytes, which is the open's arm and therefore the ceiling
 * `contract.h` asserts, and the ring-state block is 40 bytes of which this
 * ring's `OVERLAPPED` and handle already occupy 40.  There is nowhere for 88
 * bytes to go without growing the record, the record may not grow, and the
 * runtime allocates nothing at run time.  So an accept is the shared file
 * adapter's blocking `accept` on a helper thread, exactly as a listen, a bind
 * and a half-close are on every platform, and this ring answers no for it. */
typedef struct wf_windows_iocp_extensions {
    LPFN_CONNECTEX connect_ex;
} wf_windows_iocp_extensions;

static INIT_ONCE wf_windows_extension_once = INIT_ONCE_STATIC_INIT;
static wf_windows_iocp_extensions wf_windows_extensions;

static BOOL CALLBACK wf_windows_load_extensions(
    PINIT_ONCE once,
    PVOID parameter,
    PVOID *context
) {
    GUID connect_id = WSAID_CONNECTEX;
    SOCKET probe;
    DWORD answered = 0;
    (void)once;
    (void)parameter;
    (void)context;
    /* One socket of the family every provider carries, used to ask the
     * question and closed again.  It is not the socket any request is issued
     * on: the answer names the provider's entry, not this handle. */
    probe = WSASocketW(AF_INET, SOCK_STREAM, IPPROTO_TCP, NULL, 0, 0);
    if (probe == INVALID_SOCKET) {
        return TRUE;
    }
    if (WSAIoctl(
            probe,
            SIO_GET_EXTENSION_FUNCTION_POINTER,
            &connect_id,
            (DWORD)sizeof(connect_id),
            &wf_windows_extensions.connect_ex,
            (DWORD)sizeof(wf_windows_extensions.connect_ex),
            &answered,
            NULL,
            NULL
        ) != 0) {
        wf_windows_extensions.connect_ex = NULL;
    }
    (void)closesocket(probe);
    return TRUE;
}

static LPFN_CONNECTEX wf_windows_connect_ex(void) {
    if (wf__windows_socket_startup() != 0) {
        return NULL;
    }
    if (InitOnceExecuteOnce(
            &wf_windows_extension_once,
            wf_windows_load_extensions,
            NULL,
            NULL
        ) == FALSE) {
        return NULL;
    }
    return wf_windows_extensions.connect_ex;
}

/* The kinds this ring carries.
 *
 * A positioned read, and the three TCP kinds whose host call has an overlapped
 * form worth a packet: the connect, the receive and the send (ordinary native library).
 * What is missing is not a gap: `write_once` is an unpositioned stream write
 * whose current-position contract the port's positioned write does not have,
 * an open is a namespace operation with no overlapped form here, a close and a
 * status are in-memory answers, a listen and a half-close are immediate calls
 * with nothing to wait for, and an accept is the record-size measurement above.
 * Every one of those goes to the shared file adapter, exactly as the POSIX
 * bridge routes what its ring refuses. */
int wf_windows_iocp_carries(const wf_completion_record *record) {
    if (record == NULL) {
        return 0;
    }
    switch (record->request.kind) {
    case WF_FILE_PREAD:
        return record->request.operation.pread.descriptor >= 0
            && record->request.operation.pread.count != 0
            && record->request.operation.pread.count <= (size_t)MAXDWORD
            && record->request.operation.pread.offset >= 0
            && record->request.operation.pread.buffer != NULL;
    /* A connect names no descriptor at submit, because it creates its own
     * socket; this ring makes it in the submitting call for the same reason
     * the Linux ring does (`linux_io_uring.c`, `wf_linux_io_uring_submit`),
     * and on this platform for one more: the port has to take the handle
     * before the request is issued on it. */
    case WF_FILE_SOCKET_CONNECT:
        return wf_windows_connect_ex() != NULL;
    case WF_FILE_SOCKET_RECEIVE:
        return record->request.operation.receive.descriptor >= 0
            && record->request.operation.receive.count != 0
            && record->request.operation.receive.count <= (size_t)MAXDWORD
            && record->request.operation.receive.buffer != NULL;
    case WF_FILE_SOCKET_SEND:
        return record->request.operation.send.descriptor >= 0
            && record->request.operation.send.count != 0
            && record->request.operation.send.count <= (size_t)MAXDWORD
            && record->request.operation.send.buffer != NULL;
    default:
        return 0;
    }
}

/* One connect's socket, created and bound in the submitting call.
 *
 * `ConnectEx` requires a socket that is already bound, so this binds it to the
 * wildcard address of its own family -- which is what an ordinary `connect`
 * does implicitly and therefore adds no decision of the runtime's.  Nothing
 * here can wait: `WSASocketW` allocates a kernel object and `bind` of the
 * wildcard picks an ephemeral port, so a second port round trip would cost
 * more than the operation it wraps.
 *
 * The address record is not built here: the arm holds the portable value and
 * the native one in one union, and this call may still be undone by
 * `wf_windows_iocp_withdraw` when the port refuses the handle, after which the
 * bounded adapter reads that portable value and makes the same two calls. */
static int wf_windows_open_connect_socket(wf_completion_record *record) {
    wf_socket_native_address wildcard;
    unsigned wildcard_length;
    int family = wf_socket_address_family(
        &record->request.operation.endpoint.address.portable
    );
    int descriptor = wf__windows_socket_open(family);
    int one = 1;
    if (descriptor < 0) {
        return -1;
    }
    /* TCP_NODELAY on every socket this ring creates, right after creation.
     * Nagle's coalescing measured an 11.78x-14.98x throughput loss and a p99
     * tail of 41 ms against 2.4 ms without it
     * (research/investigations/io-model/SCHEDULER-FINDINGS.md, experiment
     * 17). A refusal leaves an ordinary, working socket -- just one that may
     * coalesce small writes -- so it is not folded into this call's own
     * outcome. */
    (void)setsockopt(
        (SOCKET)wf__windows_socket_handle(descriptor),
        IPPROTO_TCP,
        TCP_NODELAY,
        (const char *)&one,
        sizeof one
    );
    wildcard_length = wf_socket_native_wildcard(family, &wildcard);
    if (bind(
            (SOCKET)wf__windows_socket_handle(descriptor),
            (const struct sockaddr *)wildcard.bytes,
            (int)wildcard_length
        ) != 0) {
        (void)wf__windows_socket_close(descriptor);
        return -1;
    }
    record->request.operation.endpoint.descriptor = descriptor;
    return descriptor;
}

int wf_windows_iocp_issue_descriptor(wf_completion_record *record) {
    if (record == NULL) {
        return -1;
    }
    switch (record->request.kind) {
    case WF_FILE_PREAD:
        return record->request.operation.pread.descriptor;
    case WF_FILE_SOCKET_RECEIVE:
        return record->request.operation.receive.descriptor;
    case WF_FILE_SOCKET_SEND:
        return record->request.operation.send.descriptor;
    case WF_FILE_SOCKET_CONNECT:
        return wf_windows_open_connect_socket(record);
    default:
        return -1;
    }
}

void wf_windows_iocp_withdraw(wf_completion_record *record) {
    if (record == NULL || record->request.kind != WF_FILE_SOCKET_CONNECT) {
        return;
    }
    if (record->request.operation.endpoint.descriptor >= 0) {
        (void)wf__windows_socket_close(
            record->request.operation.endpoint.descriptor
        );
        record->request.operation.endpoint.descriptor = -1;
    }
}

/* Stores one record's terminal result and completes it.
 *
 * Every route out of a ring-owned operation ends here, so an operation's
 * completion is counted in exactly one place, and the store of DONE inside
 * `wf_completion_record_complete` is this adapter's last touch of a record
 * that is a block of the joiner's frame. */
static void wf_windows_complete_record(
    wf_windows_iocp_adapter *adapter,
    wf_completion_record *record,
    DWORD transferred,
    DWORD error_code
) {
    wf_file_result_head result;
    int socket_kind = record->request.kind == WF_FILE_SOCKET_CONNECT
        || record->request.kind == WF_FILE_SOCKET_RECEIVE
        || record->request.kind == WF_FILE_SOCKET_SEND;
    /* A positioned read uses the Whitefoot/POSIX-shaped result contract:
     * reaching the file boundary is a successful zero-byte read.  Windows
     * reports that boundary as ERROR_HANDLE_EOF for overlapped disk I/O. */
    if (error_code == ERROR_HANDLE_EOF && !socket_kind) {
        error_code = 0;
        transferred = 0;
    }
    memset(&result, 0, sizeof(result));
    result.kind = record->request.kind;
    if (socket_kind && error_code != 0) {
        /* One code per condition, whichever route produced it: the port's own
         * Win32 spelling and the submitting call's `WSAGetLastError` are
         * normalized together (`../windows_runtime.h`,
         * `wf__windows_error_from_socket`). */
        error_code = (DWORD)wf__windows_error_from_socket((int)error_code);
    }
    if (record->request.kind == WF_FILE_SOCKET_CONNECT) {
        /* This port's connect answers the descriptor of the socket it created
         * at submit, and a refusal disposes of it. The ordinary linked caller
         * restores the factory's credit on failure. A connection that was made needs
         * SO_UPDATE_CONNECT_CONTEXT before it behaves like any other socket:
         * until it is set, the socket carries none of the properties the
         * connect established, which `shutdown` and every later transfer
         * read. */
        int descriptor = record->request.operation.endpoint.descriptor;
        if (error_code != 0) {
            (void)wf__windows_socket_close(descriptor);
            record->request.operation.endpoint.descriptor = -1;
            result.value = -1;
            result.error_code = (int)error_code;
        } else {
            (void)setsockopt(
                (SOCKET)wf__windows_socket_handle(descriptor),
                SOL_SOCKET,
                SO_UPDATE_CONNECT_CONTEXT,
                NULL,
                0
            );
            result.value = descriptor;
        }
    } else if (error_code != 0) {
        result.value = -1;
        result.error_code = (int)error_code;
    } else {
        result.value = (int64_t)transferred;
    }
    atomic_fetch_sub_explicit(&adapter->in_flight, 1, memory_order_relaxed);
    atomic_fetch_add_explicit(
        &adapter->stat_completions,
        1,
        memory_order_relaxed
    );
    record->result = result;
    wf_completion_record_complete(record);
}

/* Issues one record's request on `handle`, and answers whether the kernel
 * completed it inside this call.
 *
 * The four calls below are the whole of what differs between this ring's
 * kinds; everything around them -- the record's `OVERLAPPED`, the release that
 * orders the submitter's writes, the inline-completion rule, the one
 * publication -- is one text.  Each socket call is given the record's own
 * `OVERLAPPED` and a `WSABUF` of this frame: Winsock captures the descriptor
 * array before it returns and reads the bytes it points at until the operation
 * completes, and those bytes are the submitting frame's own and outlive the
 * join. */
static BOOL wf_windows_issue(
    wf_completion_record *record,
    HANDLE handle,
    wf_windows_iocp_state *state
) {
    switch (record->request.kind) {
    case WF_FILE_SOCKET_CONNECT: {
        LPFN_CONNECTEX connect_ex = wf_windows_connect_ex();
        if (connect_ex == NULL) {
            SetLastError(ERROR_NOT_SUPPORTED);
            return FALSE;
        }
        return connect_ex(
            (SOCKET)handle,
            (const struct sockaddr *)
                record->request.operation.endpoint.address.native.bytes,
            (int)record->request.operation.endpoint.address_length,
            NULL,
            0,
            NULL,
            &state->overlapped
        );
    }
    case WF_FILE_SOCKET_RECEIVE: {
        WSABUF buffer;
        DWORD flags = 0;
        buffer.len = (ULONG)record->request.operation.receive.count;
        buffer.buf = (char *)record->request.operation.receive.buffer;
        return WSARecv(
                   (SOCKET)handle,
                   &buffer,
                   1u,
                   NULL,
                   &flags,
                   &state->overlapped,
                   NULL
               ) == 0
            ? TRUE
            : FALSE;
    }
    case WF_FILE_SOCKET_SEND: {
        WSABUF buffer;
        buffer.len = (ULONG)record->request.operation.send.count;
        buffer.buf = (char *)record->request.operation.send.buffer;
        return WSASend(
                   (SOCKET)handle,
                   &buffer,
                   1u,
                   NULL,
                   (DWORD)WF_SOCKET_SEND_FLAGS,
                   &state->overlapped,
                   NULL
               ) == 0
            ? TRUE
            : FALSE;
    }
    case WF_FILE_PREAD:
    default:
        return WF_WINDOWS_IOCP_READ_CALL(
            handle,
            record->request.operation.pread.buffer,
            (DWORD)record->request.operation.pread.count,
            NULL,
            &state->overlapped
        );
    }
}

/* The byte count of one request the kernel answered inside the submitting
 * call.  A socket asks Winsock rather than the file API, because a socket's
 * overlapped result carries the receive flags beside the count and only
 * `WSAGetOverlappedResult` reports them. */
static BOOL wf_windows_issued_result(
    wf_completion_record *record,
    HANDLE handle,
    wf_windows_iocp_state *state,
    DWORD *transferred
) {
    if (record->request.kind == WF_FILE_SOCKET_CONNECT
        || record->request.kind == WF_FILE_SOCKET_RECEIVE
        || record->request.kind == WF_FILE_SOCKET_SEND) {
        DWORD flags = 0;
        return WSAGetOverlappedResult(
            (SOCKET)handle,
            &state->overlapped,
            transferred,
            FALSE,
            &flags
        );
    }
    return WF_WINDOWS_IOCP_RESULT_CALL(
        handle,
        &state->overlapped,
        transferred,
        FALSE
    );
}

enum wf_windows_iocp_submit_result wf_windows_iocp_submit(
    wf_windows_iocp_adapter *adapter,
    wf_completion_record *record,
    HANDLE handle
) {
    wf_windows_iocp_state *state;
    ULARGE_INTEGER offset;
    DWORD transferred = 0;
    DWORD error_code;
    BOOL started;

    if (adapter == NULL || adapter->initialized == 0) {
        return WF_WINDOWS_IOCP_UNAVAILABLE;
    }
    if (!wf_windows_iocp_carries(record) || handle == NULL
        || handle == INVALID_HANDLE_VALUE) {
        return WF_WINDOWS_IOCP_SUBMIT_INVALID;
    }

    record->route = WF_COMPLETION_ROUTE_WINDOWS_IOCP;
    record->opened_descriptor = -1;
    record->open_outcome = WF_FILE_OPEN_SUCCEEDED;
    record->open_error = 0;
    state = wf_windows_state_of(record);
    memset(state, 0, sizeof(*state));
    if (record->request.kind == WF_FILE_PREAD) {
        /* A positioned read names its offset in the `OVERLAPPED`; a socket
         * request has none to name and leaves the two words zero. */
        offset.QuadPart = (ULONGLONG)record->request.operation.pread.offset;
        state->overlapped.Offset = offset.LowPart;
        state->overlapped.OffsetHigh = offset.HighPart;
    } else if (record->request.kind == WF_FILE_SOCKET_CONNECT) {
        /* The portable value is read out of the union before the native
         * record is written back over it, because the two share the arm.
         * This is the point of no return for the socket
         * `wf_windows_iocp_issue_descriptor` created: from here the record is
         * the port's and the bounded adapter will not see it. */
        wf_socket_address portable =
            record->request.operation.endpoint.address.portable;
        record->request.operation.endpoint.address_length =
            wf_socket_native_from_address(
                &portable,
                &record->request.operation.endpoint.address.native
            );
    }
    state->handle = handle;

    atomic_fetch_add_explicit(&adapter->in_flight, 1, memory_order_relaxed);
    atomic_fetch_add_explicit(
        &adapter->stat_submissions,
        1,
        memory_order_relaxed
    );
    /* Every word of the record the reaper will read is written above and by
     * the submitting bridge before this call; this release is what orders
     * them before the reaper's acquire (`contract.h`, `issued`). */
    atomic_store_explicit(&record->issued, 1u, memory_order_release);
    started = wf_windows_issue(record, handle, state);
    if (started != FALSE) {
        /* The association asked for no packet on a synchronous success, so
         * this operation is already complete and nobody else will publish it.
         * Its exact byte count is queried without waiting. */
        if (wf_windows_issued_result(record, handle, state, &transferred)
            == FALSE) {
            error_code = GetLastError();
            /* The issuing call reported success, which guarantees the
             * operation is complete.  Windows reporting otherwise would leave
             * the record's OVERLAPPED and the caller's buffer live with no
             * packet to come, so this is an invariant failure rather than an
             * I/O result. */
            if (error_code == ERROR_IO_INCOMPLETE) {
                wf_windows_iocp_fail(
                    "an inline-completed transfer reported that it is still incomplete"
                );
            }
            if (error_code == ERROR_SUCCESS) {
                error_code = ERROR_GEN_FAILURE;
            }
            transferred = 0;
        } else {
            error_code = 0;
        }
        atomic_fetch_add_explicit(
            &adapter->stat_inline_completions,
            1,
            memory_order_relaxed
        );
        wf_windows_complete_record(adapter, record, transferred, error_code);
        return WF_WINDOWS_IOCP_TARGET_OWNS;
    }
    error_code = GetLastError();
    if (error_code == ERROR_IO_PENDING) {
        return WF_WINDOWS_IOCP_TARGET_OWNS;
    }
    /* No operation is pending and Windows promises no packet for a failed
     * initiation, so this thread owns the unique terminal publication. */
    wf_windows_complete_record(adapter, record, 0, error_code);
    return WF_WINDOWS_IOCP_TARGET_OWNS;
}

/* Publishes one dequeued completion packet.  Returns zero, or a protocol
 * failure for a packet naming a record that was never issued. */
static int wf_windows_publish_packet(
    wf_windows_iocp_adapter *adapter,
    OVERLAPPED *overlapped,
    DWORD transferred,
    DWORD error_code
) {
    wf_completion_record *record = wf_windows_record_of(overlapped);
    if (record == NULL
        || atomic_load_explicit(&record->issued, memory_order_acquire) == 0
        || record->route != WF_COMPLETION_ROUTE_WINDOWS_IOCP) {
        return EPROTO;
    }
    atomic_fetch_add_explicit(
        &adapter->stat_dequeued_completions,
        1,
        memory_order_relaxed
    );
    wf_windows_complete_record(adapter, record, transferred, error_code);
    return 0;
}

typedef struct wf_windows_iocp_waiter {
    struct wf_windows_iocp_waiter *next;
    unsigned notified;
} wf_windows_iocp_waiter;

/* Polling may take a packet intended for a notified native sleeper. Preserve
 * it until that cohort has left its kernel waits. A merely announced newer
 * sleeper does not require an old packet to remain in circulation. */
static void wf_windows_return_wake(wf_windows_iocp_adapter *adapter) {
    int error = 0;
    wf_completion_wait_lock(&adapter->runtime->wait);
    if (adapter->notified_waiters != 0u
        && PostQueuedCompletionStatus(
               adapter->port,
               0,
               adapter->wake_key,
               NULL
           ) == FALSE) {
        error = (int)GetLastError();
    }
    wf_completion_wait_unlock(&adapter->runtime->wait);
    if (error != 0) {
        wf_windows_record_progress_error(adapter, error);
    }
}

int wf_windows_iocp_progress(
    wf_windows_iocp_adapter *adapter,
    size_t budget,
    size_t *published
) {
    size_t total = 0;
    size_t processed = 0;
    int first_error;

    if (published != NULL) {
        *published = 0;
    }
    if (adapter == NULL || published == NULL || budget == 0) {
        return EINVAL;
    }
    if (adapter->initialized == 0) {
        return EINVAL;
    }
    first_error = wf_windows_iocp_progress_error(adapter);
    if (first_error != 0) {
        return first_error;
    }
    while (processed < budget) {
        DWORD transferred = 0;
        ULONG_PTR key = 0;
        OVERLAPPED *overlapped = NULL;
        BOOL succeeded = GetQueuedCompletionStatus(
            adapter->port,
            &transferred,
            &key,
            &overlapped,
            0
        );
        DWORD error_code = succeeded != FALSE ? 0u : GetLastError();
        int error;
        if (overlapped == NULL) {
            if (succeeded == FALSE && error_code == WAIT_TIMEOUT) {
                break;
            }
            if (succeeded == FALSE) {
                if (first_error == 0) {
                    first_error = (int)error_code;
                }
                break;
            }
            if (key != adapter->wake_key) {
                if (first_error == 0) {
                    first_error = EPROTO;
                }
                break;
            }
            wf_windows_return_wake(adapter);
            break;
        }
        if (key != adapter->file_key) {
            if (first_error == 0) {
                first_error = EPROTO;
            }
            break;
        }
        processed += 1;
        error = wf_windows_publish_packet(
            adapter,
            overlapped,
            transferred,
            error_code
        );
        if (error != 0) {
            if (first_error == 0) {
                first_error = error;
            }
            break;
        }
        total += 1;
    }
    if (first_error != 0) {
        wf_windows_record_progress_error(adapter, first_error);
    }
    *published = total;
    return first_error;
}

/* Called under the runtime wait lock. A packet is assigned to the existing
 * native cohort, although the port itself cannot address an individual worker.
 * New kernel waiters are gated until all notified calls have returned. */
void wf_windows_iocp_notify(void *context) {
    wf_windows_iocp_adapter *adapter = (wf_windows_iocp_adapter *)context;
    if (adapter == NULL || adapter->initialized == 0) {
        return;
    }
    for (wf_windows_iocp_waiter *waiter = adapter->waiters;
         waiter != NULL;
         waiter = waiter->next) {
        if (waiter->notified != 0u) {
            continue;
        }
        waiter->notified = 1u;
        adapter->notified_waiters++;
        if (PostQueuedCompletionStatus(
                adapter->port,
                0,
                adapter->wake_key,
                NULL
            ) == FALSE) {
            /* A port this process owns that will not take a packet leaves the
             * announced sleeper asleep forever, and no later wake can repair
             * it; fail-stop here rather than hang. */
            wf_windows_iocp_fail(
                "a port this process owns refused a wake packet, which would leave an announced sleeper asleep forever"
            );
        }
        atomic_fetch_add_explicit(
            &adapter->stat_host_wake_posts,
            1,
            memory_order_relaxed
        );
    }
}

int wf_windows_iocp_park(
    wf_windows_iocp_adapter *adapter,
    uint64_t observed_epoch,
    uint32_t timeout_milliseconds
) {
    DWORD transferred = 0;
    ULONG_PTR key = 0;
    OVERLAPPED *overlapped = NULL;
    DWORD timeout;
    BOOL succeeded;
    DWORD error_code;
    int error;
    wf_windows_iocp_waiter waiter = {NULL, 0u};

    if (adapter == NULL || adapter->initialized == 0) {
        return EINVAL;
    }
    error = wf_windows_iocp_progress_error(adapter);
    if (error != 0) {
        return error;
    }
    /* Announce sleep under the same lock core publishers take.  A publisher
     * before this point leaves only an epoch fact and posts no packet; one
     * after the recheck sees the announced sleeper and posts for it. */
    wf_completion_wait_lock(&adapter->runtime->wait);
    if (wf_completion_wake_epoch(adapter->runtime) != observed_epoch) {
        wf_completion_wait_unlock(&adapter->runtime->wait);
        return 0;
    }
    /* Arm the shared notifier before the SC epoch recheck. Registering only
     * the sleeper count would leave wake_needed clear and lose a helper's
     * completion, which produces no kernel I/O packet of its own. */
    wf_completion_announce_park_locked(adapter->runtime);
    if (atomic_load_explicit(
            &adapter->runtime->wake_epoch,
            memory_order_seq_cst
        ) != observed_epoch) {
        atomic_fetch_sub_explicit(
            &adapter->runtime->parked_schedulers,
            1,
            memory_order_relaxed
        );
        wf_completion_wait_unlock(&adapter->runtime->wait);
        return 0;
    }
    if (adapter->notified_waiters != 0u) {
        /* IOCP packets have no recipient identity. Entering the port with a
         * newer epoch could steal the old cohort's entire broadcast. Block
         * here instead; the last notified native waiter wakes this condition.
         * A new epoch or a spurious condition wake simply retries scheduling. */
        enum wf_completion_wait_result waited = WF_COMPLETION_WAIT_WOKEN;
        if (timeout_milliseconds != 0u) {
            waited = wf_completion_wait_sleep(
                &adapter->runtime->wait,
                timeout_milliseconds
            );
        }
        atomic_fetch_sub_explicit(
            &adapter->runtime->parked_schedulers,
            1,
            memory_order_relaxed
        );
        wf_completion_wait_unlock(&adapter->runtime->wait);
        if (waited == WF_COMPLETION_WAIT_FAILED) {
            wf_windows_record_progress_error(adapter, EPROTO);
            return EPROTO;
        }
        return 0;
    }
    waiter.next = adapter->waiters;
    adapter->waiters = &waiter;
    wf_completion_wait_unlock(&adapter->runtime->wait);

    timeout = timeout_milliseconds == UINT32_MAX
        ? INFINITE
        : (DWORD)timeout_milliseconds;
    atomic_fetch_add_explicit(
        &adapter->stat_kernel_waits,
        1,
        memory_order_relaxed
    );
    succeeded = GetQueuedCompletionStatus(
        adapter->port,
        &transferred,
        &key,
        &overlapped,
        timeout
    );
    error_code = succeeded != FALSE ? 0u : GetLastError();

    wf_completion_wait_lock(&adapter->runtime->wait);
    {
        wf_windows_iocp_waiter **link = &adapter->waiters;
        while (*link != NULL && *link != &waiter) {
            link = &(*link)->next;
        }
        if (*link == NULL) {
            wf_windows_iocp_fail("a native waiter lost its registration");
        }
        *link = waiter.next;
        if (waiter.notified != 0u) {
            if (adapter->notified_waiters == 0u) {
                wf_windows_iocp_fail("a notification cohort underflowed");
            }
            adapter->notified_waiters--;
            if (adapter->notified_waiters == 0u) {
                wf_completion_wait_wake(&adapter->runtime->wait, 1);
            }
        }
    }
    {
        unsigned previous = atomic_fetch_sub_explicit(
            &adapter->runtime->parked_schedulers,
            1,
            memory_order_relaxed
        );
        if (previous == 0) {
            wf_windows_iocp_fail(
                "a thread left the parked count when the count was already zero"
            );
        }
    }
    wf_completion_wait_unlock(&adapter->runtime->wait);

    if (overlapped != NULL) {
        if (key != adapter->file_key) {
            wf_windows_record_progress_error(adapter, EPROTO);
            return EPROTO;
        }
        error = wf_windows_publish_packet(
            adapter,
            overlapped,
            transferred,
            error_code
        );
        if (error != 0) {
            wf_windows_record_progress_error(adapter, error);
        }
        return error;
    }
    if (succeeded != FALSE) {
        /* This call belonged to the native cohort eligible to take a packet.
         * Surplus packets may also wake a later call after that cohort drains. */
        if (key != adapter->wake_key) {
            wf_windows_record_progress_error(adapter, EPROTO);
            return EPROTO;
        }
        return 0;
    }
    if (error_code == WAIT_TIMEOUT) {
        return 0;
    }
    wf_windows_record_progress_error(adapter, (int)error_code);
    return (int)error_code;
}

size_t wf_windows_iocp_in_flight(const wf_windows_iocp_adapter *adapter) {
    if (adapter == NULL) {
        return 0;
    }
    return atomic_load_explicit(&adapter->in_flight, memory_order_acquire);
}

int wf_windows_iocp_destroy(wf_windows_iocp_adapter *adapter) {
    if (adapter == NULL || adapter->initialized == 0) {
        return EINVAL;
    }
    if (wf_windows_iocp_in_flight(adapter) != 0) {
        return EBUSY;
    }
    if (CloseHandle(adapter->port) == FALSE) {
        return (int)GetLastError();
    }
    adapter->port = NULL;
    adapter->initialized = 0;
    return 0;
}

wf_windows_iocp_statistics wf_windows_iocp_statistics_snapshot(
    const wf_windows_iocp_adapter *adapter
) {
    wf_windows_iocp_statistics statistics = {0};
    if (adapter == NULL) {
        return statistics;
    }
    statistics.submissions = atomic_load_explicit(
        &adapter->stat_submissions,
        memory_order_relaxed
    );
    statistics.inline_completions = atomic_load_explicit(
        &adapter->stat_inline_completions,
        memory_order_relaxed
    );
    statistics.dequeued_completions = atomic_load_explicit(
        &adapter->stat_dequeued_completions,
        memory_order_relaxed
    );
    statistics.completions = atomic_load_explicit(
        &adapter->stat_completions,
        memory_order_relaxed
    );
    statistics.kernel_waits = atomic_load_explicit(
        &adapter->stat_kernel_waits,
        memory_order_relaxed
    );
    statistics.host_wake_posts = atomic_load_explicit(
        &adapter->stat_host_wake_posts,
        memory_order_relaxed
    );
    return statistics;
}

#else

typedef int wf_windows_iocp_translation_unit_is_target_guarded;

#endif
