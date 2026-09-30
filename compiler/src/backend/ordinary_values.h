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
/* An enum with two or more payload-carrying variants whose tag-then-every-
 * payload product would not return in registers is laid out as a union of
 * per-variant views, each the tag followed by that variant's fields
 * (compiler/payload-enum-layout). C spells it as a union of structs that each
 * begin with the tag; `tag` reads it in every view. Every view of `IoError`
 * but `DeadlinePassed`'s carries the same two fields, and that one carries
 * none, so one struct is all of them. */
typedef struct { uint32_t tag; uint32_t code; uint8_t origin; } wf_io_error;
/* `IoError`'s variants in the order `io/module.wfm` declares them [PRE-2],
 * which is the order that numbers their tags; a test holds the two lists
 * equal. */
enum wf_io_error_tag {
    WF_IO_NOT_FOUND,
    WF_IO_PERMISSION_DENIED,
    WF_IO_ALREADY_EXISTS,
    WF_IO_NOT_DIRECTORY,
    WF_IO_IS_DIRECTORY,
    WF_IO_DIRECTORY_NOT_EMPTY,
    WF_IO_READ_ONLY,
    WF_IO_RESOURCE_BUSY,
    WF_IO_INVALID_INPUT,
    WF_IO_INVALID_PATH,
    WF_IO_UNSUPPORTED,
    WF_IO_TIMED_OUT,
    WF_IO_DEADLINE_PASSED,
    WF_IO_BROKEN_PIPE,
    WF_IO_WRITE_ZERO,
    WF_IO_UNEXPECTED_END,
    WF_IO_CONNECTION_REFUSED,
    WF_IO_CONNECTION_RESET,
    WF_IO_CONNECTION_ABORTED,
    WF_IO_NOT_CONNECTED,
    WF_IO_ADDRESS_IN_USE,
    WF_IO_ADDRESS_UNAVAILABLE,
    WF_IO_RESOURCE_EXHAUSTED,
    WF_IO_FILE_TOO_LARGE,
    WF_IO_NO_SPACE,
    WF_IO_QUOTA_EXCEEDED,
    WF_IO_CROSS_DEVICE,
    WF_IO_DEVICE_FAILURE,
    WF_IO_OTHER
};
typedef struct { uint32_t tag; uint64_t required; } wf_copy_error;
typedef struct { uint32_t tag; wf_io_error error; } wf_read_stop;
typedef struct { wf_value receive; wf_value send; } wf_connection;
typedef struct { wf_connection connection; wf_value peer; } wf_accepted_connection;
#define WF_RESULT_UNION(name, ok_type, error_type) \
    typedef union { \
        uint32_t tag; \
        struct { uint32_t tag; ok_type value; } ok; \
        struct { uint32_t tag; error_type error; } err; \
    } name
WF_RESULT_UNION(wf_value_result, wf_value, uint8_t);
WF_RESULT_UNION(wf_copy_result, uint64_t, wf_copy_error);
/* Its tag, value and one-bit error fit the return registers, so it keeps the
 * product layout. */
typedef struct { uint32_t tag; uint64_t value; uint8_t error; } wf_utf8_result;
WF_RESULT_UNION(wf_write_result, uint64_t, wf_io_error);
WF_RESULT_UNION(wf_read_result, uint64_t, wf_read_stop);
typedef wf_read_stop wf_list_stop;
WF_RESULT_UNION(wf_list_status, uint8_t, wf_list_stop);
typedef struct { wf_list_status result; uint64_t next; uint64_t entries; } wf_list_result;
WF_RESULT_UNION(wf_close_result, uint8_t, wf_io_error);
WF_RESULT_UNION(wf_open_result, wf_value, wf_io_error);
WF_RESULT_UNION(wf_connect_result, wf_connection, wf_io_error);
WF_RESULT_UNION(wf_accept_result, wf_accepted_connection, wf_io_error);
#undef WF_RESULT_UNION
/* `Option<Instant>`: one payload-carrying variant keeps the product layout,
 * the tag and then the instant, whose first word is its reading in
 * nanoseconds of the monotonic clock. */
typedef struct { uint32_t tag; wf_value value; } wf_deadline;
#define WF_OPTION_SOME 1u
/* `Inputs`, its fields in declaration order; `cwd` is the two halves of a
 * `Directory`. */
typedef struct {
    wf_value args, cwd_read, cwd_write, out, err, handles, in, clock, wall_clock;
} wf_inputs;

_Static_assert(sizeof(wf_value) == 32 && _Alignof(wf_value) == 16, "ordinary opaque layout");
_Static_assert(sizeof(wf_io_error) == 12, "ordinary error enum layout");
_Static_assert(sizeof(wf_value_result) == 48, "ordinary value Result layout");
_Static_assert(sizeof(wf_copy_result) == 24, "ordinary copy Result layout");
_Static_assert(sizeof(wf_write_result) == 16, "ordinary write Result layout");
_Static_assert(sizeof(wf_read_result) == 24, "ordinary read Result layout");
_Static_assert(sizeof(wf_list_status) == 20, "ordinary directory status Result layout");
_Static_assert(offsetof(wf_list_result, next) == 24 &&
               offsetof(wf_list_result, entries) == 32 &&
               sizeof(wf_list_result) == 40, "ordinary directory three-result layout");
_Static_assert(sizeof(wf_close_result) == 16, "ordinary close Result layout");
_Static_assert(sizeof(wf_open_result) == 48, "ordinary open Result layout");
_Static_assert(sizeof(wf_connect_result) == 80, "ordinary connect Result layout");
_Static_assert(sizeof(wf_accepted_connection) == 96, "ordinary AcceptedConnection layout");
_Static_assert(offsetof(wf_accept_result, ok.value) == 16 &&
               offsetof(wf_accept_result, err.error) == 4 &&
               sizeof(wf_accept_result) == 112, "ordinary accept Result layout");
_Static_assert(offsetof(wf_deadline, value) == 16 && sizeof(wf_deadline) == 48,
               "ordinary Option<Instant> layout");
_Static_assert(sizeof(wf_inputs) == 288, "ordinary Inputs layout");

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
void wf__body_write_once(wf_write_result *result, wf_value *factory, wf_value *output, const wf_view *source, uint64_t start, uint64_t end, const wf_deadline *deadline);
void wf__body_exit_status(wf_value *result, uint8_t code);
void wf__body_open_directory(wf_open_result *result, wf_value *factory, const wf_value *root, const wf_view *name, uint64_t start, uint64_t end);
void wf__body_open_directory_source(wf_open_result *result, wf_value *factory, const wf_value *directory);
void wf__body_directory_next(wf_list_result *result, wf_value *source, wf_view *destination, uint64_t start, uint64_t end);
void wf__body_open_file(wf_open_result *result, wf_value *factory, const wf_value *root, const wf_view *name, uint64_t start, uint64_t end);
void wf__body_close_read(wf_close_result *result, wf_value *factory, const wf_value *file);
void wf__body_close_directory(wf_close_result *result, wf_value *factory, const wf_value *directory);
void wf__body_close_directory_source(wf_close_result *result, wf_value *factory, const wf_value *source);
void wf__body_read_next(wf_read_result *result, wf_value *factory, wf_value *input, wf_view *destination, uint64_t start, uint64_t end, const wf_deadline *deadline);
void wf__body_socket_address_v4(wf_value *result, uint8_t a, uint8_t b, uint8_t c, uint8_t d, uint16_t port);
void wf__body_socket_address_v6(wf_value *result, uint16_t a, uint16_t b, uint16_t c, uint16_t d, uint16_t e, uint16_t f, uint16_t g, uint16_t h, uint16_t port);
void wf__body_tcp_listen(wf_open_result *result, wf_value *factory, const wf_value *address);
void wf__body_tcp_accept(wf_accept_result *result, wf_value *factory, wf_value *listener, const wf_deadline *deadline);
void wf__body_tcp_connect(wf_connect_result *result, wf_value *factory, const wf_value *address, const wf_deadline *deadline);
void wf__body_receive_next(wf_read_result *result, wf_value *receive, wf_view *destination, uint64_t start, uint64_t end, const wf_deadline *deadline);
void wf__body_send_once(wf_write_result *result, wf_value *send, const wf_view *source, uint64_t start, uint64_t end, const wf_deadline *deadline);
void wf__body_close_listener(wf_close_result *result, wf_value *factory, const wf_value *listener);
void wf__body_close_receive(wf_close_result *result, wf_value *factory, const wf_value *receive);
void wf__body_close_send(wf_close_result *result, wf_value *factory, const wf_value *send);
void wf__body_factory_share(wf_value *result, const wf_value *factory);
void wf__body_open_append(wf_open_result *result, wf_value *factory, const wf_value *root, const wf_view *name, uint64_t start, uint64_t end);
void wf__body_append_once(wf_write_result *result, wf_value *factory, wf_value *file, const wf_view *source, uint64_t start, uint64_t end);
void wf__body_sync_file(wf_close_result *result, wf_value *factory, wf_value *file);
void wf__body_close_write(wf_close_result *result, wf_value *factory, const wf_value *file);
void wf__body_close_directory_write(wf_close_result *result, wf_value *factory, const wf_value *directory);
void wf__body_clock_share(wf_value *result, const wf_value *clock);
void wf__body_wall_clock_share(wf_value *result, const wf_value *clock);
void wf__body_now(wf_value *result, wf_value *clock);
void wf__body_instant_after(wf_value *result, const wf_value *instant, uint64_t nanoseconds);
uint64_t wf__body_nanoseconds_from(const wf_value *earlier, const wf_value *later);
_Bool wf__body_instant_reached(const wf_value *deadline, const wf_value *instant);
int64_t wf__body_unix_nanoseconds(const wf_value *clock);
void wf__body_sleep_until(uint8_t *result, const wf_value *deadline);

/* Build launcher support: constructs ordinary argument representations. The
 * supplied argument backing remains valid until the selected call returns.
 * Failure is a build launcher failure and never a source-language verdict. */
int wf__ordinary_inputs(wf_inputs *inputs, int argc, void *argv);
uint8_t wf__ordinary_exit_code(const wf_value *status);
#endif
