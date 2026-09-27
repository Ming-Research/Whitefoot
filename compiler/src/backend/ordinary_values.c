/* Ordinary prelude implementations, linked using the same symbols and ABI as
 * Whitefoot function definitions. No compiler operation identity reaches this
 * unit. Opaque contents are this library's private representation. */
#if !defined(_WIN32)
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#endif
#include "ordinary_values.h"
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
#else
#include <fcntl.h>
#include <signal.h>
#include <unistd.h>
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
    wf_text(&result->value, arguments[position],
            wf__windows_wcslen(arguments[position]));
#else
    wf_text(&result->value, arguments[position], strlen(arguments[position]));
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
        result->error.required = bytes;
        return;
    }
    if (bytes != 0) {
        memcpy(wf_window(destination, start),
               wf_value_pointer(value), (size_t)bytes);
    }
    result->value = start + bytes;
}

#if !defined(_WIN32)
/* UTF-8 validation examines the invocation backing without copying it. */
static int wf_utf8_valid(const unsigned char *text, uint64_t length) {
    uint64_t at = 0;
    while (at < length) {
        unsigned first = text[at++];
        unsigned continuation;
        unsigned lower = 0x80, upper = 0xbf;
        if (first < 0x80) continue;
        if (first >= 0xc2 && first <= 0xdf) {
            continuation = 1;
        } else if (first >= 0xe0 && first <= 0xef) {
            continuation = 2;
            if (first == 0xe0) lower = 0xa0;
            if (first == 0xed) upper = 0x9f;
        } else if (first >= 0xf0 && first <= 0xf4) {
            continuation = 3;
            if (first == 0xf0) lower = 0x90;
            if (first == 0xf4) upper = 0x8f;
        } else return 0;
        if (length - at < continuation || text[at] < lower || text[at] > upper)
            return 0;
        at++;
        while (--continuation != 0) {
            if (text[at] < 0x80 || text[at] > 0xbf) return 0;
            at++;
        }
    }
    return 1;
}
#endif

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
        result->error.tag = 1;
        return;
    }
    if (measured.value > end - start) {
        result->tag = 1;
        result->error.required = measured.value;
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
    result->value = start + measured.value;
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
    result->value = saved;
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
    error->detail[tag].code = (uint32_t)code;
    error->detail[tag].origin = (uint8_t)origin;
}

static void wf_error(wf_io_error *error, int code, unsigned origin) {
    unsigned tag = 27;
#if defined(_WIN32)
    switch ((unsigned)code) {
    case ERROR_FILE_NOT_FOUND: case ERROR_PATH_NOT_FOUND: tag = 0; break;
    case ERROR_ACCESS_DENIED: case ERROR_NETWORK_ACCESS_DENIED: case ERROR_PRIVILEGE_NOT_HELD: tag = 1; break;
    case ERROR_ALREADY_EXISTS: case ERROR_FILE_EXISTS: tag = 2; break;
    case ERROR_DIRECTORY: tag = 3; break;
    case ERROR_DIR_NOT_EMPTY: tag = 5; break;
    case ERROR_WRITE_PROTECT: tag = 6; break;
    case ERROR_SHARING_VIOLATION: case ERROR_LOCK_VIOLATION: case ERROR_BUSY: tag = 7; break;
    case ERROR_INVALID_PARAMETER: tag = 8; break;
    case ERROR_INVALID_NAME: case ERROR_BAD_PATHNAME: case ERROR_FILENAME_EXCED_RANGE: tag = 9; break;
    case ERROR_INVALID_FUNCTION: case ERROR_NOT_SUPPORTED: case ERROR_CALL_NOT_IMPLEMENTED: tag = 10; break;
    case ERROR_TIMEOUT: case ERROR_SEM_TIMEOUT: tag = 11; break;
    case ERROR_BROKEN_PIPE: case ERROR_NO_DATA: case ERROR_PIPE_NOT_CONNECTED: tag = 12; break;
    case ERROR_CONNECTION_REFUSED: tag = 15; break;
    case ERROR_NETNAME_DELETED: tag = 16; break;
    case ERROR_CONNECTION_ABORTED: tag = 16; break;
    case ERROR_REQUEST_ABORTED: tag = 17; break;
    case ERROR_NOT_CONNECTED: tag = 18; break;
    case 10048: tag = 19; break; /* WSAEADDRINUSE. */
    case 10049: tag = 20; break; /* WSAEADDRNOTAVAIL. */
    case ERROR_TOO_MANY_OPEN_FILES: case ERROR_NOT_ENOUGH_MEMORY: case ERROR_OUTOFMEMORY: case ERROR_NO_SYSTEM_RESOURCES: tag = 21; break;
    case ERROR_FILE_TOO_LARGE: tag = 22; break;
    case ERROR_DISK_FULL: case ERROR_HANDLE_DISK_FULL: tag = 23; break;
    case ERROR_NOT_ENOUGH_QUOTA: tag = 24; break;
    case ERROR_NOT_SAME_DEVICE: tag = 25; break;
    case ERROR_NOT_READY: case ERROR_CRC: case ERROR_GEN_FAILURE: case ERROR_IO_DEVICE: tag = 26; break;
    default: break;
    }
    wf_error_class(error, tag, code, origin);
#else
    switch (code) {
    case ENOENT: tag = 0; break;
    case EACCES: case EPERM: tag = 1; break;
    case EEXIST: tag = 2; break;
    case ENOTDIR: tag = 3; break;
    case EISDIR: tag = 4; break;
    case ENOTEMPTY: tag = 5; break;
    case EROFS: tag = 6; break;
    case EBUSY: case ETXTBSY: tag = 7; break;
    case EINVAL: tag = 8; break;
    case ENAMETOOLONG: case ELOOP: tag = 9; break;
    case ENOSYS: case ENOTSUP: tag = 10; break;
#if EOPNOTSUPP != ENOTSUP
    case EOPNOTSUPP: tag = 10; break;
#endif
    case ETIMEDOUT: tag = 11; break;
    case EPIPE: tag = 12; break;
    case ECONNREFUSED: tag = 15; break;
    case ECONNRESET: tag = 16; break;
    case ECONNABORTED: tag = 17; break;
    case ENOTCONN: tag = 18; break;
    case EADDRINUSE: tag = 19; break;
    case EADDRNOTAVAIL: tag = 20; break;
    case EMFILE: case ENFILE: case ENOMEM: case ENOBUFS: tag = 21; break;
    case EFBIG: case EOVERFLOW: tag = 22; break;
    case ENOSPC: tag = 23; break;
#ifdef EDQUOT
    case EDQUOT: tag = 24; break;
#endif
    case EXDEV: tag = 25; break;
    case EIO: case ENXIO: case ENODEV: tag = 26; break;
    default: break;
    }
    wf_error_class(error, tag, code, origin);
#endif
}

static int wf_factory_take(wf_value *factory, wf_io_error *error) {
    wf_transition(factory);
    if (factory->words[0] == 0) {
        wf_error_class(error, 21, 0, 0);
        return 0;
    }
    factory->words[0]--;
    return 1;
}

static void wf_factory_return(wf_value *factory) {
    if (factory->words[0] != UINT64_MAX) factory->words[0]++;
}

static void wf_read_result_value(wf_read_result *result, int64_t amount,
                                 int error, uint64_t start, uint64_t extent) {
    memset(result, 0, sizeof(*result));
    if (amount < 0) {
        result->tag = 1;
        result->error.tag = 1;
        wf_error(&result->error.error, error, 2);
    } else if (amount == 0 && extent != 0) {
        result->tag = 1;
    } else {
        result->value = start + (uint64_t)amount;
    }
}

static void wf_write_result_value(wf_write_result *result, int64_t amount,
                                  int error, uint64_t start, uint64_t extent) {
    memset(result, 0, sizeof(*result));
    if (amount < 0) {
        result->tag = 1;
        wf_error(&result->error, error, 3);
    } else if (amount == 0 && extent != 0) {
        result->tag = 1;
        wf_error_class(&result->error, 13, 0, 0);
    } else result->value = start + (uint64_t)amount;
}

void wf__body_read_at(wf_read_result *result, wf_value *factory, wf_value *file,
                wf_view *destination, uint64_t file_offset,
                uint64_t start, uint64_t end) {
    wf_completion_record record;
    int64_t amount;
    int error;
    wf_transition(factory);
    wf_transition(file);
    wf__completion_file_pread_submit(wf_descriptor(file),
        wf_window(destination, start), end - start, file_offset, &record);
    wf__completion_file_join(&record, &amount, &error);
    wf_read_result_value(result, amount, error, start, end - start);
}

void wf__body_read_next(wf_read_result *result, wf_value *factory, wf_value *input,
                  wf_view *destination, uint64_t start, uint64_t end) {
    wf_completion_record record;
    int64_t amount;
    int error;
    wf_transition(factory);
    wf_transition(input);
    wf__completion_file_read_submit(wf_descriptor(input),
        wf_window(destination, start), end - start, &record);
    wf__completion_file_join(&record, &amount, &error);
    wf_read_result_value(result, amount, error, start, end - start);
}

void wf__body_write_once(wf_write_result *result, wf_value *factory, wf_value *output,
                   const wf_view *source, uint64_t start, uint64_t end) {
    wf_completion_record record;
    int64_t amount;
    int error;
    wf_transition(factory);
    wf_transition(output);
    wf__completion_file_write_submit(wf_descriptor(output),
        wf_window(source, start), end - start, &record);
    wf__completion_file_join(&record, &amount, &error);
    wf_write_result_value(result, amount, error, start, end - start);
}

void wf__body_receive_next(wf_read_result *result, wf_value *receive,
                     wf_view *destination, uint64_t start, uint64_t end) {
    wf_completion_record record;
    int64_t amount;
    int error;
    wf_transition(receive);
    wf__completion_socket_receive_submit(wf_descriptor(receive),
        wf_window(destination, start), end - start, &record);
    wf__completion_file_join(&record, &amount, &error);
    wf_read_result_value(result, amount, error, start, end - start);
}

void wf__body_send_once(wf_write_result *result, wf_value *send,
                  const wf_view *source, uint64_t start, uint64_t end) {
    wf_completion_record record;
    int64_t amount;
    int error;
    wf_transition(send);
    wf__completion_socket_send_submit(wf_descriptor(send),
        wf_window(source, start), end - start, &record);
    wf__completion_file_join(&record, &amount, &error);
    wf_write_result_value(result, amount, error, start, end - start);
}

#if defined(_WIN32)
#define WF_COMPONENT_BYTES 510u
#define WF_OPEN_DIRECTORY_FLAGS 0
#define WF_OPEN_COMPONENT_DIRECTORY_FLAGS 1
#define WF_OPEN_COMPONENT_FILE_FLAGS 1
#elif defined(__APPLE__)
#define WF_COMPONENT_BYTES 1023u
#define WF_OPEN_DIRECTORY_FLAGS O_DIRECTORY
#define WF_OPEN_COMPONENT_DIRECTORY_FLAGS (O_DIRECTORY | O_NOFOLLOW)
#define WF_OPEN_COMPONENT_FILE_FLAGS (O_NOFOLLOW | O_NONBLOCK)
#else
#define WF_COMPONENT_BYTES 255u
#define WF_OPEN_DIRECTORY_FLAGS O_DIRECTORY
#define WF_OPEN_COMPONENT_DIRECTORY_FLAGS (O_DIRECTORY | O_NOFOLLOW)
#define WF_OPEN_COMPONENT_FILE_FLAGS (O_NOFOLLOW | O_NONBLOCK)
#endif

static void wf_open(wf_open_result *result, wf_value *factory,
                    const wf_value *root, const void *path, int flags,
                    unsigned expected_kind, unsigned descriptor_class) {
    wf_completion_record record;
    int64_t descriptor;
    int error;
    unsigned outcome;
    memset(result, 0, sizeof(*result));
    if (!wf_factory_take(factory, &result->error)) {
        result->tag = 1;
        return;
    }
#if !defined(_WIN32)
    (void)descriptor_class;
#endif
    wf__completion_file_open_at_submit(wf_descriptor(root), path, flags, 0, 0,
        expected_kind,
#if defined(_WIN32)
        descriptor_class,
#endif
        &record);
    wf__completion_file_open_join(&record, &descriptor, &error, &outcome);
    if (outcome != WF_FILE_OPEN_SUCCEEDED) {
        wf_factory_return(factory);
        result->tag = 1;
        if (outcome == WF_FILE_OPEN_IS_DIRECTORY)
            wf_error_class(&result->error, 4, 0, 0);
        else if (outcome == WF_FILE_OPEN_OTHER_KIND)
            wf_error_class(&result->error, 10, 0, 0);
        else wf_error(&result->error, error,
                      outcome == WF_FILE_OPEN_STATUS_FAILED ? 4 : 1);
        return;
    }
    wf_descriptor_value(&result->value, (int)descriptor);
}

void wf__body_open_read(wf_open_result *result, wf_value *factory,
                  const wf_value *root, const wf_value *path) {
    wf_open(result, factory, root, wf_value_pointer(path), 0,
            WF_FILE_EXPECT_REGULAR, 1);
}

static int wf_component(unsigned char *component, const wf_view *name,
                         uint64_t start, uint64_t end) {
    uint64_t length = end - start;
    uint64_t index;
    const unsigned char *text = wf_window(name, start);
    if (length == 0 || length > WF_COMPONENT_BYTES) return 0;
#if defined(_WIN32)
    if ((length & 1) != 0) return 0;
    for (index = 0; index < length; index += 2) {
        uint16_t unit;
        memcpy(&unit, text + index, 2);
        if (unit == 0 || unit == '/' || unit == '\\') return 0;
    }
    component[length] = 0;
    component[length + 1] = 0;
#else
    for (index = 0; index < length; index++)
        if (text[index] == 0 || text[index] == '/') return 0;
    component[length] = 0;
#endif
    memcpy(component, text, (size_t)length);
    return 1;
}

static void wf_open_component(wf_open_result *result, wf_value *factory,
                              const wf_value *root, const wf_view *name,
                              uint64_t start, uint64_t end, unsigned directory) {
    alignas(2) unsigned char component[WF_COMPONENT_BYTES + 2];
    if (!wf_component(component, name, start, end)) {
        wf_transition(factory);
        memset(result, 0, sizeof(*result));
        result->tag = 1;
        wf_error_class(&result->error, 9, 0, 0);
        return;
    }
    wf_open(result, factory, root, component,
        directory ? WF_OPEN_COMPONENT_DIRECTORY_FLAGS : WF_OPEN_COMPONENT_FILE_FLAGS,
        directory ? WF_FILE_EXPECT_DIRECTORY : WF_FILE_EXPECT_REGULAR,
        directory ? 2 : 1);
}

void wf__body_open_directory(wf_open_result *result, wf_value *factory,
                       const wf_value *root, const wf_view *name,
                       uint64_t start, uint64_t end) {
    wf_open_component(result, factory, root, name, start, end, 1);
}

void wf__body_open_file(wf_open_result *result, wf_value *factory, const wf_value *root,
                  const wf_view *name, uint64_t start, uint64_t end) {
    wf_open_component(result, factory, root, name, start, end, 0);
}

void wf__body_open_directory_source(wf_open_result *result, wf_value *factory,
                              const wf_value *directory) {
#if defined(_WIN32)
    /* NtCreateFile does not normalize the Win32 spelling ".". An empty
     * relative object name reopens the supplied directory itself, producing
     * an independent enumeration cursor rather than a duplicated handle. */
    static const uint16_t self[] = { 0 };
#else
    static const char self[] = ".";
#endif
    wf_open(result, factory, directory, self, WF_OPEN_DIRECTORY_FLAGS,
            WF_FILE_EXPECT_DIRECTORY, 3);
}

static void wf_close(wf_close_result *result, wf_value *factory,
                     const wf_value *owner, int direction) {
    wf_completion_record record;
    int64_t amount;
    int error;
    wf_transition(factory);
    memset(result, 0, sizeof(*result));
    if (direction < 0)
        wf__completion_file_close_submit(wf_descriptor(owner), &record);
    else wf__completion_socket_shutdown_submit(wf_descriptor(owner),
                                               (unsigned)direction, &record);
    wf__completion_file_join(&record, &amount, &error);
    /* A close consumes its owner even when the host reports an error. An
     * interrupted close is never retried against a possibly reused number.
     * A half-close returns credit only on the actual descriptor-close attempt. */
    if (direction < 0 || amount == 1) wf_factory_return(factory);
    if (error != 0) {
        result->tag = 1;
        wf_error(&result->error, error, 8);
    }
}

void wf__body_close_read(wf_close_result *result, wf_value *factory, const wf_value *file) {
    wf_close(result, factory, file, -1);
}
void wf__body_close_directory(wf_close_result *result, wf_value *factory, const wf_value *directory) {
    wf_close(result, factory, directory, -1);
}
void wf__body_close_directory_source(wf_close_result *result, wf_value *factory, const wf_value *source) {
    wf_close(result, factory, source, -1);
}
void wf__body_close_listener(wf_close_result *result, wf_value *factory, const wf_value *listener) {
    wf_close(result, factory, listener, -1);
}
void wf__body_close_receive(wf_close_result *result, wf_value *factory, const wf_value *receive) {
    wf_close(result, factory, receive, WF_SOCKET_DIRECTION_RECEIVE);
}
void wf__body_close_send(wf_close_result *result, wf_value *factory, const wf_value *send) {
    wf_close(result, factory, send, WF_SOCKET_DIRECTION_SEND);
}

void wf__body_tcp_listen(wf_open_result *result, wf_value *factory, const wf_value *address) {
    wf_completion_record record;
    int64_t descriptor;
    int error;
    memset(result, 0, sizeof(*result));
    if (!wf_factory_take(factory, &result->error)) {
        result->tag = 1;
        return;
    }
    wf__completion_socket_listen_submit(address->words[0], address->words[1],
                                        (uint32_t)address->words[2], &record);
    wf__completion_file_join(&record, &descriptor, &error);
    if (descriptor < 0) {
        wf_factory_return(factory);
        result->tag = 1;
        wf_error(&result->error, error, 5);
    } else wf_descriptor_value(&result->value, (int)descriptor);
}

void wf__body_tcp_connect(wf_connect_result *result, wf_value *factory, const wf_value *address) {
    wf_completion_record record;
    int64_t descriptor;
    int error;
    memset(result, 0, sizeof(*result));
    if (!wf_factory_take(factory, &result->error)) {
        result->tag = 1;
        return;
    }
    wf__completion_socket_connect_submit(address->words[0], address->words[1],
                                         (uint32_t)address->words[2], &record);
    wf__completion_file_join(&record, &descriptor, &error);
    if (descriptor < 0) {
        wf_factory_return(factory);
        result->tag = 1;
        wf_error(&result->error, error, 7);
    } else {
        wf_descriptor_value(&result->value.receive, (int)descriptor);
        wf_descriptor_value(&result->value.send, (int)descriptor);
    }
}

void wf__body_tcp_accept(wf_accept_result *result, wf_value *factory, wf_value *listener) {
    wf_completion_record record;
    int64_t descriptor;
    int error;
    uint64_t low, high;
    uint32_t tag;
    memset(result, 0, sizeof(*result));
    wf_transition(listener);
    if (!wf_factory_take(factory, &result->error)) {
        result->tag = 1;
        return;
    }
    wf__completion_socket_accept_submit(wf_descriptor(listener), &record);
    wf__completion_socket_accept_join(&record, &descriptor, &error, &low, &high, &tag);
    if (descriptor < 0) {
        wf_factory_return(factory);
        result->tag = 1;
        wf_error(&result->error, error, 6);
    } else {
        wf_descriptor_value(&result->value.connection.receive, (int)descriptor);
        wf_descriptor_value(&result->value.connection.send, (int)descriptor);
        result->value.peer.words[0] = low;
        result->value.peer.words[1] = high;
        result->value.peer.words[2] = tag;
    }
}

#if !defined(_WIN32)
#include <sys/resource.h>
#endif

int wf__ordinary_inputs(wf_inputs *inputs, int argc, void *argv) {
    int cwd;
    uint64_t capacity;
    memset(inputs, 0, sizeof(*inputs));
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
        capacity = limit.rlim_cur == RLIM_INFINITY || limit.rlim_cur > (1u << 20)
            ? (1u << 20) : (uint64_t)limit.rlim_cur;
        capacity = capacity > 64 ? capacity - 64 : 0;
    }
    wf_descriptor_value(&inputs->out, STDOUT_FILENO);
    wf_descriptor_value(&inputs->err, STDERR_FILENO);
    wf_descriptor_value(&inputs->in, STDIN_FILENO);
#endif
    if (cwd < 0) return 0;
    wf_descriptor_value(&inputs->cwd, cwd);
    wf_text(&inputs->args, argv, argc > 0 ? (uint64_t)(unsigned)argc : 0);
    inputs->handles.words[0] = capacity;
    return 1;
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

void wf__body_directory_next(wf_list_result *result, wf_value *source,
                        wf_view *destination, uint64_t start, uint64_t end) {
    wf_completion_record record;
    int64_t amount, position = 0;
    int error;
    uint64_t cursor = 0, written = 0, entries = 0;
    unsigned char *window = wf_window(destination, start);
    wf_transition(source);
    wf__completion_directory_next_submit(wf_descriptor(source), window,
                                         end - start, &position, &record);
    wf__completion_file_join(&record, &amount, &error);
    memset(result, 0, sizeof(*result));
    result->next = start;
    if (amount < 0) {
        result->result.tag = 1;
        result->result.error.tag = 1;
        wf_error(&result->result.error.error, error, 2);
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
        if (named == 0 || named > WF_COMPONENT_BYTES ||
            named > extent - WF_DIRENT_NAME || named + 3 > end - start - written)
            abort();
#if defined(_WIN32)
        if ((named & 1) != 0) abort();
        for (index = 0; index < named; index += 2) {
            uint16_t unit;
            memcpy(&unit, entry + WF_DIRENT_NAME + index, 2);
            if (unit == 0 || unit == '/' || unit == '\\') abort();
        }
#else
        for (index = 0; index < named; index++)
            if (entry[WF_DIRENT_NAME + index] == 0 || entry[WF_DIRENT_NAME + index] == '/')
                abort();
#endif
        portable = kind == 0 ? 0 : kind == WF_DIRENT_REGULAR ? 1 :
            kind == WF_DIRENT_DIRECTORY ? 2 : kind == WF_DIRENT_SYMLINK ? 3 : 4;
        window[written] = (unsigned char)portable;
        window[written + 1] = (unsigned char)named;
        window[written + 2] = (unsigned char)(named >> 8);
        memmove(window + written + 3, entry + WF_DIRENT_NAME, (size_t)named);
        written += named + 3;
        cursor += extent;
        entries++;
    }
    result->next = start + written;
    result->entries = entries;
}
