/* Executable evidence for the ordinary linked library. The probe constructs
 * launcher values as a native caller would, then exercises ownership transfer,
 * refusal, ranges and explicit close. View operations enter their private
 * pointer-parameter C bodies; separate WF executions check the LLVM ABI. */
#if !defined(_WIN32)
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#endif
#include "ordinary_values.h"
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <errno.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#include <winsock2.h>
#include <ws2tcpip.h>
#include <direct.h>
#include "windows_runtime.h"
#define wf_chdir _chdir
#define wf_getcwd _getcwd
#define wf_mkdir(path) _mkdir(path)
#define wf_rmdir _rmdir
#define wf_pid() GetCurrentProcessId()
#else
#include <arpa/inet.h>
#include <fcntl.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <unistd.h>
#define wf_chdir chdir
#define wf_getcwd getcwd
#define wf_mkdir(path) mkdir(path, 0700)
#define wf_rmdir rmdir
#define wf_pid() getpid()
#endif
#include "sched/prim.h"
#include "runtime_test_guard.h"
#include "completion/socket_test.h"

#ifndef WF_COMPLETION_SHUTDOWN
#error "The ordinary-values probe requires its deterministic shutdown observer"
#endif

#if defined(_WIN32)
typedef SOCKET wf_probe_socket;
#define WF_PROBE_SEND SD_SEND
#define WF_PROBE_RECEIVE SD_RECEIVE
#else
typedef int wf_probe_socket;
#define WF_PROBE_SEND SHUT_WR
#define WF_PROBE_RECEIVE SHUT_RD
#endif

static struct {
    wf_prim_wait wait;
    wf_probe_socket socket;
    int direction;
    int armed;
    int paused;
    int resume;
    int finished;
} half_close;
static _Atomic int half_close_initialized;

/* Pause immediately before one real host shutdown. The other direction then
 * completes on a different caller: it must neither close the descriptor nor
 * return its credit while this host operation is still outstanding. */
int wf_ordinary_test_shutdown(wf_probe_socket socket, int direction) {
    if (!atomic_load(&half_close_initialized)) return shutdown(socket, direction);
    wf_prim_wait_lock(&half_close.wait);
    if (half_close.armed && half_close.socket == socket
        && half_close.direction == direction) {
        half_close.armed = 0;
        half_close.paused = 1;
        wf_prim_wait_signal(&half_close.wait);
        while (!half_close.resume) wf_prim_wait_sleep(&half_close.wait);
    }
    wf_prim_wait_unlock(&half_close.wait);
    return shutdown(socket, direction);
}

static wf_probe_socket native_socket(const wf_value *value) {
#if defined(_WIN32)
    return (SOCKET)wf__windows_socket_handle((int)value->words[0]);
#else
    return (int)value->words[0];
#endif
}

typedef struct {
    wf_value owner;
    wf_value factory;
    wf_close_result result;
    int send;
} half_close_call;

static void paused_half_close(void *argument) {
    half_close_call *call = argument;
    if (call->send) wf__body_close_send(&call->result, &call->factory, &call->owner);
    else wf__body_close_receive(&call->result, &call->factory, &call->owner);
    wf_prim_wait_lock(&half_close.wait);
    half_close.finished = 1;
    wf_prim_wait_signal(&half_close.wait);
    wf_prim_wait_unlock(&half_close.wait);
}

static void check_close(wf_close_result *result) { assert(result->tag == 0); }

static void text_probe(void) {
#if defined(_WIN32)
    static const uint16_t one[] = { 'a', 'b', 'c', 0 };
    static const uint16_t good[] = { 0xd83d, 0xde00, 0 };
    static const uint16_t bad[] = { 0xd800, 0 };
    static const uint16_t empty[] = { 0 };
    const void *words[] = { one, good, bad, empty };
#else
    const void *words[] = { "abc", "\xf0\x9f\x98\x80", "\xed\xa0\x80", "" };
#endif
    wf_value args = {{(uint64_t)(uintptr_t)words, 4, 0, 0}};
    wf_value_result value;
    wf_copy_result copied;
    wf_utf8_result measured;
    unsigned char bytes[16], expected[16];
    wf_view view = {bytes, sizeof(bytes)};
    assert(wf__body_args_count(&args) == 4);
    wf__body_arg_get(&value, &args, 0);
    assert(value.tag == 0);
    memset(bytes, 7, sizeof(bytes));
    wf__body_host_copy_utf8(&copied, &value.value, &view, 2, 3);
    memset(expected, 7, sizeof(expected));
    assert(copied.tag == 1 && copied.error.tag == 0 && copied.error.required == 3);
    assert(memcmp(bytes, expected, sizeof(bytes)) == 0);
    wf__body_host_copy_utf8(&copied, &value.value, &view, 2, 5);
    memcpy(expected + 2, "abc", 3);
    assert(copied.tag == 0 && copied.value == 5);
    assert(memcmp(bytes, expected, sizeof(bytes)) == 0);
    wf__body_arg_get(&value, &args, 1);
    wf__body_host_utf8_len(&measured, &value.value);
    assert(measured.tag == 0 && measured.value == 4);
    wf__body_host_copy_utf8(&copied, &value.value, &view, 5, 9);
    memcpy(expected + 5, "\xf0\x9f\x98\x80", 4);
    assert(copied.tag == 0 && copied.value == 9);
    assert(memcmp(bytes, expected, sizeof(bytes)) == 0);
    wf__body_arg_get(&value, &args, 2);
    wf__body_host_copy_utf8(&copied, &value.value, &view, 0, 16);
    assert(copied.tag == 1 && copied.error.tag == 1);
    assert(memcmp(bytes, expected, sizeof(bytes)) == 0);
    wf__body_arg_get(&value, &args, 3);
    wf__body_relative_path(&value, &value.value);
    assert(value.tag == 0 && value.value.words[1] == 0);
    wf__body_arg_get(&value, &args, 4);
    assert(value.tag == 1 && value.error == 0);
}

typedef void (*wf_probe_open)(wf_open_result *, wf_value *, const wf_value *, const wf_view *, uint64_t, uint64_t);
typedef void (*wf_probe_read)(wf_read_result *, wf_value *, wf_value *, wf_view *, uint64_t, uint64_t, uint64_t);
extern void wf_test_public_open(wf_open_result *, wf_value *, const wf_value *, const wf_view *, uint64_t, uint64_t);
extern void wf_test_public_read(wf_read_result *, wf_value *, wf_value *, wf_view *, uint64_t, uint64_t, uint64_t);

static void file_probe(wf_inputs *inputs, wf_probe_open open_file, wf_probe_read read_at) {
    static const char filename[] = "ordinary-values.data";
#if defined(_WIN32)
    static const uint16_t component[] = { 'o','r','d','i','n','a','r','y','-','v','a','l','u','e','s','.','d','a','t','a' };
#else
    static const unsigned char component[] = "ordinary-values.data";
#endif
    wf_view name = {(void *)component,
#if defined(_WIN32)
        sizeof(component)
#else
        sizeof(component) - 1
#endif
    };
    unsigned char bytes[4096];
    unsigned char unchanged[sizeof(bytes)];
    wf_view window = {bytes, sizeof(bytes)};
    wf_open_result opened, listing;
    wf_read_result read;
    wf_close_result closed;
    wf_value receiving_factory = {{0, 0, 0, 0}};
    wf_value limited_factory = {{1, 0, 0, 0}};
    uint64_t saved = inputs->handles.words[0];
    FILE *fixture = NULL;
#if defined(_WIN32)
    assert(fopen_s(&fixture, filename, "wb") == 0);
#else
    fixture = fopen(filename, "wb");
#endif
    assert(fixture != NULL);
    assert(fwrite("hello", 1, 5, fixture) == 5);
    assert(fclose(fixture) == 0);
    /* This valid component names a regular file, so opening it as a directory
     * fails after taking the sole credit, not at the quota refusal above the
     * host call. The next successful open must consume that same credit. */
    wf__body_open_directory(&listing, &limited_factory, &inputs->cwd, &name, 0, name.length);
#if defined(_WIN32)
    /* NtCreateFile opens without FILE_DIRECTORY_FILE; the production kind
     * check then refuses this regular object as Unsupported, with no host
     * error code. POSIX O_DIRECTORY instead fails in the host call below. */
    if (listing.tag != 1 || listing.error.tag != 10)
        fprintf(stderr, "directory kind refusal: result=%u error=%u\n",
                (unsigned)listing.tag, (unsigned)listing.error.tag);
    assert(listing.tag == 1 && listing.error.tag == 10);
    assert(listing.error.detail[10].code == 0 && listing.error.detail[10].origin == 0);
#else
    assert(listing.tag == 1 && listing.error.tag == 3);
    assert(listing.error.detail[3].code == ENOTDIR && listing.error.detail[3].origin == 1);
#endif
    assert(limited_factory.words[0] == 1);
    open_file(&opened, &limited_factory, &inputs->cwd, &name, 0, name.length);
    assert(opened.tag == 0 && limited_factory.words[0] == 0);
    wf__body_close_read(&closed, &limited_factory, &opened.value);
    check_close(&closed);
    assert(limited_factory.words[0] == 1);
    inputs->handles.words[0] = 0;
    open_file(&opened, &inputs->handles, &inputs->cwd, &name, 0, name.length);
    assert(opened.tag == 1 && opened.error.tag == 21 && inputs->handles.words[0] == 0);
    assert(opened.error.detail[21].code == 0 && opened.error.detail[21].origin == 0);
    inputs->handles.words[0] = saved;
    open_file(&opened, &inputs->handles, &inputs->cwd, &name, 0, name.length);
    assert(opened.tag == 0 && inputs->handles.words[0] == saved - 1);
    memset(bytes, 7, sizeof(bytes));
    read_at(&read, &inputs->handles, &opened.value, &window, 0, 2, 9);
    memset(unchanged, 7, sizeof(unchanged));
    memcpy(unchanged + 2, "hello", 5);
    assert(read.tag == 0 && read.value == 7);
    assert(memcmp(bytes, unchanged, sizeof(bytes)) == 0);
    memcpy(unchanged, bytes, sizeof(bytes));
    read_at(&read, &inputs->handles, &opened.value, &window, 5, 2, 9);
    assert(read.tag == 1 && read.error.tag == 0);
    assert(memcmp(bytes, unchanged, sizeof(bytes)) == 0);
    read_at(&read, &inputs->handles, &opened.value, &window, 0, 9, 9);
    assert(read.tag == 0 && read.value == 9);
    assert(memcmp(bytes, unchanged, sizeof(bytes)) == 0);
    wf__body_close_read(&closed, &receiving_factory, &opened.value);
    check_close(&closed);
    assert(inputs->handles.words[0] == saved - 1 && receiving_factory.words[0] == 1);
    assert(remove(filename) == 0);
    open_file(&opened, &inputs->handles, &inputs->cwd, &name, 0, name.length);
    assert(opened.tag == 1 && opened.error.tag == 0);
    assert(inputs->handles.words[0] == saved - 1);
#if defined(_WIN32)
    static const uint16_t invalid[] = { 'b','a','d','/','n','a','m','e' };
#else
    static const unsigned char invalid[] = { 'b','a','d','/','n','a','m','e' };
#endif
    wf_view invalid_name = {(void *)invalid, sizeof(invalid)};
    open_file(&opened, &inputs->handles, &inputs->cwd, &invalid_name, 0, invalid_name.length);
    assert(opened.tag == 1 && opened.error.tag == 9);
    assert(inputs->handles.words[0] == saved - 1);
    /* Transfer the received credit back by actually opening and closing an
     * owner, without comparing the close's factory to its creator. */
    wf__body_open_directory_source(&listing, &receiving_factory, &inputs->cwd);
    assert(listing.tag == 0 && receiving_factory.words[0] == 0);
    wf__body_close_directory_source(&closed, &inputs->handles, &listing.value);
    check_close(&closed);
    assert(inputs->handles.words[0] == saved);
}

/* The fixture has one file and whichever self/parent entries this host
 * returns. A successful batch must consume new entries, so at most three
 * successful batches can precede EOF; there is no open-ended iteration. */
struct expected_entry { const void *name; size_t bytes; unsigned kind; };
#if defined(_WIN32)
#define WF_ENTRY(name, kind) { L##name, sizeof(L##name) - sizeof(wchar_t), kind }
#else
#define WF_ENTRY(name, kind) { name, sizeof(name) - 1, kind }
#endif
static const struct expected_entry ordinary_entries[] = {
    WF_ENTRY(".", 2), WF_ENTRY("..", 2), WF_ENTRY("ordinary-directory.data", 1)
};
static unsigned directory_contents(wf_value *source, int empty_first,
                                   const struct expected_entry *expected, unsigned count) {
    unsigned seen = 0;
    unsigned char bytes[4096], unchanged[sizeof(bytes)];
    wf_view window = {bytes, sizeof(bytes)};
    wf_list_result listed;
    const size_t start = 3, end = sizeof(bytes) - 8;
    memset(bytes, 0x7d, sizeof(bytes));
    memcpy(unchanged, bytes, sizeof(bytes));
    if (empty_first) {
        wf__body_directory_next(&listed, source, &window, 11, 11);
        assert(listed.result.tag == 0 && listed.next == 11 && listed.entries == 0);
        assert(memcmp(bytes, unchanged, sizeof(bytes)) == 0);
    }
    for (unsigned batch = 0; batch < 4; ++batch) {
        memset(bytes, 0x7d, sizeof(bytes));
        memcpy(unchanged, bytes, sizeof(bytes));
        wf__body_directory_next(&listed, source, &window, start, end);
        assert(listed.next >= start && listed.next <= end);
        if (listed.result.tag != 0) {
            assert(listed.result.error.tag == 0 && listed.next == start && listed.entries == 0);
#if defined(__APPLE__)
            /* Darwin's extended getdirentries64 writes an EOF flag in the
             * supplied window's final four bytes, including a zero-byte
             * result. The remaining bytes, and both guards, stay untouched.
             * See XNU bsd/vfs/vfs_syscalls.c::getdirentries64. */
            uint32_t eof_flags = 1;
            memcpy(unchanged + end - sizeof(eof_flags), &eof_flags, sizeof(eof_flags));
#endif
            assert(memcmp(bytes, unchanged, sizeof(bytes)) == 0);
            unsigned required = ((1u << count) - 1u) & ~3u;
            assert((seen & required) == required);
            return seen;
        }
        assert(listed.next > start && listed.entries > 0);
        assert(memcmp(bytes, unchanged, start) == 0);
        assert(memcmp(bytes + end, unchanged + end, sizeof(bytes) - end) == 0);
        uint64_t at = start, entries = 0;
        while (at < listed.next) {
            assert(listed.next - at >= 3 && bytes[at] <= 4);
            size_t length = (size_t)bytes[at + 1] | ((size_t)bytes[at + 2] << 8);
            assert(length > 0 && length <= listed.next - at - 3);
            const unsigned char *name = bytes + at + 3;
            unsigned bit = 0;
            for (unsigned entry = 0; entry < count; ++entry) {
                if (length == expected[entry].bytes &&
                    memcmp(name, expected[entry].name, length) == 0) {
                    bit = 1u << entry;
                    assert(bytes[at] == expected[entry].kind);
                    break;
                }
            }
            assert(bit && !(seen & bit));
            seen |= bit;
            at += 3 + length;
            ++entries;
        }
        assert(at == listed.next && entries == listed.entries);
    }
    assert(!"directory source failed to reach EOF for the fixed fixture");
    return 0;
}

static void directory_probe(wf_inputs *inputs) {
    wf_open_result first, second;
    wf_close_result closed;
    uint64_t before = inputs->handles.words[0];
    FILE *fixture = NULL;
#if defined(_WIN32)
    assert(fopen_s(&fixture, "ordinary-directory.data", "wb") == 0);
#else
    fixture = fopen("ordinary-directory.data", "wb");
#endif
    assert(fixture && fclose(fixture) == 0);
    wf__body_open_directory_source(&first, &inputs->handles, &inputs->cwd);
    assert(first.tag == 0 && inputs->handles.words[0] == before - 1);
    wf__body_open_directory_source(&second, &inputs->handles, &inputs->cwd);
    assert(second.tag == 0 && inputs->handles.words[0] == before - 2);
    unsigned first_entries = directory_contents(&first.value, 1, ordinary_entries, 3);
    unsigned second_entries = directory_contents(&second.value, 0, ordinary_entries, 3);
    assert(first_entries == second_entries);
    wf__body_close_directory_source(&closed, &inputs->handles, &second.value);
    check_close(&closed);
    assert(inputs->handles.words[0] == before - 1);
    wf__body_close_directory_source(&closed, &inputs->handles, &first.value);
    check_close(&closed);
    assert(inputs->handles.words[0] == before);
    assert(remove("ordinary-directory.data") == 0);
}

#if defined(_WIN32)
/* A saved production DirectoryRead, not the process cwd or a private NT
 * wrapper, owns these operations. This child has exclusive cwd ownership. */
static void windows_namespace_probe(wf_inputs *inputs) {
    static const wchar_t file_name[] = L"\x4241", directory_name[] = L"\x4242";
    static const wchar_t link_name[] = L"link", missing_name[] = L"missing";
    static const struct expected_entry expected[] = {
        WF_ENTRY(".", 2), WF_ENTRY("..", 2), WF_ENTRY("\x4241", 1),
        WF_ENTRY("\x4242", 2), WF_ENTRY("link", 3), WF_ENTRY("ambient", 2)
    };
    HANDLE file = CreateFileW(file_name, GENERIC_WRITE, 0, NULL, CREATE_NEW,
                              FILE_ATTRIBUTE_NORMAL, NULL);
    DWORD written = 0;
    assert(file != INVALID_HANDLE_VALUE);
    assert(WriteFile(file, "N", 1, &written, NULL) && written == 1);
    assert(CloseHandle(file));
    assert(CreateDirectoryW(directory_name, NULL));
    assert(CreateDirectoryW(L"ambient", NULL));
    BOOLEAN linked = CreateSymbolicLinkW(link_name, file_name, 2u);
    if (!linked && GetLastError() == ERROR_INVALID_PARAMETER)
        linked = CreateSymbolicLinkW(link_name, file_name, 0);
    if (!linked) fprintf(stderr, "required real Windows symlink fixture failed: %lu\n", (unsigned long)GetLastError());
    assert(linked);
    assert(SetCurrentDirectoryW(L"ambient"));
    assert(GetFileAttributesW(file_name) == INVALID_FILE_ATTRIBUTES
           && GetLastError() == ERROR_FILE_NOT_FOUND);

    const uint64_t credits = inputs->handles.words[0];
    wf_open_result opened;
    wf_close_result closed;
    wf_read_result read;
    unsigned char byte = 0;
    wf_view destination = { &byte, 1 };
    wf_view name = { (void *)file_name, sizeof(file_name) - sizeof(wchar_t) };
    wf__body_open_file(&opened, &inputs->handles, &inputs->cwd, &name, 0, name.length);
    assert(opened.tag == 0 && inputs->handles.words[0] == credits - 1);
    wf__body_read_at(&read, &inputs->handles, &opened.value, &destination, 0, 0, 1);
    assert(read.tag == 0 && read.value == 1 && byte == 'N');
    wf__body_close_read(&closed, &inputs->handles, &opened.value); check_close(&closed);

    name.data = (void *)directory_name; name.length = sizeof(directory_name) - sizeof(wchar_t);
    wf__body_open_directory(&opened, &inputs->handles, &inputs->cwd, &name, 0, name.length);
    assert(opened.tag == 0);
    wf__body_close_directory(&closed, &inputs->handles, &opened.value); check_close(&closed);
    wf__body_open_directory_source(&opened, &inputs->handles, &inputs->cwd);
    assert(opened.tag == 0);
    (void)directory_contents(&opened.value, 1, expected, 6);
    wf__body_close_directory_source(&closed, &inputs->handles, &opened.value); check_close(&closed);
    assert(inputs->handles.words[0] == credits);

    name.data = (void *)missing_name; name.length = sizeof(missing_name) - sizeof(wchar_t);
    wf__body_open_file(&opened, &inputs->handles, &inputs->cwd, &name, 0, name.length);
    assert(opened.tag == 1 && opened.error.tag == 0);
    assert(opened.error.detail[0].code == ERROR_FILE_NOT_FOUND && opened.error.detail[0].origin == 1);
    assert(inputs->handles.words[0] == credits);

    /* Component open must dispose its terminal reparse handle on refusal;
     * RelativePath open deliberately follows that same link to the regular file. */
    DWORD handles_before = 0, handles_after = 0;
    assert(GetProcessHandleCount(GetCurrentProcess(), &handles_before));
    name.data = (void *)link_name; name.length = sizeof(link_name) - sizeof(wchar_t);
    wf__body_open_file(&opened, &inputs->handles, &inputs->cwd, &name, 0, name.length);
    assert(opened.tag == 1 && opened.error.tag == 10);
    assert(opened.error.detail[10].code == 0 && opened.error.detail[10].origin == 0);
    assert(inputs->handles.words[0] == credits);
    assert(GetProcessHandleCount(GetCurrentProcess(), &handles_after));
    assert(handles_after == handles_before);
    wf_value text = {{ (uint64_t)(uintptr_t)link_name, 4, 0, 0 }};
    wf_value_result path;
    wf__body_relative_path(&path, &text); assert(path.tag == 0);
    wf__body_open_read(&opened, &inputs->handles, &inputs->cwd, &path.value);
    assert(opened.tag == 0 && inputs->handles.words[0] == credits - 1);
    byte = 0;
    wf__body_read_at(&read, &inputs->handles, &opened.value, &destination, 0, 0, 1);
    assert(read.tag == 0 && read.value == 1 && byte == 'N');
    wf__body_close_read(&closed, &inputs->handles, &opened.value); check_close(&closed);
    assert(inputs->handles.words[0] == credits);
    assert(SetCurrentDirectoryW(L".."));
    assert(DeleteFileW(link_name) && DeleteFileW(file_name));
    assert(RemoveDirectoryW(directory_name) && RemoveDirectoryW(L"ambient"));
}
#endif

static uint16_t listener_port(const wf_value *listener) {
    unsigned port = wf_test_socket_port((int)listener->words[0]);
    assert(port != 0);
    return (uint16_t)port;
}

static void tcp_probe(wf_inputs *inputs) {
    wf_value address;
    wf_value other_factory = {{0, 0, 0, 0}};
    wf_open_result listener;
    wf_connect_result first_client, second_client;
    wf_accept_result first_server, second_server;
    wf_connection crossed_a, crossed_b;
    wf_close_result closed;
    wf_write_result sent;
    wf_read_result received;
    unsigned char byte = 'x', target = 0;
    wf_view source = {&byte, 1}, destination = {&target, 1};
    uint64_t before = inputs->handles.words[0];
    wf__body_socket_address_v4(&address, 127, 0, 0, 1, 0);
    wf__body_tcp_listen(&listener, &inputs->handles, &address);
    if (listener.tag != 0) fprintf(stderr, "listen failed: class=%u code=%u origin=%u\n",
        listener.error.tag, listener.error.detail[listener.error.tag].code,
        listener.error.detail[listener.error.tag].origin);
    assert(listener.tag == 0 && inputs->handles.words[0] == before - 1);
    wf__body_socket_address_v4(&address, 127, 0, 0, 1, listener_port(&listener.value));
    wf__body_tcp_connect(&first_client, &inputs->handles, &address);
    assert(first_client.tag == 0 && inputs->handles.words[0] == before - 2);
    wf__body_tcp_accept(&first_server, &inputs->handles, &listener.value);
    assert(first_server.tag == 0 && inputs->handles.words[0] == before - 3);
    wf__body_tcp_connect(&second_client, &inputs->handles, &address);
    assert(second_client.tag == 0 && inputs->handles.words[0] == before - 4);
    wf__body_tcp_accept(&second_server, &inputs->handles, &listener.value);
    assert(second_server.tag == 0);
    assert(inputs->handles.words[0] == before - 5);
    crossed_a.receive = first_server.value.connection.receive;
    crossed_a.send = second_server.value.connection.send;
    crossed_b.receive = second_server.value.connection.receive;
    crossed_b.send = first_server.value.connection.send;
    wf__body_close_receive(&closed, &inputs->handles, &crossed_a.receive); check_close(&closed);
    assert(inputs->handles.words[0] == before - 5 && other_factory.words[0] == 0);
    wf__body_close_send(&closed, &inputs->handles, &crossed_a.send); check_close(&closed);
    assert(inputs->handles.words[0] == before - 5 && other_factory.words[0] == 0);
    wf__body_send_once(&sent, &crossed_b.send, &source, 0, 1);
    assert(sent.tag == 0 && sent.value == 1);
    wf__body_receive_next(&received, &first_client.value.receive, &destination, 0, 1);
    assert(received.tag == 0 && received.value == 1 && target == byte);
    target = 0;
    wf__body_send_once(&sent, &second_client.value.send, &source, 0, 1);
    assert(sent.tag == 0 && sent.value == 1);
    wf__body_receive_next(&received, &crossed_b.receive, &destination, 0, 1);
    assert(received.tag == 0 && received.value == 1 && target == byte);
    wf__body_close_send(&closed, &other_factory, &crossed_b.send); check_close(&closed);
    assert(other_factory.words[0] == 1 && inputs->handles.words[0] == before - 5);
    wf__body_close_receive(&closed, &inputs->handles, &crossed_b.receive); check_close(&closed);
    assert(inputs->handles.words[0] == before - 4 && other_factory.words[0] == 1);
    wf__body_close_receive(&closed, &inputs->handles, &first_client.value.receive); check_close(&closed);
    assert(inputs->handles.words[0] == before - 4 && other_factory.words[0] == 1);
    wf__body_close_send(&closed, &inputs->handles, &first_client.value.send); check_close(&closed);
    assert(inputs->handles.words[0] == before - 3 && other_factory.words[0] == 1);
    wf__body_close_send(&closed, &inputs->handles, &second_client.value.send); check_close(&closed);
    assert(inputs->handles.words[0] == before - 3 && other_factory.words[0] == 1);
    wf__body_close_receive(&closed, &inputs->handles, &second_client.value.receive); check_close(&closed);
    assert(inputs->handles.words[0] == before - 2 && other_factory.words[0] == 1);
    wf__body_close_listener(&closed, &inputs->handles, &listener.value); check_close(&closed);
    assert(inputs->handles.words[0] == before - 1 && other_factory.words[0] == 1);
    assert(inputs->handles.words[0] + other_factory.words[0] == before);
}

static void concurrent_half_close_probe(wf_inputs *inputs, int send_first) {
    wf_value address;
    wf_open_result listener;
    wf_connect_result client, replacement;
    wf_accept_result server, replacement_server;
    wf_close_result closed;
    wf_write_result sent;
    wf_read_result received;
    wf_prim_thread thread;
    half_close_call call;
    uint64_t before = inputs->handles.words[0];
    uint64_t after_second, descriptor;
    unsigned char byte = 'q', target = 0;
    wf_view source = {&byte, 1}, destination = {&target, 1};

    wf__body_socket_address_v4(&address, 127, 0, 0, 1, 0);
    wf__body_tcp_listen(&listener, &inputs->handles, &address);
    assert(listener.tag == 0);
    wf__body_socket_address_v4(&address, 127, 0, 0, 1, listener_port(&listener.value));
    wf__body_tcp_connect(&client, &inputs->handles, &address);
    assert(client.tag == 0);
    wf__body_tcp_accept(&server, &inputs->handles, &listener.value);
    assert(server.tag == 0 && inputs->handles.words[0] == before - 3);
    descriptor = server.value.connection.send.words[0];
    memset(&call, 0, sizeof(call));
    call.send = send_first;
    call.owner = send_first ? server.value.connection.send : server.value.connection.receive;
    wf_prim_wait_lock(&half_close.wait);
    half_close.socket = native_socket(&call.owner);
    half_close.direction = send_first ? WF_PROBE_SEND : WF_PROBE_RECEIVE;
    half_close.armed = 1;
    half_close.paused = 0;
    half_close.resume = 0;
    half_close.finished = 0;
    wf_prim_wait_unlock(&half_close.wait);
    assert(wf_prim_thread_start(&thread, paused_half_close, &call, 1024u * 1024u) == 0);
    wf_prim_wait_lock(&half_close.wait);
    while (!half_close.paused) wf_prim_wait_sleep(&half_close.wait);
    wf_prim_wait_unlock(&half_close.wait);

    if (send_first) wf__body_close_receive(&closed, &inputs->handles, &server.value.connection.receive);
    else wf__body_close_send(&closed, &inputs->handles, &server.value.connection.send);
    check_close(&closed);
    after_second = inputs->handles.words[0];
    assert(wf_test_socket_open((int)descriptor));
    assert(call.factory.words[0] == 0);
    wf_prim_wait_lock(&half_close.wait);
    half_close.resume = 1;
    wf_prim_wait_signal(&half_close.wait);
    while (!half_close.finished) wf_prim_wait_sleep(&half_close.wait);
    wf_prim_wait_unlock(&half_close.wait);
    check_close(&call.result);
    assert(after_second == before - 3);
    assert(call.factory.words[0] == 1);

    /* The listener and old client stay open. Pool startup already happened
     * at their acquisition; after the paused caller publishes finished, no
     * fixture operation opens/closes a descriptor before this replacement.
     * The just-released lowest slot must therefore be reused, on both the
     * POSIX table and Windows CRT registry, without an open-until-reused loop.
     * Fresh bidirectional IO checks that its half-close state was reset. */
    wf__body_tcp_connect(&replacement, &call.factory, &address);
    assert(replacement.tag == 0 && call.factory.words[0] == 0);
    assert(replacement.value.receive.words[0] == descriptor);
    wf__body_tcp_accept(&replacement_server, &inputs->handles, &listener.value);
    assert(replacement_server.tag == 0);
    wf__body_send_once(&sent, &replacement.value.send, &source, 0, 1);
    assert(sent.tag == 0 && sent.value == 1);
    wf__body_receive_next(&received, &replacement_server.value.connection.receive, &destination, 0, 1);
    assert(received.tag == 0 && received.value == 1 && target == byte);
    target = 0;
    wf__body_send_once(&sent, &replacement_server.value.connection.send, &source, 0, 1);
    assert(sent.tag == 0 && sent.value == 1);
    wf__body_receive_next(&received, &replacement.value.receive, &destination, 0, 1);
    assert(received.tag == 0 && received.value == 1 && target == byte);
    wf__body_close_send(&closed, &inputs->handles, &replacement.value.send); check_close(&closed);
    wf__body_close_receive(&closed, &inputs->handles, &replacement.value.receive); check_close(&closed);
    wf__body_close_receive(&closed, &inputs->handles, &replacement_server.value.connection.receive); check_close(&closed);
    wf__body_close_send(&closed, &inputs->handles, &replacement_server.value.connection.send); check_close(&closed);
    wf__body_close_receive(&closed, &inputs->handles, &client.value.receive); check_close(&closed);
    wf__body_close_send(&closed, &inputs->handles, &client.value.send); check_close(&closed);
    wf__body_close_listener(&closed, &inputs->handles, &listener.value); check_close(&closed);
    assert(inputs->handles.words[0] == before && call.factory.words[0] == 0);
}

/* One logical selection can share the main runtime harness without making
 * memory-only text checks initialize I/O, threads or scratch fixtures. */
int wf_ordinary_values_tests(const char *scratch, const char *group) {
    int all = strcmp(group, "all") == 0;
    int text = all || strcmp(group, "text") == 0;
    int files = all || strcmp(group, "file") == 0;
    int directory = all || strcmp(group, "directory") == 0;
    int tcp = all || strcmp(group, "tcp") == 0;
    assert(text || files || directory || tcp);
    if (text) { text_probe(); puts("ordinary text: PASS"); }
    if (!files && !directory && !tcp) return 0;
    char previous[4096], fixture[64];
    assert(wf_getcwd(previous, sizeof(previous)) != NULL);
    assert(wf_chdir(scratch) == 0);
    assert(snprintf(fixture, sizeof(fixture), "ordinary-fixture-%lu", (unsigned long)wf_pid()) > 0);
    assert(wf_mkdir(fixture) == 0 && wf_chdir(fixture) == 0);
    wf_inputs inputs;
    wf_close_result closed;
    assert(wf__ordinary_inputs(&inputs, 0, NULL));
    assert(inputs.handles.words[0] >= 8);
    if (files) { wf_test_guard_phase("ordinary file/credits"); file_probe(&inputs, wf__body_open_file, wf__body_read_at); file_probe(&inputs, wf_test_public_open, wf_test_public_read); puts("ordinary file/credits public+body: PASS"); }
    if (directory) { wf_test_guard_phase("ordinary directory/cursors"); directory_probe(&inputs); puts("ordinary directory/cursors: PASS"); }
#if defined(_WIN32)
    if (directory) {
        wf_test_guard_phase("ordinary saved directory/native names/reparse");
        windows_namespace_probe(&inputs);
        puts("ordinary saved directory/native names/reparse: PASS");
    }
#endif
    if (tcp) {
        wf_test_guard_phase("ordinary TCP crossed halves/concurrent close/credits");
        assert(wf_prim_wait_init(&half_close.wait) == 0);
        atomic_store(&half_close_initialized, 1);
        tcp_probe(&inputs);
        concurrent_half_close_probe(&inputs, 1);
        concurrent_half_close_probe(&inputs, 0);
        atomic_store(&half_close_initialized, 0);
        wf_prim_wait_destroy(&half_close.wait);
        puts("ordinary TCP crossed halves/concurrent close/credits: PASS");
    }
    wf__body_close_directory(&closed, &inputs.handles, &inputs.cwd);
    check_close(&closed);
    assert(wf_chdir("..") == 0 && wf_rmdir(fixture) == 0);
    assert(wf_chdir(previous) == 0);
    return 0;
}

#ifndef WF_ORDINARY_TEST_IN_HARNESS
int main(int argc, char **argv) {
    assert(argc == 2 || argc == 3);
    const char *group = argc == 3 ? argv[2] : "all";
    if (strcmp(group, "text") != 0) wf_test_guard_start(180);
    int result = wf_ordinary_values_tests(argv[1], group);
    wf_test_guard_finish();
    return result;
}
#endif
