#ifndef WHITEFOOT_ORDINARY_VALUES_H
#define WHITEFOOT_ORDINARY_VALUES_H

/* Ordinary linked definitions. These declarations describe the library's C
 * representation, not additional compiler metadata. Every C body writes its
 * aggregate result through a destination pointer and takes aggregate
 * parameters by content address. Where the compiler's ABI differs, a
 * `wf__body_` C body sits behind the LLVM definition in ordinary_values.ll.
 * The ABI differs for a split range argument, and for a result small enough
 * for the return registers. */
#include <stddef.h>
#include <stdint.h>
#include <stdalign.h>

typedef struct { alignas(16) uint64_t words[4]; } wf_value;
typedef struct { void *data; uint64_t length; } wf_view;
typedef struct { uint32_t code; uint8_t origin; } wf_error_detail;
typedef struct { uint32_t tag; wf_error_detail detail[28]; } wf_io_error;
typedef struct { uint32_t tag; wf_value value; uint8_t error; } wf_value_result;
typedef struct { uint32_t tag; uint64_t required; } wf_copy_error;
typedef struct { uint32_t tag; uint64_t value; wf_copy_error error; } wf_copy_result;
typedef struct { uint32_t tag; uint64_t value; uint8_t error; } wf_utf8_result;
typedef struct { uint32_t tag; uint64_t value; wf_io_error error; } wf_write_result;
typedef struct { uint32_t tag; wf_io_error error; } wf_read_stop;
typedef struct { uint32_t tag; uint64_t value; wf_read_stop error; } wf_read_result;
typedef struct { uint32_t tag; uint8_t value; wf_read_stop error; } wf_list_status;
typedef struct { wf_list_status result; uint64_t next; uint64_t entries; } wf_list_result;
typedef struct { uint32_t tag; uint8_t value; wf_io_error error; } wf_close_result;
typedef struct { uint32_t tag; wf_value value; wf_io_error error; } wf_open_result;
typedef struct { wf_value receive; wf_value send; } wf_connection;
typedef struct { wf_connection connection; wf_value peer; } wf_accepted_connection;
typedef struct { uint32_t tag; wf_connection value; wf_io_error error; } wf_connect_result;
typedef struct { uint32_t tag; wf_accepted_connection value; wf_io_error error; } wf_accept_result;
typedef struct {
    wf_value args, cwd, out, err, handles, in;
} wf_inputs;

_Static_assert(sizeof(wf_value) == 32 && _Alignof(wf_value) == 16, "ordinary opaque layout");
_Static_assert(sizeof(wf_io_error) == 228, "ordinary error enum layout");
_Static_assert(sizeof(wf_read_result) == 248, "ordinary read Result layout");
_Static_assert(sizeof(wf_list_status) == 240, "ordinary directory status Result layout");
_Static_assert(offsetof(wf_list_result, next) == 240 &&
               offsetof(wf_list_result, entries) == 248 &&
               sizeof(wf_list_result) == 256, "ordinary directory three-result layout");
_Static_assert(sizeof(wf_open_result) == 288, "ordinary open Result layout");
_Static_assert(sizeof(wf_connect_result) == 320, "ordinary connect Result layout");
_Static_assert(sizeof(wf_accepted_connection) == 96, "ordinary AcceptedConnection layout");
_Static_assert(offsetof(wf_accept_result, value) == 16 &&
               offsetof(wf_accept_result, error) == 112 &&
               sizeof(wf_accept_result) == 352, "ordinary accept Result layout");
_Static_assert(sizeof(wf_inputs) == 192, "ordinary Inputs layout");

/* A host function's link name is its standard library identity [MOD-10],
 * `wf_std.<module>.<name>`, which no program function can take and no C
 * identifier can spell: ordinary_values.ll defines each one over the C body
 * below, `wf__body_<name>`. */
uint64_t wf__body_args_count(const wf_value *args);
void wf__body_arg_get(wf_value_result *result, const wf_value *args, uint64_t position);
uint64_t wf__body_host_bytes_len(const wf_value *value);
void wf__body_host_copy_bytes(wf_copy_result *result, const wf_value *value, wf_view *destination, uint64_t start, uint64_t end);
void wf__body_host_utf8_len(wf_utf8_result *result, const wf_value *value);
void wf__body_host_copy_utf8(wf_copy_result *result, const wf_value *value, wf_view *destination, uint64_t start, uint64_t end);
void wf__body_relative_path(wf_value_result *result, const wf_value *value);
void wf__body_open_read(wf_open_result *result, wf_value *factory, const wf_value *root, const wf_value *path);
void wf__body_read_at(wf_read_result *result, wf_value *factory, wf_value *file, wf_view *destination, uint64_t file_offset, uint64_t start, uint64_t end);
void wf__body_write_once(wf_write_result *result, wf_value *factory, wf_value *output, const wf_view *source, uint64_t start, uint64_t end);
void wf__body_exit_status(wf_value *result, uint8_t code);
void wf__body_open_directory(wf_open_result *result, wf_value *factory, const wf_value *root, const wf_view *name, uint64_t start, uint64_t end);
void wf__body_open_directory_source(wf_open_result *result, wf_value *factory, const wf_value *directory);
void wf__body_directory_next(wf_list_result *result, wf_value *source, wf_view *destination, uint64_t start, uint64_t end);
void wf__body_open_file(wf_open_result *result, wf_value *factory, const wf_value *root, const wf_view *name, uint64_t start, uint64_t end);
void wf__body_close_read(wf_close_result *result, wf_value *factory, const wf_value *file);
void wf__body_close_directory(wf_close_result *result, wf_value *factory, const wf_value *directory);
void wf__body_close_directory_source(wf_close_result *result, wf_value *factory, const wf_value *source);
void wf__body_read_next(wf_read_result *result, wf_value *factory, wf_value *input, wf_view *destination, uint64_t start, uint64_t end);
void wf__body_socket_address_v4(wf_value *result, uint8_t a, uint8_t b, uint8_t c, uint8_t d, uint16_t port);
void wf__body_socket_address_v6(wf_value *result, uint16_t a, uint16_t b, uint16_t c, uint16_t d, uint16_t e, uint16_t f, uint16_t g, uint16_t h, uint16_t port);
void wf__body_tcp_listen(wf_open_result *result, wf_value *factory, const wf_value *address);
void wf__body_tcp_accept(wf_accept_result *result, wf_value *factory, wf_value *listener);
void wf__body_tcp_connect(wf_connect_result *result, wf_value *factory, const wf_value *address);
void wf__body_receive_next(wf_read_result *result, wf_value *receive, wf_view *destination, uint64_t start, uint64_t end);
void wf__body_send_once(wf_write_result *result, wf_value *send, const wf_view *source, uint64_t start, uint64_t end);
void wf__body_close_listener(wf_close_result *result, wf_value *factory, const wf_value *listener);
void wf__body_close_receive(wf_close_result *result, wf_value *factory, const wf_value *receive);
void wf__body_close_send(wf_close_result *result, wf_value *factory, const wf_value *send);

/* Build launcher support: constructs ordinary argument representations. The
 * supplied argument backing remains valid until the selected call returns.
 * Failure is a build launcher failure and never a source-language verdict. */
int wf__ordinary_inputs(wf_inputs *inputs, int argc, void *argv);
uint8_t wf__ordinary_exit_code(const wf_value *status);
#endif
