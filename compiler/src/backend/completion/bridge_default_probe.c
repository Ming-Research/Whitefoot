#if !defined(_WIN32) && !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE 1
#endif

#include "bridge.h"
/* Only for the two ABI constants an emitted frame reserves its record block
 * by.  This probe stands in for emitted code and learns no more of the
 * record's layout than emitted code does. */
#include "contract.h"
/* The threads this probe's lanes run on are the runtime's own, so that one
 * probe runs on every platform the runtime does. */
#include "../sched/prim.h"
#include "../runtime_test_guard.h"

#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ------------------------------------------------- the probe's host leaf */

/* The four host calls this scaffold makes, and the only place it names a
 * platform.  Everything below is one implementation: the lanes, the rounds,
 * the assertions and the route check are the same source on every host, and
 * the threads they run on come from the runtime's own thread primitive.
 *
 * The fixture is made with the platform's own file calls because no operation
 * of the completion ABI creates a file: an open there resolves a name that
 * already exists. */
#if defined(_WIN32)

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <fcntl.h>
#include <io.h>

static unsigned long probe_process_id(void) {
    return (unsigned long)GetCurrentProcessId();
}

/* The fixture is opened for overlapped I/O, which is what this target's own
 * opens produce and therefore what its ring is given. */
static int probe_open_fixture(const char *path, const unsigned char *bytes, unsigned length) {
    HANDLE writer;
    HANDLE reader;
    DWORD written = 0;
    writer = CreateFileA(
        path,
        GENERIC_WRITE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        NULL,
        CREATE_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        NULL
    );
    if (writer == INVALID_HANDLE_VALUE) {
        return -1;
    }
    if (WriteFile(writer, bytes, (DWORD)length, &written, NULL) == FALSE
        || written != (DWORD)length
        || FlushFileBuffers(writer) == FALSE
        || CloseHandle(writer) == FALSE) {
        return -1;
    }
    reader = CreateFileA(
        path,
        GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        NULL,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
        NULL
    );
    if (reader == INVALID_HANDLE_VALUE) {
        return -1;
    }
    return _open_osfhandle((intptr_t)reader, _O_RDONLY | _O_BINARY);
}

static int probe_open_stream(const char *path) {
    HANDLE handle = CreateFileA(path, GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, NULL,
        OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (handle == INVALID_HANDLE_VALUE) return -1;
    return _open_osfhandle((intptr_t)handle, _O_RDONLY | _O_BINARY);
}

static void probe_close_fixture(int descriptor, const char *path) {
    (void)_close(descriptor);
    (void)DeleteFileA(path);
}

#else

#include <fcntl.h>
#include <time.h>
#include <unistd.h>

static unsigned long probe_process_id(void) {
    return (unsigned long)getpid();
}

static int probe_open_fixture(const char *path, const unsigned char *bytes, unsigned length) {
    int descriptor = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (descriptor < 0
        || write(descriptor, bytes, (size_t)length) != (ssize_t)length) {
        return -1;
    }
    return descriptor;
}

static int probe_open_stream(const char *path) { return open(path, O_RDONLY); }

static void probe_close_fixture(int descriptor, const char *path) {
    (void)close(descriptor);
    (void)unlink(path);
}

#endif

#include "socket_test.h"

/*
 * The bridge on its shipped default helper policy, which nothing else runs.
 *
 * `harness.c`'s main pins WF_IO_HELPERS for any invocation that did not name
 * one, and `completion-test` names 0, 1 and 4, so the policy a delivered
 * program actually gets -- a pool that starts empty and grows only once the
 * adapter has measured its own operations waiting, and a positioned read the
 * bridge declines to submit while there is nothing to overlap it with -- was
 * reachable from no test in the tree.  Both halves of that policy are new in
 * batch 0096 and both choose between two different routes for the same
 * writer-visible operation, which is exactly the decision a test has to watch.
 *
 * So this probe runs with WF_IO_HELPERS unset, on purpose, and refuses to run
 * with it set: an inherited value would silently turn it back into the arm the
 * harness already covers.
 *
 * What it asserts of each read is what a writer can observe and nothing else:
 * every read delivers the byte that is at its offset, with the count and error
 * the operation promises, whichever route the bridge chose for it -- and the
 * run finishes, which is the liveness half, since running an operation inside
 * submit and growing the pool are both decisions about who executes the work.
 *
 * There is no declined submission any more: every submit ends in a published
 * record, and the branch that used to hand the operation back to its caller
 * now executes it inside submit and publishes it there
 * (`research/investigations/io-model/PARK-ON-MISS.md` §7, "Every submit path
 * ends in a published record").  The branch is still watched, through the
 * runtime's count of operations the engine executed inside the submitting
 * call.
 *
 * It then asserts that the routes it exists to watch were taken at all, which
 * is the difference between covering a decision and counting it: see the
 * route check at the end of `main`.
 */

#define LANES 4
#define ROUNDS 4000
#define FILE_BYTES 4096
#define WATCHDOG_SECONDS 180

static _Atomic unsigned submitted_route;
static _Atomic unsigned nonpositioned_route;
/* Synchronous streams have a defined cursor and successful one-byte result. */
static _Atomic unsigned nonpositioned_transferred;
static _Atomic unsigned lanes_finished;
/* The entry records the lanes and the watchdog are started with. They are the
 * probe's own storage because `wf_prim_thread_start` takes the pair from the
 * caller and reads it after the creating call has returned (`../sched/prim.h`,
 * P1); statics are the simplest thing here that outlives every thread. */
static wf_prim_thread lane_threads[LANES];

typedef struct {
    int descriptor;
    int stream_descriptor;
    unsigned lane;
    int failed;
} lane_context;

/* The byte the fixture holds at `offset`. */
static unsigned char expected_byte(uint64_t offset) {
    return (unsigned char)(offset % 251u);
}

static void lane_main(void *context) {
    lane_context *self = context;
    unsigned round, stream_offset = 0;
    for (round = 0; round < ROUNDS; ++round) {
        _Alignas(WF_COMPLETION_RECORD_ALIGN)
            unsigned char record[WF_COMPLETION_RECORD_BYTES];
        unsigned char byte = 0xff;
        uint64_t offset =
            (uint64_t)((self->lane * 7u + round * 13u) % FILE_BYTES);
        int64_t value = 0;
        int error = 0;
        wf__completion_file_pread_submit(
            self->descriptor,
            &byte,
            1,
            offset,
            record
        );
        atomic_fetch_add_explicit(&submitted_route, 1, memory_order_relaxed);
        wf__completion_file_join(record, &value, &error);
        if (value != 1 || error != 0 || byte != expected_byte(offset)) {
            fprintf(
                stderr,
                "bridge default probe: lane %u round %u offset %llu: "
                "value %lld error %d byte %u expected %u\n",
                self->lane,
                round,
                (unsigned long long)offset,
                (long long)value,
                error,
                (unsigned)byte,
                (unsigned)expected_byte(offset)
            );
            self->failed = 1;
            break;
        }
        /* One non-positioned read every so often.  It is never run inside
         * submit, so it puts work in the queue and takes away the inline
         * branch's `nothing queued` precondition under the other lanes. */
        if ((round & 0x3fu) == 0) {
            _Alignas(WF_COMPLETION_RECORD_ALIGN)
                unsigned char other_record[WF_COMPLETION_RECORD_BYTES];
            unsigned char other = 0;
            wf__completion_file_read_submit(
                self->stream_descriptor,
                &other,
                1,
                other_record
            );
            atomic_fetch_add_explicit(
                &nonpositioned_route,
                1,
                memory_order_relaxed
            );
            wf__completion_file_join(other_record, &value, &error);
            if (value != 1 || error != 0 || other != expected_byte(stream_offset)) {
                fprintf(stderr, "bridge default probe: stream lane=%u offset=%u value=%lld error=%d byte=%u\n",
                        self->lane, stream_offset, (long long)value, error, (unsigned)other);
                self->failed = 1;
                break;
            }
            ++stream_offset;
            atomic_fetch_add_explicit(&nonpositioned_transferred, 1, memory_order_relaxed);
        }
    }
    atomic_fetch_add_explicit(&lanes_finished, 1, memory_order_release);
}

int main(int argc, char **argv) {
    char path[512];
    unsigned char fixture[FILE_BYTES];
    int descriptor;
    lane_context lanes[LANES];
    size_t index;
    int failed = 0;
    uint64_t ring_submissions;
    uint64_t adapter_submissions;
    unsigned submitted;
    uint64_t inline_executions;
    uint64_t helpers;
    unsigned tcp_port = 0;
    uint64_t tcp_ring_submissions = 0;
    /* The two settings this probe reads about itself, through the platform
     * layer's own setting read (`../sched/prim.h`, P4) rather than `getenv`,
     * which the MSVC ucrt marks deprecated. */
    char setting[WF_PRIM_SETTING_BYTES];

    if (argc != 2) {
        fprintf(stderr, "usage: %s SCRATCH_DIRECTORY\n", argv[0]);
        return 2;
    }
    if (wf_prim_setting_text("WF_IO_HELPERS", setting, sizeof(setting)) != 0) {
        fprintf(
            stderr,
            "bridge default probe: WF_IO_HELPERS is set; this probe is about "
            "the route taken when it is not\n"
        );
        return 2;
    }
    wf_test_guard_start(WATCHDOG_SECONDS);
    wf_test_guard_phase("default-policy file fixture");
    for (index = 0; index < FILE_BYTES; ++index) {
        fixture[index] = expected_byte((uint64_t)index);
    }
    if (snprintf(
            path,
            sizeof(path),
            "%s/wf-bridge-default-%lu",
            argv[1],
            probe_process_id()
        ) <= 0) {
        return 2;
    }
    descriptor = probe_open_fixture(path, fixture, (unsigned)FILE_BYTES);
    if (descriptor < 0) {
        (void)fprintf(stderr, "bridge default probe: fixture failed\n");
        return 2;
    }
    wf_test_guard_phase("default-policy positioned/stream reads");
    /* Observe the native positioned-read route before queue-disturbance
     * stream requests can legitimately create adapter helpers. */
    {
        _Alignas(WF_COMPLETION_RECORD_ALIGN) unsigned char record[WF_COMPLETION_RECORD_BYTES];
        unsigned char byte = 0;
        int64_t value = -1;
        int error = 0;
        wf__completion_file_pread_submit(descriptor, &byte, 1, 0, record);
        wf__completion_file_join(record, &value, &error);
        if (value != 1 || error || byte != expected_byte(0)) return 1;
        if (wf__completion_native_ring_submissions() > 0 &&
            wf__completion_target_helper_count() != 0) {
            fputs("bridge default probe: native positioned read created adapter helpers\n", stderr);
            return 1;
        }
    }
    for (index = 0; index < LANES; ++index) {
        lanes[index].descriptor = descriptor;
        lanes[index].stream_descriptor = probe_open_stream(path);
        if (lanes[index].stream_descriptor < 0) return 2;
        lanes[index].lane = (unsigned)index;
        lanes[index].failed = 0;
        if (wf_prim_thread_start(
                &lane_threads[index],
                lane_main,
                &lanes[index],
                0
            ) != 0) {
            return 2;
        }
    }
    /* The lanes are detached, as every thread this runtime starts is, so the
     * rendezvous is the count they publish rather than a join by handle. */
    while (atomic_load_explicit(&lanes_finished, memory_order_acquire)
           < (unsigned)LANES) {
        wf_prim_yield();
    }
    for (index = 0; index < LANES; ++index) {
        failed |= lanes[index].failed;
#if defined(_WIN32)
        (void)_close(lanes[index].stream_descriptor);
#else
        (void)close(lanes[index].stream_descriptor);
#endif
    }
    probe_close_fixture(descriptor, path);

    /* Which route this host's bridge actually took, and whether the counters
     * agree that the decision this probe exists for was taken at all.
     *
     * Counting is not covering.  Without these the probe reports its route
     * split and asserts nothing about it, so a bridge that stopped choosing --
     * one whose demand-driven policy did nothing, or whose native ring
     * silently stopped accepting -- would still print a PASS line, and only a
     * reader comparing two runs by eye would notice.
     *
     * The route is read from the counters rather than from the host name,
     * because a Linux kernel with io_uring disabled runs the POSIX adapter
     * and must be held to the POSIX adapter's promise.  Ring submissions
     * above zero is the native ring; anything else is the adapter, on either
     * host.
     *
     * What the adapter route owes is one of the demand-driven policy's two
     * branches.  Both branches are decided by the same measurement: a
     * positioned read is executed inside submit when the adapter measured no
     * wait and holds no helper, and a helper is started when it measured a
     * wait.  Which one this run gets is a property of how fast this host's
     * reads actually are, and that is not fixed -- the same binary on the same
     * machine runs about 15 700 of 16 000 reads inline when the host is quiet
     * and runs none inline while growing three helpers when it is loaded,
     * because under a sanitizer on a busy machine a one-byte read really does
     * cost more than the twenty microseconds the rule is asking about.
     * Requiring the inline branch would therefore be requiring the host to be
     * idle, which is not a property of the runtime.  Requiring *either* is a
     * real requirement: if neither happened, the policy took no branch in
     * sixteen thousand reads and this run covered none of it. */
    ring_submissions = wf__completion_native_ring_submissions();
    adapter_submissions = wf__completion_file_fallback_submissions();
    submitted = atomic_load_explicit(&submitted_route, memory_order_relaxed);
    inline_executions = wf__completion_inline_executions();
    helpers = wf__completion_target_helper_count();

    /* The loopback round trip runs after those counters are read, so that the
     * read-route assertions above and below are about the reads and nothing
     * else.  Its own share of the ring is the delta reported beside it. */
    {
        uint64_t before = ring_submissions;
        wf_test_guard_phase("default-policy bridge TCP lifecycle");
        failed |= wf_test_socket_lifecycle(&tcp_port);
        wf_test_guard_phase("socket address conversion");
        failed |= wf_test_socket_address_conversion();
        tcp_ring_submissions =
            wf__completion_native_ring_submissions() - before;
    }
    /* The arm that exists to reach the adapter branch above has to have
     * reached the adapter.  Without this the Makefile could set
     * WF_IO_NO_NATIVE_RING, the bridge could ignore it, and the run would pass
     * as a second copy of the native-ring arm. */
    {
        int refused = wf_prim_setting_text(
            "WF_IO_NO_NATIVE_RING",
            setting,
            sizeof(setting)
        );
        if (refused == 1 && setting[0] == '1' && setting[1] == 0
            && ring_submissions != 0) {
            fprintf(
                stderr,
                "bridge default probe: WF_IO_NO_NATIVE_RING is set and the "
                "run still submitted %llu operations to the native ring\n",
                (unsigned long long)ring_submissions
            );
            failed = 1;
        }
    }
    if (submitted == 0) {
        (void)fprintf(
            stderr,
            "bridge default probe: no positioned read was submitted\n"
        );
        failed = 1;
    }
    if (atomic_load_explicit(&nonpositioned_transferred, memory_order_relaxed)
        != (unsigned)(LANES * ((ROUNDS + 63) / 64))) {
        fputs("bridge default probe: missing successful stream disturbances\n", stderr);
        failed = 1;
    }
    if (ring_submissions == 0 && inline_executions == 0 && helpers == 0) {
        fprintf(
            stderr,
            "bridge default probe: on the POSIX adapter route the "
            "demand-driven policy took neither branch in %u positioned reads: "
            "none ran inside submit and no helper was started\n",
            submitted
        );
        failed = 1;
    }

    wf_test_guard_finish();
    if (failed != 0) {
        fprintf(
            stderr,
            "bridge default probe: FAIL submitted=%u inline=%llu "
            "non-positioned=%u helpers=%llu submissions=%llu "
            "ring=%llu adapter=%llu tcp-port=%u tcp-ring=%llu\n",
            submitted,
            (unsigned long long)inline_executions,
            atomic_load_explicit(&nonpositioned_route, memory_order_relaxed),
            (unsigned long long)helpers,
            (unsigned long long)wf__completion_file_submissions(),
            (unsigned long long)ring_submissions,
            (unsigned long long)adapter_submissions,
            tcp_port,
            (unsigned long long)tcp_ring_submissions
        );
        return 1;
    }
    fprintf(
        stderr,
        "bridge default probe: PASS submitted=%u inline=%llu "
        "non-positioned=%u helpers=%llu submissions=%llu "
        "ring=%llu adapter=%llu tcp-port=%u tcp-ring=%llu route=%s\n",
        submitted,
        (unsigned long long)inline_executions,
        atomic_load_explicit(&nonpositioned_route, memory_order_relaxed),
        (unsigned long long)helpers,
        (unsigned long long)wf__completion_file_submissions(),
        (unsigned long long)ring_submissions,
        (unsigned long long)adapter_submissions,
        tcp_port,
        (unsigned long long)tcp_ring_submissions,
        ring_submissions != 0 ? "native-ring" : "posix-adapter"
    );
    return 0;
}
