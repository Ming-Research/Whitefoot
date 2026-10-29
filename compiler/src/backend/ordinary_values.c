/* Ordinary prelude implementations, linked using the same symbols and ABI as
 * Whitefoot function definitions. No compiler operation identity reaches this
 * unit. Opaque contents are this library's private representation. */
#if defined(__APPLE__)
#ifndef _DARWIN_C_SOURCE
#define _DARWIN_C_SOURCE 1
#endif
#elif !defined(_WIN32)
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#endif
#include "ordinary_values.h"
#include "concurrent_map.h"
#include "completion/bridge.h"
#include "completion/contract.h"

#include <errno.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#include "windows_runtime.h"
#include <io.h>
#include <shellapi.h>
#include <time.h>
#else
#include <fcntl.h>
#include <locale.h>
#include <signal.h>
#include <time.h>
#include <unistd.h>
#if defined(__APPLE__)
#include <xlocale.h>
#endif
#endif

static const void *wf_value_pointer(const wf_value *value) {
    return (const void *)(uintptr_t)value->words[0];
}

/* An empty ordinary view may have a null data pointer. Keeping its zero
 * endpoint unchanged avoids performing C pointer arithmetic on that null. */
static unsigned char *wf_window(const wf_view *view, uint64_t start) {
    unsigned char *data = view->data;
    return start == 0 ? data : data + start;
}

static void wf_text(wf_value *value, const void *text, uint64_t units) {
    memset(value, 0, sizeof(*value));
    value->words[0] = (uint64_t)(uintptr_t)text;
    value->words[1] = units;
}

uint64_t wf__body_args_count(const wf_value *args) { return args->words[1]; }

void wf__body_arg_get(wf_value_result *result, const wf_value *args, uint64_t position) {
    const void *const *arguments = wf_value_pointer(args);
    memset(result, 0, sizeof(*result));
    if (position >= args->words[1]) {
        result->tag = 1;
        return;
    }
#if defined(_WIN32)
    wf_text(&result->ok.value, arguments[position],
            wf__windows_wcslen(arguments[position]));
#else
    wf_text(&result->ok.value, arguments[position], strlen(arguments[position]));
#endif
}

uint64_t wf__body_host_bytes_len(const wf_value *value) {
#if defined(_WIN32)
    return value->words[1] * UINT64_C(2);
#else
    return value->words[1];
#endif
}

void wf__body_host_copy_bytes(wf_copy_result *result, const wf_value *value,
                       wf_view *destination, uint64_t start, uint64_t end) {
    const uint64_t bytes = wf__body_host_bytes_len(value);
    memset(result, 0, sizeof(*result));
    if (bytes > end - start) {
        result->tag = 1;
        result->err.error.required = bytes;
        return;
    }
    if (bytes != 0) {
        memcpy(wf_window(destination, start),
               wf_value_pointer(value), (size_t)bytes);
    }
    result->ok.value = start + bytes;
}

/* The name bytes of a host string [PRE-2]: its bytes on POSIX, and on Windows
 * the WTF-8 spelling of its UTF-16 code units, which loses none of them. */
void wf__body_host_copy_name_bytes(wf_copy_result *result, const wf_value *value,
                                   wf_view *destination, uint64_t start, uint64_t end) {
#if defined(_WIN32)
    const unsigned char *units = wf_value_pointer(value);
    const uint64_t bytes = wf__windows_wtf8_write(units, (size_t)value->words[1], NULL);
    memset(result, 0, sizeof(*result));
    if (bytes > end - start) {
        result->tag = 1;
        result->err.error.required = bytes;
        return;
    }
    if (bytes != 0)
        (void)wf__windows_wtf8_write(units, (size_t)value->words[1],
                                     wf_window(destination, start));
    result->ok.value = start + bytes;
#else
    wf__body_host_copy_bytes(result, value, destination, start, end);
#endif
}

/* The XSI entry writes into caller storage and reports EINVAL for an unknown
 * code. In contrast, glibc's strerror_l unknown-code path allocates a hidden
 * thread-local string. _GNU_SOURCE selects the GNU strerror_r declaration,
 * so name glibc's allocation-free XSI entry here. */
#if !defined(_WIN32) && defined(__GLIBC__)
extern int __xpg_strerror_r(int code, char *buffer, size_t length);
#endif

static void wf_description_copy(wf_copy_result *result, const char *text,
                                size_t length, wf_view *destination,
                                uint64_t start, uint64_t end) {
    if (length > end - start) {
        result->tag = 1;
        result->err.error.tag = 0;
        result->err.error.required = length;
        return;
    }
    if (length != 0) memcpy(wf_window(destination, start), text, length);
    result->tag = 0;
    result->ok.value = start + length;
}

static void wf_host_code_description(wf_copy_result *result, uint32_t code,
                                     wf_view *destination,
                                     uint64_t start, uint64_t end);

void wf__body_error_copy_description(wf_copy_result *result,
                                     const wf_io_error *error,
                                     wf_view *destination,
                                     uint64_t start, uint64_t end) {
    static const char *const fixed[] = {
        "Not found", "Permission denied", "Already exists", "Not a directory",
        "Is a directory", "Directory not empty", "Read-only resource",
        "Resource busy", "Invalid input", "Invalid path", "Unsupported operation",
        "Timed out", "Deadline passed", "Cancelled", "Broken pipe",
        "Write returned zero bytes", "Unexpected end of input", "Connection refused",
        "Connection reset", "Connection aborted", "Not connected", "Address in use",
        "Address unavailable", "Resource exhausted", "File too large", "No space left",
        "Quota exceeded", "Cross-device operation", "Device failure", "Other I/O error"
    };
    const uint32_t tag = error->tag;
    const uint32_t code = tag == WF_IO_DEADLINE_PASSED || tag == WF_IO_CANCELLED
        ? 0 : error->code;
    _Static_assert(sizeof(fixed) / sizeof(fixed[0]) == WF_IO_OTHER + 1,
                   "ordinary error description table");
    memset(result, 0, sizeof(*result));
    /* Every failure before the final copy leaves destination untouched. */
    result->tag = 1;
    result->err.error.tag = 1;
    if (tag > WF_IO_OTHER) return;
    if (code == 0) {
        wf_description_copy(result, fixed[tag], strlen(fixed[tag]),
                            destination, start, end);
        return;
    }
    wf_host_code_description(result, code, destination, start, end);
}

/* [PRE-2] each IoError class's code on this host, indexed by tag; zero is a
 * class with no code here, which has no description. */
static const uint32_t wf_class_codes[WF_IO_OTHER + 1] = {
#if defined(_WIN32)
    [WF_IO_NOT_FOUND] = ERROR_FILE_NOT_FOUND,
    [WF_IO_PERMISSION_DENIED] = ERROR_ACCESS_DENIED,
    [WF_IO_ALREADY_EXISTS] = ERROR_ALREADY_EXISTS,
    [WF_IO_NOT_DIRECTORY] = ERROR_DIRECTORY,
    [WF_IO_DIRECTORY_NOT_EMPTY] = ERROR_DIR_NOT_EMPTY,
    [WF_IO_READ_ONLY] = ERROR_WRITE_PROTECT,
    [WF_IO_RESOURCE_BUSY] = ERROR_BUSY,
    [WF_IO_INVALID_INPUT] = ERROR_INVALID_PARAMETER,
    [WF_IO_INVALID_PATH] = ERROR_INVALID_NAME,
    [WF_IO_UNSUPPORTED] = ERROR_NOT_SUPPORTED,
    [WF_IO_TIMED_OUT] = ERROR_SEM_TIMEOUT,
    [WF_IO_BROKEN_PIPE] = ERROR_BROKEN_PIPE,
    [WF_IO_CONNECTION_REFUSED] = ERROR_CONNECTION_REFUSED,
    [WF_IO_CONNECTION_RESET] = ERROR_NETNAME_DELETED,
    [WF_IO_CONNECTION_ABORTED] = ERROR_REQUEST_ABORTED,
    [WF_IO_NOT_CONNECTED] = ERROR_NOT_CONNECTED,
    [WF_IO_ADDRESS_IN_USE] = 10048, /* WSAEADDRINUSE. */
    [WF_IO_ADDRESS_UNAVAILABLE] = 10049, /* WSAEADDRNOTAVAIL. */
    [WF_IO_RESOURCE_EXHAUSTED] = ERROR_TOO_MANY_OPEN_FILES,
    [WF_IO_FILE_TOO_LARGE] = ERROR_FILE_TOO_LARGE,
    [WF_IO_NO_SPACE] = ERROR_DISK_FULL,
    [WF_IO_QUOTA_EXCEEDED] = ERROR_NOT_ENOUGH_QUOTA,
    [WF_IO_CROSS_DEVICE] = ERROR_NOT_SAME_DEVICE,
    [WF_IO_DEVICE_FAILURE] = ERROR_IO_DEVICE,
#else
    [WF_IO_NOT_FOUND] = ENOENT,
    [WF_IO_PERMISSION_DENIED] = EACCES,
    [WF_IO_ALREADY_EXISTS] = EEXIST,
    [WF_IO_NOT_DIRECTORY] = ENOTDIR,
    [WF_IO_IS_DIRECTORY] = EISDIR,
    [WF_IO_DIRECTORY_NOT_EMPTY] = ENOTEMPTY,
    [WF_IO_READ_ONLY] = EROFS,
    [WF_IO_RESOURCE_BUSY] = EBUSY,
    [WF_IO_INVALID_INPUT] = EINVAL,
    [WF_IO_INVALID_PATH] = ENAMETOOLONG,
    [WF_IO_UNSUPPORTED] = ENOTSUP,
    [WF_IO_TIMED_OUT] = ETIMEDOUT,
    [WF_IO_BROKEN_PIPE] = EPIPE,
    [WF_IO_CONNECTION_REFUSED] = ECONNREFUSED,
    [WF_IO_CONNECTION_RESET] = ECONNRESET,
    [WF_IO_CONNECTION_ABORTED] = ECONNABORTED,
    [WF_IO_NOT_CONNECTED] = ENOTCONN,
    [WF_IO_ADDRESS_IN_USE] = EADDRINUSE,
    [WF_IO_ADDRESS_UNAVAILABLE] = EADDRNOTAVAIL,
    [WF_IO_RESOURCE_EXHAUSTED] = EMFILE,
    [WF_IO_FILE_TOO_LARGE] = EFBIG,
    [WF_IO_NO_SPACE] = ENOSPC,
#ifdef EDQUOT
    [WF_IO_QUOTA_EXCEEDED] = EDQUOT,
#endif
    [WF_IO_CROSS_DEVICE] = EXDEV,
    [WF_IO_DEVICE_FAILURE] = EIO,
#endif
};

/* The class's description is the host text of its code, the same text
 * error_copy_description gives a variant carrying that code; the error's own
 * code and origin are not read. */
void wf__body_error_copy_class_description(wf_copy_result *result,
                                           const wf_io_error *error,
                                           wf_view *destination,
                                           uint64_t start, uint64_t end) {
    memset(result, 0, sizeof(*result));
    result->tag = 1;
    result->err.error.tag = 1;
    if (error->tag > WF_IO_OTHER) return;
    const uint32_t code = wf_class_codes[error->tag];
    if (code == 0) return;
    wf_host_code_description(result, code, destination, start, end);
}

/* The host's message for a nonzero code, copied under the description window
 * contract. The caller has set result to DescriptionUnavailable, which every
 * failure before the final copy keeps. */
static void wf_host_code_description(wf_copy_result *result, uint32_t code,
                                     wf_view *destination,
                                     uint64_t start, uint64_t end) {
#if defined(_WIN32)
    WCHAR local_wide[128];
    WCHAR *wide = local_wide;
    DWORD capacity = (DWORD)(sizeof(local_wide) / sizeof(local_wide[0]));
    DWORD units;
    for (;;) {
        units = FormatMessageW(FORMAT_MESSAGE_FROM_SYSTEM |
                               FORMAT_MESSAGE_IGNORE_INSERTS,
                               NULL, code, MAKELANGID(LANG_ENGLISH, SUBLANG_ENGLISH_US),
                               wide, capacity, NULL);
        if (units != 0) break;
        const DWORD status = GetLastError();
        if (wide != local_wide)
            wf__runtime_give(wide, (uint64_t)capacity * sizeof(*wide));
        if (status != ERROR_INSUFFICIENT_BUFFER || capacity == (DWORD)INT_MAX)
            return;
        /* Format into caller-owned storage so the runtime pool charges every
         * temporary block. Discard each refused buffer before growing it. The
         * conversion API's signed length is the largest useful wide extent. */
        capacity = capacity <= (DWORD)INT_MAX / 2
            ? capacity * 2 : (DWORD)INT_MAX;
        wide = wf__runtime_take((uint64_t)capacity * sizeof(*wide));
    }
    while (units != 0 && (wide[units - 1] == L'\r' || wide[units - 1] == L'\n'))
        units--;
    if (units > INT_MAX) {
        if (wide != local_wide)
            wf__runtime_give(wide, (uint64_t)capacity * sizeof(*wide));
        return;
    }
    if (units == 0) {
        if (wide != local_wide)
            wf__runtime_give(wide, (uint64_t)capacity * sizeof(*wide));
        wf_description_copy(result, "", 0, destination, start, end);
        return;
    }
    int bytes = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide,
                                   (int)units, NULL, 0, NULL, NULL);
    if (bytes <= 0) {
        if (wide != local_wide)
            wf__runtime_give(wide, (uint64_t)capacity * sizeof(*wide));
        return;
    }
    if ((uint64_t)bytes > end - start) {
        result->err.error.tag = 0;
        result->err.error.required = (uint64_t)bytes;
    } else {
        char *text = wf__runtime_take((uint64_t)bytes);
        /* Convert privately: even an unexpected conversion failure must not
         * write part of the description into the caller's destination. */
        if (WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide,
                                (int)units, text, bytes, NULL, NULL) == bytes)
            wf_description_copy(result, text, (size_t)bytes, destination, start, end);
        wf__runtime_give(text, (uint64_t)bytes);
    }
    if (wide != local_wide)
        wf__runtime_give(wide, (uint64_t)capacity * sizeof(*wide));
#else
    if (code > INT_MAX) return;
#if defined(__APPLE__)
    /* Darwin exports an immutable C locale object. Borrow it directly rather
     * than asking newlocale to allocate a host-owned locale structure. */
    locale_t c_locale = _c_locale;
#else
    /* glibc returns its static immutable C object for this exact request;
     * neither newlocale here nor the matching freelocale allocates. */
    locale_t c_locale = newlocale(LC_ALL_MASK, "C", (locale_t)0);
    if (c_locale == (locale_t)0) return;
#endif
    /* Lookup into caller-owned storage first, so unknown-code formatting
     * cannot allocate an uncounted host buffer. A complete scratch result is
     * required: ERANGE may have written a truncated string. */
    char local[128];
    char *text = local;
    size_t capacity = sizeof(local);
    locale_t previous = uselocale(c_locale);
    if (previous != (locale_t)0) {
        int status;
        for (;;) {
#if defined(__GLIBC__)
            status = __xpg_strerror_r((int)code, text, capacity);
#else
            status = strerror_r((int)code, text, capacity);
#endif
            if (status != ERANGE) break;
            if (capacity > SIZE_MAX / 2) wf__runtime_exhausted();
            size_t next_capacity = capacity * 2;
            char *expanded = wf__runtime_take(next_capacity);
            if (text != local) wf__runtime_give(text, capacity);
            text = expanded;
            capacity = next_capacity;
        }
        (void)uselocale(previous);
        if (status == 0)
            wf_description_copy(result, text, strlen(text), destination, start, end);
        if (text != local) wf__runtime_give(text, capacity);
    }
#if !defined(__APPLE__)
    freelocale(c_locale);
#endif
#endif
}

/* UTF-8 validation examines the invocation backing or a window in place.
 * With surrogates set it validates WTF-8 instead: UTF-8 that also spells a
 * lone UTF-16 surrogate code unit as the three bytes ED A0..BF xx, while a
 * lead surrogate followed by a trail one must be spelled as their pair's
 * four-byte scalar, so each code unit sequence has exactly one spelling. */
static int wf_text_valid(const unsigned char *text, uint64_t length, int surrogates) {
    uint64_t at = 0;
    int after_lead = 0;
    while (at < length) {
        unsigned first = text[at++];
        unsigned continuation;
        unsigned lower = 0x80, upper = 0xbf;
        int lead = 0;
        if (first < 0x80) {
            after_lead = 0;
            continue;
        }
        if (first >= 0xc2 && first <= 0xdf) {
            continuation = 1;
        } else if (first >= 0xe0 && first <= 0xef) {
            continuation = 2;
            if (first == 0xe0) lower = 0xa0;
            if (first == 0xed && !surrogates) upper = 0x9f;
        } else if (first >= 0xf0 && first <= 0xf4) {
            continuation = 3;
            if (first == 0xf0) lower = 0x90;
            if (first == 0xf4) upper = 0x8f;
        } else return 0;
        if (length - at < continuation || text[at] < lower || text[at] > upper)
            return 0;
        if (first == 0xed && text[at] >= 0xa0) {
            if (text[at] >= 0xb0 && after_lead) return 0;
            lead = text[at] < 0xb0;
        }
        after_lead = lead;
        at++;
        while (--continuation != 0) {
            if (text[at] < 0x80 || text[at] > 0xbf) return 0;
            at++;
        }
    }
    return 1;
}

static int wf_utf8_valid(const unsigned char *text, uint64_t length) {
    return wf_text_valid(text, length, 0);
}

void wf__body_host_utf8_len(wf_utf8_result *result, const wf_value *value) {
    uint64_t length = value->words[1];
    memset(result, 0, sizeof(*result));
#if defined(_WIN32)
    if (!wf__windows_utf8_measure(wf_value_pointer(value), length, &length)) {
#else
    if (!wf_utf8_valid(wf_value_pointer(value), length)) {
#endif
        result->tag = 1;
        return;
    }
    result->value = length;
}

void wf__body_host_copy_utf8(wf_copy_result *result, const wf_value *value,
                      wf_view *destination, uint64_t start, uint64_t end) {
    wf_utf8_result measured;
    wf__body_host_utf8_len(&measured, value);
    memset(result, 0, sizeof(*result));
    if (measured.tag != 0) {
        result->tag = 1;
        result->err.error.tag = 1;
        return;
    }
    if (measured.value > end - start) {
        result->tag = 1;
        result->err.error.required = measured.value;
        return;
    }
    if (measured.value != 0) {
#if defined(_WIN32)
        (void)wf__windows_utf8_copy(wf_value_pointer(value), value->words[1],
            wf_window(destination, start), end - start);
#else
        memcpy(wf_window(destination, start),
               wf_value_pointer(value), (size_t)measured.value);
#endif
    }
    result->ok.value = start + measured.value;
}

void wf__body_relative_path(wf_value_result *result, const wf_value *value) {
    const wf_value saved = *value;
    const unsigned char *text = wf_value_pointer(&saved);
    uint64_t length = saved.words[1];
    memset(result, 0, sizeof(*result));
#if defined(_WIN32)
    if (!wf__windows_relative_path_valid((const uint16_t *)text, length)) {
#else
    if (length != 0 && (text[0] == '/' || memchr(text, 0, (size_t)length) != NULL)) {
#endif
        result->tag = 1;
        return;
    }
    result->ok.value = saved;
}

void wf__body_exit_status(wf_value *result, uint8_t code) {
    memset(result, 0, sizeof(*result));
    result->words[0] = code;
}

uint8_t wf__ordinary_exit_code(const wf_value *status) {
    return (uint8_t)status->words[0];
}

void wf__body_socket_address_v4(wf_value *result, uint8_t a, uint8_t b, uint8_t c,
                          uint8_t d, uint16_t port) {
    memset(result, 0, sizeof(*result));
    result->words[0] = (uint64_t)a | ((uint64_t)b << 8) |
        ((uint64_t)c << 16) | ((uint64_t)d << 24);
    result->words[2] = port;
}

void wf__body_socket_address_v6(wf_value *result, uint16_t a, uint16_t b, uint16_t c,
                          uint16_t d, uint16_t e, uint16_t f, uint16_t g,
                          uint16_t h, uint16_t port) {
    const uint16_t groups[8] = { a, b, c, d, e, f, g, h };
    unsigned index;
    memset(result, 0, sizeof(*result));
    for (index = 0; index < 8; index++) {
        unsigned shift = (index % 4) * 16;
        result->words[index / 4] |= ((uint64_t)(groups[index] >> 8) << shift)
            | ((uint64_t)(groups[index] & 255) << (shift + 8));
    }
    result->words[2] = (uint64_t)port | WF_SOCKET_FAMILY_V6;
}

/* Byte `index` of an address's sixteen, by the rule the two constructors
 * above write them with and `completion/contract.h` states: byte `i` occupies
 * bits `8 * (i % 8)` of word `i / 8`, the IPv4 octets are bytes 0 to 3 and an
 * IPv6 group is its high byte followed by its low byte. */
static unsigned wf_address_byte(const wf_value *address, unsigned index) {
    return (unsigned)((address->words[index / 8u] >> (8u * (index % 8u))) & 0xffu);
}

/* [PRE-2] the family and components the constructors took, read back by the
 * same layout rule, so every address reads as the call that built it. */
void wf__body_socket_address_ip(wf_ip_address *result, const wf_value *address) {
    unsigned index;
    memset(result, 0, sizeof(*result));
    if ((address->words[2] & WF_SOCKET_FAMILY_V6) != 0) {
        result->tag = WF_IP_ADDRESS_V6;
        for (index = 0; index < 8u; index++) {
            result->v6.groups[index] =
                (uint16_t)((wf_address_byte(address, 2u * index) << 8)
                           | wf_address_byte(address, 2u * index + 1u));
        }
    } else {
        result->tag = WF_IP_ADDRESS_V4;
        for (index = 0; index < 4u; index++) {
            result->v4.octets[index] = (uint8_t)wf_address_byte(address, index);
        }
    }
}

uint16_t wf__body_socket_address_port(const wf_value *address) {
    return (uint16_t)(address->words[2] & WF_SOCKET_PORT_MASK);
}

static int wf_descriptor(const wf_value *value) { return (int)value->words[0]; }

static void wf_descriptor_value(wf_value *value, int descriptor) {
    memset(value, 0, sizeof(*value));
    value->words[0] = (uint64_t)(unsigned)descriptor;
}

static void wf_transition(wf_value *state) {
    if (state->words[1] != UINT64_MAX) state->words[1]++;
}

static void wf_error_class(wf_io_error *error, unsigned tag, int code, unsigned origin) {
    memset(error, 0, sizeof(*error));
    error->tag = tag;
    error->code = (uint32_t)code;
    error->origin = (uint8_t)origin;
}

static void wf_error(wf_io_error *error, int code, unsigned origin) {
    unsigned tag = WF_IO_OTHER;
#if defined(_WIN32)
    switch ((unsigned)code) {
    case ERROR_FILE_NOT_FOUND: case ERROR_PATH_NOT_FOUND: tag = WF_IO_NOT_FOUND; break;
    case ERROR_ACCESS_DENIED: case ERROR_NETWORK_ACCESS_DENIED: case ERROR_PRIVILEGE_NOT_HELD: tag = WF_IO_PERMISSION_DENIED; break;
    case ERROR_ALREADY_EXISTS: case ERROR_FILE_EXISTS: tag = WF_IO_ALREADY_EXISTS; break;
    case ERROR_DIRECTORY: tag = WF_IO_NOT_DIRECTORY; break;
    case ERROR_DIR_NOT_EMPTY: tag = WF_IO_DIRECTORY_NOT_EMPTY; break;
    case ERROR_WRITE_PROTECT: tag = WF_IO_READ_ONLY; break;
    case ERROR_SHARING_VIOLATION: case ERROR_LOCK_VIOLATION: case ERROR_BUSY: tag = WF_IO_RESOURCE_BUSY; break;
    case ERROR_INVALID_PARAMETER: tag = WF_IO_INVALID_INPUT; break;
    case ERROR_INVALID_NAME: case ERROR_BAD_PATHNAME: case ERROR_FILENAME_EXCED_RANGE: tag = WF_IO_INVALID_PATH; break;
    case ERROR_INVALID_FUNCTION: case ERROR_NOT_SUPPORTED: case ERROR_CALL_NOT_IMPLEMENTED: tag = WF_IO_UNSUPPORTED; break;
    case ERROR_TIMEOUT: case ERROR_SEM_TIMEOUT: tag = WF_IO_TIMED_OUT; break;
    case ERROR_BROKEN_PIPE: case ERROR_NO_DATA: case ERROR_PIPE_NOT_CONNECTED: tag = WF_IO_BROKEN_PIPE; break;
    case ERROR_CONNECTION_REFUSED: tag = WF_IO_CONNECTION_REFUSED; break;
    case ERROR_NETNAME_DELETED: tag = WF_IO_CONNECTION_RESET; break;
    case ERROR_CONNECTION_ABORTED: tag = WF_IO_CONNECTION_RESET; break;
    case ERROR_REQUEST_ABORTED: tag = WF_IO_CONNECTION_ABORTED; break;
    case ERROR_NOT_CONNECTED: tag = WF_IO_NOT_CONNECTED; break;
    case 10048: tag = WF_IO_ADDRESS_IN_USE; break; /* WSAEADDRINUSE. */
    case 10049: tag = WF_IO_ADDRESS_UNAVAILABLE; break; /* WSAEADDRNOTAVAIL. */
    case ERROR_TOO_MANY_OPEN_FILES: case ERROR_NOT_ENOUGH_MEMORY: case ERROR_OUTOFMEMORY: case ERROR_NO_SYSTEM_RESOURCES: tag = WF_IO_RESOURCE_EXHAUSTED; break;
    case ERROR_FILE_TOO_LARGE: tag = WF_IO_FILE_TOO_LARGE; break;
    case ERROR_DISK_FULL: case ERROR_HANDLE_DISK_FULL: tag = WF_IO_NO_SPACE; break;
    case ERROR_NOT_ENOUGH_QUOTA: tag = WF_IO_QUOTA_EXCEEDED; break;
    case ERROR_NOT_SAME_DEVICE: tag = WF_IO_CROSS_DEVICE; break;
    case ERROR_NOT_READY: case ERROR_CRC: case ERROR_GEN_FAILURE: case ERROR_IO_DEVICE: tag = WF_IO_DEVICE_FAILURE; break;
    default: break;
    }
    wf_error_class(error, tag, code, origin);
#else
    switch (code) {
    case ENOENT: tag = WF_IO_NOT_FOUND; break;
    case EACCES: case EPERM: tag = WF_IO_PERMISSION_DENIED; break;
    case EEXIST: tag = WF_IO_ALREADY_EXISTS; break;
    case ENOTDIR: tag = WF_IO_NOT_DIRECTORY; break;
    case EISDIR: tag = WF_IO_IS_DIRECTORY; break;
    case ENOTEMPTY: tag = WF_IO_DIRECTORY_NOT_EMPTY; break;
    case EROFS: tag = WF_IO_READ_ONLY; break;
    case EBUSY: case ETXTBSY: tag = WF_IO_RESOURCE_BUSY; break;
    case EINVAL: tag = WF_IO_INVALID_INPUT; break;
    case ENAMETOOLONG: case ELOOP: tag = WF_IO_INVALID_PATH; break;
    case ENOSYS: case ENOTSUP: tag = WF_IO_UNSUPPORTED; break;
#if EOPNOTSUPP != ENOTSUP
    case EOPNOTSUPP: tag = WF_IO_UNSUPPORTED; break;
#endif
    case ETIMEDOUT: tag = WF_IO_TIMED_OUT; break;
    case EPIPE: tag = WF_IO_BROKEN_PIPE; break;
    case ECONNREFUSED: tag = WF_IO_CONNECTION_REFUSED; break;
    case ECONNRESET: tag = WF_IO_CONNECTION_RESET; break;
    case ECONNABORTED: tag = WF_IO_CONNECTION_ABORTED; break;
    case ENOTCONN: tag = WF_IO_NOT_CONNECTED; break;
    case EADDRINUSE: tag = WF_IO_ADDRESS_IN_USE; break;
    case EADDRNOTAVAIL: tag = WF_IO_ADDRESS_UNAVAILABLE; break;
    case EMFILE: case ENFILE: case ENOMEM: case ENOBUFS: tag = WF_IO_RESOURCE_EXHAUSTED; break;
    case EFBIG: case EOVERFLOW: tag = WF_IO_FILE_TOO_LARGE; break;
    case ENOSPC: tag = WF_IO_NO_SPACE; break;
#ifdef EDQUOT
    case EDQUOT: tag = WF_IO_QUOTA_EXCEEDED; break;
#endif
    case EXDEV: tag = WF_IO_CROSS_DEVICE; break;
    case EIO: case ENXIO: case ENODEV: tag = WF_IO_DEVICE_FAILURE; break;
    default: break;
    }
    wf_error_class(error, tag, code, origin);
#endif
}

/* The handle budget a factory draws on. The invocation's factory and every
 * factory `factory_share` relates to it name one process cell in their third
 * word, so an acquisition through any of them spends a credit of that one
 * budget; a factory built without one keeps its budget in its first word.
 * The cell is updated atomically: every acquisition and close is a waiting
 * call, and the contexts that make them may run on different drivers
 * (`research/investigations/io-model/WAITS.md`, Experiment 5). A factory
 * with its own budget belongs to one owner, and the same atomic updates
 * serve it. */
static uint64_t wf_handle_budget;

static uint64_t *wf_factory_budget(wf_value *factory) {
    return factory->words[2] != 0
        ? (uint64_t *)(uintptr_t)factory->words[2]
        : &factory->words[0];
}

static int wf_factory_take(wf_value *factory, wf_io_error *error) {
    uint64_t *budget = wf_factory_budget(factory);
    uint64_t credits = __atomic_load_n(budget, __ATOMIC_ACQUIRE);
    wf_transition(factory);
    for (;;) {
        if (credits == 0) {
            wf_error_class(error, WF_IO_RESOURCE_EXHAUSTED, 0, 0);
            return 0;
        }
        if (__atomic_compare_exchange_n(
                budget, &credits, credits - 1, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
            return 1;
        }
    }
}

static void wf_factory_return(wf_value *factory) {
    uint64_t *budget = wf_factory_budget(factory);
    uint64_t credits = __atomic_load_n(budget, __ATOMIC_ACQUIRE);
    while (credits != UINT64_MAX
           && !__atomic_compare_exchange_n(
               budget, &credits, credits + 1, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
    }
}

/* WF_NAME_BYTES bounds a name in name bytes [PRE-2]; WF_COMPONENT_BYTES
 * bounds the native component a name becomes. They differ only on Windows,
 * where a name is WTF-8 and its component UTF-16. */
#if defined(_WIN32)
#define WF_COMPONENT_BYTES WF_WINDOWS_COMPONENT_MAX_BYTES
#define WF_NAME_BYTES WF_WINDOWS_NAME_MAX_BYTES
#define WF_OPEN_DIRECTORY_FLAGS 0
#define WF_OPEN_COMPONENT_DIRECTORY_FLAGS 1
#define WF_OPEN_COMPONENT_FILE_FLAGS 1
#define WF_OPEN_APPEND_FLAGS 1
#define WF_OPEN_APPEND_MODE 0u
#define WF_OPEN_APPEND_HAS_MODE 0u
#define WF_OPEN_APPEND_CLASS WF_WINDOWS_DESCRIPTOR_CLASS_WRITE_FILE
#elif defined(__APPLE__)
#define WF_COMPONENT_BYTES 1023u
#define WF_NAME_BYTES WF_COMPONENT_BYTES
#define WF_OPEN_DIRECTORY_FLAGS O_DIRECTORY
#define WF_OPEN_COMPONENT_DIRECTORY_FLAGS (O_DIRECTORY | O_NOFOLLOW)
#define WF_OPEN_COMPONENT_FILE_FLAGS (O_NOFOLLOW | O_NONBLOCK)
#else
#define WF_COMPONENT_BYTES 255u
#define WF_NAME_BYTES WF_COMPONENT_BYTES
#define WF_OPEN_DIRECTORY_FLAGS O_DIRECTORY
#define WF_OPEN_COMPONENT_DIRECTORY_FLAGS (O_DIRECTORY | O_NOFOLLOW)
#define WF_OPEN_COMPONENT_FILE_FLAGS (O_NOFOLLOW | O_NONBLOCK)
#endif
#if !defined(_WIN32)
/* A file opened for appending is created readable and writable by everyone
 * the process's file-creation mask allows, as `open` with `O_CREAT` and a
 * shell's redirection create one. */
#define WF_OPEN_APPEND_FLAGS (O_WRONLY | O_APPEND | O_CREAT | O_NOFOLLOW | O_NONBLOCK)
#define WF_OPEN_APPEND_MODE 0666u
#define WF_OPEN_APPEND_HAS_MODE 1u
#define WF_OPEN_APPEND_CLASS 0u
#endif

/* The block a waiting context keeps for its one pending host operation
 * (`wf__context_operation`): the completion record, then what an operation's
 * finish reads besides the record.  Every waiting body below is a start,
 * which submits into it or answers at once, and a finish, which reads it once
 * the record is complete; the frame that calls them suspends between the two
 * (design/compiler/waiting-contexts.md).  A start answers 0 when
 * it wrote the result itself and submitted nothing, 1 when it submitted an
 * operation that has already completed, and 2 when the operation is still
 * pending; only 2 makes the frame wait, and 1 and 2 are read by the finish.
 * The blocking bodies the probes call are the two with a join between them. */
typedef struct wf_host_operation {
    wf_completion_record record;
    /* A directory read's cursor, which the host may write at completion, or
     * the size of the block a host-path open copied its window into. */
    int64_t position;
    /* Path components stay live until an open or namespace change completes. */
    alignas(2) unsigned char component[WF_COMPONENT_BYTES + 2];
    /* Rename's second name, or a host path copied from a window [PRE-2],
     * borrowed from the runtime pool until the finish. */
    unsigned char *destination;
} wf_host_operation;
_Static_assert(offsetof(wf_host_operation, record) == 0,
               "a context's operation block begins with its record");
_Static_assert(sizeof(wf_host_operation) <= WF_CONTEXT_OPERATION_BYTES,
               "the host operation must fit the block a context keeps");
_Static_assert(_Alignof(wf_host_operation) <= WF_CONTEXT_OPERATION_ALIGN,
               "the host operation must not out-align the block a context keeps");

/* The reading of the monotonic clock an `Option<Instant>` deadline names,
 * or zero for none [PRE-2].  The latest instant is no deadline: the clock
 * never reaches it, and the record reserves it for a fired one. */
static uint64_t wf_deadline_reading(const wf_deadline *deadline) {
    uint64_t reading;
    if (deadline == NULL || deadline->tag != WF_OPTION_SOME) return 0;
    reading = deadline->value.ticks;
    if (reading == WF_COMPLETION_DEADLINE_FIRED) return 0;
    return reading == 0 ? 1 : reading;
}

/* Hands the deadline to the submit that follows, and answers for the
 * submitted operation. */
static void wf_before_submit(const wf_deadline *deadline) {
    wf__completion_next_deadline(wf_deadline_reading(deadline));
}

static int wf_submitted(wf_host_operation *operation) {
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

static void wf_read_result_value(wf_read_result *result, int64_t amount,
                                 int error, uint64_t start, uint64_t extent) {
    memset(result, 0, sizeof(*result));
    if (amount < 0) {
        result->tag = 1;
        result->err.error.tag = 1;
        wf_error(&result->err.error.error, error, 2);
    } else if (amount == 0 && extent != 0) {
        result->tag = 1;
    } else {
        result->ok.value = start + (uint64_t)amount;
    }
}

static void wf_write_result_value(wf_write_result *result, int64_t amount,
                                  int error, uint64_t start, uint64_t extent) {
    memset(result, 0, sizeof(*result));
    if (amount < 0) {
        result->tag = 1;
        wf_error(&result->err.error, error, 3);
    } else if (amount == 0 && extent != 0) {
        result->tag = 1;
        wf_error_class(&result->err.error, WF_IO_WRITE_ZERO, 0, 0);
    } else result->ok.value = start + (uint64_t)amount;
}

static void wf_transfer_read(wf_read_result *result, wf_host_operation *operation,
                             uint64_t start, uint64_t end) {
    int64_t amount;
    int error;
    wf__completion_file_join(&operation->record, &amount, &error);
    if (wf__completion_deadline_passed(&operation->record)
        || wf__completion_cancelled(&operation->record)) {
        memset(result, 0, sizeof(*result));
        result->tag = 1;
        result->err.error.tag = 1;
        wf_error_class(&result->err.error.error, (wf__completion_cancelled(&operation->record) ? WF_IO_CANCELLED : WF_IO_DEADLINE_PASSED), 0, 0);
        return;
    }
    wf_read_result_value(result, amount, error, start, end - start);
}

static void wf_transfer_write(wf_write_result *result, wf_host_operation *operation,
                              uint64_t start, uint64_t end) {
    int64_t amount;
    int error;
    wf__completion_file_join(&operation->record, &amount, &error);
    if (wf__completion_deadline_passed(&operation->record)
        || wf__completion_cancelled(&operation->record)) {
        memset(result, 0, sizeof(*result));
        result->tag = 1;
        wf_error_class(&result->err.error, (wf__completion_cancelled(&operation->record) ? WF_IO_CANCELLED : WF_IO_DEADLINE_PASSED), 0, 0);
        return;
    }
    wf_write_result_value(result, amount, error, start, end - start);
}

int wf__body_read_at_start(wf_read_result *result, wf_value *factory, wf_value *file,
                           wf_view *destination, uint64_t file_offset,
                           uint64_t start, uint64_t end, wf_host_operation *operation) {
    (void)result;
    wf_transition(factory);
    wf_transition(file);
    wf__completion_file_pread_submit(wf_descriptor(file),
        wf_window(destination, start), end - start, file_offset, &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_read_at_finish(wf_read_result *result, wf_value *factory, wf_value *file,
                             wf_view *destination, uint64_t file_offset,
                             uint64_t start, uint64_t end, wf_host_operation *operation) {
    (void)factory;
    (void)file;
    (void)destination;
    (void)file_offset;
    wf_transfer_read(result, operation, start, end);
}

void wf__body_read_at(wf_read_result *result, wf_value *factory, wf_value *file,
                wf_view *destination, uint64_t file_offset,
                uint64_t start, uint64_t end) {
    wf_host_operation operation;
    if (wf__body_read_at_start(result, factory, file, destination, file_offset,
                               start, end, &operation))
        wf__body_read_at_finish(result, factory, file, destination, file_offset,
                                start, end, &operation);
}

int wf__body_read_next_start(wf_read_result *result, wf_value *factory, wf_value *input,
                             wf_view *destination, uint64_t start, uint64_t end,
                             const wf_deadline *deadline, const wf_cancel_handle *cancel, wf_host_operation *operation) {
    (void)result;
    wf_transition(factory);
    wf_transition(input);
    wf_before_submit(deadline);
    void *watched = wf__cancel_wait_state(cancel->state);
    if (watched != NULL)
        return wf__completion_file_read_watched_submit(wf_descriptor(input),
        wf_window(destination, start), end - start, watched, &operation->record);
    wf__completion_file_read_submit(wf_descriptor(input),
        wf_window(destination, start), end - start, &operation->record);
    return wf_submitted(operation);
}

void wf__body_read_next_finish(wf_read_result *result, wf_value *factory, wf_value *input,
                               wf_view *destination, uint64_t start, uint64_t end,
                               const wf_deadline *deadline, const wf_cancel_handle *cancel, wf_host_operation *operation) {
    (void)factory;
    (void)input;
    (void)destination;
    (void)deadline;
    (void)cancel;
    wf_transfer_read(result, operation, start, end);
}

void wf__body_read_next(wf_read_result *result, wf_value *factory, wf_value *input,
                  wf_view *destination, uint64_t start, uint64_t end,
                  const wf_deadline *deadline, const wf_cancel_handle *cancel) {
    wf_host_operation operation;
    if (wf__body_read_next_start(result, factory, input, destination, start, end, deadline, cancel,
                                 &operation))
        wf__body_read_next_finish(result, factory, input, destination, start, end, deadline, cancel,
                                  &operation);
}

int wf__body_write_once_start(wf_write_result *result, wf_value *factory, wf_value *output,
                              const wf_view *source, uint64_t start, uint64_t end,
                              const wf_deadline *deadline, const wf_cancel_handle *cancel, wf_host_operation *operation) {
    (void)result;
    wf_transition(factory);
    wf_transition(output);
    wf_before_submit(deadline);
    void *watched = wf__cancel_wait_state(cancel->state);
    if (watched != NULL)
        return wf__completion_file_write_watched_submit(wf_descriptor(output),
        wf_window(source, start), end - start, watched, &operation->record);
    wf__completion_file_write_submit(wf_descriptor(output),
        wf_window(source, start), end - start, &operation->record);
    return wf_submitted(operation);
}

void wf__body_write_once_finish(wf_write_result *result, wf_value *factory, wf_value *output,
                                const wf_view *source, uint64_t start, uint64_t end,
                                const wf_deadline *deadline, const wf_cancel_handle *cancel, wf_host_operation *operation) {
    (void)factory;
    (void)output;
    (void)source;
    (void)deadline;
    (void)cancel;
    wf_transfer_write(result, operation, start, end);
}

void wf__body_write_once(wf_write_result *result, wf_value *factory, wf_value *output,
                   const wf_view *source, uint64_t start, uint64_t end,
                   const wf_deadline *deadline, const wf_cancel_handle *cancel) {
    wf_host_operation operation;
    if (wf__body_write_once_start(result, factory, output, source, start, end, deadline, cancel,
                                  &operation))
        wf__body_write_once_finish(result, factory, output, source, start, end, deadline, cancel,
                                   &operation);
}

int wf__body_receive_next_start(wf_read_result *result, wf_value *receive,
                                wf_view *destination, uint64_t start, uint64_t end,
                                const wf_deadline *deadline, const wf_cancel_handle *cancel, wf_host_operation *operation) {
    (void)result;
    wf_transition(receive);
    wf_before_submit(deadline);
    void *watched = wf__cancel_wait_state(cancel->state);
    if (watched != NULL)
        return wf__completion_socket_receive_watched_submit(wf_descriptor(receive),
        wf_window(destination, start), end - start, watched, &operation->record);
    wf__completion_socket_receive_submit(wf_descriptor(receive),
        wf_window(destination, start), end - start, &operation->record);
    return wf_submitted(operation);
}

void wf__body_receive_next_finish(wf_read_result *result, wf_value *receive,
                                  wf_view *destination, uint64_t start, uint64_t end,
                                  const wf_deadline *deadline, const wf_cancel_handle *cancel, wf_host_operation *operation) {
    (void)receive;
    (void)destination;
    (void)deadline;
    (void)cancel;
    wf_transfer_read(result, operation, start, end);
}

void wf__body_receive_next(wf_read_result *result, wf_value *receive,
                     wf_view *destination, uint64_t start, uint64_t end,
                     const wf_deadline *deadline, const wf_cancel_handle *cancel) {
    wf_host_operation operation;
    if (wf__body_receive_next_start(result, receive, destination, start, end, deadline, cancel,
                                    &operation))
        wf__body_receive_next_finish(result, receive, destination, start, end, deadline, cancel,
                                     &operation);
}

int wf__body_send_once_start(wf_write_result *result, wf_value *send,
                             const wf_view *source, uint64_t start, uint64_t end,
                             const wf_deadline *deadline, const wf_cancel_handle *cancel, wf_host_operation *operation) {
    (void)result;
    wf_transition(send);
    wf_before_submit(deadline);
    void *watched = wf__cancel_wait_state(cancel->state);
    if (watched != NULL)
        return wf__completion_socket_send_watched_submit(wf_descriptor(send),
        wf_window(source, start), end - start, watched, &operation->record);
    wf__completion_socket_send_submit(wf_descriptor(send),
        wf_window(source, start), end - start, &operation->record);
    return wf_submitted(operation);
}

void wf__body_send_once_finish(wf_write_result *result, wf_value *send,
                               const wf_view *source, uint64_t start, uint64_t end,
                               const wf_deadline *deadline, const wf_cancel_handle *cancel, wf_host_operation *operation) {
    (void)send;
    (void)source;
    (void)deadline;
    (void)cancel;
    wf_transfer_write(result, operation, start, end);
}

void wf__body_send_once(wf_write_result *result, wf_value *send,
                  const wf_view *source, uint64_t start, uint64_t end,
                  const wf_deadline *deadline, const wf_cancel_handle *cancel) {
    wf_host_operation operation;
    if (wf__body_send_once_start(result, send, source, start, end, deadline, cancel, &operation))
        wf__body_send_once_finish(result, send, source, start, end, deadline, cancel, &operation);
}

static int wf_open_start_mode(wf_open_result *result, wf_value *factory,
                              const wf_value *root, const void *path, int flags,
                              unsigned mode, unsigned has_mode,
                              unsigned expected_kind, unsigned descriptor_class,
                              wf_host_operation *operation) {
    memset(result, 0, sizeof(*result));
    if (!wf_factory_take(factory, &result->err.error)) {
        result->tag = 1;
        return 0;
    }
#if !defined(_WIN32)
    (void)descriptor_class;
#endif
    wf__completion_file_open_at_submit(wf_descriptor(root), path, flags, mode, has_mode,
        expected_kind,
#if defined(_WIN32)
        descriptor_class,
#endif
        &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

static int wf_open_start(wf_open_result *result, wf_value *factory,
                         const wf_value *root, const void *path, int flags,
                         unsigned expected_kind, unsigned descriptor_class,
                         wf_host_operation *operation) {
    return wf_open_start_mode(result, factory, root, path, flags, 0, 0,
                              expected_kind, descriptor_class, operation);
}

static void wf_open_finish(wf_open_result *result, wf_value *factory,
                           wf_host_operation *operation) {
    int64_t descriptor;
    int error;
    unsigned outcome;
    wf__completion_file_open_join(&operation->record, &descriptor, &error, &outcome);
    memset(result, 0, sizeof(*result));
    if (outcome != WF_FILE_OPEN_SUCCEEDED) {
        wf_factory_return(factory);
        result->tag = 1;
        if (outcome == WF_FILE_OPEN_IS_DIRECTORY)
            wf_error_class(&result->err.error, WF_IO_IS_DIRECTORY, 0, 0);
        else if (outcome == WF_FILE_OPEN_OTHER_KIND)
            wf_error_class(&result->err.error, WF_IO_UNSUPPORTED, 0, 0);
        else wf_error(&result->err.error, error,
                      outcome == WF_FILE_OPEN_STATUS_FAILED ? 4 : 1);
        return;
    }
    wf_descriptor_value(&result->ok.value, (int)descriptor);
}

int wf__body_open_read_start(wf_open_result *result, wf_value *factory,
                             const wf_value *root, const wf_value *path,
                             wf_host_operation *operation) {
    return wf_open_start(result, factory, root, wf_value_pointer(path), 0,
                         WF_FILE_EXPECT_REGULAR, 1, operation);
}

void wf__body_open_read_finish(wf_open_result *result, wf_value *factory,
                               const wf_value *root, const wf_value *path,
                               wf_host_operation *operation) {
    (void)root;
    (void)path;
    wf_open_finish(result, factory, operation);
}

void wf__body_open_read(wf_open_result *result, wf_value *factory,
                  const wf_value *root, const wf_value *path) {
    wf_host_operation operation;
    if (wf__body_open_read_start(result, factory, root, path, &operation))
        wf__body_open_read_finish(result, factory, root, path, &operation);
}

/* [PRE-2] The host-path opens. POSIX and Windows hosts supply `HostPaths`.
 * A POSIX host looks a path up with `openat` from the base's descriptor,
 * which starts an absolute path at the root directory, resolves each `..`
 * after the links before it and follows every link the lookup reaches, the
 * last component's included: unlike a name, a path is opened without
 * O_NOFOLLOW, and a directory open never passes O_CREAT. A Windows host
 * hands the UTF-16 path to the worker under its host-path flag, which
 * applies the Win32 dialect (`windows_runtime.c`) and follows every reparse
 * point the lookup reaches, and never creates. Both halves of a directory
 * are the descriptor the lookup returned and its duplicate, so they name the
 * one directory that lookup opened. */
enum { WF_HOST_OPEN_FILE, WF_HOST_OPEN_DIRECTORY, WF_HOST_OPEN_PAIR };

static int wf_host_refused(uint8_t *tag, wf_io_error *error, wf_value *factory, unsigned kind) {
    wf_transition(factory);
    *tag = 1;
    wf_error_class(error, kind, 0, 0);
    return 0;
}

#if defined(_WIN32)
/* A window's path holds fewer UTF-16 code units than this. */
#define WF_HOST_PATH_UNITS ((uint64_t)WF_WINDOWS_HOST_PATH_MAX_UNITS)
#define WF_HOST_FILE_FLAGS WF_WINDOWS_HOST_PATH
#define WF_HOST_DIRECTORY_FLAGS WF_WINDOWS_HOST_PATH
#else
/* A window's path holds fewer bytes than this. */
#if defined(PATH_MAX)
#define WF_HOST_PATH_BYTES ((uint64_t)PATH_MAX)
#else
#define WF_HOST_PATH_BYTES UINT64_C(4096)
#endif
#define WF_HOST_FILE_FLAGS 0
#define WF_HOST_DIRECTORY_FLAGS WF_OPEN_DIRECTORY_FLAGS
#endif

/* Spends the credits an open of `shape` holds on success: two for a pair,
 * whose first comes back when the second is refused. */
static int wf_host_take(wf_io_error *error, wf_value *factory, unsigned shape) {
    if (!wf_factory_take(factory, error)) return 0;
    if (shape == WF_HOST_OPEN_PAIR && !wf_factory_take(factory, error)) {
        wf_factory_return(factory);
        return 0;
    }
    return 1;
}

static int wf_host_submit(const wf_value *base, const void *path, unsigned shape,
                          wf_host_operation *operation) {
    wf__completion_file_open_at_submit(
        wf_descriptor(base), path,
        shape == WF_HOST_OPEN_FILE ? WF_HOST_FILE_FLAGS : WF_HOST_DIRECTORY_FLAGS, 0, 0,
        shape == WF_HOST_OPEN_FILE ? WF_FILE_EXPECT_REGULAR : WF_FILE_EXPECT_DIRECTORY,
#if defined(_WIN32)
        shape == WF_HOST_OPEN_FILE ? WF_WINDOWS_DESCRIPTOR_CLASS_READ_FILE
                                   : WF_WINDOWS_DESCRIPTOR_CLASS_DIRECTORY_ROOT,
#endif
        &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

/* A window's path is copied, terminated, into a pool block of `bytes` the
 * operation keeps until its finish has joined the open: the host may read
 * the path after the submit returns. The block's size stays in the
 * operation's position, which an open does not otherwise use, and the finish
 * gives the block back. */
static void *wf_host_copy_block(wf_host_operation *operation, uint64_t bytes) {
    unsigned char *copy = wf__runtime_take(bytes);
    operation->destination = copy;
    operation->position = (int64_t)bytes;
    return copy;
}

#if defined(_WIN32)
/* The UTF-16 code units of UTF-8 or WTF-8 text: one for each scalar or
 * surrogate below U+10000, which a lead byte below 0xf0 starts, and two for
 * each other scalar. */
static uint64_t wf_utf8_utf16_units(const unsigned char *text, uint64_t length) {
    uint64_t units = 0, at;
    for (at = 0; at < length; at++) {
        if ((text[at] & 0xc0u) != 0x80u) units += text[at] >= 0xf0u ? 2u : 1u;
    }
    return units;
}

/* Encodes UTF-8 or WTF-8 text as UTF-16 code units; a WTF-8 surrogate
 * decodes to the code unit it spells. */
static void wf_utf8_to_utf16(const unsigned char *text, uint64_t length, uint16_t *units) {
    uint64_t at = 0, out = 0;
    while (at < length) {
        uint32_t scalar = text[at];
        unsigned more = scalar < 0x80u ? 0u : scalar < 0xe0u ? 1u : scalar < 0xf0u ? 2u : 3u;
        if (more != 0) scalar &= 0x3fu >> more;
        at++;
        for (; more != 0; more--) scalar = (scalar << 6) | (text[at++] & 0x3fu);
        if (scalar >= 0x10000u) {
            scalar -= 0x10000u;
            units[out++] = (uint16_t)(0xd800u | (scalar >> 10));
            units[out++] = (uint16_t)(0xdc00u | (scalar & 0x3ffu));
        } else {
            units[out++] = (uint16_t)scalar;
        }
    }
}

/* A native window is UTF-16LE code units, unpaired surrogates included; a
 * UTF-8 window is converted to them. */
static int wf_host_bytes_start(uint8_t *tag, wf_io_error *error, wf_value *factory,
                               const wf_value *base, const wf_view *path, uint64_t start,
                               uint64_t end, uint8_t encoding, unsigned shape,
                               wf_host_operation *operation) {
    uint64_t length = end - start, units, at;
    const unsigned char *text = wf_window(path, start);
    uint16_t *copy;
    if (length == 0) return wf_host_refused(tag, error, factory, WF_IO_INVALID_PATH);
    if (encoding == WF_PATH_ENCODING_UTF8) {
        if (memchr(text, 0, (size_t)length) != NULL || !wf_utf8_valid(text, length))
            return wf_host_refused(tag, error, factory, WF_IO_INVALID_PATH);
        units = wf_utf8_utf16_units(text, length);
    } else {
        if ((length & 1u) != 0) return wf_host_refused(tag, error, factory, WF_IO_INVALID_PATH);
        for (at = 0; at < length; at += 2)
            if (text[at] == 0 && text[at + 1] == 0)
                return wf_host_refused(tag, error, factory, WF_IO_INVALID_PATH);
        units = length / 2;
    }
    if (units >= WF_HOST_PATH_UNITS) return wf_host_refused(tag, error, factory, WF_IO_INVALID_PATH);
    if (!wf_host_take(error, factory, shape)) {
        *tag = 1;
        return 0;
    }
    copy = wf_host_copy_block(operation, (units + 1) * 2);
    if (encoding == WF_PATH_ENCODING_UTF8) wf_utf8_to_utf16(text, length, copy);
    else memcpy(copy, text, (size_t)length);
    copy[units] = 0;
    return wf_host_submit(base, copy, shape, operation);
}
#else
static int wf_host_bytes_start(uint8_t *tag, wf_io_error *error, wf_value *factory,
                               const wf_value *base, const wf_view *path, uint64_t start,
                               uint64_t end, uint8_t encoding, unsigned shape,
                               wf_host_operation *operation) {
    uint64_t length = end - start;
    const unsigned char *text = wf_window(path, start);
    unsigned char *copy;
    if (length == 0 || length >= WF_HOST_PATH_BYTES || memchr(text, 0, (size_t)length) != NULL
        || (encoding == WF_PATH_ENCODING_UTF8 && !wf_utf8_valid(text, length)))
        return wf_host_refused(tag, error, factory, WF_IO_INVALID_PATH);
    if (!wf_host_take(error, factory, shape)) {
        *tag = 1;
        return 0;
    }
    copy = wf_host_copy_block(operation, length + 1);
    memcpy(copy, text, (size_t)length);
    copy[length] = 0;
    return wf_host_submit(base, copy, shape, operation);
}
#endif

/* An argument's native code units are NUL-terminated invocation backing that
 * outlives the open, so the host reads them in place. */
static int wf_host_string_start(uint8_t *tag, wf_io_error *error, wf_value *factory,
                                const wf_value *base, const wf_value *path, unsigned shape,
                                wf_host_operation *operation) {
    if (path->words[1] == 0) return wf_host_refused(tag, error, factory, WF_IO_INVALID_PATH);
    if (!wf_host_take(error, factory, shape)) {
        *tag = 1;
        return 0;
    }
    operation->destination = NULL;
    return wf_host_submit(base, wf_value_pointer(path), shape, operation);
}

static void wf_host_release_copy(wf_host_operation *operation) {
    if (operation->destination != NULL)
        wf__runtime_give(operation->destination, (uint64_t)operation->position);
    operation->destination = NULL;
}

static void wf_host_open_finish(wf_open_result *result, wf_value *factory,
                                wf_host_operation *operation) {
    wf_open_finish(result, factory, operation);
    wf_host_release_copy(operation);
}

/* A refused open has returned one credit and the pair returns the other. A
 * refused duplicate closes the opened descriptor, so no half survives. */
static void wf_host_pair_finish(wf_directory_result *result, wf_value *factory,
                                wf_host_operation *operation) {
    wf_open_result opened;
    int reader, writer, error;
    wf_open_finish(&opened, factory, operation);
    wf_host_release_copy(operation);
    memset(result, 0, sizeof(*result));
    if (opened.tag != 0) {
        wf_factory_return(factory);
        result->tag = 1;
        result->err.error = opened.err.error;
        return;
    }
    reader = wf_descriptor(&opened.ok.value);
#if defined(_WIN32)
    writer = wf__windows_duplicate_directory(reader);
    error = writer < 0 ? *wf__windows_error_location() : 0;
    if (writer < 0) wf__windows_discard_descriptor(reader);
#else
    writer = fcntl(reader, F_DUPFD_CLOEXEC, 0);
    error = writer < 0 ? errno : 0;
    if (writer < 0) (void)close(reader);
#endif
    if (writer < 0) {
        wf_factory_return(factory);
        wf_factory_return(factory);
        result->tag = 1;
        wf_error(&result->err.error, error, 1);
        return;
    }
    wf_descriptor_value(&result->ok.value.read, reader);
    wf_descriptor_value(&result->ok.value.write, writer);
}

int wf__body_open_host_read_start(wf_open_result *result, wf_value *factory,
                                  const wf_value *paths, const wf_value *base,
                                  const wf_value *path, wf_host_operation *operation) {
    (void)paths;
    memset(result, 0, sizeof(*result));
    return wf_host_string_start(&result->tag, &result->err.error, factory, base, path,
                                WF_HOST_OPEN_FILE, operation);
}

void wf__body_open_host_read_finish(wf_open_result *result, wf_value *factory,
                                    const wf_value *paths, const wf_value *base,
                                    const wf_value *path, wf_host_operation *operation) {
    (void)paths; (void)base; (void)path;
    wf_host_open_finish(result, factory, operation);
}

void wf__body_open_host_read(wf_open_result *result, wf_value *factory, const wf_value *paths,
                             const wf_value *base, const wf_value *path) {
    wf_host_operation operation;
    if (wf__body_open_host_read_start(result, factory, paths, base, path, &operation))
        wf__body_open_host_read_finish(result, factory, paths, base, path, &operation);
}

int wf__body_open_host_read_bytes_start(wf_open_result *result, wf_value *factory,
                                        const wf_value *paths, const wf_value *base,
                                        const wf_view *path, uint64_t start, uint64_t end,
                                        uint8_t encoding, wf_host_operation *operation) {
    (void)paths;
    memset(result, 0, sizeof(*result));
    return wf_host_bytes_start(&result->tag, &result->err.error, factory, base, path, start, end,
                               encoding, WF_HOST_OPEN_FILE, operation);
}

void wf__body_open_host_read_bytes_finish(wf_open_result *result, wf_value *factory,
                                          const wf_value *paths, const wf_value *base,
                                          const wf_view *path, uint64_t start, uint64_t end,
                                          uint8_t encoding, wf_host_operation *operation) {
    (void)paths; (void)base; (void)path; (void)start; (void)end; (void)encoding;
    wf_host_open_finish(result, factory, operation);
}

void wf__body_open_host_read_bytes(wf_open_result *result, wf_value *factory, const wf_value *paths,
                                   const wf_value *base, const wf_view *path, uint64_t start,
                                   uint64_t end, uint8_t encoding) {
    wf_host_operation operation;
    if (wf__body_open_host_read_bytes_start(result, factory, paths, base, path, start, end,
                                            encoding, &operation))
        wf__body_open_host_read_bytes_finish(result, factory, paths, base, path, start, end,
                                             encoding, &operation);
}

int wf__body_open_host_directory_read_start(wf_open_result *result, wf_value *factory,
                                            const wf_value *paths, const wf_value *base,
                                            const wf_value *path, wf_host_operation *operation) {
    (void)paths;
    memset(result, 0, sizeof(*result));
    return wf_host_string_start(&result->tag, &result->err.error, factory, base, path,
                                WF_HOST_OPEN_DIRECTORY, operation);
}

void wf__body_open_host_directory_read_finish(wf_open_result *result, wf_value *factory,
                                              const wf_value *paths, const wf_value *base,
                                              const wf_value *path, wf_host_operation *operation) {
    (void)paths; (void)base; (void)path;
    wf_host_open_finish(result, factory, operation);
}

void wf__body_open_host_directory_read(wf_open_result *result, wf_value *factory,
                                       const wf_value *paths, const wf_value *base,
                                       const wf_value *path) {
    wf_host_operation operation;
    if (wf__body_open_host_directory_read_start(result, factory, paths, base, path, &operation))
        wf__body_open_host_directory_read_finish(result, factory, paths, base, path, &operation);
}

int wf__body_open_host_directory_read_bytes_start(wf_open_result *result, wf_value *factory,
                                                  const wf_value *paths, const wf_value *base,
                                                  const wf_view *path, uint64_t start,
                                                  uint64_t end, uint8_t encoding,
                                                  wf_host_operation *operation) {
    (void)paths;
    memset(result, 0, sizeof(*result));
    return wf_host_bytes_start(&result->tag, &result->err.error, factory, base, path, start, end,
                               encoding, WF_HOST_OPEN_DIRECTORY, operation);
}

void wf__body_open_host_directory_read_bytes_finish(wf_open_result *result, wf_value *factory,
                                                    const wf_value *paths, const wf_value *base,
                                                    const wf_view *path, uint64_t start,
                                                    uint64_t end, uint8_t encoding,
                                                    wf_host_operation *operation) {
    (void)paths; (void)base; (void)path; (void)start; (void)end; (void)encoding;
    wf_host_open_finish(result, factory, operation);
}

void wf__body_open_host_directory_read_bytes(wf_open_result *result, wf_value *factory,
                                             const wf_value *paths, const wf_value *base,
                                             const wf_view *path, uint64_t start, uint64_t end,
                                             uint8_t encoding) {
    wf_host_operation operation;
    if (wf__body_open_host_directory_read_bytes_start(result, factory, paths, base, path, start,
                                                      end, encoding, &operation))
        wf__body_open_host_directory_read_bytes_finish(result, factory, paths, base, path, start,
                                                       end, encoding, &operation);
}

int wf__body_open_host_directory_start(wf_directory_result *result, wf_value *factory,
                                       const wf_value *paths, const wf_value *base,
                                       const wf_value *path, wf_host_operation *operation) {
    (void)paths;
    memset(result, 0, sizeof(*result));
    return wf_host_string_start(&result->tag, &result->err.error, factory, base, path,
                                WF_HOST_OPEN_PAIR, operation);
}

void wf__body_open_host_directory_finish(wf_directory_result *result, wf_value *factory,
                                         const wf_value *paths, const wf_value *base,
                                         const wf_value *path, wf_host_operation *operation) {
    (void)paths; (void)base; (void)path;
    wf_host_pair_finish(result, factory, operation);
}

void wf__body_open_host_directory(wf_directory_result *result, wf_value *factory,
                                  const wf_value *paths, const wf_value *base,
                                  const wf_value *path) {
    wf_host_operation operation;
    if (wf__body_open_host_directory_start(result, factory, paths, base, path, &operation))
        wf__body_open_host_directory_finish(result, factory, paths, base, path, &operation);
}

int wf__body_open_host_directory_bytes_start(wf_directory_result *result, wf_value *factory,
                                             const wf_value *paths, const wf_value *base,
                                             const wf_view *path, uint64_t start, uint64_t end,
                                             uint8_t encoding, wf_host_operation *operation) {
    (void)paths;
    memset(result, 0, sizeof(*result));
    return wf_host_bytes_start(&result->tag, &result->err.error, factory, base, path, start, end,
                               encoding, WF_HOST_OPEN_PAIR, operation);
}

void wf__body_open_host_directory_bytes_finish(wf_directory_result *result, wf_value *factory,
                                               const wf_value *paths, const wf_value *base,
                                               const wf_view *path, uint64_t start, uint64_t end,
                                               uint8_t encoding, wf_host_operation *operation) {
    (void)paths; (void)base; (void)path; (void)start; (void)end; (void)encoding;
    wf_host_pair_finish(result, factory, operation);
}

void wf__body_open_host_directory_bytes(wf_directory_result *result, wf_value *factory,
                                        const wf_value *paths, const wf_value *base,
                                        const wf_view *path, uint64_t start, uint64_t end,
                                        uint8_t encoding) {
    wf_host_operation operation;
    if (wf__body_open_host_directory_bytes_start(result, factory, paths, base, path, start, end,
                                                 encoding, &operation))
        wf__body_open_host_directory_bytes_finish(result, factory, paths, base, path, start, end,
                                                  encoding, &operation);
}

/* `.` and `..` name the directory itself and its parent, never an entry
 * directly below the root, so a component operation refuses them [PRE-2]:
 * a write half must not reach the directory above it. They are the same
 * name bytes on every host. */
static int wf_name_is_dots(const unsigned char *text, uint64_t length) {
    return (length == 1 && text[0] == '.')
        || (length == 2 && text[0] == '.' && text[1] == '.');
}

/* Copies a name, given in name bytes [PRE-2], into component as the native
 * component it names, terminated. */
static int wf_component(unsigned char *component, const wf_view *name,
                         uint64_t start, uint64_t end) {
    uint64_t length = end - start;
    uint64_t index;
    const unsigned char *text = wf_window(name, start);
    if (length == 0 || length > WF_NAME_BYTES) return 0;
    if (wf_name_is_dots(text, length)) return 0;
#if defined(_WIN32)
    {
        /* A name is WTF-8 here, and its component the UTF-16 code units it
         * spells. Every byte below 0x80 is the code unit of the same value. */
        uint16_t units[WF_COMPONENT_BYTES / 2];
        uint64_t count;
        for (index = 0; index < length; index++)
            /* A colon selects a data stream rather than a file component. */
            if (text[index] == 0 || text[index] == '/' || text[index] == '\\' ||
                text[index] == ':')
                return 0;
        if (!wf_text_valid(text, length, 1)) return 0;
        count = wf_utf8_utf16_units(text, length);
        if (count > WF_COMPONENT_BYTES / 2) return 0;
        wf_utf8_to_utf16(text, length, units);
        memcpy(component, units, (size_t)count * 2);
        component[count * 2] = 0;
        component[count * 2 + 1] = 0;
    }
#else
    for (index = 0; index < length; index++)
        if (text[index] == 0 || text[index] == '/') return 0;
    memcpy(component, text, (size_t)length);
    component[length] = 0;
#endif
    return 1;
}

/* The component is copied into the operation block, where it stays until
 * the open completes: the host may read the path after the submit returns. */
static int wf_open_component_start(wf_open_result *result, wf_value *factory,
                                   const wf_value *root, const wf_view *name,
                                   uint64_t start, uint64_t end, unsigned directory,
                                   wf_host_operation *operation) {
    if (!wf_component(operation->component, name, start, end)) {
        wf_transition(factory);
        memset(result, 0, sizeof(*result));
        result->tag = 1;
        wf_error_class(&result->err.error, WF_IO_INVALID_PATH, 0, 0);
        return 0;
    }
    return wf_open_start(result, factory, root, operation->component,
        directory ? WF_OPEN_COMPONENT_DIRECTORY_FLAGS : WF_OPEN_COMPONENT_FILE_FLAGS,
        directory ? WF_FILE_EXPECT_DIRECTORY : WF_FILE_EXPECT_REGULAR,
        directory ? 2 : 1, operation);
}

int wf__body_open_directory_start(wf_open_result *result, wf_value *factory,
                                  const wf_value *root, const wf_view *name,
                                  uint64_t start, uint64_t end,
                                  wf_host_operation *operation) {
    return wf_open_component_start(result, factory, root, name, start, end, 1, operation);
}

void wf__body_open_directory_finish(wf_open_result *result, wf_value *factory,
                                    const wf_value *root, const wf_view *name,
                                    uint64_t start, uint64_t end,
                                    wf_host_operation *operation) {
    (void)root;
    (void)name;
    (void)start;
    (void)end;
    wf_open_finish(result, factory, operation);
}

void wf__body_open_directory(wf_open_result *result, wf_value *factory,
                       const wf_value *root, const wf_view *name,
                       uint64_t start, uint64_t end) {
    wf_host_operation operation;
    if (wf__body_open_directory_start(result, factory, root, name, start, end, &operation))
        wf__body_open_directory_finish(result, factory, root, name, start, end, &operation);
}

int wf__body_open_directory_write_start(wf_open_result *result, wf_value *factory,
                                        const wf_value *root, const wf_view *name,
                                        uint64_t start, uint64_t end,
                                        wf_host_operation *operation) {
    /* The row reads root, as open_append's does [PRE-2]: nothing here may
     * write it, since overlapped readers can share it. */
    memset(result, 0, sizeof(*result));
    if (!wf_component(operation->component, name, start, end)) {
        wf_transition(factory);
        result->tag = 1;
        wf_error_class(&result->err.error, WF_IO_INVALID_PATH, 0, 0);
        return 0;
    }
    if (!wf_factory_take(factory, &result->err.error)) {
        result->tag = 1;
        return 0;
    }
    wf__completion_directory_write_open_submit(
        wf_descriptor(root), operation->component, &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_open_directory_write_finish(wf_open_result *result, wf_value *factory,
                                         const wf_value *root, const wf_view *name,
                                         uint64_t start, uint64_t end,
                                         wf_host_operation *operation) {
    (void)root;
    (void)name;
    (void)start;
    (void)end;
    wf_open_finish(result, factory, operation);
}

void wf__body_open_directory_write(wf_open_result *result, wf_value *factory,
                                   const wf_value *root, const wf_view *name,
                                   uint64_t start, uint64_t end) {
    wf_host_operation operation;
    if (wf__body_open_directory_write_start(result, factory, root, name, start, end, &operation))
        wf__body_open_directory_write_finish(result, factory, root, name, start, end, &operation);
}

int wf__body_open_file_start(wf_open_result *result, wf_value *factory,
                             const wf_value *root, const wf_view *name,
                             uint64_t start, uint64_t end,
                             wf_host_operation *operation) {
    return wf_open_component_start(result, factory, root, name, start, end, 0, operation);
}

void wf__body_open_file_finish(wf_open_result *result, wf_value *factory,
                               const wf_value *root, const wf_view *name,
                               uint64_t start, uint64_t end,
                               wf_host_operation *operation) {
    (void)root;
    (void)name;
    (void)start;
    (void)end;
    wf_open_finish(result, factory, operation);
}

void wf__body_open_file(wf_open_result *result, wf_value *factory, const wf_value *root,
                  const wf_view *name, uint64_t start, uint64_t end) {
    wf_host_operation operation;
    if (wf__body_open_file_start(result, factory, root, name, start, end, &operation))
        wf__body_open_file_finish(result, factory, root, name, start, end, &operation);
}

/* [PRE-2] opens a file below a directory's write half for appending,
 * creating it when no entry has the name. */
int wf__body_open_append_start(wf_open_result *result, wf_value *factory,
                               const wf_value *root, const wf_view *name,
                               uint64_t start, uint64_t end,
                               wf_host_operation *operation) {
    if (!wf_component(operation->component, name, start, end)) {
        wf_transition(factory);
        memset(result, 0, sizeof(*result));
        result->tag = 1;
        wf_error_class(&result->err.error, WF_IO_INVALID_PATH, 0, 0);
        return 0;
    }
    return wf_open_start_mode(result, factory, root, operation->component,
        WF_OPEN_APPEND_FLAGS, WF_OPEN_APPEND_MODE, WF_OPEN_APPEND_HAS_MODE,
        WF_FILE_EXPECT_REGULAR, WF_OPEN_APPEND_CLASS, operation);
}

void wf__body_open_append_finish(wf_open_result *result, wf_value *factory,
                                 const wf_value *root, const wf_view *name,
                                 uint64_t start, uint64_t end,
                                 wf_host_operation *operation) {
    (void)root;
    (void)name;
    (void)start;
    (void)end;
    wf_open_finish(result, factory, operation);
}

void wf__body_open_append(wf_open_result *result, wf_value *factory, const wf_value *root,
                          const wf_view *name, uint64_t start, uint64_t end) {
    wf_host_operation operation;
    if (wf__body_open_append_start(result, factory, root, name, start, end, &operation))
        wf__body_open_append_finish(result, factory, root, name, start, end, &operation);
}

/* [PRE-2] one host write at the end of a file opened for appending. */
int wf__body_append_once_start(wf_write_result *result, wf_value *factory, wf_value *file,
                               const wf_view *source, uint64_t start, uint64_t end,
                               wf_host_operation *operation) {
    (void)result;
    wf_transition(factory);
    wf_transition(file);
    wf__completion_file_append_submit(wf_descriptor(file),
        wf_window(source, start), end - start, &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_append_once_finish(wf_write_result *result, wf_value *factory, wf_value *file,
                                 const wf_view *source, uint64_t start, uint64_t end,
                                 wf_host_operation *operation) {
    (void)factory;
    (void)file;
    (void)source;
    wf_transfer_write(result, operation, start, end);
}

void wf__body_append_once(wf_write_result *result, wf_value *factory, wf_value *file,
                          const wf_view *source, uint64_t start, uint64_t end) {
    wf_host_operation operation;
    if (wf__body_append_once_start(result, factory, file, source, start, end, &operation))
        wf__body_append_once_finish(result, factory, file, source, start, end, &operation);
}

/* [PRE-2] hands the file's appended bytes to the host's durability
 * mechanism; its outcome is the host's answer and nothing more. */
int wf__body_sync_file_start(wf_close_result *result, wf_value *factory, wf_value *file,
                             wf_host_operation *operation) {
    (void)result;
    wf_transition(factory);
    wf_transition(file);
    wf__completion_file_sync_submit(wf_descriptor(file), &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_sync_file_finish(wf_close_result *result, wf_value *factory, wf_value *file,
                               wf_host_operation *operation) {
    int64_t amount;
    int error;
    (void)factory;
    (void)file;
    wf__completion_file_join(&operation->record, &amount, &error);
    memset(result, 0, sizeof(*result));
    if (amount < 0) {
        result->tag = 1;
        wf_error(&result->err.error, error, 9);
    }
}

void wf__body_sync_file(wf_close_result *result, wf_value *factory, wf_value *file) {
    wf_host_operation operation;
    if (wf__body_sync_file_start(result, factory, file, &operation))
        wf__body_sync_file_finish(result, factory, file, &operation);
}

/* [PRE-2] set the file length, retaining append-at-end behavior. */
int wf__body_truncate_file_start(wf_close_result *result, wf_value *factory, wf_value *file,
                                 uint64_t length, wf_host_operation *operation) {
    (void)result;
    wf_transition(factory);
    wf_transition(file);
    wf__completion_file_truncate_submit(wf_descriptor(file), length, &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_truncate_file_finish(wf_close_result *result, wf_value *factory, wf_value *file,
                                   uint64_t length, wf_host_operation *operation) {
    int64_t amount;
    int error;
    (void)factory;
    (void)file;
    (void)length;
    wf__completion_file_join(&operation->record, &amount, &error);
    memset(result, 0, sizeof(*result));
    if (amount < 0) {
        result->tag = 1;
        wf_error(&result->err.error, error, 9);
    }
}

void wf__body_truncate_file(wf_close_result *result, wf_value *factory, wf_value *file, uint64_t length) {
    wf_host_operation operation;
    if (wf__body_truncate_file_start(result, factory, file, length, &operation))
        wf__body_truncate_file_finish(result, factory, file, length, &operation);
}

/* The unit-result namespace operations share the existing host error mapping. */
static void wf_namespace_finish(wf_close_result *result, wf_host_operation *operation) {
    int64_t amount;
    int error;
    wf__completion_file_join(&operation->record, &amount, &error);
    memset(result, 0, sizeof(*result));
    if (amount < 0) {
        result->tag = 1;
        wf_error(&result->err.error, error, 9);
    }
}

static int wf_namespace_invalid(wf_close_result *result) {
    memset(result, 0, sizeof(*result));
    result->tag = 1;
    wf_error_class(&result->err.error, WF_IO_INVALID_PATH, 0, 0);
    return 0;
}

static int wf_rename_prepare(wf_close_result *result,
                             const wf_view *from, uint64_t from_start, uint64_t from_end,
                             const wf_view *to, uint64_t to_start, uint64_t to_end,
                             wf_host_operation *operation) {
    if (!wf_component(operation->component, from, from_start, from_end))
        return wf_namespace_invalid(result);
    operation->destination = wf__runtime_take(WF_COMPONENT_BYTES + 2);
    if (!wf_component(operation->destination, to, to_start, to_end)) {
        wf__runtime_give(operation->destination, WF_COMPONENT_BYTES + 2);
        operation->destination = NULL;
        return wf_namespace_invalid(result);
    }
    return 1;
}

static void wf_rename_finish(wf_close_result *result, wf_host_operation *operation) {
    wf_namespace_finish(result, operation);
    /* Join has ended host access on success, refusal or cancellation. A
     * submitted operation completed in start still comes through finish. */
    wf__runtime_give(operation->destination, WF_COMPONENT_BYTES + 2);
    operation->destination = NULL;
}

int wf__body_rename_file_start(wf_close_result *result, wf_value *factory, wf_value *root,
                               const wf_view *from, uint64_t from_start, uint64_t from_end,
                               const wf_view *to, uint64_t to_start, uint64_t to_end,
                               wf_host_operation *operation) {
    wf_transition(factory);
    wf_transition(root);
    if (!wf_rename_prepare(result, from, from_start, from_end, to, to_start, to_end, operation))
        return 0;
    wf__completion_file_rename_submit(wf_descriptor(root), operation->component,
                                      operation->destination, &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_rename_file_finish(wf_close_result *result, wf_value *factory, wf_value *root,
                                 const wf_view *from, uint64_t from_start, uint64_t from_end,
                                 const wf_view *to, uint64_t to_start, uint64_t to_end,
                                 wf_host_operation *operation) {
    (void)factory; (void)root; (void)from; (void)from_start; (void)from_end;
    (void)to; (void)to_start; (void)to_end;
    wf_rename_finish(result, operation);
}

void wf__body_rename_file(wf_close_result *result, wf_value *factory, wf_value *root,
                          const wf_view *from, uint64_t from_start, uint64_t from_end,
                          const wf_view *to, uint64_t to_start, uint64_t to_end) {
    wf_host_operation operation;
    if (wf__body_rename_file_start(result, factory, root, from, from_start, from_end,
                                  to, to_start, to_end, &operation))
        wf__body_rename_file_finish(result, factory, root, from, from_start, from_end,
                                    to, to_start, to_end, &operation);
}

int wf__body_move_file_start(wf_close_result *result, wf_value *factory, wf_value *from_root,
                               const wf_view *from, uint64_t from_start, uint64_t from_end,
                               wf_value *to_root, const wf_view *to, uint64_t to_start, uint64_t to_end,
                               wf_host_operation *operation) {
    wf_transition(factory);
    wf_transition(from_root);
    wf_transition(to_root);
    if (!wf_rename_prepare(result, from, from_start, from_end, to, to_start, to_end, operation))
        return 0;
    wf__completion_file_move_submit(wf_descriptor(from_root), operation->component,
                                    wf_descriptor(to_root), operation->destination, &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_move_file_finish(wf_close_result *result, wf_value *factory, wf_value *from_root,
                                 const wf_view *from, uint64_t from_start, uint64_t from_end,
                                 wf_value *to_root, const wf_view *to, uint64_t to_start, uint64_t to_end,
                                 wf_host_operation *operation) {
    (void)factory; (void)from_root; (void)to_root; (void)from; (void)from_start; (void)from_end;
    (void)to; (void)to_start; (void)to_end;
    wf_rename_finish(result, operation);
}

void wf__body_move_file(wf_close_result *result, wf_value *factory, wf_value *from_root,
                          const wf_view *from, uint64_t from_start, uint64_t from_end,
                          wf_value *to_root, const wf_view *to, uint64_t to_start, uint64_t to_end) {
    wf_host_operation operation;
    if (wf__body_move_file_start(result, factory, from_root, from, from_start, from_end,
                                  to_root, to, to_start, to_end, &operation))
        wf__body_move_file_finish(result, factory, from_root, from, from_start, from_end,
                                    to_root, to, to_start, to_end, &operation);
}

int wf__body_remove_file_start(wf_close_result *result, wf_value *factory, wf_value *root,
                               const wf_view *name, uint64_t start, uint64_t end,
                               wf_host_operation *operation) {
    wf_transition(factory);
    wf_transition(root);
    if (!wf_component(operation->component, name, start, end))
        return wf_namespace_invalid(result);
    wf__completion_file_remove_submit(wf_descriptor(root), operation->component, &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_remove_file_finish(wf_close_result *result, wf_value *factory, wf_value *root,
                                 const wf_view *name, uint64_t start, uint64_t end,
                                 wf_host_operation *operation) {
    (void)factory; (void)root; (void)name; (void)start; (void)end;
    wf_namespace_finish(result, operation);
}

void wf__body_remove_file(wf_close_result *result, wf_value *factory, wf_value *root,
                          const wf_view *name, uint64_t start, uint64_t end) {
    wf_host_operation operation;
    if (wf__body_remove_file_start(result, factory, root, name, start, end, &operation))
        wf__body_remove_file_finish(result, factory, root, name, start, end, &operation);
}

int wf__body_sync_directory_start(wf_close_result *result, wf_value *factory, wf_value *root,
                                  wf_host_operation *operation) {
    (void)result;
    wf_transition(factory);
    wf_transition(root);
    wf__completion_directory_sync_submit(wf_descriptor(root), &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_sync_directory_finish(wf_close_result *result, wf_value *factory, wf_value *root,
                                    wf_host_operation *operation) {
    (void)factory; (void)root;
    wf_namespace_finish(result, operation);
}

void wf__body_sync_directory(wf_close_result *result, wf_value *factory, wf_value *root) {
    wf_host_operation operation;
    if (wf__body_sync_directory_start(result, factory, root, &operation))
        wf__body_sync_directory_finish(result, factory, root, &operation);
}

int wf__body_open_directory_source_start(wf_open_result *result, wf_value *factory,
                                         const wf_value *directory,
                                         wf_host_operation *operation) {
#if defined(_WIN32)
    /* NtCreateFile does not normalize the Win32 spelling ".". An empty
     * relative object name reopens the supplied directory itself, producing
     * an independent enumeration cursor rather than a duplicated handle. */
    static const uint16_t self[] = { 0 };
#else
    static const char self[] = ".";
#endif
    return wf_open_start(result, factory, directory, self, WF_OPEN_DIRECTORY_FLAGS,
                         WF_FILE_EXPECT_DIRECTORY, 3, operation);
}

void wf__body_open_directory_source_finish(wf_open_result *result, wf_value *factory,
                                           const wf_value *directory,
                                           wf_host_operation *operation) {
    (void)directory;
    wf_open_finish(result, factory, operation);
}

void wf__body_open_directory_source(wf_open_result *result, wf_value *factory,
                              const wf_value *directory) {
    wf_host_operation operation;
    if (wf__body_open_directory_source_start(result, factory, directory, &operation))
        wf__body_open_directory_source_finish(result, factory, directory, &operation);
}

static int wf_close_start(wf_close_result *result, wf_value *factory,
                          const wf_value *owner, int direction,
                          wf_host_operation *operation) {
    wf_transition(factory);
    memset(result, 0, sizeof(*result));
    if (direction < 0)
        wf__completion_file_close_submit(wf_descriptor(owner), &operation->record);
    else wf__completion_socket_shutdown_submit(wf_descriptor(owner),
                                               (unsigned)direction, &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

static void wf_close_finish(wf_close_result *result, wf_value *factory, int direction,
                            wf_host_operation *operation) {
    int64_t amount;
    int error;
    wf__completion_file_join(&operation->record, &amount, &error);
    memset(result, 0, sizeof(*result));
    /* A close consumes its owner even when the host reports an error. An
     * interrupted close is never retried against a possibly reused number.
     * A half-close returns credit only on the actual descriptor-close attempt. */
    if (direction < 0 || amount == 1) wf_factory_return(factory);
    if (error != 0) {
        result->tag = 1;
        wf_error(&result->err.error, error, 8);
    }
}

#define WF_CLOSE_BODY(name, direction)                                                     \
    int wf__body_##name##_start(wf_close_result *result, wf_value *factory,                \
                                const wf_value *owner, wf_host_operation *operation) {     \
        return wf_close_start(result, factory, owner, direction, operation);               \
    }                                                                                      \
    void wf__body_##name##_finish(wf_close_result *result, wf_value *factory,              \
                                  const wf_value *owner, wf_host_operation *operation) {   \
        (void)owner;                                                                       \
        wf_close_finish(result, factory, direction, operation);                            \
    }                                                                                      \
    void wf__body_##name(wf_close_result *result, wf_value *factory,                       \
                         const wf_value *owner) {                                          \
        wf_host_operation operation;                                                       \
        if (wf__body_##name##_start(result, factory, owner, &operation))                   \
            wf__body_##name##_finish(result, factory, owner, &operation);                  \
    }

WF_CLOSE_BODY(close_read, -1)
WF_CLOSE_BODY(close_write, -1)
WF_CLOSE_BODY(close_directory, -1)
WF_CLOSE_BODY(close_directory_write, -1)
WF_CLOSE_BODY(close_directory_source, -1)
WF_CLOSE_BODY(close_listener, -1)
WF_CLOSE_BODY(close_receive, WF_SOCKET_DIRECTION_RECEIVE)
WF_CLOSE_BODY(close_send, WF_SOCKET_DIRECTION_SEND)

/* [PRE-2] a second factory on the same budget. A factory with no budget cell
 * is only ever built by hand in a probe; a source program's factories all
 * descend from the invocation's. */
void wf__body_factory_share(wf_value *result, const wf_value *factory) {
    memset(result, 0, sizeof(*result));
    result->words[2] = factory->words[2] != 0
        ? factory->words[2]
        : (uint64_t)(uintptr_t)&factory->words[0];
}

/* Process readings are ordered through the ordinary capability value. */
void wf__body_meter_share(wf_value *result, const wf_value *meter) {
    (void)meter;
    memset(result, 0, sizeof(*result));
}

uint64_t wf__body_heap_in_use(wf_value *meter) {
    wf_transition(meter);
    return wf__heap_in_use();
}

/* A map is one pointer to the runtime's map. The keyed-table unit supplies
 * the same pool-granted and host mapping accounting used by heap_in_use. No
 * entry hold is required. */
uint64_t wf__body_release_map_reserve(void *const *map, wf_value *meter) {
    wf_transition(meter);
    return wf__keyed_table_release_reserve(*map);
}

void wf__body_resident_bytes(wf_optional_bytes *result, wf_value *meter) {
    wf_transition(meter);
    memset(result, 0, sizeof(*result));
    if (wf__resident_bytes(&result->value)) {
        result->tag = WF_OPTION_SOME;
    }
}

/* [PRE-2] `std::time`.  A clock handle carries nothing: the host has one
 * monotonic clock and one calendar time, and a handle is the authority to
 * read one of them.  An `Instant` holds its reading in nanoseconds in its
 * first word, the one representation a deadline hands the runtime. */
void wf__body_clock_share(wf_value *result, const wf_value *clock) {
    (void)clock;
    memset(result, 0, sizeof(*result));
}

void wf__body_wall_clock_share(wf_value *result, const wf_value *clock) {
    (void)clock;
    memset(result, 0, sizeof(*result));
}

void wf__body_now(wf_instant *result, wf_value *clock) {
    wf_transition(clock);
    memset(result, 0, sizeof(*result));
    result->ticks = wf__completion_monotonic_ns();
}

void wf__body_instant_after(wf_instant *result, const wf_instant *instant, uint64_t nanoseconds) {
    uint64_t reading = instant->ticks;
    memset(result, 0, sizeof(*result));
    result->ticks = nanoseconds > UINT64_MAX - reading ? UINT64_MAX : reading + nanoseconds;
}

uint64_t wf__body_nanoseconds_from(const wf_instant *earlier, const wf_instant *later) {
    return later->ticks > earlier->ticks ? later->ticks - earlier->ticks : 0;
}

_Bool wf__body_instant_reached(const wf_instant *deadline, const wf_instant *instant) {
    return instant->ticks >= deadline->ticks;
}

int64_t wf__body_unix_nanoseconds(const wf_value *clock) {
    (void)clock;
#if defined(_WIN32)
    /* 100-nanosecond intervals since 1601-01-01, which is 11,644,473,600
     * seconds before the Unix epoch. */
    FILETIME now;
    uint64_t intervals;
    GetSystemTimePreciseAsFileTime(&now);
    intervals = ((uint64_t)now.dwHighDateTime << 32) | now.dwLowDateTime;
    return ((int64_t)intervals - INT64_C(116444736000000000)) * 100;
#else
    struct timespec now;
    if (clock_gettime(CLOCK_REALTIME, &now) != 0) return 0;
    return (int64_t)now.tv_sec * INT64_C(1000000000) + (int64_t)now.tv_nsec;
#endif
}

/* Days from 1970-01-01 to a proleptic Gregorian date, counting March as the
 * first month of a 400-year era so that February's leap day ends the year;
 * the method is Howard Hinnant's days_from_civil
 * (https://howardhinnant.github.io/date_algorithms.html). */
static int64_t wf_days_from_civil(int64_t year, int64_t month, int64_t day) {
    int64_t shifted = month <= 2 ? year - 1 : year;
    int64_t era = (shifted >= 0 ? shifted : shifted - 399) / 400;
    int64_t year_of_era = shifted - era * 400;
    int64_t day_of_year = (153 * (month > 2 ? month - 3 : month + 9) + 2) / 5 + day - 1;
    int64_t day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    return era * 146097 + day_of_era - 719468;
}

/* [PRE-2] The offset is the host conversion's local date and time, read as a
 * UTC date and time, minus the second it converted, the greatest second not
 * after `at`, and is reported only when it is less than one day either way,
 * the bound the signature's postconditions state. `localtime` implies
 * `tzset`; `localtime_r` does not, so the reading calls it, which also
 * observes a changed system zone. */
void wf__body_local_offset_seconds(wf_optional_i64 *result, const wf_value *clock, int64_t at) {
    int64_t second = at / INT64_C(1000000000);
    struct tm local;
    (void)clock;
    memset(result, 0, sizeof(*result));
    if (at % INT64_C(1000000000) < 0) second -= 1;
#if defined(_WIN32)
    __time64_t moment = (__time64_t)second;
    _tzset();
    if (_localtime64_s(&local, &moment) != 0) return;
#else
    time_t moment = (time_t)second;
    if ((int64_t)moment != second) return;
    tzset();
    if (localtime_r(&moment, &local) == NULL) return;
#endif
    int64_t days = wf_days_from_civil((int64_t)local.tm_year + 1900, (int64_t)local.tm_mon + 1,
                                      (int64_t)local.tm_mday);
    int64_t wall = days * INT64_C(86400) + (int64_t)local.tm_hour * 3600 +
                   (int64_t)local.tm_min * 60 + (int64_t)local.tm_sec;
    int64_t offset = wall - second;
    if (offset <= -INT64_C(86400) || offset >= INT64_C(86400)) return;
    result->tag = WF_OPTION_SOME;
    result->value = offset;
}

/* [PRE-2] no host operation: the driver ends the sleep at its deadline
 * or when its watch fires. */
int wf__body_sleep_until_start(wf_sleep_result *result, const wf_instant *deadline, const wf_cancel_handle *cancel,
                               wf_host_operation *operation) {
    uint64_t reading = deadline->ticks;
    (void)result;
    void *watched = wf__cancel_wait_state(cancel->state);
    if (watched != NULL)
        return wf__completion_sleep_watched_submit(reading == 0 ? 1 : reading,
            watched, &operation->record);
    wf__completion_sleep_submit(reading == 0 ? 1 : reading, &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_sleep_until_finish(wf_sleep_result *result, const wf_instant *deadline, const wf_cancel_handle *cancel,
                                 wf_host_operation *operation) {
    int64_t amount;
    int error;
    (void)deadline;
    (void)cancel;
    /* A context's finish reads a record its driver already completed; a
     * sleep joined outside every context waits out the clock here. */
    wf__completion_file_join(&operation->record, &amount, &error);
    memset(result, 0, sizeof(*result));
    result->tag = wf__completion_cancelled(&operation->record) ? 1u : 0u;
}

void wf__body_sleep_until(wf_sleep_result *result, const wf_instant *deadline, const wf_cancel_handle *cancel) {
    wf_host_operation operation;
    if (wf__body_sleep_until_start(result, deadline, cancel, &operation))
        wf__body_sleep_until_finish(result, deadline, cancel, &operation);
}

int wf__body_tcp_listen_start(wf_open_result *result, wf_value *factory,
                              const wf_value *address, wf_host_operation *operation) {
    memset(result, 0, sizeof(*result));
    if (!wf_factory_take(factory, &result->err.error)) {
        result->tag = 1;
        return 0;
    }
    wf__completion_socket_listen_submit(address->words[0], address->words[1],
                                        (uint32_t)address->words[2], &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

void wf__body_tcp_listen_finish(wf_open_result *result, wf_value *factory,
                                const wf_value *address, wf_host_operation *operation) {
    int64_t descriptor;
    int error;
    (void)address;
    wf__completion_file_join(&operation->record, &descriptor, &error);
    memset(result, 0, sizeof(*result));
    if (descriptor < 0) {
        wf_factory_return(factory);
        result->tag = 1;
        wf_error(&result->err.error, error, 5);
    } else wf_descriptor_value(&result->ok.value, (int)descriptor);
}

void wf__body_tcp_listen(wf_open_result *result, wf_value *factory, const wf_value *address) {
    wf_host_operation operation;
    if (wf__body_tcp_listen_start(result, factory, address, &operation))
        wf__body_tcp_listen_finish(result, factory, address, &operation);
}

int wf__body_tcp_connect_start(wf_connect_result *result, wf_value *factory,
                               const wf_value *address, const wf_deadline *deadline, const wf_cancel_handle *cancel,
                               wf_host_operation *operation) {
    memset(result, 0, sizeof(*result));
    if (!wf_factory_take(factory, &result->err.error)) {
        result->tag = 1;
        return 0;
    }
    wf_before_submit(deadline);
    void *watched = wf__cancel_wait_state(cancel->state);
    if (watched != NULL)
        return wf__completion_socket_connect_watched_submit(address->words[0], address->words[1],
                                         (uint32_t)address->words[2], watched, &operation->record);
    wf__completion_socket_connect_submit(address->words[0], address->words[1],
                                         (uint32_t)address->words[2], &operation->record);
    return wf_submitted(operation);
}

void wf__body_tcp_connect_finish(wf_connect_result *result, wf_value *factory,
                                 const wf_value *address, const wf_deadline *deadline, const wf_cancel_handle *cancel,
                                 wf_host_operation *operation) {
    int64_t descriptor;
    int error;
    (void)address;
    (void)deadline;
    (void)cancel;
    wf__completion_file_join(&operation->record, &descriptor, &error);
    memset(result, 0, sizeof(*result));
    if (descriptor < 0) {
        wf_factory_return(factory);
        result->tag = 1;
        if (wf__completion_deadline_passed(&operation->record)
        || wf__completion_cancelled(&operation->record))
            wf_error_class(&result->err.error, (wf__completion_cancelled(&operation->record) ? WF_IO_CANCELLED : WF_IO_DEADLINE_PASSED), 0, 0);
        else wf_error(&result->err.error, error, 7);
    } else {
        wf_descriptor_value(&result->ok.value.receive, (int)descriptor);
        wf_descriptor_value(&result->ok.value.send, (int)descriptor);
    }
}

void wf__body_tcp_connect(wf_connect_result *result, wf_value *factory, const wf_value *address,
                          const wf_deadline *deadline, const wf_cancel_handle *cancel) {
    wf_host_operation operation;
    if (wf__body_tcp_connect_start(result, factory, address, deadline, cancel, &operation))
        wf__body_tcp_connect_finish(result, factory, address, deadline, cancel, &operation);
}

int wf__body_tcp_accept_start(wf_accept_result *result, wf_value *factory,
                              wf_value *listener, const wf_deadline *deadline, const wf_cancel_handle *cancel,
                              wf_host_operation *operation) {
    memset(result, 0, sizeof(*result));
    wf_transition(listener);
    if (!wf_factory_take(factory, &result->err.error)) {
        result->tag = 1;
        return 0;
    }
    wf_before_submit(deadline);
    void *watched = wf__cancel_wait_state(cancel->state);
    if (watched != NULL)
        return wf__completion_socket_accept_watched_submit(wf_descriptor(listener), watched, &operation->record);
    wf__completion_socket_accept_submit(wf_descriptor(listener), &operation->record);
    return wf_submitted(operation);
}

void wf__body_tcp_accept_finish(wf_accept_result *result, wf_value *factory,
                                wf_value *listener, const wf_deadline *deadline, const wf_cancel_handle *cancel,
                                wf_host_operation *operation) {
    int64_t descriptor;
    int error;
    uint64_t low, high;
    uint32_t tag;
    (void)listener;
    (void)deadline;
    (void)cancel;
    wf__completion_socket_accept_join(&operation->record, &descriptor, &error,
                                      &low, &high, &tag);
    memset(result, 0, sizeof(*result));
    if (descriptor < 0) {
        wf_factory_return(factory);
        result->tag = 1;
        if (wf__completion_deadline_passed(&operation->record)
        || wf__completion_cancelled(&operation->record))
            wf_error_class(&result->err.error, (wf__completion_cancelled(&operation->record) ? WF_IO_CANCELLED : WF_IO_DEADLINE_PASSED), 0, 0);
        else wf_error(&result->err.error, error, 6);
    } else {
        wf_descriptor_value(&result->ok.value.connection.receive, (int)descriptor);
        wf_descriptor_value(&result->ok.value.connection.send, (int)descriptor);
        result->ok.value.peer.words[0] = low;
        result->ok.value.peer.words[1] = high;
        result->ok.value.peer.words[2] = tag;
    }
}

void wf__body_tcp_accept(wf_accept_result *result, wf_value *factory, wf_value *listener,
                         const wf_deadline *deadline, const wf_cancel_handle *cancel) {
    wf_host_operation operation;
    if (wf__body_tcp_accept_start(result, factory, listener, deadline, cancel, &operation))
        wf__body_tcp_accept_finish(result, factory, listener, deadline, cancel, &operation);
}

/* [PRE-2] the host's report of the connection's own endpoint. Both halves of
 * one connection carry its one descriptor, which stays open until the second
 * half-close, so a live receiving half always names an open socket. The query
 * spends no handle credit and changes neither half. */
void wf__body_local_address(wf_open_result *result, const wf_value *receive) {
    uint64_t low, high;
    uint32_t tag;
    int error = wf__completion_socket_local_address(wf_descriptor(receive), &low, &high, &tag);
    memset(result, 0, sizeof(*result));
    if (error != 0) {
        result->tag = 1;
        wf_error(&result->err.error, error, 10);
        return;
    }
    result->ok.value.words[0] = low;
    result->ok.value.words[1] = high;
    result->ok.value.words[2] = tag;
}

void wf__body_cancel_source(wf_cancel_handle *result) {
    result->state = wf__cancel_new();
}

void wf__body_cancel_share(wf_cancel_handle *result, const wf_cancel_handle *source) {
    wf__shared_share(source->state);
    *result = *source;
}

void wf__body_cancel_watch(wf_cancel_handle *result, const wf_cancel_handle *source) {
    wf__body_cancel_share(result, source);
}

void *wf__body_cancel_state(const wf_cancel_handle *watch) {
    wf__shared_share(watch->state);
    return watch->state;
}

int wf__body_cancel_fire_start(uint8_t *result, const wf_cancel_handle *source,
                               wf_host_operation *operation) {
    (void)result;
    return wf__shared_start(source->state, &operation->record);
}

void wf__body_cancel_fire_finish(uint8_t *result, const wf_cancel_handle *source,
                                 wf_host_operation *operation) {
    (void)operation;
    wf__cancel_fire_held(source->state);
    *result = 0;
}

void wf__body_cancel_never(wf_cancel_handle *result) {
    result->state = wf__cancel_never();
}

void wf__body_close_cancel_source(const wf_cancel_handle *source) {
    /* CancelState has no releasing fields; this is the same shared-handle
     * release the compiler derives for the struct's private field. */
    if (wf__shared_release(source->state)) wf__shared_free(source->state);
}

void wf__body_close_cancel_watch(const wf_cancel_handle *watch) {
    wf__body_close_cancel_source(watch);
}

void wf__body_stop_listen(wf_open_result *result, wf_value *factory, const wf_value *stops) {
    (void)stops;
    memset(result, 0, sizeof(*result));
    /* ResourceBusy describes the single delivery stream even when its first
     * listener spent this factory's last credit. Hold the lifecycle across
     * the quota reservation so concurrent factories cannot both acquire it. */
    int error = wf__stop_listen_begin();
    if (error != 0) {
        wf_transition(factory);
        result->tag = 1;
        wf_error(&result->err.error, error, 5);
        return;
    }
    if (!wf_factory_take(factory, &result->err.error)) {
        (void)wf__stop_listen_finish(0);
        result->tag = 1;
        return;
    }
    error = wf__stop_listen_finish(1);
    if (error != 0) {
        wf_factory_return(factory);
        result->tag = 1;
        wf_error(&result->err.error, error, 5);
    }
}

int wf__body_stop_next_start(wf_stop_result *result, wf_value *factory,
                             wf_value *listener, const wf_deadline *deadline, const wf_cancel_handle *cancel,
                             wf_host_operation *operation) {
    (void)result;
    wf_transition(factory);
    wf_transition(listener);
    wf_before_submit(deadline);
    void *watched = wf__cancel_wait_state(cancel->state);
    if (watched != NULL)
        return wf__completion_stop_next_watched_submit(watched, &operation->record);
    wf__completion_stop_next_submit(&operation->record);
    return wf_submitted(operation);
}

void wf__body_stop_next_finish(wf_stop_result *result, wf_value *factory,
                               wf_value *listener, const wf_deadline *deadline, const wf_cancel_handle *cancel,
                               wf_host_operation *operation) {
    (void)factory; (void)listener; (void)deadline; (void)cancel;
    int64_t kind;
    int error;
    wf__completion_file_join(&operation->record, &kind, &error);
    memset(result, 0, sizeof(*result));
    if (kind < 0) {
        result->tag = 1;
        if (wf__completion_deadline_passed(&operation->record)
        || wf__completion_cancelled(&operation->record))
            wf_error_class(&result->err.error, (wf__completion_cancelled(&operation->record) ? WF_IO_CANCELLED : WF_IO_DEADLINE_PASSED), 0, 0);
        else wf_error(&result->err.error, error, 2);
    } else result->ok.value = (uint32_t)kind;
}

void wf__body_stop_next(wf_stop_result *result, wf_value *factory, wf_value *listener,
                        const wf_deadline *deadline, const wf_cancel_handle *cancel) {
    wf_host_operation operation;
    if (wf__body_stop_next_start(result, factory, listener, deadline, cancel, &operation))
        wf__body_stop_next_finish(result, factory, listener, deadline, cancel, &operation);
}

int wf__body_close_stop_listener_start(wf_close_result *result, wf_value *factory,
                                       const wf_value *listener, wf_host_operation *operation) {
    (void)result; (void)listener;
    wf_transition(factory);
    wf__completion_stop_close_submit(&operation->record);
    return wf_submitted(operation);
}

void wf__body_close_stop_listener_finish(wf_close_result *result, wf_value *factory,
                                         const wf_value *listener, wf_host_operation *operation) {
    (void)listener;
    wf_close_finish(result, factory, -1, operation);
}

void wf__body_close_stop_listener(wf_close_result *result, wf_value *factory,
                                  const wf_value *listener) {
    wf_host_operation operation;
    if (wf__body_close_stop_listener_start(result, factory, listener, &operation))
        wf__body_close_stop_listener_finish(result, factory, listener, &operation);
}

#if !defined(_WIN32)
#include <sys/resource.h>

/* [PRE-2] c(n): the handle credits a soft open-files limit n gives, keeping
 * 64 descriptors for the runtime and bounding the budget at 2^20. A host
 * without the limit starts with 4096 credits and never adds any. */
static uint64_t wf_limit_credits(uint64_t soft) {
    uint64_t bounded = soft > (UINT64_C(1) << 20) ? (UINT64_C(1) << 20) : soft;
    return bounded > 64 ? bounded - 64 : 0;
}

/* A limit the host reports as unlimited reads as 2^64 - 1 [PRE-2]. */
static uint64_t wf_limit_value(rlim_t value) {
    return value == RLIM_INFINITY ? UINT64_MAX : (uint64_t)value;
}
#endif

int wf__ordinary_inputs(wf_inputs *inputs, int argc, void *argv) {
    int cwd;
    uint64_t capacity;
    memset(inputs, 0, sizeof(*inputs));
    inputs->drivers = wf__context_driver_count();
    inputs->process_id.tag = WF_OPTION_SOME;
#if defined(_WIN32)
    inputs->process_id.value = (uint64_t)GetCurrentProcessId();
#else
    inputs->process_id.value = (uint64_t)getpid();
#endif
    if (wf__stop_initialize() != 0) return 0;
#if defined(_WIN32)
    /* The build launcher has an ordinary narrow-argv main. Recover Windows'
     * original UTF-16 arguments here; the backing belongs to the enclosing
     * invocation and remains live through every ordinary Args/HostString use. */
    wchar_t **wide_argv = CommandLineToArgvW(GetCommandLineW(), &argc);
    if (wide_argv == NULL) return 0;
    argv = wide_argv;
    cwd = wf__windows_open_cwd(NULL, 0);
    if (cwd < 0) {
        LocalFree(wide_argv);
        return 0;
    }
    capacity = 4096;
    wf_descriptor_value(&inputs->out, wf__windows_stdout_descriptor());
    wf_descriptor_value(&inputs->err, wf__windows_stderr_descriptor());
    wf_descriptor_value(&inputs->in, wf__windows_stdin_descriptor());
#else
    struct rlimit limit;
    if (signal(SIGPIPE, SIG_IGN) == SIG_ERR) return 0;
    cwd = open(".", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    capacity = 0;
    if (getrlimit(RLIMIT_NOFILE, &limit) == 0) {
        capacity = wf_limit_credits(wf_limit_value(limit.rlim_cur));
    }
    wf_descriptor_value(&inputs->out, STDOUT_FILENO);
    wf_descriptor_value(&inputs->err, STDERR_FILENO);
    wf_descriptor_value(&inputs->in, STDIN_FILENO);
#endif
    if (cwd < 0) return 0;
    wf_descriptor_value(&inputs->cwd_read, cwd);
    /* The write half is a descriptor of its own on the same directory, so
     * each half closes alone [PRE-2]. */
#if defined(_WIN32)
    cwd = wf__windows_open_cwd(NULL, 0);
#else
    cwd = fcntl(cwd, F_DUPFD_CLOEXEC, 0);
#endif
    if (cwd < 0) return 0;
    wf_descriptor_value(&inputs->cwd_write, cwd);
    /* [PRE-2] POSIX and Windows hosts grant host-path lookup. The
     * authority's own value is empty: it names no descriptor and owns
     * nothing to close. */
    inputs->host_paths.tag = WF_OPTION_SOME;
    wf_text(&inputs->args, argv, argc > 0 ? (uint64_t)(unsigned)argc : 0);
    wf_handle_budget = capacity;
    inputs->handles.words[2] = (uint64_t)(uintptr_t)&wf_handle_budget;
    return 1;
}

/* [PRE-2] The open-files limit, origin 11. A raise reads the limit, sets
 * the soft limit and adds the credits it gives under one lock, so two
 * contexts raising through related factories add each credit once. */
#define WF_LIMIT_ORIGIN 11u
#if !defined(_WIN32)
static unsigned char wf_limit_lock;
#endif

void wf__body_open_files_limit(wf_limit_result *result, const wf_value *factory) {
    (void)factory;
    memset(result, 0, sizeof(*result));
#if defined(_WIN32)
    result->tag = 1;
    wf_error_class(&result->err.error, WF_IO_UNSUPPORTED, 0, 0);
#else
    struct rlimit limit;
    if (getrlimit(RLIMIT_NOFILE, &limit) != 0) {
        result->tag = 1;
        wf_error(&result->err.error, errno, WF_LIMIT_ORIGIN);
        return;
    }
    result->ok.value.soft = wf_limit_value(limit.rlim_cur);
    result->ok.value.hard = wf_limit_value(limit.rlim_max);
#endif
}

void wf__body_raise_open_files_limit(wf_limit_result *result, wf_value *factory, uint64_t wanted) {
    memset(result, 0, sizeof(*result));
    wf_transition(factory);
#if defined(_WIN32)
    (void)wanted;
    result->tag = 1;
    wf_error_class(&result->err.error, WF_IO_UNSUPPORTED, 0, 0);
#else
    struct rlimit limit;
    int failure = 0;
    while (__atomic_test_and_set(&wf_limit_lock, __ATOMIC_ACQUIRE)) {
    }
    if (getrlimit(RLIMIT_NOFILE, &limit) != 0) {
        failure = errno;
    } else {
        uint64_t soft = wf_limit_value(limit.rlim_cur);
        uint64_t hard = wf_limit_value(limit.rlim_max);
        if (wanted > soft) {
            /* A request above the hard limit goes to the host too, which
             * refuses a soft limit above the hard one (POSIX: EINVAL). The
             * limits reported and the credits added are those the host
             * holds after the call, read back, since a finite request may
             * map to RLIM_INFINITY or the host may adjust it. */
            struct rlimit raised = limit;
            raised.rlim_cur = wanted >= (uint64_t)RLIM_INFINITY ? RLIM_INFINITY : (rlim_t)wanted;
            if (setrlimit(RLIMIT_NOFILE, &raised) != 0) {
                failure = errno;
            } else {
                struct rlimit after;
                uint64_t before_credits = wf_limit_credits(soft);
                uint64_t after_credits;
                if (getrlimit(RLIMIT_NOFILE, &after) != 0) after = raised;
                soft = wf_limit_value(after.rlim_cur);
                hard = wf_limit_value(after.rlim_max);
                after_credits = wf_limit_credits(soft);
                if (after_credits > before_credits) {
                    __atomic_add_fetch(wf_factory_budget(factory), after_credits - before_credits,
                                       __ATOMIC_ACQ_REL);
                }
            }
        }
        result->ok.value.soft = soft;
        result->ok.value.hard = hard;
    }
    __atomic_clear(&wf_limit_lock, __ATOMIC_RELEASE);
    if (failure != 0) {
        memset(result, 0, sizeof(*result));
        result->tag = 1;
        wf_error(&result->err.error, failure, WF_LIMIT_ORIGIN);
    }
#endif
}

#if defined(_WIN32)
#define WF_DIRENT_RECORD 0u
#define WF_DIRENT_NAME_LENGTH 2u
#define WF_DIRENT_KIND 4u
#define WF_DIRENT_NAME 5u
#define WF_DIRENT_REGULAR 1u
#define WF_DIRENT_DIRECTORY 2u
#define WF_DIRENT_SYMLINK 3u
#else
#include <dirent.h>
#define WF_DIRENT_RECORD offsetof(struct dirent, d_reclen)
#define WF_DIRENT_KIND offsetof(struct dirent, d_type)
#define WF_DIRENT_NAME offsetof(struct dirent, d_name)
#define WF_DIRENT_REGULAR DT_REG
#define WF_DIRENT_DIRECTORY DT_DIR
#define WF_DIRENT_SYMLINK DT_LNK
#if defined(__APPLE__)
#define WF_DIRENT_NAME_LENGTH offsetof(struct dirent, d_namlen)
#endif
#endif

int wf__body_directory_next_start(wf_list_result *result, wf_value *source,
                                  wf_view *destination, uint64_t start, uint64_t end,
                                  wf_host_operation *operation) {
    (void)result;
    wf_transition(source);
    operation->position = 0;
    wf__completion_directory_next_submit(wf_descriptor(source),
                                         wf_window(destination, start), end - start,
                                         &operation->position, &operation->record);
    return wf__completion_pending(&operation->record) ? 2 : 1;
}

/* A host batch's native records are rewritten in place into the portable
 * records [PRE-2], which are never longer: one kind byte, the name's byte
 * count as a little-endian u16, and the name bytes. `.` and `..` are left
 * out. The engine reads a batch holding only them again within the same
 * operation, so a nonempty batch here holds an entry and a successful batch
 * over a nonempty window is never empty. */
void wf__body_directory_next_finish(wf_list_result *result, wf_value *source,
                                    wf_view *destination, uint64_t start, uint64_t end,
                                    wf_host_operation *operation) {
    int64_t amount;
    int error;
    uint64_t cursor = 0, written = 0, entries = 0;
    unsigned char *window = wf_window(destination, start);
    (void)source;
    wf__completion_file_join(&operation->record, &amount, &error);
    memset(result, 0, sizeof(*result));
    result->next = start;
    if (amount < 0) {
        result->result.tag = 1;
        result->result.err.error.tag = 1;
        wf_error(&result->result.err.error.error, error, 2);
    } else if (amount == 0 && end != start) {
        result->result.tag = 1;
    }
    if (amount <= 0) return;
    if ((uint64_t)amount > end - start) abort();
    while (cursor < (uint64_t)amount) {
        uint16_t extent;
        uint64_t named, index;
        unsigned kind, portable;
        unsigned char *entry = window + cursor;
        if ((uint64_t)amount - cursor < WF_DIRENT_NAME) abort();
        memcpy(&extent, entry + WF_DIRENT_RECORD, sizeof(extent));
        if (extent <= WF_DIRENT_NAME || extent > (uint64_t)amount - cursor) abort();
        kind = entry[WF_DIRENT_KIND];
#if defined(WF_DIRENT_NAME_LENGTH)
        {
            uint16_t length;
            memcpy(&length, entry + WF_DIRENT_NAME_LENGTH, sizeof(length));
            named = length;
        }
#else
        for (named = 0; named < extent - WF_DIRENT_NAME; named++)
            if (entry[WF_DIRENT_NAME + named] == 0) break;
        if (named == extent - WF_DIRENT_NAME) abort();
#endif
        if (named == 0 || named > WF_NAME_BYTES ||
            named > extent - WF_DIRENT_NAME || named + 3 > end - start - written)
            abort();
        for (index = 0; index < named; index++) {
            unsigned char byte = entry[WF_DIRENT_NAME + index];
#if defined(_WIN32)
            if (byte == '\\') abort();
#endif
            if (byte == 0 || byte == '/') abort();
        }
        cursor += extent;
        if (wf_name_is_dots(entry + WF_DIRENT_NAME, named)) continue;
        portable = kind == 0 ? 0 : kind == WF_DIRENT_REGULAR ? 1 :
            kind == WF_DIRENT_DIRECTORY ? 2 : kind == WF_DIRENT_SYMLINK ? 3 : 4;
        window[written] = (unsigned char)portable;
        window[written + 1] = (unsigned char)named;
        window[written + 2] = (unsigned char)(named >> 8);
        memmove(window + written + 3, entry + WF_DIRENT_NAME, (size_t)named);
        written += named + 3;
        entries++;
    }
    if (entries == 0) abort();
    result->next = start + written;
    result->entries = entries;
}

void wf__body_directory_next(wf_list_result *result, wf_value *source,
                        wf_view *destination, uint64_t start, uint64_t end) {
    wf_host_operation operation;
    if (wf__body_directory_next_start(result, source, destination, start, end, &operation))
        wf__body_directory_next_finish(result, source, destination, start, end, &operation);
}
