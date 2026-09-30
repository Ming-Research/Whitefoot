#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0602
#endif

/*
 * The Windows host leaf of the file adapter: one host call per request kind,
 * and nothing else.
 *
 * `file_adapter.c` owns the queue, the helpers, the claim and the progress
 * pass, and is one implementation for every platform.  This unit is the one
 * thing that cannot be, and `file_posix.c` is its twin.  Every call it makes is
 * one of `windows_runtime.c`'s workers, so the NtCreateFile relative open, the
 * WriteFile with the stream's own current position, and the directory batch are
 * written once each and reached from here.
 *
 * What this target does *not* qualify is refused as an outcome rather than as
 * a terminated process.  The emitter emits the whole seven-entry submit ABI on
 * every target and the Windows row admits an open, a positioned read, a stream
 * write, a close, a directory batch and the six TCP kinds; a shape the row does
 * not admit reaches this switch and is published as a failed operation with the
 * host's own refusal code, exactly as `wf_bridge_complete_refused` publishes an
 * offset the target ABI cannot express.
 *
 * The socket arms are Winsock calls and nothing else, and they are the twin of
 * `file_posix.c`'s in exactly the sense this unit is that unit's twin: the
 * address conversion, the backlog, the send flags and the pair's two-count are
 * shared (`socket_address.h`, `file_adapter.c`), and what is written here is
 * the call.  A socket reached from this unit is a *blocking* socket:
 * `WSA_FLAG_OVERLAPPED` says the completion port may carry a request issued
 * with an `OVERLAPPED`, and says nothing about a call made without one, so
 * `recv`, `send`, `accept` and `connect` here wait on the helper thread the
 * bounded adapter exists to supply and post no packet to any port the handle
 * is bound to.  That is why the `hEvent` low-bit rule the positioned read
 * needs has no counterpart here: this unit issues no overlapped socket
 * request.
 */

#include "file_adapter.h"
#include "socket_address.h"
#include "../windows_runtime.h"

#include <windows.h>

#include <errno.h>
#include <io.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#if !defined(WF_COMPLETION_SHUTDOWN)
#define WF_COMPLETION_SHUTDOWN shutdown
#else
extern int WF_COMPLETION_SHUTDOWN(SOCKET, int);
#endif

/* The kind the request names, refused where this target has no facility. */
static wf_file_result wf_file_windows_refused(
    const wf_file_request *request,
    DWORD error_code
) {
    wf_file_result result;
    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;
    result.head.error_code = (int)error_code;
    return result;
}

/* One positioned read, made to wait here rather than on the port.
 *
 * This is the route a Windows run takes when it has no completion ring at all
 * -- `WF_IO_NO_NATIVE_RING`, and the negative control the gate runs -- and the
 * route a run *with* a ring takes for every shape that ring refuses: a count
 * above MAXDWORD, a negative offset, a descriptor of a class it does not
 * carry.  So the handle read here may well be one the port already has, and
 * this call says so rather than hoping otherwise.
 *
 * The offset travels in an `OVERLAPPED` because that is how a Windows read
 * names one.  The event beside it is this call's own, so two reads of one
 * handle cannot take each other's completion -- and its low-order bit is set,
 * which is the documented way to say that this one operation posts no packet
 * to whatever completion port the handle is bound to.  Without that bit a read
 * here on a bound handle would deliver its completion to the ring's port, and
 * the reaper would recover a "record" by subtraction from an `OVERLAPPED` that
 * is a local of this frame.  A handle value with its low bits set still names
 * the same object to every wait, so `GetOverlappedResult` below is unaffected;
 * the bit belongs to the `OVERLAPPED`, and `CloseHandle` gets the event
 * itself. */
static wf_file_result wf_file_windows_pread(const wf_file_request *request) {
    wf_file_result result;
    OVERLAPPED overlapped;
    ULARGE_INTEGER offset;
    HANDLE handle;
    HANDLE event;
    DWORD transferred = 0;
    DWORD error_code = 0;

    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;
    if (request->operation.pread.count == 0) {
        result.head.value = 0;
        return result;
    }
    if (request->operation.pread.count > (size_t)MAXDWORD
        || request->operation.pread.offset < 0) {
        result.head.error_code = ERROR_INVALID_PARAMETER;
        return result;
    }
    handle = wf__windows_completion_descriptor_handle(
        request->operation.pread.descriptor
    );
    if (handle == INVALID_HANDLE_VALUE) {
        result.head.error_code = ERROR_INVALID_HANDLE;
        return result;
    }
    event = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (event == NULL) {
        result.head.error_code = (int)GetLastError();
        return result;
    }
    memset(&overlapped, 0, sizeof(overlapped));
    offset.QuadPart = (ULONGLONG)request->operation.pread.offset;
    overlapped.Offset = offset.LowPart;
    overlapped.OffsetHigh = offset.HighPart;
    /* The low bit is the "no completion packet for this operation" flag; see
     * the header comment. */
    overlapped.hEvent = (HANDLE)((uintptr_t)event | (uintptr_t)1);
    if (ReadFile(
            handle,
            request->operation.pread.buffer,
            (DWORD)request->operation.pread.count,
            NULL,
            &overlapped
        ) == FALSE) {
        error_code = GetLastError();
        if (error_code == ERROR_IO_PENDING) {
            error_code = GetOverlappedResult(
                             handle,
                             &overlapped,
                             &transferred,
                             TRUE
                         ) != FALSE
                ? 0u
                : GetLastError();
        }
    } else if (GetOverlappedResult(handle, &overlapped, &transferred, TRUE)
               == FALSE) {
        error_code = GetLastError();
    }
    (void)CloseHandle(event);
    /* Reaching the file boundary is a successful zero-byte read in this
     * contract; Windows reports it as ERROR_HANDLE_EOF. */
    if (error_code == ERROR_HANDLE_EOF) {
        error_code = 0;
        transferred = 0;
    }
    if (error_code != 0) {
        result.head.error_code = (int)error_code;
        return result;
    }
    result.head.value = (int64_t)transferred;
    return result;
}

/* One unpositioned stream read (ordinary native library), made to wait here.
 *
 * A Windows read names its position in an `OVERLAPPED`, and this operation has
 * none to name: the position it reads at is the handle's own. The two shapes a
 * standard input can be are handled by one call, and neither is a fork of any
 * logic here.
 *
 * A console handle is not opened for overlapped I/O and has no overlapped
 * form at all, so `ReadFile` with a null `OVERLAPPED` is the only thing that
 * reads it; that call advances the console's own position and blocks this
 * helper's thread until the user supplies a line, which is exactly what the
 * bounded adapter's helper thread exists for.
 *
 * A redirected pipe or file is the same call. A pipe handle the process
 * inherited is likewise not opened overlapped, so a null `OVERLAPPED` reads it
 * at its own position and returns when the writer supplies bytes or closes;
 * a redirected regular file also advances the handle's own file pointer, which
 * is what an unpositioned read means. Passing an `OVERLAPPED` here would be
 * wrong in both cases rather than faster: on a synchronous handle Windows
 * ignores it for the wait and still uses the offset it carries, which would
 * turn a stream read into a positioned read at zero and re-read the first
 * bytes forever.
 *
 * End of stream is a successful zero-byte read in this contract. A pipe whose
 * writer closed reports `ERROR_BROKEN_PIPE`, which is that end and not a
 * failure, so it is normalized to zero here exactly as a file's
 * `ERROR_HANDLE_EOF` is. */
static wf_file_result wf_file_windows_read(const wf_file_request *request) {
    wf_file_result result;
    HANDLE handle;
    DWORD transferred = 0;
    DWORD error_code = 0;

    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;
    if (request->operation.read.count == 0) {
        result.head.value = 0;
        return result;
    }
    if (request->operation.read.count > (size_t)MAXDWORD) {
        result.head.error_code = ERROR_INVALID_PARAMETER;
        return result;
    }
    handle = wf__windows_completion_descriptor_handle(
        request->operation.read.descriptor
    );
    if (handle == INVALID_HANDLE_VALUE) {
        result.head.error_code = ERROR_INVALID_HANDLE;
        return result;
    }
    if (ReadFile(
            handle,
            request->operation.read.buffer,
            (DWORD)request->operation.read.count,
            &transferred,
            NULL
        ) == FALSE) {
        error_code = GetLastError();
    }
    if (error_code == ERROR_HANDLE_EOF || error_code == ERROR_BROKEN_PIPE) {
        error_code = 0;
        transferred = 0;
    }
    if (error_code != 0) {
        result.head.error_code = (int)error_code;
        return result;
    }
    result.head.value = (int64_t)transferred;
    return result;
}

static wf_file_result wf_file_windows_open_at(const wf_file_request *request) {
    wf_file_result result;
    HANDLE root;
    int error_code = 0;
    unsigned open_outcome = WF_FILE_OPEN_FAILED;
    int descriptor;

    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;
    root = wf__windows_completion_descriptor_handle(
        request->operation.open_at.directory
    );
    if (root == INVALID_HANDLE_VALUE) {
        result.head.error_code = ERROR_INVALID_HANDLE;
        result.head.open_outcome = WF_FILE_OPEN_FAILED;
        return result;
    }
    descriptor = wf__windows_completion_file_open_at_worker(
        root,
        request->operation.open_at.path,
        request->operation.open_at.flags,
        request->operation.open_at.mode,
        request->operation.open_at.has_mode,
        (unsigned)request->operation.open_at.expected_kind,
        request->operation.open_at.descriptor_class,
        &error_code,
        &open_outcome
    );
    result.head.error_code = error_code;
    result.head.open_outcome = (enum wf_file_open_outcome)open_outcome;
    result.head.value = descriptor;
    return result;
}

static wf_file_result wf_file_windows_write(const wf_file_request *request) {
    wf_file_result result;
    HANDLE handle;
    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;
    handle = wf__windows_completion_descriptor_handle(
        request->operation.write.descriptor
    );
    if (handle == INVALID_HANDLE_VALUE) {
        result.head.error_code = ERROR_INVALID_HANDLE;
        return result;
    }
    result.head.value = wf__windows_completion_file_write_worker(
        handle,
        request->operation.write.buffer,
        (uint64_t)request->operation.write.count
    );
    if (result.head.value < 0) {
        result.head.error_code = *wf__windows_error_location();
    }
    return result;
}

/* Whether one descriptor names a Winsock object this runtime created. */
static int wf_file_windows_is_socket(int descriptor) {
    unsigned descriptor_class = WF_WINDOWS_DESCRIPTOR_CLASS_ANY;
    return wf__windows_completion_descriptor_state(
               descriptor,
               &descriptor_class,
               NULL
           ) != 0
        && descriptor_class == WF_WINDOWS_DESCRIPTOR_CLASS_SOCKET;
}

static wf_file_result wf_file_windows_close(const wf_file_request *request) {
    wf_file_result result;
    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    /* A listener's explicit close is the ordinary close every descriptor-shaped
     * resource takes (ordinary native library), and on this target the object behind it decides
     * the call: a Winsock object is ended by `closesocket`, a file by the CRT's
     * own close. */
    if (wf_file_windows_is_socket(request->operation.close.descriptor)) {
        result.head.value =
            wf__windows_socket_close(request->operation.close.descriptor);
        if (result.head.value < 0) {
            result.head.error_code = *wf__windows_error_location();
        }
        return result;
    }
    /* The registry row goes before the handle does, so the descriptor number
     * the host may hand out again starts with no class and no association. */
    wf__windows_completion_forget_descriptor(
        request->operation.close.descriptor
    );
    result.head.value = _close(request->operation.close.descriptor);
    if (result.head.value < 0) {
        result.head.error_code = ERROR_INVALID_HANDLE;
    }
    return result;
}

/* ---------------------------------------------------------- the six TCP */

/* The socket one request operates on, or INVALID_SOCKET. */
static SOCKET wf_file_windows_socket(int descriptor) {
    return (SOCKET)wf__windows_socket_handle(descriptor);
}

/* The host's own refusal of the socket call that just failed, in the one form
 * both this route and the completion port report
 * (`../windows_runtime.h`, `wf__windows_error_from_socket`). */
static int wf_file_windows_socket_error(void) {
    return wf__windows_error_from_socket(WSAGetLastError());
}

/* TCP_NODELAY on every socket this leaf creates or accepts, right after the
 * host hands it over.  Nagle's coalescing measured an 11.78x-14.98x
 * throughput loss and a p99 tail of 41 ms against 2.4 ms without it
 * (research/investigations/io-model/SCHEDULER-FINDINGS.md, experiment 17).
 * A refusal leaves an ordinary, working socket -- just one that may coalesce
 * small writes -- so it is never folded into the operation's own outcome. */
static void wf_file_windows_disable_nagle(SOCKET native_socket) {
    int one = 1;
    (void)setsockopt(
        native_socket,
        IPPROTO_TCP,
        TCP_NODELAY,
        (const char *)&one,
        sizeof one
    );
}

/* One endpoint operation's two preparations: the host's own address record,
 * and one socket of that address's family.
 *
 * Both endpoint kinds do exactly these two things before their own host call,
 * and both dispose of the socket on any refusal. The ordinary linked caller
 * restores the factory's reserved credit on failure. This is
 * `file_posix.c`'s `wf_socket_endpoint` with this platform's socket call. */
static int wf_file_windows_endpoint(
    const wf_file_request *request,
    wf_socket_native_address *native,
    unsigned *length
) {
    *length = wf_socket_native_from_address(
        &request->operation.endpoint.address.portable,
        native
    );
    return wf__windows_socket_open(
        wf_socket_address_family(&request->operation.endpoint.address.portable)
    );
}

/* One listen: a socket of the address's family, bound and listening, or the
 * host's own refusal with nothing left behind. */
static wf_file_result wf_file_windows_socket_listen(
    const wf_file_request *request
) {
    wf_file_result result;
    wf_socket_native_address native;
    unsigned length = 0;
    int endpoint;
    int refusal;

    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;
    endpoint = wf_file_windows_endpoint(request, &native, &length);
    if (endpoint < 0) {
        result.head.error_code = *wf__windows_error_location();
        return result;
    }
    {
        /* Every call below names the Winsock object; `endpoint` is the
         * descriptor number the program's own accounting is written in. */
        SOCKET native_socket = wf_file_windows_socket(endpoint);
        wf_file_windows_disable_nagle(native_socket);
        if (bind(
                native_socket,
                (const struct sockaddr *)native.bytes,
                (int)length
            ) == 0
            && listen(native_socket, WF_SOCKET_BACKLOG) == 0) {
            result.head.value = endpoint;
            return result;
        }
    }
    refusal = wf_file_windows_socket_error();
    (void)wf__windows_socket_close(endpoint);
    result.head.error_code = refusal;
    return result;
}

/* One connect, and the same disposal on refusal. */
static wf_file_result wf_file_windows_socket_connect(
    const wf_file_request *request
) {
    wf_file_result result;
    wf_socket_native_address native;
    unsigned length = 0;
    int endpoint;
    int refusal;

    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;
    endpoint = wf_file_windows_endpoint(request, &native, &length);
    if (endpoint < 0) {
        result.head.error_code = *wf__windows_error_location();
        return result;
    }
    {
        SOCKET native_socket = wf_file_windows_socket(endpoint);
        wf_file_windows_disable_nagle(native_socket);
        if (connect(
                native_socket,
                (const struct sockaddr *)native.bytes,
                (int)length
            ) == 0) {
            result.head.value = endpoint;
            return result;
        }
    }
    refusal = wf_file_windows_socket_error();
    (void)wf__windows_socket_close(endpoint);
    result.head.error_code = refusal;
    return result;
}

/* One accept.  The connection Winsock hands over is adopted into a descriptor
 * of this runtime's own socket class, and the peer record the host wrote is
 * rewritten in place as the portable form the accept join publishes -- the
 * same rewrite every other engine makes, so the join reads one form. */
static wf_file_result wf_file_windows_socket_accept(wf_file_request *request) {
    wf_file_result result;
    SOCKET listener;
    SOCKET taken;
    int length = (int)sizeof(request->operation.accept.peer.native);
    int descriptor;

    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;
    listener = wf_file_windows_socket(request->operation.accept.descriptor);
    if (listener == INVALID_SOCKET) {
        result.head.error_code = (int)ERROR_INVALID_HANDLE;
        return result;
    }
    taken = accept(
        listener,
        (struct sockaddr *)request->operation.accept.peer.native.bytes,
        &length
    );
    if (taken == INVALID_SOCKET) {
        result.head.error_code = wf_file_windows_socket_error();
        return result;
    }
    wf_file_windows_disable_nagle(taken);
    /* A connection inherits its listener's properties, so it is already
     * overlapped and already not inherited; what it does not have is a
     * descriptor number and a row in this runtime's own table. */
    errno = 0;
    descriptor = _open_osfhandle((intptr_t)taken, 0);
    if (descriptor < 0
        || wf__windows_completion_register_descriptor(
               descriptor,
               WF_WINDOWS_DESCRIPTOR_CLASS_SOCKET
           ) != 0) {
        if (descriptor >= 0) {
            (void)wf__windows_socket_close(descriptor);
        } else {
            (void)closesocket(taken);
        }
        result.head.error_code = (int)ERROR_NOT_ENOUGH_MEMORY;
        return result;
    }
    request->operation.accept.peer_length = (unsigned)length;
    wf_socket_publish_peer(request);
    result.head.value = descriptor;
    return result;
}

/* One transfer attempt on one direction of one connection (ordinary native library).  Winsock
 * counts bytes in an `int`, so a longer range is the host's own refusal and
 * never a silent truncation. */
static wf_file_result wf_file_windows_socket_transfer(
    const wf_file_request *request
) {
    wf_file_result result;
    SOCKET connection;
    int receiving = request->kind == WF_FILE_SOCKET_RECEIVE;
    size_t count = receiving ? request->operation.receive.count
                             : request->operation.send.count;
    int descriptor = receiving ? request->operation.receive.descriptor
                               : request->operation.send.descriptor;
    int transferred;

    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;
    if (count == 0) {
        result.head.value = 0;
        return result;
    }
    if (count > (size_t)INT_MAX) {
        result.head.error_code = (int)ERROR_INVALID_PARAMETER;
        return result;
    }
    connection = wf_file_windows_socket(descriptor);
    if (connection == INVALID_SOCKET) {
        result.head.error_code = (int)ERROR_INVALID_HANDLE;
        return result;
    }
    transferred = receiving
        ? recv(
              connection,
              (char *)request->operation.receive.buffer,
              (int)count,
              0
          )
        : send(
              connection,
              (const char *)request->operation.send.buffer,
              (int)count,
              WF_SOCKET_SEND_FLAGS
          );
    if (transferred == SOCKET_ERROR) {
        result.head.error_code = wf_file_windows_socket_error();
        return result;
    }
    result.head.value = (int64_t)transferred;
    return result;
}

/* One direction's half-close, and the close of the target's object when it is
 * the pair's second release (ordinary native library).
 *
 * Each direction completes shutdown before publishing to the shared count in
 * `file_adapter.c`, so the second publisher may close the socket only after
 * both directions have finished using it. `SD_RECEIVE` and `SD_SEND` are this
 * platform's spelling of `SHUT_RD` and `SHUT_WR`. */
static wf_file_result wf_file_windows_socket_shutdown(
    const wf_file_request *request
) {
    wf_file_result result;
    SOCKET connection;
    int descriptor = request->operation.shutdown.descriptor;
    int direction =
        request->operation.shutdown.direction == WF_SOCKET_DIRECTION_SEND
        ? SD_SEND
        : SD_RECEIVE;
    int last;

    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    result.head.value = -1;
    connection = wf_file_windows_socket(descriptor);
    if (connection == INVALID_SOCKET) {
        result.head.error_code = (int)ERROR_INVALID_HANDLE;
        return result;
    }
    /* Publish completion only after the last use of this native socket, so
     * the other direction cannot close and recycle it before shutdown. */
    (void)WF_COMPLETION_SHUTDOWN(connection, direction);
    last = wf_file_connection_release(descriptor);
    if (last == 0) {
        result.head.value = 0;
        return result;
    }
    if (wf__windows_socket_close(descriptor) < 0) {
        result.head.error_code = *wf__windows_error_location();
    }
    result.head.value = 1;
    return result;
}

#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
static wf_file_result wf_file_windows_directory_next(
    const wf_file_request *request
) {
    wf_file_result result;
    memset(&result, 0, sizeof(result));
    result.head.kind = request->kind;
    if (request->operation.directory_next.count == 0) {
        result.head.value = 0;
        return result;
    }
    result.head.value = wf__windows_directory_batch(
        request->operation.directory_next.descriptor,
        request->operation.directory_next.buffer,
        (uint64_t)request->operation.directory_next.count,
        request->operation.directory_next.position
    );
    if (result.head.value < 0) {
        result.head.error_code = *wf__windows_error_location();
    }
    return result;
}
#endif

wf_file_result wf_file_execute_direct(wf_file_request *request) {
    if (!wf_file_request_valid(request)) {
        wf_file_result result;
        memset(&result, 0, sizeof(result));
        result.head.kind = request == NULL ? 0 : request->kind;
        result.head.value = -1;
        result.head.error_code = ERROR_INVALID_PARAMETER;
        return result;
    }
    switch (request->kind) {
    case WF_FILE_OPEN_AT:
        return wf_file_windows_open_at(request);
    case WF_FILE_PREAD:
        return wf_file_windows_pread(request);
    case WF_FILE_WRITE:
        return wf_file_windows_write(request);
    case WF_FILE_CLOSE:
        return wf_file_windows_close(request);
    case WF_FILE_READ:
        return wf_file_windows_read(request);
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
    case WF_FILE_DIRECTORY_NEXT:
        return wf_file_windows_directory_next(request);
#endif
    case WF_FILE_SOCKET_LISTEN:
        return wf_file_windows_socket_listen(request);
    case WF_FILE_SOCKET_ACCEPT:
        return wf_file_windows_socket_accept(request);
    case WF_FILE_SOCKET_CONNECT:
        return wf_file_windows_socket_connect(request);
    case WF_FILE_SOCKET_RECEIVE:
    case WF_FILE_SOCKET_SEND:
        return wf_file_windows_socket_transfer(request);
    case WF_FILE_SOCKET_SHUTDOWN:
        return wf_file_windows_socket_shutdown(request);
    default:
        /* An unknown private request kind has no host operation. */
        return wf_file_windows_refused(request, ERROR_NOT_SUPPORTED);
    }
}

/* The monotonic clock, which is a host call and therefore this leaf's; see
 * `file_adapter.h` for its two shared readers.  The performance counter is
 * monotonic and its frequency is fixed for the life of the system, so the
 * frequency is asked for once. */
uint64_t wf_file_monotonic_ns(void) {
    static LARGE_INTEGER frequency;
    LARGE_INTEGER now;
    if (frequency.QuadPart == 0 && QueryPerformanceFrequency(&frequency) == FALSE) {
        return 0;
    }
    if (frequency.QuadPart == 0 || QueryPerformanceCounter(&now) == FALSE) {
        return 0;
    }
    return (uint64_t)((now.QuadPart / frequency.QuadPart) * 1000000000ll)
        + (uint64_t)(
              ((now.QuadPart % frequency.QuadPart) * 1000000000ll)
              / frequency.QuadPart
          );
}

/* Sockets here have the completion port, so no context waits on a
 * descriptor's readiness; without the port, a context's socket wait blocks
 * the thread as every wait did before contexts. */
int wf_file_readiness_supported(void) {
    return 0;
}

int wf_file_wait_readiness(wf_file_readiness *entries, size_t count, int timeout_ms) {
    (void)entries;
    (void)count;
    (void)timeout_ms;
    return -1;
}

/* This platform's sockets are blocking objects of the completion port and the
 * adapter, so no transfer is attempted without an engine: every one is left
 * for the route the bridge chooses. */
int wf_file_transfer_now(const wf_file_request *request, wf_file_result *result) {
    (void)request;
    (void)result;
    return 0;
}
