/* Compiler-owned Windows v0.39 direct host runtime.
 *
 * Writer-visible resources remain CRT i32 descriptors. Native HANDLE values
 * are recovered only inside these target-selected operations; relative opens
 * are rooted by NtCreateFile's OBJECT_ATTRIBUTES.RootDirectory and never by
 * ambient-path concatenation.
 */

#define WIN32_LEAN_AND_MEAN
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0602
#endif

#include "windows_runtime.h"

/* Winsock first, so that `<windows.h>` cannot pull the version-1 declarations
 * in ahead of it.  WIN32_LEAN_AND_MEAN above already excludes them; this order
 * makes that independent of the macro. */
#include <winsock2.h>
#include <ws2tcpip.h>

#include <windows.h>
#include <winternl.h>

#include <errno.h>
#include <fcntl.h>
#include <io.h>
#include <limits.h>
#include <stddef.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if !defined(_WIN32)
#error "windows_runtime.c requires a Windows target"
#endif

#ifndef OBJ_CASE_INSENSITIVE
#define OBJ_CASE_INSENSITIVE 0x00000040UL
#endif
#ifndef FILE_OPEN
#define FILE_OPEN 0x00000001UL
#endif
#ifndef FILE_SYNCHRONOUS_IO_NONALERT
#define FILE_SYNCHRONOUS_IO_NONALERT 0x00000020UL
#endif
#ifndef FILE_OPEN_IF
#define FILE_OPEN_IF 0x00000003UL
#endif
#ifndef FILE_NON_DIRECTORY_FILE
#define FILE_NON_DIRECTORY_FILE 0x00000040UL
#endif
#ifndef FILE_OPEN_REPARSE_POINT
#define FILE_OPEN_REPARSE_POINT 0x00200000UL
#endif
#ifndef ERROR_MR_MID_NOT_FOUND
#define ERROR_MR_MID_NOT_FOUND 317UL
#endif

#define WF_WINDOWS_NO_FOLLOW 1
#define WF_WINDOWS_EXPECT_ANY 0u
#define WF_WINDOWS_EXPECT_REGULAR 1u
#define WF_WINDOWS_EXPECT_DIRECTORY 2u

#define WF_WINDOWS_OPEN_SUCCEEDED 0u
#define WF_WINDOWS_OPEN_FAILED 1u
#define WF_WINDOWS_OPEN_STATUS_FAILED 2u
#define WF_WINDOWS_OPEN_IS_DIRECTORY 3u
#define WF_WINDOWS_OPEN_OTHER_KIND 4u

#define WF_WINDOWS_MODE_DIRECTORY UINT16_C(0x4000)
#define WF_WINDOWS_MODE_REGULAR UINT16_C(0x8000)
#define WF_WINDOWS_MODE_SYMLINK UINT16_C(0xa000)

#define WF_WINDOWS_DIRECTORY_NATIVE_HEADER 64u
#define WF_WINDOWS_DIRECTORY_RAW_HEADER 5u
#define WF_WINDOWS_COMPONENT_MAX_BYTES 510u
#define WF_WINDOWS_DIRECTORY_SCRATCH_BYTES 8192u
#define WF_WINDOWS_FILE_DIRECTORY_INFORMATION_CLASS \
    ((FILE_INFORMATION_CLASS)1)
#define WF_WINDOWS_STATUS_NO_MORE_FILES \
    ((NTSTATUS)(LONG)0x80000006UL)
#define WF_WINDOWS_STATUS_PENDING ((NTSTATUS)(LONG)0x00000103UL)

typedef NTSTATUS(NTAPI *wf_windows_nt_create_file_fn)(
    PHANDLE,
    ACCESS_MASK,
    POBJECT_ATTRIBUTES,
    PIO_STATUS_BLOCK,
    PLARGE_INTEGER,
    ULONG,
    ULONG,
    ULONG,
    ULONG,
    PVOID,
    ULONG
);

typedef NTSTATUS(NTAPI *wf_windows_nt_query_directory_file_fn)(
    HANDLE,
    HANDLE,
    PIO_APC_ROUTINE,
    PVOID,
    PIO_STATUS_BLOCK,
    PVOID,
    ULONG,
    FILE_INFORMATION_CLASS,
    BOOLEAN,
    PUNICODE_STRING,
    BOOLEAN
);

typedef ULONG(NTAPI *wf_windows_rtl_status_to_dos_error_fn)(NTSTATUS);

typedef struct wf_windows_nt_api {
    wf_windows_nt_create_file_fn create_file;
    wf_windows_nt_query_directory_file_fn query_directory_file;
    wf_windows_rtl_status_to_dos_error_fn status_to_dos_error;
} wf_windows_nt_api;

typedef struct wf_windows_file_directory_information {
    ULONG next_entry_offset;
    ULONG file_index;
    LARGE_INTEGER creation_time;
    LARGE_INTEGER last_access_time;
    LARGE_INTEGER last_write_time;
    LARGE_INTEGER change_time;
    LARGE_INTEGER end_of_file;
    LARGE_INTEGER allocation_size;
    ULONG file_attributes;
    ULONG file_name_length;
    WCHAR file_name[1];
} wf_windows_file_directory_information;

typedef union wf_windows_directory_scratch {
    ULONG_PTR alignment;
    unsigned char bytes[WF_WINDOWS_DIRECTORY_SCRATCH_BYTES];
} wf_windows_directory_scratch;

typedef struct wf_windows_status_cell {
    uint16_t mode;
    uint16_t reserved;
    uint32_t attributes;
} wf_windows_status_cell;

_Static_assert(CHAR_BIT == 8, "the runtime ABI requires eight-bit bytes");
_Static_assert(sizeof(int) == 4, "the runtime ABI requires i32 int");
_Static_assert(sizeof(size_t) == 8, "the target row requires 64-bit size_t");
_Static_assert(sizeof(uint16_t) == 2, "UTF-16 code units must be 16-bit");
_Static_assert(sizeof(WCHAR) == 2, "Windows WCHAR must be UTF-16-sized");
_Static_assert(sizeof(NTSTATUS) == 4, "NTSTATUS detail must remain 32-bit");
_Static_assert(
    sizeof(wf_windows_status_cell) == 8
        && offsetof(wf_windows_status_cell, mode) == 0,
    "the compiler-owned status cell layout changed"
);
_Static_assert(
    offsetof(wf_windows_file_directory_information, file_name)
        == WF_WINDOWS_DIRECTORY_NATIVE_HEADER,
    "FILE_DIRECTORY_INFORMATION layout changed"
);
_Static_assert(
    sizeof(wf_windows_nt_create_file_fn) == sizeof(FARPROC)
        && sizeof(wf_windows_nt_query_directory_file_fn) == sizeof(FARPROC)
        && sizeof(wf_windows_rtl_status_to_dos_error_fn) == sizeof(FARPROC),
    "GetProcAddress does not fit the native procedure pointers"
);

/* This leaf's one fail-stop.
 *
 * Each `abort` below is a trusted-computing-base defect, not an operation
 * outcome, and a fail-stop that writes nothing cannot be diagnosed from a
 * crash log: the release UCRT ends such a process through the fast-fail path,
 * which a shell reports as a bare status with no message at all. This writes
 * one line and then aborts exactly where the bare call did, and changes no
 * control flow. */
static _Noreturn void wf_windows_fail(const char *reason) {
    (void)fprintf(stderr, "whitefoot windows runtime: %s\n", reason);
    (void)fflush(stderr);
    abort();
}

static _Thread_local int wf_windows_error_code;

static void wf_windows_record_error(DWORD error_code) {
    DWORD recorded = error_code;
    if (recorded == ERROR_SUCCESS || recorded > (DWORD)INT_MAX) {
        recorded = ERROR_GEN_FAILURE;
    }
    wf_windows_error_code = (int)recorded;
}

static DWORD wf_windows_error_from_errno(int error_code) {
    switch (error_code) {
    case 0:
        return ERROR_GEN_FAILURE;
    case EACCES:
        return ERROR_ACCESS_DENIED;
    case EBADF:
        return ERROR_INVALID_HANDLE;
    case EINVAL:
        return ERROR_INVALID_PARAMETER;
    case EMFILE:
        return ERROR_TOO_MANY_OPEN_FILES;
    case ENOENT:
        return ERROR_FILE_NOT_FOUND;
    case ENOMEM:
        return ERROR_NOT_ENOUGH_MEMORY;
    default:
        return ERROR_GEN_FAILURE;
    }
}

/* One code per condition, whichever route produced it; `windows_runtime.h`
 * says at the declaration why the two routes need it.
 *
 * The right column is the code the ordinary native library's error conversion
 * already uses for that condition, so this adds
 * no class and moves no existing mapping.  The two address refusals keep their
 * Winsock numbers, because bind and listen never reach the completion port and
 * those are the numbers that table already names. */
int wf__windows_error_from_socket(int error_code) {
    switch (error_code) {
    /* A peer that refused the connection. */
    case WSAECONNREFUSED:
    case (int)ERROR_CONNECTION_REFUSED:
        return (int)ERROR_CONNECTION_REFUSED;
    /* A peer that reset the connection, and the port's own spelling of it. */
    case WSAECONNRESET:
    case WSAENETRESET:
    case (int)ERROR_NETNAME_DELETED:
        return (int)ERROR_NETNAME_DELETED;
    /* An aborted connection.  Both spellings answer ERROR_REQUEST_ABORTED,
     * which is the code that table classifies `ConnectionAborted`; the port's
     * own ERROR_CONNECTION_ABORTED is classified `ConnectionReset` there, and
     * one condition may not answer two classes depending on its route. */
    case WSAECONNABORTED:
    case (int)ERROR_CONNECTION_ABORTED:
        return (int)ERROR_REQUEST_ABORTED;
    /* A send on a direction this program has already shut down, which is what
     * `BrokenPipe` means (ordinary native library). */
    case WSAESHUTDOWN:
    case (int)ERROR_GRACEFUL_DISCONNECT:
    case (int)ERROR_NO_DATA:
        return (int)ERROR_BROKEN_PIPE;
    case WSAETIMEDOUT:
    case (int)ERROR_TIMEOUT:
        return (int)ERROR_SEM_TIMEOUT;
    case WSAENOTCONN:
        return (int)ERROR_NOT_CONNECTED;
    case WSAEACCES:
        return (int)ERROR_ACCESS_DENIED;
    case WSAEMFILE:
        return (int)ERROR_TOO_MANY_OPEN_FILES;
    case WSAENOBUFS:
        return (int)ERROR_NOT_ENOUGH_MEMORY;
    case WSAEINVAL:
        return (int)ERROR_INVALID_PARAMETER;
    case WSAEAFNOSUPPORT:
    case WSAEPROTONOSUPPORT:
    case WSAEOPNOTSUPP:
        return (int)ERROR_NOT_SUPPORTED;
    /* Every other code is its own answer, so a condition with no portable
     * distinction reaches source as `Other` rather than as something else. */
    default:
        return error_code;
    }
}

/* ------------------------------------------------------------- Winsock */

static INIT_ONCE wf_windows_socket_once = INIT_ONCE_STATIC_INIT;
static DWORD wf_windows_socket_startup_error;

static BOOL CALLBACK wf_windows_socket_startup_once(
    PINIT_ONCE once,
    PVOID parameter,
    PVOID *context
) {
    WSADATA data;
    int started;
    (void)once;
    (void)parameter;
    (void)context;
    memset(&data, 0, sizeof(data));
    started = WSAStartup(MAKEWORD(2, 2), &data);
    wf_windows_socket_startup_error = started == 0 ? ERROR_SUCCESS
                                                   : (DWORD)started;
    /* The once has run either way: a refusal is remembered above and reported
     * to every caller, rather than re-attempted per operation. */
    return TRUE;
}

int wf__windows_socket_startup(void) {
    if (InitOnceExecuteOnce(
            &wf_windows_socket_once,
            wf_windows_socket_startup_once,
            NULL,
            NULL
        ) == FALSE) {
        return (int)ERROR_GEN_FAILURE;
    }
    if (wf_windows_socket_startup_error != ERROR_SUCCESS) {
        return (int)wf_windows_socket_startup_error;
    }
    return 0;
}

uintptr_t wf__windows_socket_handle(int descriptor) {
    HANDLE native = wf__windows_completion_descriptor_handle(descriptor);
    return native == INVALID_HANDLE_VALUE ? (uintptr_t)INVALID_SOCKET
                                          : (uintptr_t)native;
}

int wf__windows_socket_open(int family) {
    SOCKET created;
    int descriptor;
    int startup = wf__windows_socket_startup();
    if (startup != 0) {
        wf_windows_record_error((DWORD)startup);
        return -1;
    }
    created = WSASocketW(
        family,
        SOCK_STREAM,
        IPPROTO_TCP,
        NULL,
        0,
        WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT
    );
    if (created == INVALID_SOCKET) {
        wf_windows_record_error(
            (DWORD)wf__windows_error_from_socket(WSAGetLastError())
        );
        return -1;
    }
    /* The descriptor number is the CRT's, because every writer-visible
     * resource on this target is a CRT i32 descriptor and the (ordinary native library) handle
     * factory's capacity argument is written in that one numbering
     * (`completion/file_adapter.h`, WF_FILE_CONNECTION_DESCRIPTORS).  The
     * adoption stores this socket's handle in that table and nothing else;
     * every host call below reaches the socket through it. */
    errno = 0;
    descriptor = _open_osfhandle((intptr_t)created, 0);
    if (descriptor < 0) {
        int saved_errno = errno;
        (void)closesocket(created);
        wf_windows_record_error(wf_windows_error_from_errno(saved_errno));
        return -1;
    }
    {
        int registered = wf__windows_completion_register_descriptor(
            descriptor,
            WF_WINDOWS_DESCRIPTOR_CLASS_SOCKET
        );
        if (registered != 0) {
            (void)wf__windows_socket_close(descriptor);
            wf_windows_record_error((DWORD)registered);
            return -1;
        }
    }
    return descriptor;
}

int wf__windows_socket_close(int descriptor) {
    SOCKET native = (SOCKET)wf__windows_socket_handle(descriptor);
    int refusal = 0;
    if (native == INVALID_SOCKET) {
        wf_windows_record_error(ERROR_INVALID_HANDLE);
        return -1;
    }
    /* The registry row goes before the object does, so the descriptor number
     * the host may hand out again starts with no class and no association. */
    wf__windows_completion_forget_descriptor(descriptor);
    if (closesocket(native) != 0) {
        refusal = wf__windows_error_from_socket(WSAGetLastError());
    }
    /* `closesocket` ends the Winsock object; this releases the CRT row the
     * number lives in.  Its own `CloseHandle` necessarily fails, because the
     * object it names is already gone -- the number is what this call is for,
     * and the operation's outcome is the one `closesocket` reported. */
    (void)_close(descriptor);
    if (refusal != 0) {
        wf_windows_record_error((DWORD)refusal);
        return -1;
    }
    return 0;
}

static int wf_windows_nt_success(NTSTATUS status) {
    return status >= 0;
}

static int wf_windows_handle_valid(HANDLE handle) {
    return handle != NULL && handle != INVALID_HANDLE_VALUE;
}

/* ------------------------------------------------ the descriptor registry */

/* What this runtime remembers about the descriptors it produced: the resource
 * class each one became, and whether the completion port has taken its handle.
 *
 * This is all that is left of the descriptor ledger the deleted bounded pool
 * needed, and `windows_runtime.h` says beside the declarations why each of the
 * two facts has a reader that cannot do without it and why nothing else does.
 * The table is indexed by descriptor and grows; it is never shrunk, because a
 * descriptor number the host reuses reuses its row.
 *
 * The lock is exclusive for every operation.  These are startup-rate calls --
 * one per open, one per close, one per first submission on a descriptor -- and
 * a reader-writer split would buy nothing measurable at that rate. */
typedef struct wf_windows_descriptor_row {
    unsigned descriptor_class;
    unsigned port_associated;
    unsigned present;
} wf_windows_descriptor_row;

static SRWLOCK wf_windows_registry_lock = SRWLOCK_INIT;
static wf_windows_descriptor_row *wf_windows_registry;
static size_t wf_windows_registry_capacity;

static int wf_windows_registry_grow(size_t required) {
    wf_windows_descriptor_row *grown;
    size_t capacity;
    if (required <= wf_windows_registry_capacity) {
        return 1;
    }
    capacity = wf_windows_registry_capacity == 0 ? 64u : wf_windows_registry_capacity;
    while (capacity < required && capacity <= SIZE_MAX / 2u) {
        capacity *= 2u;
    }
    if (capacity < required
        || capacity > SIZE_MAX / sizeof(*wf_windows_registry)) {
        return 0;
    }
    grown = wf_windows_registry == NULL
        ? (wf_windows_descriptor_row *)HeapAlloc(
              GetProcessHeap(),
              HEAP_ZERO_MEMORY,
              capacity * sizeof(*wf_windows_registry)
          )
        : (wf_windows_descriptor_row *)HeapReAlloc(
              GetProcessHeap(),
              HEAP_ZERO_MEMORY,
              wf_windows_registry,
              capacity * sizeof(*wf_windows_registry)
          );
    if (grown == NULL) {
        return 0;
    }
    wf_windows_registry = grown;
    wf_windows_registry_capacity = capacity;
    return 1;
}

HANDLE wf__windows_completion_descriptor_handle(int descriptor) {
    intptr_t native = descriptor < 0 ? -1 : _get_osfhandle(descriptor);
    return native == -1 ? INVALID_HANDLE_VALUE : (HANDLE)native;
}

int wf__windows_completion_register_descriptor(
    int descriptor,
    unsigned descriptor_class
) {
    if (descriptor < 0
        || !wf_windows_handle_valid(
               wf__windows_completion_descriptor_handle(descriptor)
           )) {
        return ERROR_INVALID_HANDLE;
    }
    AcquireSRWLockExclusive(&wf_windows_registry_lock);
    if (!wf_windows_registry_grow((size_t)descriptor + 1u)) {
        ReleaseSRWLockExclusive(&wf_windows_registry_lock);
        return ERROR_NOT_ENOUGH_MEMORY;
    }
    wf_windows_registry[descriptor].descriptor_class = descriptor_class;
    wf_windows_registry[descriptor].port_associated = 0u;
    wf_windows_registry[descriptor].present = 1u;
    ReleaseSRWLockExclusive(&wf_windows_registry_lock);
    return 0;
}

int wf__windows_completion_descriptor_state(
    int descriptor,
    unsigned *descriptor_class,
    unsigned *port_associated
) {
    int present = 0;
    if (descriptor_class != NULL) {
        *descriptor_class = WF_WINDOWS_DESCRIPTOR_CLASS_ANY;
    }
    if (port_associated != NULL) {
        *port_associated = 0u;
    }
    if (descriptor < 0) {
        return 0;
    }
    AcquireSRWLockExclusive(&wf_windows_registry_lock);
    if ((size_t)descriptor < wf_windows_registry_capacity
        && wf_windows_registry[descriptor].present != 0u) {
        present = 1;
        if (descriptor_class != NULL) {
            *descriptor_class = wf_windows_registry[descriptor].descriptor_class;
        }
        if (port_associated != NULL) {
            *port_associated = wf_windows_registry[descriptor].port_associated;
        }
    }
    ReleaseSRWLockExclusive(&wf_windows_registry_lock);
    return present;
}

/* Whether the completion port may use this descriptor's handle, binding it
 * exactly once if it must.
 *
 * The whole of it -- the class rule, the "has it been bound" question, the
 * binding itself and the record of it -- happens inside one hold of the table's
 * own lock, and that is the point of the call.  `CreateIoCompletionPort` binds
 * a handle exactly once and answers ERROR_INVALID_PARAMETER for a handle a port
 * already has, and there is no host call that asks whether it already has one.
 * So the fact lives in this table, and a check outside the lock followed by a
 * bind is a race with a real consequence rather than a wasted call: two threads
 * offering the first two records on one descriptor both read "not bound", the
 * first binds, the second's bind fails, and that second record falls to the
 * bounded adapter -- which then issues host I/O on a handle that is now on this
 * port.  The lock makes the second thread either wait for the first to finish
 * binding or find the handle already bound, and in both cases submit.
 *
 * `bind` is the caller's, because the port is the completion runtime's and this
 * unit does not reach it (`windows_runtime.h`).  It runs under the lock, which
 * is what serializes it; it answers nonzero when the handle is now the port's.
 *
 * A descriptor this runtime never opened -- a probe's own `_open_osfhandle` of
 * a `CreateFile` -- has no row until the first offer on it.  It is admitted on
 * the host's own answer that it is a disk file, and given a row under class ANY
 * so the association fact has somewhere to live; without the row, every offer
 * would re-ask `GetFileType` and re-attempt the bind.  A row under any other
 * class is a directory or an output, which this ring does not carry.
 *
 * A socket is admitted on its class alone and never on `GetFileType`, which
 * answers FILE_TYPE_PIPE for one: the class is the fact that this runtime
 * created it with `WSA_FLAG_OVERLAPPED`, which is what the port needs
 * (ordinary native library).
 *
 * The lock is held across three host calls.  This is once per descriptor, not
 * once per operation: the second and every later offer on the same descriptor
 * takes the already-bound answer and makes no host call at all. */
int wf__windows_completion_ring_handle(
    int descriptor,
    int (*bind)(HANDLE handle, void *context),
    void *context,
    HANDLE *handle
) {
    HANDLE native;
    int admitted = 0;
    if (descriptor < 0 || bind == NULL || handle == NULL) {
        return 0;
    }
    native = wf__windows_completion_descriptor_handle(descriptor);
    if (!wf_windows_handle_valid(native)) {
        return 0;
    }
    AcquireSRWLockExclusive(&wf_windows_registry_lock);
    if (!wf_windows_registry_grow((size_t)descriptor + 1u)) {
        ReleaseSRWLockExclusive(&wf_windows_registry_lock);
        return 0;
    }
    if (wf_windows_registry[descriptor].present == 0u) {
        if (GetFileType(native) != FILE_TYPE_DISK) {
            ReleaseSRWLockExclusive(&wf_windows_registry_lock);
            return 0;
        }
        wf_windows_registry[descriptor].descriptor_class =
            WF_WINDOWS_DESCRIPTOR_CLASS_ANY;
        wf_windows_registry[descriptor].port_associated = 0u;
        wf_windows_registry[descriptor].present = 1u;
    } else if (wf_windows_registry[descriptor].descriptor_class
                   != WF_WINDOWS_DESCRIPTOR_CLASS_READ_FILE
               && wf_windows_registry[descriptor].descriptor_class
                   != WF_WINDOWS_DESCRIPTOR_CLASS_SOCKET
               && wf_windows_registry[descriptor].descriptor_class
                   != WF_WINDOWS_DESCRIPTOR_CLASS_ANY) {
        ReleaseSRWLockExclusive(&wf_windows_registry_lock);
        return 0;
    }
    if (wf_windows_registry[descriptor].port_associated != 0u) {
        admitted = 1;
    } else if (bind(native, context) != 0) {
        wf_windows_registry[descriptor].port_associated = 1u;
        admitted = 1;
    }
    ReleaseSRWLockExclusive(&wf_windows_registry_lock);
    if (admitted != 0) {
        *handle = native;
    }
    return admitted;
}

void wf__windows_completion_forget_descriptor(int descriptor) {
    if (descriptor < 0) {
        return;
    }
    AcquireSRWLockExclusive(&wf_windows_registry_lock);
    if ((size_t)descriptor < wf_windows_registry_capacity) {
        wf_windows_registry[descriptor].descriptor_class =
            WF_WINDOWS_DESCRIPTOR_CLASS_ANY;
        wf_windows_registry[descriptor].port_associated = 0u;
        wf_windows_registry[descriptor].present = 0u;
    }
    ReleaseSRWLockExclusive(&wf_windows_registry_lock);
}

static int wf_windows_close_provisional_handle(HANDLE *handle) {
    if (handle != NULL && wf_windows_handle_valid(*handle)) {
        if (CloseHandle(*handle) == FALSE) {
            wf_windows_fail(
                "a provisional handle this runtime owns could not be closed"
            );
        }
        *handle = INVALID_HANDLE_VALUE;
        return 1;
    }
    return 0;
}

static int wf_windows_resolve_procedure(
    HMODULE module,
    const char *name,
    void *destination,
    size_t destination_size
) {
    FARPROC address;
    if (module == NULL || name == NULL || destination == NULL) {
        return 0;
    }
    address = GetProcAddress(module, name);
    if (address == NULL || destination_size != sizeof(address)) {
        return 0;
    }
    memcpy(destination, &address, sizeof(address));
    return 1;
}

static int wf_windows_resolve_nt_api(wf_windows_nt_api *api) {
    HMODULE module = GetModuleHandleW(L"ntdll.dll");
    if (api == NULL || module == NULL) {
        return 0;
    }
    memset(api, 0, sizeof(*api));
    return wf_windows_resolve_procedure(
               module,
               "NtCreateFile",
               &api->create_file,
               sizeof(api->create_file)
           )
        && wf_windows_resolve_procedure(
               module,
               "NtQueryDirectoryFile",
               &api->query_directory_file,
               sizeof(api->query_directory_file)
           )
        && wf_windows_resolve_procedure(
               module,
               "RtlNtStatusToDosError",
               &api->status_to_dos_error,
               sizeof(api->status_to_dos_error)
           );
}

static DWORD wf_windows_nt_error(
    const wf_windows_nt_api *api,
    NTSTATUS status
) {
    ULONG mapped = api->status_to_dos_error(status);
    return mapped == ERROR_SUCCESS || mapped == ERROR_MR_MID_NOT_FOUND
        ? ERROR_GEN_FAILURE
        : (DWORD)mapped;
}

static int wf_windows_adopt_handle(HANDLE handle, int open_flags) {
    int descriptor;
    int saved_errno;
    errno = 0;
    descriptor = _open_osfhandle((intptr_t)handle, open_flags);
    saved_errno = errno;
    if (descriptor < 0) {
        DWORD mapped = wf_windows_error_from_errno(saved_errno);
        if (CloseHandle(handle) == FALSE) {
            wf_windows_fail(
                "a handle could not be closed after its descriptor adoption failed"
            );
        }
        wf_windows_record_error(mapped);
        return -1;
    }
    return descriptor;
}

static uint16_t wf_windows_mode_from_attributes(DWORD attributes) {
    if ((attributes & FILE_ATTRIBUTE_REPARSE_POINT) != 0) {
        return WF_WINDOWS_MODE_SYMLINK;
    }
    if ((attributes & FILE_ATTRIBUTE_DIRECTORY) != 0) {
        return WF_WINDOWS_MODE_DIRECTORY;
    }
    if ((attributes & FILE_ATTRIBUTE_DEVICE) != 0) {
        return 0;
    }
    return WF_WINDOWS_MODE_REGULAR;
}

static unsigned wf_windows_kind_outcome(
    unsigned expected_kind,
    DWORD attributes
) {
    uint16_t mode = wf_windows_mode_from_attributes(attributes);
    if (expected_kind == WF_WINDOWS_EXPECT_ANY
        || (expected_kind == WF_WINDOWS_EXPECT_REGULAR
            && mode == WF_WINDOWS_MODE_REGULAR)
        || (expected_kind == WF_WINDOWS_EXPECT_DIRECTORY
            && mode == WF_WINDOWS_MODE_DIRECTORY)) {
        return WF_WINDOWS_OPEN_SUCCEEDED;
    }
    return mode == WF_WINDOWS_MODE_DIRECTORY
        ? WF_WINDOWS_OPEN_IS_DIRECTORY
        : WF_WINDOWS_OPEN_OTHER_KIND;
}

static int wf_windows_bounded_wcslen(
    const uint16_t *text,
    size_t *unit_count
) {
    size_t index;
    size_t maximum = (size_t)USHRT_MAX / sizeof(uint16_t);
    if (text == NULL || unit_count == NULL) {
        return 0;
    }
    for (index = 0; index <= maximum; ++index) {
        if (text[index] == 0) {
            *unit_count = index;
            return 1;
        }
    }
    return 0;
}

uint64_t wf__windows_wcslen(const uint16_t *text) {
    uint64_t length = 0;
    if (text == NULL) {
        return 0;
    }
    while (text[length] != 0) {
        length += 1;
    }
    return length;
}

int wf__windows_utf8_measure(
    const uint16_t *text,
    uint64_t unit_count,
    uint64_t *encoded_length
) {
    uint64_t index = 0;
    uint64_t measured = 0;
    if (encoded_length == NULL || (text == NULL && unit_count != 0)) {
        return 0;
    }
    while (index < unit_count) {
        uint16_t unit = text[index];
        uint64_t addition;
        if (unit >= UINT16_C(0xd800) && unit <= UINT16_C(0xdbff)) {
            uint16_t low;
            if (index + 1 >= unit_count) {
                return 0;
            }
            low = text[index + 1];
            if (low < UINT16_C(0xdc00) || low > UINT16_C(0xdfff)) {
                return 0;
            }
            addition = 4;
            index += 2;
        } else {
            if (unit >= UINT16_C(0xdc00) && unit <= UINT16_C(0xdfff)) {
                return 0;
            }
            addition = unit <= UINT16_C(0x007f)
                ? 1
                : (unit <= UINT16_C(0x07ff) ? 2 : 3);
            index += 1;
        }
        if (measured > UINT64_MAX - addition) {
            return 0;
        }
        measured += addition;
    }
    *encoded_length = measured;
    return 1;
}

int wf__windows_utf8_copy(
    const uint16_t *text,
    uint64_t unit_count,
    uint8_t *destination,
    uint64_t destination_capacity
) {
    uint64_t required;
    uint64_t source = 0;
    uint64_t target = 0;
    if (!wf__windows_utf8_measure(text, unit_count, &required)
        || required > destination_capacity
        || (destination == NULL && required != 0)) {
        return 0;
    }
    while (source < unit_count) {
        uint32_t scalar = text[source];
        if (scalar >= UINT32_C(0xd800) && scalar <= UINT32_C(0xdbff)) {
            uint32_t low = text[source + 1];
            scalar = UINT32_C(0x10000)
                + ((scalar - UINT32_C(0xd800)) << 10)
                + (low - UINT32_C(0xdc00));
            source += 2;
        } else {
            source += 1;
        }
        if (scalar <= UINT32_C(0x7f)) {
            destination[target] = (uint8_t)scalar;
            target += 1;
        } else if (scalar <= UINT32_C(0x7ff)) {
            destination[target] = (uint8_t)(UINT32_C(0xc0) | (scalar >> 6));
            destination[target + 1] =
                (uint8_t)(UINT32_C(0x80) | (scalar & UINT32_C(0x3f)));
            target += 2;
        } else if (scalar <= UINT32_C(0xffff)) {
            destination[target] = (uint8_t)(UINT32_C(0xe0) | (scalar >> 12));
            destination[target + 1] = (uint8_t)(
                UINT32_C(0x80) | ((scalar >> 6) & UINT32_C(0x3f))
            );
            destination[target + 2] =
                (uint8_t)(UINT32_C(0x80) | (scalar & UINT32_C(0x3f)));
            target += 3;
        } else {
            destination[target] = (uint8_t)(UINT32_C(0xf0) | (scalar >> 18));
            destination[target + 1] = (uint8_t)(
                UINT32_C(0x80) | ((scalar >> 12) & UINT32_C(0x3f))
            );
            destination[target + 2] = (uint8_t)(
                UINT32_C(0x80) | ((scalar >> 6) & UINT32_C(0x3f))
            );
            destination[target + 3] =
                (uint8_t)(UINT32_C(0x80) | (scalar & UINT32_C(0x3f)));
            target += 4;
        }
    }
    return target == required ? 1 : 0;
}

int wf__windows_relative_path_valid(
    const uint16_t *text,
    uint64_t unit_count
) {
    uint64_t index;
    if (unit_count == 0) {
        return 1;
    }
    if (text == NULL) {
        return 0;
    }
    if (text[0] == UINT16_C('/') || text[0] == UINT16_C('\\')) {
        return 0;
    }
    if (unit_count >= 2
        && ((text[0] >= UINT16_C('A') && text[0] <= UINT16_C('Z'))
            || (text[0] >= UINT16_C('a') && text[0] <= UINT16_C('z')))
        && text[1] == UINT16_C(':')) {
        return 0;
    }
    for (index = 0; index < unit_count; ++index) {
        if (text[index] == 0) {
            return 0;
        }
    }
    return 1;
}

int *wf__windows_error_location(void) {
    return &wf_windows_error_code;
}

int wf__windows_open_cwd(const void *unused_path, int flags, ...) {
    HANDLE handle;
    int descriptor;
    (void)unused_path;
    if (flags != 0) {
        wf_windows_record_error(ERROR_INVALID_PARAMETER);
        return -1;
    }
    handle = CreateFileW(
        L".",
        FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES
            | SYNCHRONIZE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        NULL,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS,
        NULL
    );
    if (handle == INVALID_HANDLE_VALUE) {
        wf_windows_record_error(GetLastError());
        return -1;
    }
    descriptor = wf_windows_adopt_handle(handle, _O_RDONLY | _O_BINARY);
    if (descriptor >= 0
        && wf__windows_completion_register_descriptor(
               descriptor,
               WF_WINDOWS_DESCRIPTOR_CLASS_DIRECTORY_ROOT
           ) != 0) {
        if (_close(descriptor) != 0) {
            wf_windows_fail(
                "a directory root descriptor could not be closed after its registration failed"
            );
        }
        wf_windows_fail(
            "a directory root descriptor could not be registered"
        );
    }
    return descriptor;
}

static int wf_windows_duplicate_descriptor(
    int descriptor,
    unsigned descriptor_class
) {
    int duplicate;
    int saved_errno;
    errno = 0;
    duplicate = _dup(descriptor);
    saved_errno = errno;
    if (duplicate < 0) {
        wf_windows_record_error(wf_windows_error_from_errno(saved_errno));
    } else if (wf__windows_completion_register_descriptor(
                   duplicate,
                   descriptor_class
               ) != 0) {
        if (_close(duplicate) != 0) {
            wf_windows_fail(
                "a standard descriptor could not be closed after its registration failed"
            );
        }
        wf_windows_fail(
            "a standard descriptor could not be registered"
        );
    }
    return duplicate;
}

int wf__windows_stdout_descriptor(void) {
    return wf_windows_duplicate_descriptor(1, WF_WINDOWS_DESCRIPTOR_CLASS_OUTPUT);
}

int wf__windows_stderr_descriptor(void) {
    return wf_windows_duplicate_descriptor(2, WF_WINDOWS_DESCRIPTOR_CLASS_OUTPUT);
}

/* The (ordinary native library) standard input stream.
 *
 * It is registered under its own class, not the output one, because the two
 * are read and written by different request kinds and the registry's class is
 * what a leaf checks before it uses a descriptor. A console handle has no
 * overlapped form and a redirected pipe or file does, but neither difference
 * is decided here: this entry duplicates and registers the descriptor, and
 * `file_windows.c` decides per request how to read it. */
int wf__windows_stdin_descriptor(void) {
    return wf_windows_duplicate_descriptor(0, WF_WINDOWS_DESCRIPTOR_CLASS_INPUT);
}

int64_t wf__windows_diagnostic_write(const void *bytes, uint64_t length) {
    HANDLE handle;
    DWORD written = 0;
    if (length == 0) {
        return 0;
    }
    if (bytes == NULL || length > (uint64_t)MAXDWORD) {
        wf_windows_record_error(ERROR_INVALID_PARAMETER);
        return -1;
    }
    handle = GetStdHandle(STD_ERROR_HANDLE);
    if (handle == NULL || handle == INVALID_HANDLE_VALUE) {
        DWORD error = GetLastError();
        wf_windows_record_error(
            error == ERROR_SUCCESS ? ERROR_INVALID_HANDLE : error
        );
        return -1;
    }
    if (WriteFile(handle, bytes, (DWORD)length, &written, NULL) == FALSE) {
        wf_windows_record_error(GetLastError());
        return -1;
    }
    return (int64_t)written;
}

static void wf_windows_open_return_provisional_handle(HANDLE *handle) {
    if (handle == NULL || !wf_windows_handle_valid(*handle)) {
        return;
    }
    if (!wf_windows_close_provisional_handle(handle)) {
        wf_windows_fail(
            "an open could not return its provisional handle"
        );
    }
}

int wf__windows_completion_file_open_at_worker(
    HANDLE root,
    const char *path,
    int flags,
    unsigned mode,
    unsigned has_mode,
    unsigned expected_kind,
    unsigned descriptor_class,
    int *error_code,
    unsigned *open_outcome
) {
    const uint16_t *units = (const uint16_t *)(const void *)path;
    wf_windows_nt_api api;
    HANDLE opened = INVALID_HANDLE_VALUE;
    size_t unit_count;
    UNICODE_STRING name;
    OBJECT_ATTRIBUTES attributes;
    IO_STATUS_BLOCK io_status;
    FILE_ATTRIBUTE_TAG_INFO tag_information;
    ACCESS_MASK desired_access;
    ULONG create_options = 0;
    NTSTATUS status;
    DWORD wait_result;
    unsigned outcome;
    int descriptor;
    (void)mode;

    if (error_code != NULL) {
        *error_code = 0;
    }
    if (open_outcome != NULL) {
        *open_outcome = WF_WINDOWS_OPEN_FAILED;
    }
    if (path == NULL || error_code == NULL || open_outcome == NULL
        || (flags != 0 && flags != WF_WINDOWS_NO_FOLLOW)
        || has_mode > 1u || has_mode != 0u
        || expected_kind > WF_WINDOWS_EXPECT_DIRECTORY
        || !wf_windows_handle_valid(root)
        || (expected_kind == WF_WINDOWS_EXPECT_REGULAR
            && descriptor_class
                != WF_WINDOWS_DESCRIPTOR_CLASS_READ_FILE
            && descriptor_class
                != WF_WINDOWS_DESCRIPTOR_CLASS_WRITE_FILE)
        || (expected_kind == WF_WINDOWS_EXPECT_DIRECTORY
            && descriptor_class
                != WF_WINDOWS_DESCRIPTOR_CLASS_DIRECTORY_ROOT
            && descriptor_class
                != WF_WINDOWS_DESCRIPTOR_CLASS_DIRECTORY_SOURCE)) {
        wf_windows_record_error(ERROR_INVALID_PARAMETER);
        if (error_code != NULL) {
            *error_code = ERROR_INVALID_PARAMETER;
        }
        return -1;
    }
    if (!wf_windows_bounded_wcslen(units, &unit_count)) {
        wf_windows_record_error(ERROR_FILENAME_EXCED_RANGE);
        *error_code = ERROR_FILENAME_EXCED_RANGE;
        return -1;
    }
    if (!wf__windows_relative_path_valid(units, (uint64_t)unit_count)) {
        wf_windows_record_error(ERROR_INVALID_NAME);
        *error_code = ERROR_INVALID_NAME;
        return -1;
    }
    if (!wf_windows_resolve_nt_api(&api)) {
        DWORD error = GetLastError();
        error = error == ERROR_SUCCESS ? ERROR_PROC_NOT_FOUND : error;
        wf_windows_record_error(error);
        *error_code = (int)error;
        return -1;
    }

    memset(&name, 0, sizeof(name));
    name.Length = (USHORT)(unit_count * sizeof(uint16_t));
    name.MaximumLength = name.Length;
    name.Buffer = (PWSTR)(void *)units;
    memset(&attributes, 0, sizeof(attributes));
    attributes.Length = (ULONG)sizeof(attributes);
    attributes.RootDirectory = root;
    attributes.ObjectName = &name;
    attributes.Attributes = OBJ_CASE_INSENSITIVE;
    memset(&io_status, 0, sizeof(io_status));

    desired_access = FILE_READ_ATTRIBUTES | SYNCHRONIZE;
    if (descriptor_class == WF_WINDOWS_DESCRIPTOR_CLASS_WRITE_FILE) {
        desired_access |= FILE_APPEND_DATA;
        create_options |= FILE_SYNCHRONOUS_IO_NONALERT | FILE_NON_DIRECTORY_FILE;
    } else if (expected_kind == WF_WINDOWS_EXPECT_REGULAR) {
        desired_access |= FILE_READ_DATA;
    } else if (expected_kind == WF_WINDOWS_EXPECT_DIRECTORY) {
        desired_access |= FILE_LIST_DIRECTORY | FILE_TRAVERSE;
        create_options |= FILE_SYNCHRONOUS_IO_NONALERT;
    }
    if (flags == WF_WINDOWS_NO_FOLLOW) {
        create_options |= FILE_OPEN_REPARSE_POINT;
    }
    status = api.create_file(
        &opened,
        desired_access,
        &attributes,
        &io_status,
        NULL,
        0,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        descriptor_class == WF_WINDOWS_DESCRIPTOR_CLASS_WRITE_FILE ? FILE_OPEN_IF : FILE_OPEN,
        create_options,
        NULL,
        0
    );
    if (status == WF_WINDOWS_STATUS_PENDING) {
        if (!wf_windows_handle_valid(opened)) {
            wf_windows_record_error(ERROR_INVALID_HANDLE);
            *error_code = ERROR_INVALID_HANDLE;
            return -1;
        }
        wait_result = WaitForSingleObject(opened, INFINITE);
        if (wait_result != WAIT_OBJECT_0) {
            DWORD error = wait_result == WAIT_FAILED
                ? GetLastError()
                : ERROR_GEN_FAILURE;
            if (error == ERROR_SUCCESS) {
                error = ERROR_GEN_FAILURE;
            }
            wf_windows_open_return_provisional_handle(&opened);
            wf_windows_record_error(error);
            *error_code = (int)error;
            return -1;
        }
        status = io_status.Status;
    }
    if (status == WF_WINDOWS_STATUS_PENDING
        || !wf_windows_nt_success(status)) {
        DWORD error = wf_windows_nt_error(&api, status);
        wf_windows_open_return_provisional_handle(&opened);
        wf_windows_record_error(error);
        *error_code = (int)error;
        return -1;
    }
    if (!wf_windows_handle_valid(opened)) {
        wf_windows_record_error(ERROR_INVALID_HANDLE);
        *error_code = ERROR_INVALID_HANDLE;
        return -1;
    }
    if (GetFileInformationByHandleEx(
            opened,
            FileAttributeTagInfo,
            &tag_information,
            (DWORD)sizeof(tag_information)
        ) == FALSE) {
        DWORD error = GetLastError();
        wf_windows_open_return_provisional_handle(&opened);
        wf_windows_record_error(error);
        *error_code = (int)error;
        *open_outcome = WF_WINDOWS_OPEN_STATUS_FAILED;
        return -1;
    }
    outcome = wf_windows_kind_outcome(
        expected_kind,
        tag_information.FileAttributes
    );
    if (outcome != WF_WINDOWS_OPEN_SUCCEEDED) {
        wf_windows_open_return_provisional_handle(&opened);
        *open_outcome = outcome;
        return -1;
    }
    descriptor = wf_windows_adopt_handle(
        opened,
        descriptor_class == WF_WINDOWS_DESCRIPTOR_CLASS_WRITE_FILE
            ? _O_WRONLY | _O_APPEND | _O_BINARY
            : _O_RDONLY | _O_BINARY
    );
    if (descriptor < 0) {
        *error_code = wf_windows_error_code;
        return -1;
    }
    {
        /* An open records the descriptor's class in this runtime's own table
         * and stops there.  It does not reach the completion port, and this
         * layering is not a detail: a program that submits nothing links this
         * unit and the floor and no completion runtime at all, so a call from
         * here into the bridge would be an unresolved symbol in every such
         * link.  The port association is the ring's own and happens where the
         * ring already does it -- lazily, at the first offer of a record on
         * this descriptor, in `completion/bridge.c`'s
         * `wf_bridge_windows_port_handle`, which is also the only place that
         * knows whether this run has a ring to associate with. */
        int registration_error = wf__windows_completion_register_descriptor(
            descriptor,
            descriptor_class
        );
        if (registration_error != 0) {
            (void)_close(descriptor);
            /* Registry storage is an execution resource introduced by this
             * backend. It cannot become a source-level open failure, and there
             * is no permitted synchronous fallback. */
            wf_windows_fail(
                "an opened descriptor could not be registered, and an open has no permitted synchronous fallback"
            );
        }
    }
    *open_outcome = WF_WINDOWS_OPEN_SUCCEEDED;
    return descriptor;
}

int64_t wf__windows_completion_file_write_worker(
    HANDLE handle,
    const void *buffer,
    uint64_t count
) {
    DWORD written = 0;
    if (count == 0) {
        return 0;
    }
    if (buffer == NULL || count > (uint64_t)MAXDWORD) {
        wf_windows_record_error(ERROR_INVALID_PARAMETER);
        return -1;
    }
    if (WriteFile(handle, buffer, (DWORD)count, &written, NULL) == FALSE) {
        wf_windows_record_error(GetLastError());
        return -1;
    }
    return (int64_t)written;
}

static int wf_windows_directory_record_valid(
    const unsigned char *record_bytes,
    size_t remaining,
    size_t *native_extent,
    size_t *raw_extent
) {
    const wf_windows_file_directory_information *record;
    size_t minimum;
    size_t name_bytes;
    size_t index;
    if (remaining < WF_WINDOWS_DIRECTORY_NATIVE_HEADER
        || record_bytes == NULL || native_extent == NULL
        || raw_extent == NULL) {
        return 0;
    }
    record = (const wf_windows_file_directory_information *)record_bytes;
    name_bytes = (size_t)record->file_name_length;
    if (name_bytes == 0 || name_bytes > WF_WINDOWS_COMPONENT_MAX_BYTES
        || (name_bytes & 1u) != 0
        || name_bytes > remaining - WF_WINDOWS_DIRECTORY_NATIVE_HEADER) {
        return 0;
    }
    minimum = WF_WINDOWS_DIRECTORY_NATIVE_HEADER + name_bytes;
    if (record->next_entry_offset == 0) {
        *native_extent = remaining;
    } else {
        *native_extent = (size_t)record->next_entry_offset;
        if (*native_extent < minimum || *native_extent > remaining
            || (*native_extent & (sizeof(ULONG_PTR) - 1u)) != 0) {
            return 0;
        }
    }
    for (index = 0; index < name_bytes; index += sizeof(uint16_t)) {
        uint16_t unit;
        memcpy(
            &unit,
            record_bytes + WF_WINDOWS_DIRECTORY_NATIVE_HEADER + index,
            sizeof(unit)
        );
        if (unit == 0 || unit == UINT16_C('/') || unit == UINT16_C('\\')) {
            return 0;
        }
    }
    *raw_extent = WF_WINDOWS_DIRECTORY_RAW_HEADER + name_bytes;
    return *raw_extent <= (size_t)USHRT_MAX;
}

static uint8_t wf_windows_directory_kind(DWORD attributes) {
    if ((attributes & FILE_ATTRIBUTE_REPARSE_POINT) != 0) {
        return UINT8_C(3);
    }
    if ((attributes & FILE_ATTRIBUTE_DIRECTORY) != 0) {
        return UINT8_C(2);
    }
    if ((attributes & FILE_ATTRIBUTE_DEVICE) != 0) {
        return UINT8_C(0);
    }
    return UINT8_C(1);
}

static int64_t wf_windows_directory_batch_worker(
    HANDLE directory,
    void *buffer,
    uint64_t count,
    int64_t *position
) {
    wf_windows_nt_api api;
    wf_windows_directory_scratch scratch;
    HANDLE event;
    IO_STATUS_BLOCK io_status;
    ULONG query_capacity;
    NTSTATUS status;
    size_t transferred;
    size_t source;
    size_t required;
    unsigned char *destination = (unsigned char *)buffer;
    (void)position;

    if (position == NULL || (buffer == NULL && count != 0)) {
        wf_windows_record_error(ERROR_INVALID_PARAMETER);
        return -1;
    }
    if (count == 0) {
        return 0;
    }
    if (count < WF_WINDOWS_DIRECTORY_NATIVE_HEADER
            + WF_WINDOWS_COMPONENT_MAX_BYTES
        || count > (uint64_t)INT64_MAX) {
        wf_windows_record_error(
            count > (uint64_t)INT64_MAX
                ? ERROR_INVALID_PARAMETER
                : ERROR_INSUFFICIENT_BUFFER
        );
        return -1;
    }
    if (!wf_windows_resolve_nt_api(&api)) {
        DWORD error = GetLastError();
        wf_windows_record_error(
            error == ERROR_SUCCESS ? ERROR_PROC_NOT_FOUND : error
        );
        return -1;
    }
    query_capacity = count < WF_WINDOWS_DIRECTORY_SCRATCH_BYTES
        ? (ULONG)count
        : (ULONG)WF_WINDOWS_DIRECTORY_SCRATCH_BYTES;
    event = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (event == NULL) {
        wf_windows_record_error(GetLastError());
        return -1;
    }
    memset(&io_status, 0, sizeof(io_status));
    status = api.query_directory_file(
        directory,
        event,
        NULL,
        NULL,
        &io_status,
        scratch.bytes,
        query_capacity,
        WF_WINDOWS_FILE_DIRECTORY_INFORMATION_CLASS,
        FALSE,
        NULL,
        FALSE
    );
    if (status == WF_WINDOWS_STATUS_PENDING) {
        if (WaitForSingleObject(event, INFINITE) != WAIT_OBJECT_0) {
            DWORD error = GetLastError();
            (void)CloseHandle(event);
            wf_windows_record_error(
                error == ERROR_SUCCESS ? ERROR_GEN_FAILURE : error
            );
            return -1;
        }
        status = io_status.Status;
    }
    (void)CloseHandle(event);
    if (status == WF_WINDOWS_STATUS_NO_MORE_FILES) {
        return 0;
    }
    if (!wf_windows_nt_success(status)) {
        wf_windows_record_error(wf_windows_nt_error(&api, status));
        return -1;
    }
    transferred = (size_t)io_status.Information;
    if (transferred == 0) {
        return 0;
    }
    if (transferred > (size_t)query_capacity) {
        wf_windows_record_error(ERROR_INVALID_DATA);
        return -1;
    }

    source = 0;
    required = 0;
    for (;;) {
        size_t native_extent;
        size_t raw_extent;
        const wf_windows_file_directory_information *record;
        if (!wf_windows_directory_record_valid(
                scratch.bytes + source,
                transferred - source,
                &native_extent,
                &raw_extent
            ) || required > (size_t)count - raw_extent) {
            wf_windows_record_error(ERROR_INVALID_DATA);
            return -1;
        }
        record = (const wf_windows_file_directory_information *)(
            scratch.bytes + source
        );
        required += raw_extent;
        if (record->next_entry_offset == 0) {
            break;
        }
        source += native_extent;
    }

    source = 0;
    required = 0;
    for (;;) {
        const wf_windows_file_directory_information *record =
            (const wf_windows_file_directory_information *)(
                scratch.bytes + source
            );
        size_t native_extent;
        size_t raw_extent;
        size_t name_bytes = (size_t)record->file_name_length;
        size_t index;
        (void)wf_windows_directory_record_valid(
            scratch.bytes + source,
            transferred - source,
            &native_extent,
            &raw_extent
        );
        destination[required] = (uint8_t)(raw_extent & 0xffu);
        destination[required + 1] = (uint8_t)((raw_extent >> 8) & 0xffu);
        destination[required + 2] = (uint8_t)(name_bytes & 0xffu);
        destination[required + 3] =
            (uint8_t)((name_bytes >> 8) & 0xffu);
        destination[required + 4] = wf_windows_directory_kind(
            record->file_attributes
        );
        for (index = 0; index < name_bytes; index += sizeof(uint16_t)) {
            uint16_t unit;
            memcpy(
                &unit,
                scratch.bytes + source
                    + WF_WINDOWS_DIRECTORY_NATIVE_HEADER + index,
                sizeof(unit)
            );
            destination[required + WF_WINDOWS_DIRECTORY_RAW_HEADER + index] =
                (uint8_t)(unit & UINT16_C(0xff));
            destination[
                required + WF_WINDOWS_DIRECTORY_RAW_HEADER + index + 1
            ] = (uint8_t)(unit >> 8);
        }
        required += raw_extent;
        if (record->next_entry_offset == 0) {
            break;
        }
        source += native_extent;
    }
    return (int64_t)required;
}

int64_t wf__windows_completion_directory_next_worker(
    HANDLE directory,
    void *buffer,
    uint64_t count,
    int64_t *position
) {
    return wf_windows_directory_batch_worker(
        directory,
        buffer,
        count,
        position
    );
}

int64_t wf__windows_directory_batch(
    int descriptor,
    void *buffer,
    uint64_t count,
    int64_t *position
) {
    unsigned descriptor_class = WF_WINDOWS_DESCRIPTOR_CLASS_ANY;
    HANDLE handle;
    if (count == 0) {
        return 0;
    }
    handle = wf__windows_completion_descriptor_handle(descriptor);
    if (!wf_windows_handle_valid(handle)
        || !wf__windows_completion_descriptor_state(
               descriptor,
               &descriptor_class,
               NULL
           )
        || descriptor_class != WF_WINDOWS_DESCRIPTOR_CLASS_DIRECTORY_SOURCE) {
        wf_windows_record_error(ERROR_INVALID_HANDLE);
        return -1;
    }
    return wf__windows_completion_directory_next_worker(
        handle,
        buffer,
        count,
        position
    );
}
