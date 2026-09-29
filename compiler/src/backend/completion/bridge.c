#if !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif
/* F_NOCACHE, the Darwin half of the WF_IO_NOCACHE target policy applied by the
 * inline helper in file_adapter.h, is a BSD extension the POSIX macro above
 * hides, so the Darwin namespace is asked for as well. */
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE 1
#endif

/*
 * Compiler-owned finite file-completion bridge.
 *
 * The emitted module can submit only the typed open/read/write/status/close
 * and directory descriptors below. There is deliberately no callback,
 * function pointer, or generic thunk in this ABI: a file helper can execute
 * only file_adapter.c's closed switch.
 *
 * Every submit ends in a published record.  The record is a block of the
 * submitting frame, so there is nothing to claim and nothing to refuse: the
 * bridge either hands the record to the ring, or queues it on the bounded
 * POSIX adapter, or executes the operation here and completes the record
 * itself.  All three return 1 and the join reads the record
 * (`research/investigations/io-model/PARK-ON-MISS.md` §7).
 *
 * A weak LLVM fallback makes an emitted module independently linkable.
 *
 * One implementation for every platform.  The routing, the in-place wait, the
 * own-record run, the joins, the statistics and process configuration helpers
 * are written once; the only thing that differs is the platform's kernel
 * completion ring, which is behind the eight names of "the ring" below --
 * io_uring on Linux, the completion port on Windows, and none elsewhere.  The
 * one `#if` outside that section is the extra `descriptor_class` argument the
 * emitter emits per target on `wf__completion_file_open_at_submit`.
 */

#include "contract.h"
#include "bridge.h"
#include "file_adapter.h"

#include "../sched/entry.h"
#include "../sched/prim.h"
#if defined(__linux__)
#include "linux_io_uring.h"
#elif defined(_WIN32)
#include "windows_iocp.h"
#include "../windows_runtime.h"
#endif

#if !defined(_WIN32)
#include <sys/mman.h>
#endif

#include <errno.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define WF_BRIDGE_MAX_HELPERS 8u
/* The policy's ceiling and the adapter's storage are two names for one number,
 * and the adapter's is the one that decides: it carries an entry record per
 * helper (`file_adapter.h`), and a policy asking for more than it can hold
 * would be refused at init rather than granted. */
_Static_assert(
    WF_BRIDGE_MAX_HELPERS <= WF_FILE_MAX_HELPERS,
    "the helper policy may not ask for more helpers than the adapter holds"
);
/* How many completions one progress pass reaps before it returns to the
 * scheduler loop.  It was one, and one is what made the reap the serial
 * resource of the TCP echo control test: every idle thread took the
 * submission lock to kick and the completion lock to read a single entry,
 * and 64 connections ping-ponging through one ring made 720 thousand futex
 * calls a run.  At 64 the same run makes 19 thousand and the round-trip rate
 * doubles, 35 to 69 thousand a second on the development host; 1024 measures
 * the same as 64 (`research/investigations/io-model/RESULTS.md` section 6). */
#define WF_BRIDGE_REAP_BUDGET 64u
static wf_completion_runtime wf_bridge_runtime;
static wf_file_adapter wf_bridge_adapter;
static unsigned wf_bridge_once;
static unsigned wf_bridge_file_once;
static int wf_bridge_error;
static int wf_bridge_file_error;
/* The readiness flags below are atomic, and that is not decoration.
 *
 * Each is written by one of the once-initializers and read by entry points
 * that run no once at all: `wf__completion_file_pread_submit` asks whether the
 * pool is pinned and whether the adapter exists *before* it reaches the
 * routing decision, and the statistics entries read them from wherever a
 * program calls them.  A program with two threads therefore has an ordinary
 * unsynchronized read of a flag another thread is writing, which is a data
 * race whatever the values happen to be.  Declaring them `_Atomic` makes the
 * plain reads and writes below sequentially consistent accesses, which on the
 * hosts this runs on is at worst a load-acquire instruction and on x86-64 an
 * ordinary load.
 *
 * Whether WF_IO_HELPERS named the pool, which pins the route as well as the
 * count: see wf_bridge_helper_policy. */
static _Atomic int wf_bridge_helpers_pinned;
static _Atomic unsigned wf_bridge_ready;
static _Atomic unsigned wf_bridge_file_ready;
/* Records completed, and of those the ones the engine executed inside the
 * submitting call itself. */
static _Atomic uint64_t wf_bridge_publications;
static _Atomic uint64_t wf_bridge_inline_executions;
static int wf_bridge_progress(void);
static void wf_bridge_park(uint64_t observed_epoch);

/* The bridge's one fail-stop.
 *
 * Every `abort` in this unit is a trusted-computing-base defect rather than an
 * operation outcome, and each one is reached from a different place, so the
 * only thing that tells a reader of a crash log which one fired is the line it
 * wrote on the way out.  A bare `abort` writes nothing, and on Windows the
 * release UCRT ends such a process through the fast-fail path, which a shell
 * reports as a bare status with no message at all -- a fail-stop nobody can
 * diagnose.  This changes no control flow: it writes one line and then aborts
 * exactly where the bare call did.
 *
 * The channel is `stderr` and the write is unbuffered, because the next thing
 * this process does is die. */
static _Noreturn void wf_bridge_fail(const char *reason) {
    (void)fprintf(stderr, "whitefoot completion: %s\n", reason);
    (void)fflush(stderr);
    abort();
}

/* The platform's kernel completion ring, behind eight names; see "the ring"
 * below for what each one owes and why this is the only part of the bridge
 * that is written more than once. */
static int wf_bridge_ring_start(void);
static int wf_bridge_ring_ready(void);
static int wf_bridge_ring_offer(wf_completion_record *record);
static int wf_bridge_ring_progress(void);
static void wf_bridge_ring_flush(void);
static int wf_bridge_ring_park(uint64_t observed_epoch);
static void wf_bridge_ring_shutdown(void);
static uint64_t wf_bridge_ring_submissions(void);
static uint64_t wf_bridge_ring_submission_enters(void);


/* The helper policy, in one place.
 *
 * A written WF_IO_HELPERS pins the count: `*initial` and `*cap` are both the
 * written value, so a program asked for four helpers gets four and never a
 * fifth, and a program asked for none keeps the zero-helper path where a
 * waiting thread is itself the target engine.
 *
 * Unset selects one of two policies: a ready native ring keeps initial and
 * cap at zero; otherwise the pool starts empty and may grow on demand within
 * WF_BRIDGE_MAX_HELPERS. That ceiling is a provisional implementation limit,
 * not a CPU count or a bound on outstanding operations. */
static void wf_bridge_helper_policy(size_t *initial, size_t *cap) {
    unsigned long written = 0;
    /* One rule for every startup setting this runtime reads (`sched/entry.h`):
     * unset means the policy below chooses, an integer from 0 through this
     * bridge's own ceiling pins the pool, and anything else has already ended
     * the run at the core's entry. */
    if (wf__sched_setting("WF_IO_HELPERS", WF_BRIDGE_MAX_HELPERS, &written)) {
        *initial = (size_t)written;
        *cap = (size_t)written;
        /* A written setting is an instruction about how to run, so the
         * runtime stops choosing: it pins the pool exactly and keeps every
         * admitted operation on the queued completion path.  That is what
         * makes a pinned line of a measurement a measurement of the
         * completion path rather than of the policy that may run an
         * operation inline instead. */
        wf_bridge_helpers_pinned = 1;
        return;
    }
    /* With a ready native ring, adapter requests run on a waiting caller and
     * the default pool stays empty. The warm Linux many-file comparison in
     * research/investigations/io-model/RESULTS.md found helper handoff more
     * expensive than the adapter work it moved. This is that workload's
     * selection ground, not a guarantee that every native engine carries
     * every transfer or that adapter work never waits. WF_IO_HELPERS can
     * still pin a pool for a different target or workload. */
    if (wf_bridge_ring_ready()) {
        *initial = 0u;
        *cap = 0u;
        return;
    }
    /* The fallback starts empty to avoid a handoff when there is no wait to
     * overlap. Within the cap, growth requires queued demand beyond the held
     * helpers and a measured long wait; a peer-bound request bypasses those
     * two tests so progress need not await a completed blocking operation.
     *
     * The ceiling is the bridge's own, not the machine's core count. A blocked
     * helper does not occupy a CPU. The three-CPU macOS comparisons in
     * research/investigations/io-model/RESULTS.md found useful width at four
     * and eight helpers. They support going beyond CPU count, not eight as a
     * universal optimum. Eight remains the provisional helper storage limit;
     * the queued operation count is not bounded by it. */
    *cap = WF_BRIDGE_MAX_HELPERS;
    *initial = 0u;
}

static int wf_bridge_target_progress_one(void) {
    return wf_bridge_file_ready != 0
        && wf_file_adapter_progress(&wf_bridge_adapter, 1u) != 0;
}

/* One newly queued request is announced once, under the queue lock the
 * enqueue already holds, and reaches exactly one helper.  With zero helpers a
 * sleeping thread is itself the target's engine, so the same announcement
 * has to reach the wake epoch as well. */
static void wf_bridge_notify_target(void) {
    wf_completion_notify_target(&wf_bridge_runtime);
}

static void wf_bridge_initialize_file(void) {
    size_t requested = 0;
    size_t cap = 0;
    wf_bridge_helper_policy(&requested, &cap);
    wf_bridge_file_error = wf_file_adapter_init(
        &wf_bridge_adapter,
        &wf_bridge_runtime,
        WF_BRIDGE_MAX_HELPERS,
        requested
    );
    if (wf_bridge_file_error == 0
        && wf_file_adapter_set_helper_cap(&wf_bridge_adapter, cap) == 0) {
        wf_bridge_file_ready = 1;
    }
}

static int wf_bridge_ensure_file(void) {
    wf__sched_once(&wf_bridge_file_once, wf_bridge_initialize_file);
    return wf_bridge_file_ready != 0;
}

static void wf_bridge_shutdown(void) {
    if (wf_bridge_ready == 0) {
        return;
    }
    if (wf_bridge_file_ready != 0) {
        (void)wf_file_adapter_shutdown(&wf_bridge_adapter);
        wf_bridge_file_ready = 0;
    }
    wf_bridge_ring_shutdown();
    (void)wf_completion_runtime_destroy(&wf_bridge_runtime);
    wf_bridge_ready = 0;
}

/* ------------------------------------------------------------- the ring */

/* The platform's kernel completion ring, behind eight names.
 *
 * This is the whole of what the bridge cannot write once.  Everything else in
 * this unit -- the routing, the helper policy, the in-place wait, the
 * own-record run, the joins, the statistics and the
 * process configuration helpers -- is one implementation, and each of the three arms
 * below is exactly its platform's ring behind these names:
 *
 *   start      builds it, or answers that this run has none;
 *   ready      whether it exists;
 *   offer      hands it one record, or answers that it has no form for it;
 *   progress   reaps what is ready, without waiting;
 *   flush      rings a deferred doorbell before this thread blocks elsewhere;
 *   park       sleeps on it until an event arrives;
 *   shutdown   takes it down at process exit;
 *   submissions / submission_enters, its two counters.
 *
 * A platform with no ring answers "no" to all of them and every operation
 * takes the bounded adapter, which is the Darwin route and the route
 * WF_IO_NO_NATIVE_RING selects on either of the other two. */

#if defined(__linux__) || defined(_WIN32)

/* `wf_bridge_fail` for a fail-stop whose site holds the code the target
 * answered.  It lives with the ring rather than beside its sibling above
 * because its every caller is one of the ring seam's calls, and a target with
 * no ring makes none of them.
 *
 * The name of the site says which of the ring's calls failed; the code says
 * what it failed with, and the two are different questions.  EPROTO from a
 * reaping pass means the port handed back something that is not one of this
 * runtime's records, which is a different defect from any Win32 or errno value
 * in the same place -- and a log that carries only the site cannot tell them
 * apart without another run. */
static _Noreturn void wf_bridge_fail_with_code(const char *reason, int code) {
    (void)fprintf(stderr, "whitefoot completion: %s: error %d\n", reason, code);
    (void)fflush(stderr);
    abort();
}

/* WF_IO_NO_NATIVE_RING: run this process on the bounded adapter route.
 *
 * Runtime policy of the same class as WF_IO_NOCACHE and WF_IO_HELPERS, and
 * test-only within that class: no Whitefoot source names it, no accepted
 * program changes meaning under it, and no byte any operation produces
 * differs with it set.  Absent -- and any value other than the exact text "1"
 * -- is today's behaviour exactly.
 *
 * It exists because the route a host takes is not a choice a test can
 * otherwise make.  A host with a kernel completion ring takes every positioned
 * read into it, so the adapter branch of `bridge_default_probe`'s route
 * assertion cannot fire there at all, and the negative control it provides was
 * carried by the Darwin CI host alone.  Skipping the ring's start reaches the
 * same state a host without a ring reaches, which is a path the runtime
 * already has rather than one this setting adds.
 *
 * It is read once, before the only call that starts the ring, through the
 * platform layer's own setting read (`../sched/prim.h`, P4) rather than
 * `getenv`, because a shared unit that names `getenv` does not compile under
 * the MSVC ucrt. */
static int wf_bridge_native_ring_refused(void) {
    char text[WF_PRIM_SETTING_BYTES];
    return wf_prim_setting_text("WF_IO_NO_NATIVE_RING", text, sizeof(text)) == 1
        && text[0] == '1' && text[1] == 0;
}

#endif

#if defined(__linux__)

static wf_linux_io_uring_adapter wf_bridge_linux_adapter;
static _Atomic unsigned wf_bridge_linux_ready;
/* The one piece of bridge readiness a thread may observe without running the
 * initializer itself.  `wf_bridge_ring_flush` must not create a ring for a
 * program that only ever makes direct calls, so it cannot go through the
 * once-control; this release/acquire pair is what orders the ring's
 * construction before another thread's flush of it. */
static _Atomic unsigned wf_bridge_doorbell_ready;

static int wf_bridge_ring_ready(void) {
    return wf_bridge_linux_ready != 0;
}

/* The ring a thread submits to, reaps and parks on: a driver thread's own
 * when it has one, and otherwise the process's, which the entry's driver and
 * every thread that is not a driver use. */
static _Thread_local wf_linux_io_uring_adapter *wf_bridge_thread_adapter;

static wf_linux_io_uring_adapter *wf_bridge_linux_current(void) {
    return wf_bridge_thread_adapter != NULL
        ? wf_bridge_thread_adapter
        : &wf_bridge_linux_adapter;
}

static int wf_bridge_ring_start(void) {
    if (wf_bridge_native_ring_refused()) {
        return 0;
    }
    if (wf_linux_io_uring_init(
            &wf_bridge_linux_adapter,
            &wf_bridge_runtime,
            WF_LINUX_IO_URING_DEPTH,
            WF_LINUX_IO_URING_COMPLETIONS
        ) != 0) {
        return 0;
    }
    if (wf_completion_set_wake_callback(
            &wf_bridge_runtime,
            wf_linux_io_uring_notify,
            &wf_bridge_linux_adapter
        ) != 0) {
        (void)wf_linux_io_uring_destroy(&wf_bridge_linux_adapter);
        return 0;
    }
    wf_bridge_linux_ready = 1;
    atomic_store_explicit(&wf_bridge_doorbell_ready, 1, memory_order_release);
    return 1;
}

static int wf_bridge_ring_offer(wf_completion_record *record) {
    if (!wf_bridge_ring_ready() || !wf_linux_io_uring_carries(record)) {
        return 0;
    }
    if (wf_linux_io_uring_submit(wf_bridge_linux_current(), record)
        != WF_LINUX_IO_URING_TARGET_OWNS) {
        /* The kind and shape were both answered before the record was
         * offered, so any other answer is a target-runtime failure. */
        wf_bridge_fail(
            "the io_uring target refused a record whose kind and shape it had already accepted"
        );
    }
    {
        int progress_error =
            wf_linux_io_uring_progress_error(wf_bridge_linux_current());
        if (progress_error != 0) {
            wf_bridge_fail_with_code(
                "the io_uring target reported a failure while submitting",
                progress_error
            );
        }
    }
    return 1;
}

static int wf_bridge_ring_progress(void) {
    size_t published = 0;
    if (!wf_bridge_ring_ready()) {
        return 0;
    }
    {
        int reap_error = wf_linux_io_uring_progress(
            wf_bridge_linux_current(),
            WF_BRIDGE_REAP_BUDGET,
            0,
            &published
        );
        if (reap_error != 0) {
            /* Target ownership has already transferred. Falling back now would
             * duplicate an operation, and ignoring the error would strand its
             * owned operation forever. A target-runtime failure is a fail-stop
             * TCB defect, not a writer-visible IoError. */
            wf_bridge_fail_with_code(
                "the io_uring target failed while reaping completions",
                reap_error
            );
        }
    }
    return published != 0;
}

static void wf_bridge_ring_flush(void) {
    if (wf_bridge_thread_adapter != NULL) {
        (void)wf_linux_io_uring_flush(wf_bridge_thread_adapter);
    } else if (atomic_load_explicit(&wf_bridge_doorbell_ready, memory_order_acquire)
        != 0) {
        (void)wf_linux_io_uring_flush(&wf_bridge_linux_adapter);
    }
}

static int wf_bridge_ring_park(uint64_t observed_epoch) {
    if (!wf_bridge_ring_ready()) {
        return 0;
    }
    {
        int park_error = wf_linux_io_uring_park(
            wf_bridge_linux_current(),
            observed_epoch,
            UINT32_MAX
        );
        if (park_error != 0) {
            wf_bridge_fail_with_code(
                "the io_uring target failed while parking on the ring",
                park_error
            );
        }
    }
    return 1;
}

static void wf_bridge_ring_shutdown(void) {
    if (!wf_bridge_ring_ready()) {
        return;
    }
    atomic_store_explicit(&wf_bridge_doorbell_ready, 0, memory_order_release);
    (void)wf_linux_io_uring_destroy(&wf_bridge_linux_adapter);
    wf_bridge_linux_ready = 0;
}

static uint64_t wf_bridge_ring_submissions(void) {
    return wf_bridge_ring_ready()
        ? wf_linux_io_uring_statistics_snapshot(&wf_bridge_linux_adapter)
              .submissions
        : 0u;
}

static uint64_t wf_bridge_ring_submission_enters(void) {
    return wf_bridge_ring_ready()
        ? wf_linux_io_uring_statistics_snapshot(&wf_bridge_linux_adapter)
              .submission_enters
        : 0u;
}

/* The ring's counters as one line, for the same observer that prints the
 * core's: what the kernel was asked and how often a thread slept for it. */
int wf__bridge_report(char *buffer, size_t capacity) {
    wf_linux_io_uring_statistics ring;
    int written;
    if (buffer == NULL || capacity == 0u || !wf_bridge_ring_ready()) {
        return 0;
    }
    ring = wf_linux_io_uring_statistics_snapshot(&wf_bridge_linux_adapter);
    written = snprintf(
        buffer,
        capacity,
        "ring: submissions=%llu submission_enters=%llu completions=%llu "
        "kernel_waits=%llu kernel_wakes=%llu host_wake_writes=%llu "
        "overflow_flushes=%llu runtime_parks=%llu inline=%llu",
        (unsigned long long)ring.submissions,
        (unsigned long long)ring.submission_enters,
        (unsigned long long)ring.completions,
        (unsigned long long)ring.kernel_waits,
        (unsigned long long)ring.kernel_wakes,
        (unsigned long long)ring.host_wake_writes,
        (unsigned long long)ring.overflow_flushes,
        (unsigned long long)atomic_load_explicit(
            &wf_bridge_runtime.stat_parks,
            memory_order_relaxed
        ),
        (unsigned long long)atomic_load_explicit(
            &wf_bridge_inline_executions,
            memory_order_relaxed
        )
    );
    return written > 0 && (size_t)written < capacity;
}

#elif defined(_WIN32)

static wf_windows_iocp_adapter wf_bridge_windows_adapter;
static _Atomic unsigned wf_bridge_windows_ready;

static int wf_bridge_ring_ready(void) {
    return atomic_load_explicit(&wf_bridge_windows_ready, memory_order_acquire)
        != 0u;
}

/* WF_REQUIRE_WINDOWS_IOCP: the one environment check this bridge makes that
 * has no counterpart on the other platform.
 *
 * The POSIX side has no such facility -- its own required-ring runs are the
 * harness's WF_REQUIRE_LINUX_IO_URING, which is the harness's setting and not
 * the bridge's -- so this stays a Windows-only check rather than being given a
 * shared name that one platform would never answer.  It exists because correct
 * bytes alone would also be produced by a run that never reached the port at
 * all: the `completion-windows` job's "Compile and run a real Whitefoot program
 * through IOCP" and "Open target-native Windows components from compiler-emitted
 * buffers" steps set it, and this exit-time assertion is what makes those steps
 * evidence about the ring rather than about the adapter. */
static unsigned wf_bridge_windows_require_ring;

static void wf_bridge_verify_required_ring(void) {
    wf_windows_iocp_statistics statistics;
    if (wf_bridge_windows_require_ring == 0u) {
        return;
    }
    statistics = wf_windows_iocp_statistics_snapshot(
        &wf_bridge_windows_adapter
    );
    if (statistics.submissions == 0
        || statistics.completions != statistics.submissions) {
        wf_bridge_fail(
            "WF_REQUIRE_WINDOWS_IOCP was set but native IOCP was unavailable, unused, or incomplete"
        );
    }
}

static int wf_bridge_windows_ring_required(void) {
    char required[WF_PRIM_SETTING_BYTES];
    return wf_prim_setting_text(
               "WF_REQUIRE_WINDOWS_IOCP",
               required,
               sizeof(required)
           ) == 1
        && required[0] == '1' && required[1] == 0;
}

static int wf_bridge_ring_start(void) {
    /* Register before refusal or initialization can select the adapter. A
     * required-native run must fail even when no port was ever initialized. */
    if (wf_bridge_windows_ring_required()) {
        wf_bridge_windows_require_ring = 1u;
        if (atexit(wf_bridge_verify_required_ring) != 0) {
            wf_bridge_fail(
                "the completion port's exit-time check could not be registered"
            );
        }
    }
    if (wf_bridge_native_ring_refused()) {
        return 0;
    }
    if (wf_windows_iocp_init(
            &wf_bridge_windows_adapter,
            &wf_bridge_runtime,
            0
        ) != 0) {
        return 0;
    }
    if (wf_completion_set_wake_callback(
            &wf_bridge_runtime,
            wf_windows_iocp_notify,
            &wf_bridge_windows_adapter
        ) != 0) {
        (void)wf_windows_iocp_destroy(&wf_bridge_windows_adapter);
        return 0;
    }
    atomic_store_explicit(&wf_bridge_windows_ready, 1u, memory_order_release);
    return 1;
}

/* Binds one handle to this run's port.  The body of the association, without
 * the question of whether it has already been made: that question and this
 * answer are one critical section, and the section is the descriptor table's,
 * which is why this arrives there as a function rather than being written
 * there (`../windows_runtime.h`, `wf__windows_completion_ring_handle`). */
static int wf_bridge_windows_bind(HANDLE handle, void *context) {
    (void)context;
    return wf_windows_iocp_associate(&wf_bridge_windows_adapter, handle) == 0;
}

/* The handle the port may take for this descriptor, or none.
 *
 * A descriptor this runtime opened as a regular file for reading is the
 * ordinary case and its class says so.  A descriptor the process made some
 * other way -- a probe's own fixture -- is admitted on the one fact the port
 * needs, that it names a disk file.  Either way the association is made once
 * and remembered, because `CreateIoCompletionPort` takes a handle exactly once
 * and no host call asks whether it already has; and it is made under the
 * table's lock, because every lane of a program may offer its first record on
 * one descriptor at the same moment. */
static int wf_bridge_windows_port_handle(int descriptor, HANDLE *handle) {
    return wf__windows_completion_ring_handle(
        descriptor,
        wf_bridge_windows_bind,
        NULL,
        handle
    );
}

static int wf_bridge_ring_offer(wf_completion_record *record) {
    HANDLE handle;
    int descriptor;
    if (!wf_bridge_ring_ready() || !wf_windows_iocp_carries(record)) {
        return 0;
    }
    /* The descriptor this request will be issued on.  A connect has none until
     * the ring makes its socket, which it does here for the same reason the
     * Linux ring makes it in its own submit and for one more: the port takes a
     * handle before a request is issued on it.  A record the port then refuses
     * has that socket taken back, so the bounded adapter sees exactly what was
     * offered (`windows_iocp.h`). */
    descriptor = wf_windows_iocp_issue_descriptor(record);
    if (descriptor < 0) {
        return 0;
    }
    if (!wf_bridge_windows_port_handle(descriptor, &handle)) {
        wf_windows_iocp_withdraw(record);
        return 0;
    }
    if (wf_windows_iocp_submit(&wf_bridge_windows_adapter, record, handle)
        != WF_WINDOWS_IOCP_TARGET_OWNS) {
        wf_bridge_fail(
            "the completion port refused a record whose kind and shape it had already accepted"
        );
    }
    {
        int progress_error =
            wf_windows_iocp_progress_error(&wf_bridge_windows_adapter);
        if (progress_error != 0) {
            wf_bridge_fail_with_code(
                "the completion port reported a failure while submitting",
                progress_error
            );
        }
    }
    return 1;
}

static int wf_bridge_ring_progress(void) {
    size_t published = 0;
    if (!wf_bridge_ring_ready()) {
        return 0;
    }
    {
        int reap_error = wf_windows_iocp_progress(
            &wf_bridge_windows_adapter,
            1u,
            &published
        );
        if (reap_error != 0) {
            wf_bridge_fail_with_code(
                "the completion port failed while reaping completions",
                reap_error
            );
        }
    }
    return published != 0;
}

/* Nothing is deferred on this port: a request reaches the kernel inside the
 * call that issues it, so there is no doorbell to ring. */
static void wf_bridge_ring_flush(void) {}

static int wf_bridge_ring_park(uint64_t observed_epoch) {
    if (!wf_bridge_ring_ready()) {
        return 0;
    }
    {
        int park_error = wf_windows_iocp_park(
            &wf_bridge_windows_adapter,
            observed_epoch,
            UINT32_MAX
        );
        if (park_error != 0) {
            wf_bridge_fail_with_code(
                "the completion port failed while parking on the port",
                park_error
            );
        }
    }
    return 1;
}

static void wf_bridge_ring_shutdown(void) {
    if (!wf_bridge_ring_ready()) {
        return;
    }
    (void)wf_windows_iocp_destroy(&wf_bridge_windows_adapter);
    atomic_store_explicit(&wf_bridge_windows_ready, 0u, memory_order_release);
}

static uint64_t wf_bridge_ring_submissions(void) {
    return wf_bridge_ring_ready()
        ? wf_windows_iocp_statistics_snapshot(&wf_bridge_windows_adapter)
              .submissions
        : 0u;
}

/* The port takes each request inside the call that issues it, so there is no
 * deferred doorbell and no count of the calls that carried one. */
static uint64_t wf_bridge_ring_submission_enters(void) {
    return 0u;
}

/* The port's counters are the probe's to print; the observer's ring line is
 * the Linux ring's alone. */
int wf__bridge_report(char *buffer, size_t capacity) {
    (void)buffer;
    (void)capacity;
    return 0;
}

#else

/* A target with no kernel completion ring in the supported set: its qualified
 * path is the bounded typed adapter, and every one of these answers "no".
 *
 * This arm consults WF_IO_NO_NATIVE_RING nowhere, and its reader is not
 * compiled here: a run on a target with no ring is already the route that
 * setting selects, so a start that read it could only agree with itself.  The
 * reader sits above with the two arms that have something to refuse. */
static int wf_bridge_ring_ready(void) {
    return 0;
}

static int wf_bridge_ring_start(void) {
    return 0;
}

static int wf_bridge_ring_offer(wf_completion_record *record) {
    (void)record;
    return 0;
}

static int wf_bridge_ring_progress(void) {
    return 0;
}

static void wf_bridge_ring_flush(void) {}

static int wf_bridge_ring_park(uint64_t observed_epoch) {
    (void)observed_epoch;
    return 0;
}

static void wf_bridge_ring_shutdown(void) {}

static uint64_t wf_bridge_ring_submissions(void) {
    return 0u;
}

static uint64_t wf_bridge_ring_submission_enters(void) {
    return 0u;
}

int wf__bridge_report(char *buffer, size_t capacity) {
    (void)buffer;
    (void)capacity;
    return 0;
}

#endif

/* The wake epoch, and it comes up before everything else the bridge has.
 *
 * The core sleeps and wakes on one primitive (design §7, platform item 2), and
 * "one" has to mean one for the life of the process rather than one at a time.
 * The three seam functions below answer from `wf_bridge_runtime`, so a thread
 * that parked before this unit had a runtime would sleep on `prim_host.c`'s own
 * condition variable while every wake after it went to this one -- a lost wake
 * and, with no timeout anywhere in this design, a hang.  Two things make that
 * reachable rather than theoretical: a worker enters its scheduler loop at the
 * core's entry, before the program's first operation, and the first operation's
 * once-control is a window another thread can park inside.
 *
 * So the epoch has its own start, taken by whichever of the two arrives first:
 * a seam call, or the bridge's own initializer.  It is a mutex, a condition
 * variable and a counter -- no ring, no helper, no descriptor -- so a program
 * that never submits anything pays for those and nothing else.  Both sleep
 * mechanisms announce themselves against this one epoch under this one lock,
 * the ring's `epoll_wait` included, so one wake reaches a sleeper on either. */
static unsigned wf_bridge_wake_once;
static int wf_bridge_wake_error;
static _Atomic unsigned wf_bridge_wake_ready;

static void wf_bridge_initialize_wake(void) {
    wf_bridge_wake_error = wf_completion_runtime_init(&wf_bridge_runtime);
    if (wf_bridge_wake_error == 0) {
        wf_bridge_wake_ready = 1;
    }
}

static int wf_bridge_ensure_wake(void) {
    wf__sched_once(&wf_bridge_wake_once, wf_bridge_initialize_wake);
    return wf_bridge_wake_ready != 0;
}

static void wf_bridge_initialize(void) {
    if (!wf_bridge_ensure_wake()) {
        wf_bridge_error = wf_bridge_wake_error != 0 ? wf_bridge_wake_error : EAGAIN;
        return;
    }
    /* The ring where this run has one, and the bounded typed adapter where it
     * does not: a target with no kernel completion facility for regular files
     * -- Darwin in the supported set, and either of the other two under
     * WF_IO_NO_NATIVE_RING -- must have the adapter before it is ready, because
     * that adapter is then the whole engine. */
    if (!wf_bridge_ring_start() && !wf_bridge_ensure_file()) {
        (void)wf_completion_runtime_destroy(&wf_bridge_runtime);
        return;
    }
    wf_bridge_ready = 1;
    if (atexit(wf_bridge_shutdown) != 0) {
        /* Registration failure changes cleanup at process exit, not the
         * completion contract of any admitted operation. */
    }
}

/* The bridge, or nothing.
 *
 * A bridge that cannot initialize leaves no engine to run the operation and
 * no drain to publish it, so it is a trusted-computing-base failure and
 * terminates deterministically where the floor does.  It is not an operation
 * outcome and there is no arm left to fall to (design §7). */
static void wf_bridge_require(void) {
    wf__sched_once(&wf_bridge_once, wf_bridge_initialize);
    if (wf_bridge_ready == 0) {
        wf_bridge_fail(
            "the completion bridge could not initialize"
        );
    }
}

/* -------------------------------------------------- the one publication */

/* Publish exactly once, then notify using only permanent engine storage.
 * The waiter may reclaim its frame as soon as it observes DONE. */
static int wf_context_record_published(const wf_completion_record *record);

void wf_completion_record_complete(wf_completion_record *record) {
    if (!wf_bridge_ensure_wake()) wf_bridge_fail("completion wake initialization failed");
    atomic_fetch_add_explicit(&wf_bridge_publications, 1, memory_order_relaxed);
    wf_completion_record_publish(record);
    /* The address only: the waiter may reclaim the record from here on. A
     * record that woke a context on this very driver needs no other wake;
     * every other one may have a joiner parked on the process's wake. */
    if (!wf_context_record_published(record)) {
        wf_completion_notify_target(&wf_bridge_runtime);
    }
}

/* --------------------------------------------------------- target progress */

/* Rings the deferred io_uring doorbell before this thread does something the
 * ring cannot see through.
 *
 * Staging an SQE costs no syscall, so a submission leaves work the kernel has
 * not been told about.  That is exactly what makes deferring worth 15 % of the
 * eight-wide benchmark's wall time, and exactly what makes an unguarded
 * blocking call a hazard: an open the program has already submitted would sit
 * in the submission queue, untouched, for as long as this thread waits in a
 * direct `openat`.  Every entry point below that can block outside the ring
 * flushes first.  On a target with no ring this is nothing. */
static void wf_bridge_flush_target(void) {
    wf_bridge_ring_flush();
}

/* Flush the deferred doorbell, reap the ring, and run one queued request when
 * this thread is the queue's only engine.  Returns nonzero when it moved
 * something.
 *
 * This is primitive 7 (§7.1) and it may block: with no helper the bounded
 * pass executes a queued host `open` or `close` on the calling thread.  That
 * is now the only place a host call is made for a queued operation. */
static int wf_bridge_progress(void) {
    int progressed = wf_bridge_ring_progress();
    /* A thread executes a queued request only when no target helper exists.
     * With helpers, taking an unrelated request here could block the exact
     * frame which is waiting on a completion they have already published. */
    if (wf_file_adapter_helper_count(&wf_bridge_adapter) == 0) {
        progressed |= wf_bridge_target_progress_one();
    }
    return progressed;
}

static void wf_bridge_park(uint64_t observed_epoch) {
    if (wf_bridge_ring_park(observed_epoch)) {
        /* Reap what the ring has first; the caller's next turn re-reads its
         * own record. */
        (void)wf_bridge_progress();
        return;
    }
    {
        enum wf_completion_park_result parked =
            wf_completion_park_if_unchanged(
                &wf_bridge_runtime,
                observed_epoch,
                UINT32_MAX
            );
        if (parked == WF_COMPLETION_PARK_FAILED) {
            wf_bridge_fail(
                "the completion runtime's park failed"
            );
        }
    }
}

/* ------------------------------------------------------- waiting contexts */

/* Contexts [WAIT-2]: the root, which runs the entry on the floor's thread,
 * and every call a spawn starts [WAIT-3].
 *
 * A context is a chain of resumable frames
 * (design/compiler/waiting-contexts.md).  Every waiting function
 * is an LLVM coroutine: its frame comes from the running context's arena, a
 * call transfers into the callee and a finished callee back into its caller
 * without returning here, and a frame that has to wait registers its context
 * and suspends, which returns to the driver below.  The driver resumes the
 * next ready context, or reaps and parks the thread until one is ready.
 *
 * They all live on that one thread, and a context changes hands only where
 * one of its frames suspends.  A compute task never waits [PAR-1, PAR-2], so
 * no suspension happens between a compute offer and its join, and the
 * scheduler's lane state stays with the thread.  With one thread and hand-
 * overs only there, the queues need no lock and a context started with a
 * factory shares its budget counter without an atomic.
 *
 * A record still carries no waiter: a parked context names its record, a
 * record this thread publishes wakes its context by the record's address, and
 * a pass over the parked contexts finds the ones another thread published. */

static unsigned wf_bridge_record_state(const wf_completion_record *record);
static int wf_bridge_transfer_now(wf_completion_record *record);
static void wf_bridge_execute_here(wf_completion_record *record);

/* The coroutine entries, defined over LLVM's intrinsics in the ordinary
 * library every program links.  A build that links the bridge without it,
 * such as the completion probes, starts no context and links these weak
 * definitions. */
__attribute__((weak)) void wf__coro_resume(void *frame) {
    (void)frame;
    wf_bridge_fail("contexts need the ordinary library, which this build does not link");
}
__attribute__((weak)) void wf__coro_destroy(void *frame) {
    (void)frame;
    wf_bridge_fail("contexts need the ordinary library, which this build does not link");
}
__attribute__((weak)) int wf__coro_done(void *frame) {
    (void)frame;
    wf_bridge_fail("contexts need the ordinary library, which this build does not link");
}

/* The memory contexts and their frames come from: blocks of host regions
 * this section reserves itself, never the program's allocator, which the
 * runtime every build links does not call [STOR-8].  A frame is the resumable
 * form of a stack, and stacks were always the host's, reserved by the floor.
 *
 * A region is one 64 MiB reservation, so thousands of contexts share one
 * kernel mapping, and only the pages a frame touches become resident.  Blocks
 * are powers of two from 512 bytes to 2 MiB, carved from the current region
 * in order and kept on a free list per size once released; every block is
 * aligned to at least its smallest size.  A larger request gets a reservation
 * of its own, released with it.  Every driver thread takes and gives blocks,
 * and a context made on one driver may finish on another, so the lists are
 * taken under a spin lock; that happens when a context starts or finishes
 * and when a frame outgrows its chunk, not on every call. */
#define WF_POOL_SMALLEST_SHIFT 9u
#define WF_POOL_CLASSES 13u
#define WF_POOL_LARGEST ((size_t)1u << (WF_POOL_SMALLEST_SHIFT + WF_POOL_CLASSES - 1u))
#define WF_POOL_REGION_BYTES ((size_t)64u * 1024u * 1024u)
#define WF_POOL_LARGE_GRAIN ((size_t)64u * 1024u)

typedef struct wf_pool_block wf_pool_block;
struct wf_pool_block {
    wf_pool_block *next;
};

static wf_pool_block *wf_pool_released[WF_POOL_CLASSES];
static unsigned char *wf_pool_cursor;
static size_t wf_pool_remaining;
static atomic_flag wf_pool_lock = ATOMIC_FLAG_INIT;

static void wf_spin_lock(atomic_flag *flag) {
    while (atomic_flag_test_and_set_explicit(flag, memory_order_acquire)) {
        wf_prim_spin_hint();
    }
}

static void wf_spin_unlock(atomic_flag *flag) {
    atomic_flag_clear_explicit(flag, memory_order_release);
}

/* A frame no memory can hold ends the program: no source outcome can refuse
 * a waiting call, exactly as none can refuse an ordinary call's stack frame
 * [SCOPE-3]. */
static _Noreturn void wf_context_exhausted(void) {
    wf_bridge_fail("no memory could be reserved for a waiting function's frame");
}

static void *wf_pool_host_reserve(size_t bytes) {
#if defined(_WIN32)
    return VirtualAlloc(NULL, bytes, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
#else
    int flags = MAP_PRIVATE | MAP_ANONYMOUS;
#if defined(MAP_NORESERVE)
    flags |= MAP_NORESERVE;
#endif
    void *region = mmap(NULL, bytes, PROT_READ | PROT_WRITE, flags, -1, 0);
    return region == MAP_FAILED ? NULL : region;
#endif
}

static void wf_pool_host_release(void *block, size_t bytes) {
#if defined(_WIN32)
    (void)bytes;
    (void)VirtualFree(block, 0, MEM_RELEASE);
#else
    (void)munmap(block, bytes);
#endif
}

static unsigned wf_pool_class_of(size_t size) {
    unsigned index = 0;
    while (((size_t)1u << (WF_POOL_SMALLEST_SHIFT + index)) < size) {
        index += 1u;
    }
    return index;
}

/* Hands what is left of the current region to the free lists, largest
 * blocks first, so a new region wastes none of the old one. */
static void wf_pool_retire_region(void) {
    unsigned index = WF_POOL_CLASSES;
    while (index > 0u) {
        size_t size;
        index -= 1u;
        size = (size_t)1u << (WF_POOL_SMALLEST_SHIFT + index);
        while (wf_pool_remaining >= size) {
            wf_pool_block *block = (wf_pool_block *)(void *)wf_pool_cursor;
            block->next = wf_pool_released[index];
            wf_pool_released[index] = block;
            wf_pool_cursor += size;
            wf_pool_remaining -= size;
        }
    }
    wf_pool_cursor = NULL;
    wf_pool_remaining = 0;
}

/* A block of at least `bytes`; its whole size is stored in `granted`. */
static void *wf_pool_take_locked(size_t bytes, size_t *granted);

static void *wf_pool_take(size_t bytes, size_t *granted) {
    void *block;
    wf_spin_lock(&wf_pool_lock);
    block = wf_pool_take_locked(bytes, granted);
    wf_spin_unlock(&wf_pool_lock);
    return block;
}

static void *wf_pool_take_locked(size_t bytes, size_t *granted) {
    unsigned index;
    size_t size;
    void *block;
    if (bytes > WF_POOL_LARGEST) {
        if (bytes > SIZE_MAX - WF_POOL_LARGE_GRAIN) {
            wf_context_exhausted();
        }
        size = (bytes + WF_POOL_LARGE_GRAIN - 1u) / WF_POOL_LARGE_GRAIN * WF_POOL_LARGE_GRAIN;
        block = wf_pool_host_reserve(size);
        if (block == NULL) {
            wf_context_exhausted();
        }
        *granted = size;
        return block;
    }
    index = wf_pool_class_of(bytes);
    size = (size_t)1u << (WF_POOL_SMALLEST_SHIFT + index);
    *granted = size;
    if (wf_pool_released[index] != NULL) {
        wf_pool_block *reused = wf_pool_released[index];
        wf_pool_released[index] = reused->next;
        return reused;
    }
    if (wf_pool_remaining < size) {
        unsigned char *region;
        wf_pool_retire_region();
        region = (unsigned char *)wf_pool_host_reserve(WF_POOL_REGION_BYTES);
        if (region == NULL) {
            wf_context_exhausted();
        }
        wf_pool_cursor = region;
        wf_pool_remaining = WF_POOL_REGION_BYTES;
    }
    block = wf_pool_cursor;
    wf_pool_cursor += size;
    wf_pool_remaining -= size;
    return block;
}

static void wf_pool_give(void *block, size_t granted) {
    wf_pool_block *released;
    unsigned index;
    if (block == NULL) {
        return;
    }
    if (granted > WF_POOL_LARGEST) {
        wf_pool_host_release(block, granted);
        return;
    }
    index = wf_pool_class_of(granted);
    released = (wf_pool_block *)block;
    wf_spin_lock(&wf_pool_lock);
    released->next = wf_pool_released[index];
    wf_pool_released[index] = released;
    wf_spin_unlock(&wf_pool_lock);
}

/* One chunk of a context's frame arena.  Frames are allocated and released
 * last in, first out, because a caller releases its callee's frame before it
 * continues and a context's outermost frame is released last. */
typedef struct wf_context_chunk wf_context_chunk;
struct wf_context_chunk {
    wf_context_chunk *previous;
    size_t used;
    size_t capacity;
    /* Keeps the frames that follow at the alignment LLVM's frames need. */
    uint64_t align[1];
};
#define WF_CONTEXT_CHUNK_HEADER ((sizeof(wf_context_chunk) + 15u) / 16u * 16u)
/* A context's first chunk holds its argument block, its outermost frame and
 * small callees.  A frame that does not fit gets a chunk of its own size plus
 * room for the small frames it calls, so a large frame, such as one holding a
 * connection's window, keeps its callees beside it instead of doubling into a
 * chunk the host allocator would map separately. */
#define WF_CONTEXT_FIRST_CHUNK 1024u
#define WF_CONTEXT_CHUNK_SLACK 4096u

typedef struct wf_context wf_context;
struct wf_context {
    /* The frame the driver resumes when the context is chosen. */
    void *resume;
    /* The context's outermost frame, done when the context has finished. */
    void *root;
    wf_context *next;
    wf_context *previous;
    /* The record a parked or polling context waits on; NULL while it runs,
     * is ready, or waits for the contexts it started. */
    wf_completion_record *record;
    /* The starting activation's group: the count of its unfinished contexts
     * and the context waiting for it to reach zero. */
    uint64_t *group;
    /* The driver that last ran the context, where it is made ready: a parked
     * context waits on that driver's ring.  A ready context moves to a
     * driver that takes it. */
    struct wf_driver *driver;
    /* With no ring: the descriptor and events a context polling for
     * readiness waits on, and zero events otherwise. */
    int poll_descriptor;
    unsigned poll_events;
    /* The chain of parked contexts whose records hash alike. */
    wf_context *record_next;
    /* The frame arena: the chunk frames are taken from, and one emptied
     * chunk kept for the next call that needs more than the current one. */
    wf_context_chunk *arena;
    wf_context_chunk *spare;
    /* The pool block this record occupies, zero for the root's. */
    size_t pool_bytes;
    /* While the context waits for a shared object: how many unlocks woke it
     * to try again without its getting the object, which keeps its place at
     * the head of the object's queue and, at WF_SHARED_HANDOFF, makes the
     * next unlock hand it the object [SHARE-3]; whether it asked to write;
     * and whether an unlock handed it the object while it was parked. */
    uint32_t shared_woken;
    uint32_t shared_write;
    uint32_t shared_granted;
    /* How many waits in a row the host, an object or a join answered at once
     * since the driver last resumed the context [WAIT-2]. */
    uint32_t passes;
    /* The one host operation the context has pending. */
    union {
        unsigned char bytes[WF_CONTEXT_OPERATION_BYTES];
        uint64_t align[2];
    } operation;
};
_Static_assert(
    WF_CONTEXT_OPERATION_BYTES >= sizeof(wf_completion_record),
    "a context's operation block must hold its completion record"
);

/* The group an activation keeps in its frame: two words the emitted code
 * zeroes at entry.  Word 1 holds a context pointer. */
_Static_assert(sizeof(uintptr_t) <= sizeof(uint64_t), "a group word holds a context");

/* Parked contexts are found by the address of their record. */
#define WF_CONTEXT_RECORD_BUCKETS 4096u
/* The most drivers a program runs, whatever WF_DRIVERS asks for. */
#define WF_DRIVER_LIMIT 64u

/* One driver: a thread that runs ready contexts, each on the ring of the
 * driver that runs it (`research/investigations/io-model/WAITS.md`,
 * Experiment 5).  Everything here but the run queue and the two flags is its
 * own thread's alone. */
typedef struct wf_driver wf_driver;
struct wf_driver {
    /* The contexts ready to run here, which a driver with none may take
     * about half of, and their count, read without the lock as a hint. */
    atomic_flag run_lock;
    wf_context *run_head;
    wf_context *run_tail;
    _Atomic unsigned run_count;
    /* Set while this driver is parked with nothing to run, and when a driver
     * that made work ready woke it to look for some. */
    _Atomic unsigned idle;
    _Atomic unsigned searcher;
    wf_context *parked;
    /* Contexts waiting for a descriptor's readiness, with no ring to wait
     * in; only a program with one driver has any. */
    wf_context *polling;
    size_t polling_count;
    wf_file_readiness polls[WF_FILE_READINESS_BATCH];
    wf_context *polled[WF_FILE_READINESS_BATCH];
    /* Parked contexts by the address of their record. A record published on
     * this driver's thread wakes its context here at once; one another
     * thread publishes is found by a pass over the parked contexts, which
     * runs only after such a publication. */
    wf_context *by_record[WF_CONTEXT_RECORD_BUCKETS];
    uint64_t foreign_seen;
    /* The wake this driver parks on: the process's for driver 0, its own for
     * every other, whose ring's wake it is. */
    wf_completion_runtime *runtime;
    wf_completion_runtime own_runtime;
#if defined(__linux__)
    wf_linux_io_uring_adapter own_adapter;
    wf_prim_thread thread;
#endif
    _Atomic unsigned exited;
    size_t pool_bytes;
    /* Contexts resumed since this driver last looked for host completions. */
    unsigned runs_since_reap;
    /* This driver's contexts parked on a host operation's record or polling a
     * descriptor: the waits a host outcome can end. Written only by this
     * driver's thread, and read by another only once this one is idle. */
    _Atomic unsigned host_waits;
};

static wf_context wf_context_root;
/* Driver 0 runs the entry on the floor's thread, and every program has it. */
static wf_driver wf_driver_root;
static wf_driver *wf_drivers[WF_DRIVER_LIMIT];
static _Atomic unsigned wf_driver_count;
static _Atomic unsigned wf_drivers_stopping;
static unsigned wf_drivers_once;
static _Thread_local wf_driver *wf_driver_self;
static _Thread_local wf_context *wf_context_current;
/* Set when the running context has parked where another driver may make it
 * ready: waiting for its group or for a shared object.  Another driver may
 * then resume, finish and release the context before its frame has returned
 * here, so the driver that ran it reads nothing of it afterwards. */
static _Thread_local int wf_context_parked_away;
/* The context `wf__context_prepare` made and `wf__context_launch` starts. */
static _Thread_local wf_context *wf_context_prepared;
/* Started and not finished, on every driver; the root is not counted. */
static _Atomic uint64_t wf_context_live;
/* Threads inside `wf_drivers_notify_others`, which a helper thread enters
 * after it publishes a record. */
static _Atomic unsigned wf_drivers_notifying;
/* Publications made on any thread but the one running the published
 * record's context, counted after each one is published; each driver keeps
 * the count its last pass over its parked contexts saw. */
static _Atomic uint64_t wf_context_foreign_publications;

/* Drivers parked with nothing to run, and whether one has been woken to
 * look for work and has not looked yet: while one has, no other is woken. */
static _Atomic unsigned wf_drivers_idle;
static _Atomic unsigned wf_drivers_searching;

/* Appends a ready context to a driver's run queue. */
static void wf_run_push(wf_driver *driver, wf_context *context) {
    wf_spin_lock(&driver->run_lock);
    context->next = NULL;
    if (driver->run_tail != NULL) {
        driver->run_tail->next = context;
    } else {
        driver->run_head = context;
    }
    driver->run_tail = context;
    atomic_store_explicit(
        &driver->run_count,
        atomic_load_explicit(&driver->run_count, memory_order_relaxed) + 1u,
        memory_order_relaxed
    );
    wf_spin_unlock(&driver->run_lock);
}

static wf_context *wf_run_take(wf_driver *driver) {
    wf_context *context;
    if (atomic_load_explicit(&driver->run_count, memory_order_relaxed) == 0u) {
        return NULL;
    }
    wf_spin_lock(&driver->run_lock);
    context = driver->run_head;
    if (context != NULL) {
        driver->run_head = context->next;
        if (driver->run_head == NULL) {
            driver->run_tail = NULL;
        }
        context->next = NULL;
        atomic_store_explicit(
            &driver->run_count,
            atomic_load_explicit(&driver->run_count, memory_order_relaxed) - 1u,
            memory_order_relaxed
        );
    }
    wf_spin_unlock(&driver->run_lock);
    return context;
}

/* Wakes one parked driver to look for work, when some driver is parked and
 * none has been woken for that already. */
static void wf_drivers_wake_one(void) {
    unsigned count;
    unsigned index;
    unsigned expected = 0u;
    if (atomic_load_explicit(&wf_drivers_idle, memory_order_seq_cst) == 0u
        || atomic_load_explicit(&wf_drivers_searching, memory_order_relaxed) != 0u) {
        return;
    }
    if (!atomic_compare_exchange_strong_explicit(
            &wf_drivers_searching, &expected, 1u,
            memory_order_seq_cst, memory_order_relaxed)) {
        return;
    }
    count = atomic_load_explicit(&wf_driver_count, memory_order_acquire);
    for (index = 0; index < count; index++) {
        wf_driver *driver = wf_drivers[index];
        if (driver == NULL || driver == wf_driver_self) {
            continue;
        }
        if (atomic_exchange_explicit(&driver->idle, 0u, memory_order_seq_cst) != 0u) {
            atomic_fetch_sub_explicit(&wf_drivers_idle, 1u, memory_order_seq_cst);
            atomic_store_explicit(&driver->searcher, 1u, memory_order_relaxed);
            wf_completion_notify_target(driver->runtime);
            return;
        }
    }
    atomic_store_explicit(&wf_drivers_searching, 0u, memory_order_seq_cst);
}

/* Makes a context ready on the driver it last ran on, waking that driver
 * when it is another; with more ready here than this driver runs next, a
 * parked driver is woken to take some. */
static void wf_context_ready(wf_context *context) {
    wf_driver *target = context->driver;
    wf_run_push(target, context);
    if (target != wf_driver_self) {
        /* The epoch rises after the queue holds the context, so a driver
         * that sampled the epoch before this either sees the context or
         * parks on a changed epoch, which returns at once. */
        wf_completion_notify_target(target->runtime);
    } else if (atomic_load_explicit(&target->run_count, memory_order_relaxed) > 1u) {
        wf_drivers_wake_one();
    }
}

/* How many waits in a row a context may have answered at once before it
 * offers its driver to another ready context. */
#define WF_CONTEXT_YIELD_PASSES 64u

/* [WAIT-2] one more wait the host, an object or a join answered without
 * suspending the running context. Drivers do not preempt, so a context whose
 * waits keep being answered at once, such as one polling an object for
 * another context's write, would otherwise hold its driver while the context
 * it waits for is ready. After WF_CONTEXT_YIELD_PASSES such waits, when
 * another context is ready on this driver, the context goes to the back of
 * the run queue and answers 1, and its frame suspends as for any wait; the
 * driver resets the count whenever it resumes the context. A context alone on
 * its driver continues, since there is no one to yield to. */
static int wf_context_pass(wf_context *self, void *frame) {
    wf_driver *driver = wf_driver_self;
    self->passes += 1u;
    if (self->passes < WF_CONTEXT_YIELD_PASSES) {
        return 0;
    }
    self->passes = 0u;
    if (driver == NULL
        || atomic_load_explicit(&driver->run_count, memory_order_relaxed) == 0u) {
        return 0;
    }
    self->resume = frame;
    wf_context_parked_away = 1;
    wf_context_ready(self);
    return 1;
}

/* Moves about half of another driver's ready contexts here, never the
 * root's, which runs on the entry's thread.  A ready context has no
 * operation in flight, so it can run on any driver; it submits its next
 * operation to the ring of the driver that runs it.  Returns nonzero when it
 * moved one. */
static int wf_driver_steal(wf_driver *driver) {
    unsigned count = atomic_load_explicit(&wf_driver_count, memory_order_acquire);
    unsigned offset;
    unsigned self_index = 0;
    for (offset = 0; offset < count; offset++) {
        if (wf_drivers[offset] == driver) {
            self_index = offset;
        }
    }
    for (offset = 1; offset < count; offset++) {
        wf_driver *victim = wf_drivers[(self_index + offset) % count];
        wf_context *taken = NULL;
        wf_context *taken_tail = NULL;
        wf_context *previous = NULL;
        wf_context *context;
        unsigned available;
        unsigned wanted;
        unsigned moved = 0;
        if (victim == NULL || victim == driver) {
            continue;
        }
        available = atomic_load_explicit(&victim->run_count, memory_order_relaxed);
        if (available == 0u) {
            continue;
        }
        wf_spin_lock(&victim->run_lock);
        available = atomic_load_explicit(&victim->run_count, memory_order_relaxed);
        wanted = (available + 1u) / 2u;
        context = victim->run_head;
        while (context != NULL && moved < wanted) {
            wf_context *next = context->next;
            if (context == &wf_context_root) {
                previous = context;
            } else {
                if (previous != NULL) {
                    previous->next = next;
                } else {
                    victim->run_head = next;
                }
                if (victim->run_tail == context) {
                    victim->run_tail = previous;
                }
                context->next = NULL;
                if (taken_tail != NULL) {
                    taken_tail->next = context;
                } else {
                    taken = context;
                }
                taken_tail = context;
                moved += 1u;
            }
            context = next;
        }
        atomic_store_explicit(&victim->run_count, available - moved, memory_order_relaxed);
        wf_spin_unlock(&victim->run_lock);
        if (moved == 0u) {
            continue;
        }
        while (taken != NULL) {
            wf_context *next = taken->next;
            taken->driver = driver;
            wf_run_push(driver, taken);
            taken = next;
        }
        return 1;
    }
    return 0;
}

/* A driver woken to look for work has looked: another may be woken now,
 * and is when this one found work, since more may be waiting. */
static void wf_driver_end_search(wf_driver *driver, int found) {
    if (atomic_exchange_explicit(&driver->searcher, 0u, memory_order_relaxed) == 0u) {
        return;
    }
    atomic_store_explicit(&wf_drivers_searching, 0u, memory_order_seq_cst);
    if (found) {
        wf_drivers_wake_one();
    }
}

static void wf_context_link(wf_context **list, wf_context *context) {
    context->previous = NULL;
    context->next = *list;
    if (*list != NULL) {
        (*list)->previous = context;
    }
    *list = context;
}

static void wf_context_unlink(wf_context **list, wf_context *context) {
    if (context->previous != NULL) {
        context->previous->next = context->next;
    } else {
        *list = context->next;
    }
    if (context->next != NULL) {
        context->next->previous = context->previous;
    }
    context->next = NULL;
    context->previous = NULL;
}

static size_t wf_context_record_bucket(const wf_completion_record *record) {
    uint64_t key = (uint64_t)(uintptr_t)record;
    key = (key >> 3) * UINT64_C(0x9e3779b97f4a7c15);
    return (size_t)(key >> 52) % WF_CONTEXT_RECORD_BUCKETS;
}

static void wf_context_park(wf_driver *driver, wf_context *context) {
    size_t bucket = wf_context_record_bucket(context->record);
    atomic_store_explicit(
        &driver->host_waits,
        atomic_load_explicit(&driver->host_waits, memory_order_relaxed) + 1u,
        memory_order_relaxed
    );
    wf_context_link(&driver->parked, context);
    context->record_next = driver->by_record[bucket];
    driver->by_record[bucket] = context;
}

static void wf_context_unpark(wf_driver *driver, wf_context *context) {
    wf_context **link = &driver->by_record[wf_context_record_bucket(context->record)];
    while (*link != NULL && *link != context) {
        link = &(*link)->record_next;
    }
    if (*link == context) {
        *link = context->record_next;
    }
    context->record_next = NULL;
    wf_context_unlink(&driver->parked, context);
    atomic_store_explicit(
        &driver->host_waits,
        atomic_load_explicit(&driver->host_waits, memory_order_relaxed) - 1u,
        memory_order_relaxed
    );
}

/* A context whose descriptor the host reported ready makes its operation now:
 * a transfer is retried without waiting until the host takes it, and an
 * accept is made once its listener is readable.  Returns nonzero when the
 * record is complete and the context can run. */
static int wf_context_readiness_answered(wf_context *context) {
    wf_completion_record *record = context->record;
    if (record->request.kind == WF_FILE_SOCKET_ACCEPT) {
        wf_bridge_execute_here(record);
        return 1;
    }
    return wf_bridge_transfer_now(record);
}

/* Polls the descriptors of the contexts waiting on readiness, at most one
 * batch of them, and makes every one whose operation the host then answers
 * ready to run.  timeout_ms bounds the wait; negative waits until one is
 * ready.  Returns nonzero when it moved one. */
static int wf_context_poll(wf_driver *driver, int timeout_ms) {
    size_t count = 0;
    size_t index;
    int answered;
    int moved = 0;
    wf_context *context = driver->polling;
    while (context != NULL && count < WF_FILE_READINESS_BATCH) {
        driver->polls[count].descriptor = context->poll_descriptor;
        driver->polls[count].events = context->poll_events;
        driver->polls[count].ready = 0;
        driver->polled[count] = context;
        count += 1;
        context = context->next;
    }
    if (count == 0) {
        return 0;
    }
    /* A batch that leaves contexts unpolled must not wait: those are polled
     * after the ones this batch finds ready rotate out of the list. */
    if (context != NULL) {
        timeout_ms = 0;
    }
    answered = wf_file_wait_readiness(driver->polls, count, timeout_ms);
    if (answered < 0) {
        wf_bridge_fail("a context could not wait for a descriptor's readiness");
    }
    for (index = 0; index < count; index++) {
        if (driver->polls[index].ready != 0) {
            wf_context *ready = driver->polled[index];
            if (!wf_context_readiness_answered(ready)) {
                continue;
            }
            wf_context_unlink(&driver->polling, ready);
            driver->polling_count -= 1u;
            atomic_store_explicit(
                &driver->host_waits,
                atomic_load_explicit(&driver->host_waits, memory_order_relaxed) - 1u,
                memory_order_relaxed
            );
            ready->poll_events = 0;
            ready->record = NULL;
            wf_context_ready(ready);
            moved = 1;
        }
    }
    return moved || answered > 0;
}

/* Every driver but this thread's is told to look over its parked contexts. */
static void wf_drivers_notify_others(void) {
    unsigned count;
    unsigned index;
    /* Counted before the driver count is read, so that `wf_drivers_end`,
     * which lowers the count and then waits for this to reach zero, never
     * releases a driver a helper thread is still notifying. */
    atomic_fetch_add_explicit(&wf_drivers_notifying, 1u, memory_order_seq_cst);
    count = atomic_load_explicit(&wf_driver_count, memory_order_seq_cst);
    for (index = 0; index < count; index++) {
        wf_driver *driver = wf_drivers[index];
        if (driver != NULL && driver != wf_driver_self) {
            wf_completion_notify_target(driver->runtime);
        }
    }
    atomic_fetch_sub_explicit(&wf_drivers_notifying, 1u, memory_order_seq_cst);
}

/* Wakes the context parked on a record this thread just published: at once
 * when it is this driver's, and otherwise, unless it is the running
 * context's own, by counting a publication every driver's next pass over its
 * parked contexts looks for. */
static int wf_context_record_published(const wf_completion_record *record) {
    wf_driver *driver = wf_driver_self;
    wf_context *context = NULL;
    if (driver != NULL) {
        context = driver->by_record[wf_context_record_bucket(record)];
        while (context != NULL && context->record != record) {
            context = context->record_next;
        }
    }
    if (context != NULL) {
        wf_context_unpark(driver, context);
        context->record = NULL;
        wf_context_ready(context);
        return 1;
    }
    if (driver != NULL && wf_context_current != NULL
        && (const void *)record == (const void *)wf_context_current->operation.bytes) {
        /* The running context's own operation, answered before it waits:
         * it reads its record before it parks, and nothing else waits on
         * it.  Every other record, such as one the shared helper pool
         * completed and this driver's progress published, may be parked on
         * any driver. */
        return 1;
    }
    atomic_fetch_add_explicit(&wf_context_foreign_publications, 1, memory_order_release);
    wf_drivers_notify_others();
    return 0;
}

/* Moves every parked context of this driver whose record is done to the
 * ready queue, once another thread has published since the last pass;
 * records this thread publishes wake their contexts as they are published.
 * Returns nonzero when it moved one. */
static int wf_context_harvest(wf_driver *driver) {
    int moved = 0;
    wf_context *context = driver->parked;
    uint64_t foreign = atomic_load_explicit(
        &wf_context_foreign_publications,
        memory_order_acquire
    );
    if (foreign == driver->foreign_seen) {
        return 0;
    }
    driver->foreign_seen = foreign;
    while (context != NULL) {
        wf_context *next = context->next;
        if (wf_bridge_record_state(context->record) == WF_COMPLETION_DONE) {
            wf_context_unpark(driver, context);
            context->record = NULL;
            wf_context_ready(context);
            moved = 1;
        }
        context = next;
    }
    return moved;
}

static void *wf_context_allocate(wf_context *context, uint64_t bytes) {
    wf_context_chunk *chunk = context->arena;
    size_t rounded;
    void *frame;
    if (bytes > SIZE_MAX / 4u) {
        wf_context_exhausted();
    }
    rounded = ((size_t)bytes + 15u) / 16u * 16u;
    if (chunk == NULL || chunk->capacity - chunk->used < rounded) {
        size_t wanted = rounded + WF_CONTEXT_CHUNK_SLACK;
        wf_context_chunk *fresh = context->spare;
        if (chunk == NULL && rounded <= WF_CONTEXT_FIRST_CHUNK - WF_CONTEXT_CHUNK_HEADER) {
            wanted = WF_CONTEXT_FIRST_CHUNK - WF_CONTEXT_CHUNK_HEADER;
        }
        if (fresh != NULL && fresh->capacity >= rounded) {
            context->spare = NULL;
        } else {
            size_t granted;
            if (fresh != NULL) {
                wf_pool_give(fresh, WF_CONTEXT_CHUNK_HEADER + fresh->capacity);
            }
            context->spare = NULL;
            fresh = (wf_context_chunk *)wf_pool_take(WF_CONTEXT_CHUNK_HEADER + wanted, &granted);
            fresh->capacity = granted - WF_CONTEXT_CHUNK_HEADER;
        }
        fresh->used = 0;
        fresh->previous = chunk;
        context->arena = fresh;
        chunk = fresh;
    }
    frame = (unsigned char *)chunk + WF_CONTEXT_CHUNK_HEADER + chunk->used;
    chunk->used += rounded;
    return frame;
}

static void wf_context_release(wf_context *context, void *frame) {
    wf_context_chunk *chunk = context->arena;
    unsigned char *base;
    if (chunk == NULL) {
        wf_bridge_fail("a frame was released from a context with no frames");
    }
    base = (unsigned char *)chunk + WF_CONTEXT_CHUNK_HEADER;
    if ((unsigned char *)frame < base || (unsigned char *)frame >= base + chunk->used) {
        wf_bridge_fail("a frame was released out of order");
    }
    chunk->used = (size_t)((unsigned char *)frame - base);
    if (chunk->used == 0 && chunk->previous != NULL) {
        context->arena = chunk->previous;
        if (context->spare != NULL) {
            wf_pool_give(context->spare, WF_CONTEXT_CHUNK_HEADER + context->spare->capacity);
        }
        context->spare = chunk;
    }
}

static void wf_context_release_arena(wf_context *context) {
    while (context->arena != NULL) {
        wf_context_chunk *previous = context->arena->previous;
        wf_pool_give(context->arena, WF_CONTEXT_CHUNK_HEADER + context->arena->capacity);
        context->arena = previous;
    }
    if (context->spare != NULL) {
        wf_pool_give(context->spare, WF_CONTEXT_CHUNK_HEADER + context->spare->capacity);
    }
    context->spare = NULL;
}

void *wf__context_frame_allocate(uint64_t bytes) {
    if (wf_context_current == NULL) {
        wf_bridge_fail("a waiting function was entered outside every context");
    }
    return wf_context_allocate(wf_context_current, bytes);
}

void wf__context_frame_release(void *frame) {
    if (wf_context_current == NULL) {
        wf_bridge_fail("a waiting function's frame was released outside every context");
    }
    wf_context_release(wf_context_current, frame);
}

void *wf__context_operation(void) {
    if (wf_context_current == NULL) {
        wf_bridge_fail("a host operation waited outside every context");
    }
    return wf_context_current->operation.bytes;
}

/* The frame's operation did not complete in its start.  One that completes
 * without suspending the context is counted as such a wait [WAIT-2].  A
 * readiness-routed
 * socket operation is made here when the host answers it without waiting,
 * and otherwise waits for its descriptor; an operation with no ring and no
 * readiness form is made here; every other one parks the context on its
 * record, on the driver running it. */
int wf__context_wait(void *operation, void *frame) {
    wf_context *self = wf_context_current;
    wf_driver *driver = wf_driver_self;
    wf_completion_record *record = (wf_completion_record *)operation;
    if (self == NULL || frame == NULL || driver == NULL) {
        wf_bridge_fail("a host operation waited outside every context");
    }
    if (record->route == WF_COMPLETION_ROUTE_READINESS) {
        const wf_file_request *request = &record->request;
        switch (request->kind) {
            case WF_FILE_SOCKET_RECEIVE:
                if (wf_bridge_transfer_now(record)) return wf_context_pass(self, frame);
                self->poll_descriptor = request->operation.receive.descriptor;
                self->poll_events = WF_FILE_READABLE;
                break;
            case WF_FILE_SOCKET_SEND:
                if (wf_bridge_transfer_now(record)) return wf_context_pass(self, frame);
                self->poll_descriptor = request->operation.send.descriptor;
                self->poll_events = WF_FILE_WRITABLE;
                break;
            case WF_FILE_SOCKET_ACCEPT:
                self->poll_descriptor = request->operation.accept.descriptor;
                self->poll_events = WF_FILE_READABLE;
                break;
            default:
                wf_bridge_execute_here(record);
                return wf_context_pass(self, frame);
        }
        self->record = record;
        self->resume = frame;
        atomic_store_explicit(
            &driver->host_waits,
            atomic_load_explicit(&driver->host_waits, memory_order_relaxed) + 1u,
            memory_order_relaxed
        );
        wf_context_link(&driver->polling, self);
        driver->polling_count += 1u;
        return 1;
    }
    if (wf_bridge_record_state(record) == WF_COMPLETION_DONE) {
        return wf_context_pass(self, frame);
    }
    self->record = record;
    self->resume = frame;
    wf_context_park(driver, self);
    return 1;
}

/* A host operation its start answered, with its result written or its
 * record complete: one more wait that did not suspend, which may make the
 * running context yield. It reads no record, so a target whose submissions
 * complete in their start needs nothing of it. */
int wf__context_pass(void *frame) {
    wf_context *self = wf_context_current;
    if (self == NULL || frame == NULL) {
        return 0;
    }
    return wf_context_pass(self, frame);
}

/* [WAIT-3] reserves a new context and returns the block its call's arguments
 * are stored in; `wf__context_launch` starts it.  No source outcome can
 * refuse a start, so memory the host will not reserve ends the program. */
void *wf__context_prepare(uint64_t bytes) {
    wf_context *context;
    if (wf_context_prepared != NULL) {
        wf_bridge_fail("a context was prepared while another waited to start");
    }
    {
        size_t granted;
        context = (wf_context *)wf_pool_take(sizeof(*context), &granted);
        memset(context, 0, sizeof(*context));
        context->pool_bytes = granted;
    }
    wf_context_prepared = context;
    return wf_context_allocate(context, bytes);
}

static void wf_drivers_begin(void);

/* The first context other than the root makes every operation the ring does
 * not carry a helper's: a scheduler thread inside a pipe's read or write, a
 * connect, or an open on a host with no ring would stop every context it
 * runs, the peer the operation waits on among them [WAIT-2].  The pool may
 * then grow to the bridge's ceiling even beside a ready ring, whose default
 * pool is empty; a written WF_IO_HELPERS keeps its pinned count, and a pinned
 * zero keeps the waiting thread as the queue's engine, as it asks. */
static unsigned wf_bridge_contexts_once;
static void wf_bridge_hold_for_contexts(void) {
    if (!wf_bridge_ensure_file()) {
        return;
    }
    if (atomic_load_explicit(&wf_bridge_helpers_pinned, memory_order_relaxed) == 0) {
        (void)wf_file_adapter_set_helper_cap(&wf_bridge_adapter, WF_BRIDGE_MAX_HELPERS);
    }
    (void)wf_file_adapter_hold_for_contexts(&wf_bridge_adapter);
}

/* A group's count while the last context to finish takes its waiter. The
 * finisher touches the group only until it stores zero, because once the
 * count reads zero the starting activation may leave and release the frame
 * the group lives in. */
#define WF_GROUP_CLOSING UINT64_MAX

/* Counts one more context in the group, after the last of the ones before
 * it has finished closing, whose store of zero would otherwise erase it. */
static void wf_group_add(uint64_t *group) {
    uint64_t count = __atomic_load_n(&group[0], __ATOMIC_ACQUIRE);
    for (;;) {
        if (count == WF_GROUP_CLOSING) {
            wf_prim_spin_hint();
            count = __atomic_load_n(&group[0], __ATOMIC_ACQUIRE);
            continue;
        }
        if (__atomic_compare_exchange_n(
                &group[0], &count, count + 1u, 0, __ATOMIC_SEQ_CST, __ATOMIC_ACQUIRE)) {
            return;
        }
    }
}

/* Starts the prepared context: its outermost frame is made here, in the new
 * context, so its arena holds it; the context joins the starting
 * activation's group and is made ready on the starter's driver, where it
 * waits its turn behind the ready contexts unless a parked driver takes it
 * first, while the starter continues with its next statement.  The first
 * start begins the other drivers. */
void wf__context_launch(uint64_t *group, void *arguments, void *(*start)(void *arguments)) {
    wf_context *context = wf_context_prepared;
    wf_context *starter = wf_context_current;
    void *frame;
    if (group == NULL || arguments == NULL || start == NULL || context == NULL
        || starter == NULL || wf_driver_self == NULL) {
        wf_bridge_fail("a context was started without its group, arguments or call");
    }
    wf_context_prepared = NULL;
    wf_context_current = context;
    frame = start(arguments);
    wf_context_current = starter;
    if (frame == NULL) {
        wf_bridge_fail("a started context has no frame");
    }
    context->root = frame;
    context->resume = frame;
    context->group = group;
    context->driver = wf_driver_self;
    wf__sched_once(&wf_drivers_once, wf_drivers_begin);
    wf__sched_once(&wf_bridge_contexts_once, wf_bridge_hold_for_contexts);
    wf_group_add(group);
    atomic_fetch_add_explicit(&wf_context_live, 1u, memory_order_relaxed);
    wf_context_ready(context);
}


/* Returns zero when every context the group counts has finished, and
 * otherwise records the running context as the group's waiter and returns
 * nonzero, after which its frame suspends until the last one finishes. */
int wf__context_join_wait(uint64_t *group, void *frame) {
    wf_context *self = wf_context_current;
    uint64_t count;
    for (;;) {
        count = __atomic_load_n(&group[0], __ATOMIC_ACQUIRE);
        if (count == 0u) {
            return self != NULL && frame != NULL ? wf_context_pass(self, frame) : 0;
        }
        if (count != WF_GROUP_CLOSING) {
            break;
        }
        wf_prim_spin_hint();
    }
    if (self == NULL || frame == NULL) {
        wf_bridge_fail("a join waited outside every context");
    }
    self->resume = frame;
    wf_context_parked_away = 1;
    __atomic_store_n(&group[1], (uint64_t)(uintptr_t)self, __ATOMIC_SEQ_CST);
    for (;;) {
        count = __atomic_load_n(&group[0], __ATOMIC_SEQ_CST);
        if (count != WF_GROUP_CLOSING) {
            break;
        }
        wf_prim_spin_hint();
    }
    if (count == 0u) {
        uint64_t expected = (uint64_t)(uintptr_t)self;
        if (__atomic_compare_exchange_n(
                &group[1], &expected, 0u, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST)) {
            self->resume = NULL;
            wf_context_parked_away = 0;
            return 0;
        }
        /* The last context took this waiter and wakes it. */
    }
    return 1;
}

/* One started context of `group` has finished; the last one wakes the
 * group's waiter, wherever it runs. */
static void wf_group_finish(uint64_t *group) {
    uint64_t count = __atomic_load_n(&group[0], __ATOMIC_ACQUIRE);
    uint64_t waiter;
    for (;;) {
        if (count == 0u || count == WF_GROUP_CLOSING) {
            wf_bridge_fail("a context finished in a group that counted none");
        }
        if (count > 1u) {
            if (__atomic_compare_exchange_n(
                    &group[0], &count, count - 1u, 0, __ATOMIC_SEQ_CST, __ATOMIC_ACQUIRE)) {
                return;
            }
            continue;
        }
        if (__atomic_compare_exchange_n(
                &group[0], &count, WF_GROUP_CLOSING, 0, __ATOMIC_SEQ_CST, __ATOMIC_ACQUIRE)) {
            break;
        }
    }
    waiter = __atomic_exchange_n(&group[1], 0u, __ATOMIC_SEQ_CST);
    __atomic_store_n(&group[0], 0u, __ATOMIC_SEQ_CST);
    if (waiter != 0u) {
        wf_context_ready((wf_context *)(uintptr_t)waiter);
    }
}

/* A started context's outermost frame has finished: release it and the
 * context, and wake the starting activation when it was the group's last. */
static void wf_context_finish(wf_context *context) {
    uint64_t *group = context->group;
    wf_context *previous = wf_context_current;
    wf_context_current = context;
    wf__coro_destroy(context->root);
    wf_context_current = previous;
    wf_context_release_arena(context);
    /* Counted out before the group, so a starter the group wakes never sees
     * this context live. */
    atomic_fetch_sub_explicit(&wf_context_live, 1u, memory_order_release);
    wf_group_finish(group);
    wf_pool_give(context, context->pool_bytes);
}

/* ------------------------------------------------------ shared objects */

/* A shared object [SHARE-1]: this header, then its state at
 * WF_SHARED_STATE_OFFSET.  `holders` counts the atomic statements holding
 * it: zero when it is free, the number of readers, or WF_SHARED_WRITER.
 * The entries take a read or a write request, but lowering makes only write
 * requests today, so the read path runs in no program (docs/todo.md).
 * A statement that finds the object held spins for a bounded time, since a
 * holder's block cannot wait and so its holder is running; one that still
 * finds it held parks in `waiting`.  An unlock wakes the first parked
 * statement to try again rather than handing it the object, so the object is
 * rarely held by a context no driver is running, which would make every
 * later statement park behind it; a woken statement that misses again goes
 * back to the head of the queue, and once WF_SHARED_HANDOFF unlocks have
 * woken it in vain the next one hands it the object, so a parked statement
 * is overtaken a bounded number of times [SHARE-3].  Contexts whose guard
 * read false wait in `watching` until a statement that writes the object
 * ends. */
typedef struct wf_shared {
    _Atomic uint64_t handles;
    atomic_flag lock;
    _Atomic uint64_t holders;
    wf_context *waiting_head;
    wf_context *waiting_tail;
    wf_context *watching;
    size_t pool_bytes;
} wf_shared;
_Static_assert(
    sizeof(wf_shared) <= WF_SHARED_STATE_OFFSET,
    "a shared object's header must end before its state"
);
#define WF_SHARED_WRITER UINT64_MAX
/* How many times a statement that finds its object held looks again before
 * it parks: long enough for a block's compute, short against a park and a
 * wake. */
#define WF_SHARED_SPINS 256u
/* How many unlocks wake a parked statement to try again before the next one
 * hands it the object: one retry lets the common case keep the object with a
 * running context, and the bound keeps a statement from being overtaken
 * without end. */
#define WF_SHARED_HANDOFF 2u

void *wf__shared_new(uint64_t state_bytes) {
    size_t granted;
    wf_shared *shared;
    if (state_bytes > SIZE_MAX - WF_SHARED_STATE_OFFSET) {
        wf_context_exhausted();
    }
    shared = (wf_shared *)wf_pool_take(
        WF_SHARED_STATE_OFFSET + (size_t)state_bytes,
        &granted
    );
    memset(shared, 0, sizeof(*shared));
    atomic_store_explicit(&shared->handles, 1u, memory_order_relaxed);
    atomic_store_explicit(&shared->holders, 0u, memory_order_relaxed);
    atomic_flag_clear(&shared->lock);
    shared->pool_bytes = granted;
    return shared;
}

void wf__shared_share(void *object) {
    wf_shared *shared = (wf_shared *)object;
    atomic_fetch_add_explicit(&shared->handles, 1u, memory_order_relaxed);
}

int wf__shared_release(void *object) {
    wf_shared *shared = (wf_shared *)object;
    return atomic_fetch_sub_explicit(&shared->handles, 1u, memory_order_acq_rel) == 1u;
}

void wf__shared_free(void *object) {
    wf_shared *shared = (wf_shared *)object;
    wf_pool_give(shared, shared->pool_bytes);
}

/* Whether a statement asking to write, or to read, may hold the object now.
 * Called under the object's lock or, as a hint, without it. */
static int wf_shared_admits(wf_shared *shared, uint32_t write) {
    uint64_t holders = atomic_load_explicit(&shared->holders, memory_order_relaxed);
    return write != 0u ? holders == 0u : holders != WF_SHARED_WRITER;
}

/* Takes a hold for a statement asking to write, or to read, under the
 * object's lock once wf_shared_admits has admitted it. */
static void wf_shared_hold_locked(wf_shared *shared, uint32_t write) {
    uint64_t holders = atomic_load_explicit(&shared->holders, memory_order_relaxed);
    atomic_store_explicit(
        &shared->holders,
        write != 0u ? WF_SHARED_WRITER : holders + 1u,
        memory_order_relaxed
    );
}

/* Parks the running context in the object's queue: at its head when an
 * unlock woke it and it missed again, at its tail otherwise.  Called under
 * the object's lock. */
static void wf_shared_park_locked(wf_shared *shared, wf_context *self, uint32_t write) {
    self->next = NULL;
    self->shared_write = write;
    if (self->shared_woken != 0u && shared->waiting_head != NULL) {
        self->next = shared->waiting_head;
        shared->waiting_head = self;
    } else if (shared->waiting_tail != NULL) {
        shared->waiting_tail->next = self;
        shared->waiting_tail = self;
    } else {
        shared->waiting_head = self;
        shared->waiting_tail = self;
    }
}

/* Wakes the first parked statement once the object is free: to try again,
 * or, once WF_SHARED_HANDOFF unlocks have woken it in vain, holding the
 * object.  Called under the object's lock; returns the context to make ready
 * once the lock is released. */
static wf_context *wf_shared_wake_locked(wf_shared *shared) {
    wf_context *first = shared->waiting_head;
    if (first == NULL) {
        return NULL;
    }
    shared->waiting_head = first->next;
    if (shared->waiting_head == NULL) {
        shared->waiting_tail = NULL;
    }
    first->next = NULL;
    if (first->shared_woken >= WF_SHARED_HANDOFF) {
        wf_shared_hold_locked(shared, first->shared_write);
        first->shared_granted = 1u;
    } else {
        first->shared_woken += 1u;
    }
    return first;
}

static void wf_shared_ready_all(wf_context *list) {
    while (list != NULL) {
        wf_context *next = list->next;
        wf_context_ready(list);
        list = next;
    }
}

/* Answers 0 when the running context now holds the object, and 1 when it
 * has parked the frame, which then suspends; the emitted code calls this
 * again when the frame resumes, and that call answers 0 at once when an
 * unlock handed the parked context the object. */
int wf__shared_acquire(void *object, uint32_t write, void *frame) {
    wf_shared *shared = (wf_shared *)object;
    wf_context *self = wf_context_current;
    unsigned spins = 0u;
    if (self == NULL || frame == NULL) {
        wf_bridge_fail("an atomic statement ran outside every context");
    }
    if (self->shared_granted != 0u) {
        self->shared_granted = 0u;
        self->shared_woken = 0u;
        return 0;
    }
    /* Before trying the object, never while holding it: a statement's block
     * does not wait, and a yield is a wait. */
    if (wf_context_pass(self, frame)) {
        return 1;
    }
    for (;;) {
        if (wf_shared_admits(shared, write)) {
            wf_spin_lock(&shared->lock);
            if (wf_shared_admits(shared, write)) {
                wf_shared_hold_locked(shared, write);
                self->shared_woken = 0u;
                wf_spin_unlock(&shared->lock);
                return 0;
            }
            wf_spin_unlock(&shared->lock);
        }
        if (spins >= WF_SHARED_SPINS) {
            break;
        }
        spins += 1u;
        wf_prim_spin_hint();
    }
    wf_spin_lock(&shared->lock);
    if (wf_shared_admits(shared, write)) {
        wf_shared_hold_locked(shared, write);
        self->shared_woken = 0u;
        wf_spin_unlock(&shared->lock);
        return 0;
    }
    self->resume = frame;
    wf_context_parked_away = 1;
    wf_shared_park_locked(shared, self, write);
    wf_spin_unlock(&shared->lock);
    return 1;
}

/* Ends one hold under the object's lock and returns the contexts to make
 * ready: the first parked statement once the object is free, woken or handed
 * the object, and, after a write, every context watching for one. */
static wf_context *wf_shared_end_hold_locked(wf_shared *shared, uint32_t write, int wrote) {
    wf_context *ready = NULL;
    wf_context *woken = NULL;
    if (write != 0u) {
        atomic_store_explicit(&shared->holders, 0u, memory_order_relaxed);
    } else {
        atomic_fetch_sub_explicit(&shared->holders, 1u, memory_order_relaxed);
    }
    if (wrote) {
        ready = shared->watching;
        shared->watching = NULL;
    }
    if (atomic_load_explicit(&shared->holders, memory_order_relaxed) == 0u) {
        woken = wf_shared_wake_locked(shared);
    }
    if (woken != NULL) {
        woken->next = ready;
        ready = woken;
    }
    return ready;
}

void wf__shared_unlock(void *object, uint32_t write) {
    wf_shared *shared = (wf_shared *)object;
    wf_context *ready;
    wf_spin_lock(&shared->lock);
    ready = wf_shared_end_hold_locked(shared, write, write != 0u);
    wf_spin_unlock(&shared->lock);
    wf_shared_ready_all(ready);
}

int wf__shared_watch(void *object, uint32_t write, void *frame) {
    wf_shared *shared = (wf_shared *)object;
    wf_context *self = wf_context_current;
    wf_context *ready;
    if (self == NULL || frame == NULL) {
        wf_bridge_fail("an atomic statement ran outside every context");
    }
    wf_spin_lock(&shared->lock);
    self->resume = frame;
    self->shared_woken = 0u;
    self->next = shared->watching;
    shared->watching = self;
    wf_context_parked_away = 1;
    /* The guard wrote nothing, so no watcher has a change to see. */
    ready = wf_shared_end_hold_locked(shared, write, 0);
    wf_spin_unlock(&shared->lock);
    wf_shared_ready_all(ready);
    return 1;
}

/* [WAIT-2] whether the program can take no further step: every driver has
 * announced that it is idle with nothing ready and no context waiting for a
 * host outcome, so every context that has not finished waits for a false
 * guard or for another context. Called by a driver that has just announced
 * its own idleness. A driver that makes a context ready is running, not idle,
 * and a helper's completion ends a counted host wait only when a running
 * driver harvests it, so a positive answer is final. Each driver's counts are
 * its own thread's, published by its seq_cst announcement of idleness. Every slot of the driver
 * table is read, not only the counted ones, because a driver that has just
 * started may take contexts before the count includes it. */
static int wf_contexts_stuck(const wf_driver *self) {
    unsigned index;
    for (index = 0; index < WF_DRIVER_LIMIT; index++) {
        wf_driver *other = wf_drivers[index];
        if (other == NULL) {
            continue;
        }
        if (atomic_load_explicit(&other->run_count, memory_order_seq_cst) != 0u
            || atomic_load_explicit(&other->host_waits, memory_order_seq_cst) != 0u) {
            return 0;
        }
        if (other != self
            && atomic_load_explicit(&other->idle, memory_order_seq_cst) == 0u) {
            return 0;
        }
    }
    return 1;
}

/* How many contexts a driver resumes before it looks for host completions
 * although contexts are still ready. */
#define WF_DRIVER_REAP_RUNS 64u

/* [WAIT-2] a context whose host outcome has arrived proceeds even while the
 * contexts ready here keep making one another ready, as two that hand a guard
 * back and forth do: the driver otherwise reaps its ring, publishes helper
 * completions and polls readiness only when it has nothing ready. */
static void wf_driver_reap(wf_driver *driver) {
    driver->runs_since_reap = 0u;
    (void)wf_bridge_progress();
    (void)wf_context_harvest(driver);
    if (driver->polling != NULL) {
        (void)wf_context_poll(driver, 0);
    }
}

/* Runs this driver's contexts: resumes the next ready context, and with none
 * ready, or after WF_DRIVER_REAP_RUNS resumptions, reaps completions and
 * polls for readiness; with none ready it then parks the thread until a
 * context is ready.  Driver 0 returns when the root's outermost frame has
 * finished, and every other driver when the program stops them. */
static void wf_context_drive(wf_driver *driver) {
    for (;;) {
        wf_context *next = wf_run_take(driver);
        if (next != NULL) {
            void *frame = next->resume;
            driver->runs_since_reap += 1u;
            if (driver->runs_since_reap >= WF_DRIVER_REAP_RUNS) {
                wf_driver_reap(driver);
            }
            wf_driver_end_search(driver, 1);
            if (atomic_load_explicit(&driver->run_count, memory_order_relaxed) != 0u) {
                wf_drivers_wake_one();
            }
            next->resume = NULL;
            next->driver = driver;
            next->passes = 0u;
            wf_context_current = next;
            wf_context_parked_away = 0;
            wf__coro_resume(frame);
            wf_context_current = NULL;
            if (wf_context_parked_away) {
                continue;
            }
            if (wf__coro_done(next->root)) {
                if (next == &wf_context_root) {
                    return;
                }
                wf_context_finish(next);
            }
            continue;
        }
        if (driver != &wf_driver_root
            && atomic_load_explicit(&wf_drivers_stopping, memory_order_acquire) != 0u) {
            return;
        }
        driver->runs_since_reap = 0u;
        if (wf_context_harvest(driver) || wf_bridge_progress()) {
            continue;
        }
        {
            int stole = wf_driver_steal(driver);
            wf_driver_end_search(driver, stole);
            if (stole) {
                continue;
            }
        }
        {
            uint64_t epoch = wf_completion_wake_epoch(driver->runtime);
            if (wf_context_harvest(driver)
                || atomic_load_explicit(&driver->run_count, memory_order_relaxed) != 0u) {
                continue;
            }
            if (driver != &wf_driver_root
                && atomic_load_explicit(&wf_drivers_stopping, memory_order_acquire) != 0u) {
                return;
            }
            if (driver->polling != NULL) {
                /* A record another thread completes is seen within a
                 * millisecond; with none pending the poll waits for a peer. */
                (void)wf_context_poll(driver, driver->parked != NULL ? 1 : -1);
                continue;
            }
            /* Announced before the last look, so a driver that makes a
             * context ready after that look sees this one parked and wakes
             * it. */
            atomic_store_explicit(&driver->idle, 1u, memory_order_seq_cst);
            atomic_fetch_add_explicit(&wf_drivers_idle, 1u, memory_order_seq_cst);
            if (wf_contexts_stuck(driver)) {
                wf_bridge_fail(
                    "every context waits for a guard or for another context, and no host operation is outstanding"
                );
            }
            if (atomic_load_explicit(&driver->run_count, memory_order_seq_cst) == 0u
                && !wf_driver_steal(driver)) {
                wf_bridge_park(epoch);
            }
            if (atomic_exchange_explicit(&driver->idle, 0u, memory_order_seq_cst) != 0u) {
                atomic_fetch_sub_explicit(&wf_drivers_idle, 1u, memory_order_seq_cst);
            }
        }
    }
}

#if defined(__linux__)

/* The floor's stack reservation, which a driver thread takes like the
 * entry's thread; a probe that links the bridge without the floor starts no
 * driver and links this default of the host's own size. */
__attribute__((weak)) size_t wf__floor_stack_bytes(void) {
    return 0u;
}

/* A driver thread other than the entry's: it runs on the floor's stack
 * reservation, attached to the floor like a compute worker, and drives the
 * contexts placed on it with its own ring until the program stops it. */
static void wf_driver_main(void *argument) {
    wf_driver *driver = (wf_driver *)argument;
    wf_prim_floor_attach();
    wf_driver_self = driver;
    wf_bridge_thread_adapter = &driver->own_adapter;
    wf_context_drive(driver);
    wf_bridge_thread_adapter = NULL;
    wf_driver_self = NULL;
    atomic_store_explicit(&driver->exited, 1u, memory_order_release);
}

/* The drivers after the entry's, started the first time a context starts:
 * WF_DRIVERS of them in all, one per CPU the process may run on when it is
 * not written, and only where the host has a ring for each to wait in. A
 * driver that cannot be made leaves the program with the ones already
 * running. */
static void wf_drivers_begin(void) {
    unsigned long written = 0;
    unsigned wanted = wf_prim_online_cpus();
    unsigned index;
    if (wf__sched_setting("WF_DRIVERS", WF_DRIVER_LIMIT, &written) && written != 0u) {
        wanted = (unsigned)written;
    }
    if (wanted == 0u || !wf_bridge_ring_ready()) {
        wanted = 1u;
    }
    if (wanted > WF_DRIVER_LIMIT) {
        wanted = WF_DRIVER_LIMIT;
    }
    for (index = 1u; index < wanted; index++) {
        size_t granted;
        wf_driver *driver = (wf_driver *)wf_pool_take(sizeof(*driver), &granted);
        memset(driver, 0, sizeof(*driver));
        driver->pool_bytes = granted;
        atomic_flag_clear(&driver->run_lock);
        if (wf_completion_runtime_init(&driver->own_runtime) != 0) {
            wf_pool_give(driver, granted);
            break;
        }
        driver->runtime = &driver->own_runtime;
        if (wf_linux_io_uring_init(
                &driver->own_adapter,
                &driver->own_runtime,
                WF_LINUX_IO_URING_DEPTH,
                WF_LINUX_IO_URING_COMPLETIONS
            ) != 0) {
            (void)wf_completion_runtime_destroy(&driver->own_runtime);
            wf_pool_give(driver, granted);
            break;
        }
        if (wf_completion_set_wake_callback(
                &driver->own_runtime,
                wf_linux_io_uring_notify,
                &driver->own_adapter
            ) != 0) {
            (void)wf_linux_io_uring_destroy(&driver->own_adapter);
            (void)wf_completion_runtime_destroy(&driver->own_runtime);
            wf_pool_give(driver, granted);
            break;
        }
        wf_drivers[index] = driver;
        if (wf_prim_thread_start(
                &driver->thread,
                wf_driver_main,
                driver,
                wf__floor_stack_bytes()
            ) != 0) {
            wf_drivers[index] = NULL;
            (void)wf_linux_io_uring_destroy(&driver->own_adapter);
            (void)wf_completion_runtime_destroy(&driver->own_runtime);
            wf_pool_give(driver, granted);
            break;
        }
        atomic_store_explicit(&wf_driver_count, index + 1u, memory_order_release);
    }
}

/* Stops every driver but the entry's once the root has finished, when no
 * context is left to run, and releases their rings only after every one has
 * stopped, since a running driver looks through the others for work. */
static void wf_drivers_end(void) {
    unsigned count = atomic_load_explicit(&wf_driver_count, memory_order_acquire);
    unsigned index;
    atomic_store_explicit(&wf_drivers_stopping, 1u, memory_order_release);
    for (index = 1u; index < count; index++) {
        wf_driver *driver = wf_drivers[index];
        while (atomic_load_explicit(&driver->exited, memory_order_acquire) == 0u) {
            wf_completion_notify_target(driver->runtime);
            wf_prim_yield();
        }
    }
    atomic_store_explicit(&wf_driver_count, 1u, memory_order_seq_cst);
    /* A helper thread may still be notifying the drivers after publishing
     * the last record a finished context waited on; it read the count
     * before the store above, so its notifications end before these
     * drivers' wakes are released. */
    while (atomic_load_explicit(&wf_drivers_notifying, memory_order_seq_cst) != 0u) {
        wf_prim_yield();
    }
    for (index = 1u; index < count; index++) {
        wf_driver *driver = wf_drivers[index];
        (void)wf_linux_io_uring_destroy(&driver->own_adapter);
        (void)wf_completion_runtime_destroy(&driver->own_runtime);
        wf_drivers[index] = NULL;
        wf_pool_give(driver, driver->pool_bytes);
    }
}

#else

/* A host with no ring runs one driver. */
static void wf_drivers_begin(void) {}
static void wf_drivers_end(void) {}

#endif

void wf__context_root_begin(void) {
    if (wf_driver_self != NULL) {
        wf_bridge_fail("the root context was begun twice");
    }
    wf_bridge_require();
    atomic_flag_clear(&wf_driver_root.run_lock);
    wf_driver_root.runtime = &wf_bridge_runtime;
    wf_drivers[0] = &wf_driver_root;
    atomic_store_explicit(&wf_driver_count, 1u, memory_order_release);
    wf_driver_self = &wf_driver_root;
    wf_context_root.driver = &wf_driver_root;
    wf_context_current = &wf_context_root;
}

void wf__context_root_run(void *frame) {
    if (wf_driver_self != &wf_driver_root || wf_context_current != &wf_context_root
        || frame == NULL) {
        wf_bridge_fail("the root context was run without being begun");
    }
    wf_context_root.root = frame;
    wf_context_root.resume = frame;
    wf_context_ready(&wf_context_root);
    wf_context_drive(&wf_driver_root);
    if (atomic_load_explicit(&wf_context_live, memory_order_acquire) != 0u) {
        wf_bridge_fail("the entry finished while contexts it started were running");
    }
    wf_drivers_end();
    wf_context_current = &wf_context_root;
    wf__coro_destroy(frame);
    wf_context_release_arena(&wf_context_root);
    wf_context_current = NULL;
}

/* ------------------------------------------------------------- the join */

/* How long a joining thread looks at its own record before announcing sleep.
 *
 * The clock is `wf_file_monotonic_ns`, the adapter's platform leaf, because a
 * monotonic clock is a host call: zero from it means the clock could not be
 * read, which is the same answer as a broken one for every use here, and both
 * have to end a bounded wait rather than extend it.
 *
 * Announcing sleep and being woken is two system calls, paid by the waiter and
 * by whichever thread publishes.  A helper pool only exists when the adapter
 * measured operations that wait, and those waits end while this thread has
 * nothing else to do, so a bounded look before sleeping trades a little idle
 * CPU for both of those calls.  It is a bound on wasted CPU: a wait longer
 * than this window still ends in a sleep, and one shorter than it never
 * becomes a pair of system calls.
 *
 * It watches the one record this thread is waiting on, where it used to watch
 * a process-wide count of ready events that no longer exists (design §7: a
 * completion is published straight into its record). */
#define WF_BRIDGE_JOIN_SPIN_NS 10000u

static unsigned wf_bridge_record_state(const wf_completion_record *record) {
    return atomic_load_explicit(&record->state, memory_order_acquire);
}

/* Waits a bounded time for this record to be completed by someone else.
 * Returns 1 when it was, so the caller re-reads it instead of parking. */
static int wf_bridge_spin_for_completion(const wf_completion_record *record) {
    uint64_t started = wf_file_monotonic_ns();
    uint64_t deadline;
    unsigned turn = 0;
    /* A clock this thread cannot read is bounded by the `now == 0` term of
     * the periodic sample below, not by this early return.  That sample
     * treats a zero reading exactly as it treats a passed deadline, so a
     * failed clock ends the spin within 64 turns whatever happens here. */
    if (started == 0) {
        return wf_bridge_record_state(record) != WF_COMPLETION_PENDING;
    }
    deadline = started + WF_BRIDGE_JOIN_SPIN_NS;
    for (;;) {
        if (wf_bridge_record_state(record) != WF_COMPLETION_PENDING) {
            return 1;
        }
        turn += 1;
        if ((turn & 0x3fu) == 0) {
            uint64_t now = wf_file_monotonic_ns();
            if (now == 0 || now >= deadline) {
                return 0;
            }
        }
    }
}

/* Join may execute its own queued request, but never another frame's
 * potentially blocking request while helpers exist. Flush deferred native
 * submissions before entering a blocking host call. */
static int wf_bridge_run_own(wf_completion_record *record) {
    if (wf_bridge_file_ready == 0
        || !wf_file_adapter_claim_own(&wf_bridge_adapter, record)) {
        return 0;
    }
    /* The host call below is outside every path the ring can see through. */
    wf_bridge_flush_target();
    wf_file_adapter_run_claimed(&wf_bridge_adapter, record);
    return 1;
}

/* Waits, blocking this thread, until descriptor is ready for events. */
static void wf_bridge_wait_ready(int descriptor, unsigned events) {
    wf_file_readiness readiness;
    readiness.descriptor = descriptor;
    readiness.events = events;
    readiness.ready = 0;
    if (wf_file_wait_readiness(&readiness, 1, -1) < 0) {
        wf_bridge_fail("a join could not wait for a descriptor's readiness");
    }
}

/* Makes a readiness-routed operation: a transfer is retried without
 * waiting until the host takes it, an accept is made once its listener is
 * readable.  A frame's wait does this without blocking the thread
 * (`wf__context_wait`); a blocking join, which no context makes, waits here. */
static void wf_bridge_join_readiness(wf_completion_record *record) {
    const wf_file_request *request = &record->request;
    for (;;) {
        switch (request->kind) {
            case WF_FILE_SOCKET_RECEIVE:
                if (wf_bridge_transfer_now(record)) return;
                wf_bridge_wait_ready(request->operation.receive.descriptor, WF_FILE_READABLE);
                break;
            case WF_FILE_SOCKET_SEND:
                if (wf_bridge_transfer_now(record)) return;
                wf_bridge_wait_ready(request->operation.send.descriptor, WF_FILE_WRITABLE);
                break;
            case WF_FILE_SOCKET_ACCEPT:
                wf_bridge_wait_ready(request->operation.accept.descriptor, WF_FILE_READABLE);
                wf_bridge_execute_here(record);
                return;
            default:
                wf_bridge_execute_here(record);
                return;
        }
    }
}

/* Waits in place for a record: the join the completion probes and the
 * harness make.  A waiting function never calls it; its frame suspends
 * instead (`wf__context_wait`). */
static void wf_bridge_join(wf_completion_record *record) {
    /* A finish reads a record its frame's wait already saw complete, which
     * for a readiness-routed transfer must not be made a second time. */
    if (wf_bridge_record_state(record) == WF_COMPLETION_DONE) return;
    if (record->route == WF_COMPLETION_ROUTE_READINESS) {
        wf_bridge_join_readiness(record);
    }
    for (;;) {
        if (wf_bridge_record_state(record) == WF_COMPLETION_DONE) return;
        if (wf_bridge_run_own(record) || wf_bridge_progress()) continue;
        /* Capture before checking DONE: publication either precedes this
         * epoch (and its acquire orders the result), or advances the epoch
         * and prevents sleep. The wait implementation registers/rechecks
         * under its native lock, closing the notification-before-park race. */
        uint64_t epoch = wf_completion_wake_epoch(&wf_bridge_runtime);
        if (wf_bridge_record_state(record) != WF_COMPLETION_DONE
            && !wf_bridge_spin_for_completion(record)) {
            wf_bridge_park(epoch);
        }
    }
}

int wf__completion_pending(const void *record) {
    if (record == NULL) {
        wf_bridge_fail("a pending test was given no record");
    }
    return wf_bridge_record_state((const wf_completion_record *)record)
        != WF_COMPLETION_DONE;
}

static wf_completion_record *wf_bridge_record_of(const void *record) {
    if (record == NULL) {
        wf_bridge_fail(
            "a join was given no record"
        );
    }
    return (wf_completion_record *)(uintptr_t)record;
}

void wf__completion_file_join(
    const void *record,
    int64_t *value,
    int *error_code
) {
    wf_completion_record *held = wf_bridge_record_of(record);
    if (value == NULL || error_code == NULL) {
        wf_bridge_fail(
            "a join was given no place to publish its result"
        );
    }
    wf_bridge_join(held);
    *value = held->result.value;
    *error_code = held->result.error_code;
}

void wf__completion_file_open_join(
    const void *record,
    int64_t *value,
    int *error_code,
    unsigned *open_outcome
) {
    wf_completion_record *held = wf_bridge_record_of(record);
    if (value == NULL || error_code == NULL || open_outcome == NULL) {
        wf_bridge_fail(
            "an open join was given no place to publish its result"
        );
    }
    wf_bridge_join(held);
    if (held->result.kind != WF_FILE_OPEN_AT) {
        wf_bridge_fail(
            "an open join was given a record that is not an open"
        );
    }
    *value = held->result.value;
    *error_code = held->result.error_code;
    *open_outcome = (unsigned)held->result.open_outcome;
}

/* The accept's join returns the peer address as three scalars. The ordinary
 * linked implementation builds its SocketAddress value from them, so neither
 * side holds a pointer into the other's layout.
 *
 * A refused accept publishes the all-zero address, which the linked caller
 * ignores: a failed accept carries an error and no peer address. */
void wf__completion_socket_accept_join(
    const void *record,
    int64_t *value,
    int *error_code,
    uint64_t *peer_low,
    uint64_t *peer_high,
    uint32_t *peer_tag
) {
    wf_completion_record *held = wf_bridge_record_of(record);
    if (value == NULL || error_code == NULL || peer_low == NULL
        || peer_high == NULL || peer_tag == NULL) {
        wf_bridge_fail(
            "an accept join was given no place to publish its result"
        );
    }
    wf_bridge_join(held);
    if (held->result.kind != WF_FILE_SOCKET_ACCEPT) {
        wf_bridge_fail(
            "an accept join was given a record that is not an accept"
        );
    }
    *value = held->result.value;
    *error_code = held->result.error_code;
    *peer_low = held->request.operation.accept.peer.portable.words[0];
    *peer_high = held->request.operation.accept.peer.portable.words[1];
    *peer_tag = held->request.operation.accept.peer.portable.port_and_family;
}

/* ----------------------------------------------------------- the submits */

/* Whether the operation has no external action at all, because its transfer
 * range is empty.
 *
 * The emitted code no longer holds this back: with one lowering per operation
 * an empty range is submitted like any other and the runtime completes it, so
 * every kind that carries a byte count answers here -- directory enumeration
 * included, whose host facility refuses a zero-sized batch rather than
 * reporting an empty one (design section 8, "One lowering for every I/O
 * operation"). */
static int wf_bridge_file_request_is_empty(const wf_file_request *request) {
    switch (request->kind) {
        case WF_FILE_READ:
            return request->operation.read.count == 0;
        case WF_FILE_WRITE:
            return request->operation.write.count == 0;
        case WF_FILE_PREAD:
            return request->operation.pread.count == 0;
        case WF_FILE_SOCKET_RECEIVE:
            return request->operation.receive.count == 0;
        case WF_FILE_SOCKET_SEND:
            return request->operation.send.count == 0;
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
        case WF_FILE_DIRECTORY_NEXT:
            return request->operation.directory_next.count == 0;
#endif
        default:
            return 0;
    }
}

/* Executes the record's operation here and publishes it.
 *
 * This is where an operation with no kernel completion form ends up, and it
 * is the runtime's engine running the operation rather than a path an emitted
 * program can take (design §7.1, primitive 7).  The blocking host call is the
 * one the deleted direct family used to make on the caller's behalf; what
 * changed is that the record is published at the end, so every submit path
 * ends in a published record (design §7).  A refusal the host gives it,
 * including an open that found no descriptor, is the outcome the program
 * sees. Factory quota is ordinary library state outside this private engine;
 * native descriptor refusal is reported unchanged to that library. */
static void wf_bridge_execute_here(wf_completion_record *record) {
    wf_file_result result;
    record->route = WF_COMPLETION_ROUTE_INLINE;
    atomic_fetch_add_explicit(
        &wf_bridge_inline_executions,
        1,
        memory_order_relaxed
    );
    /* The host call below is outside every path the ring can see through. */
    wf_bridge_flush_target();
    result = wf_file_execute_timed(&wf_bridge_adapter, &record->request);
    wf_file_complete_record(record, &result);
}

/* An empty transfer has no external action at all, so there is nothing to
 * execute and nothing to overlap: the record is completed here. */
static void wf_bridge_complete_empty(wf_completion_record *record) {
    wf_file_result result;
    memset(&result, 0, sizeof(result));
    result.head.kind = record->request.kind;
    result.head.value = 0;
    record->route = WF_COMPLETION_ROUTE_INLINE;
    atomic_fetch_add_explicit(
        &wf_bridge_inline_executions,
        1,
        memory_order_relaxed
    );
    wf_file_complete_record(record, &result);
}

/* Queues the record on the bounded POSIX adapter, or executes it here when
 * that adapter cannot be built.  Either way the record is the runtime's. */
static void wf_bridge_submit_file(wf_completion_record *record) {
    if (!wf_bridge_ensure_file()) {
        wf_bridge_execute_here(record);
        return;
    }
    if (wf_file_adapter_submit(&wf_bridge_adapter, record)
        != WF_FILE_TARGET_OWNS) {
        /* The shape was checked before the record was filled, so the only
         * answer left is an adapter that has stopped admitting -- the process
         * is exiting -- and the operation is executed here instead of being
         * left unpublished. */
        wf_bridge_execute_here(record);
        return;
    }
    wf_bridge_notify_target();
}

/* Whether a positioned transfer is better made where it was stated than
 * queued.
 *
 * The completion path exists so that a program is not stalled by a wait it
 * could have overlapped.  When the bounded adapter holds no helper, has
 * nothing queued, and has measured its own operations as not waiting, there is
 * no wait to overlap and no other thread to overlap it on: the queued
 * operation would be executed by this very thread, at its join, after a queue
 * crossing.  Executing it here is the same host call without any of that, and
 * the record is published at the end either way, so this is a throughput
 * choice and never an outcome.
 *
 * Only a *positioned* transfer takes it, and that is the whole liveness
 * argument.  An offset is meaningful only on a seekable object, and the typed
 * opens that produce one admit nothing but a regular file, so a positioned
 * read waits on storage.  A non-positioned read or write may be waiting on
 * something another part of the same program has to do — a pipe the program
 * itself must drain — and running one where it was stated could stall the very
 * thread that would unblock it.  Those keep the queue.
 *
 * A written WF_IO_HELPERS takes nothing inline: it pins the route with the
 * count.
 *
 * The measurement keeps running while this is true, because every inline
 * execution is timed by the same adapter, so a program whose reads start
 * waiting is queueing again within a few operations. */
static int wf_bridge_positioned_read_runs_on_caller(uint64_t count) {
    return count != 0
        && wf_bridge_helpers_pinned == 0
        && wf_bridge_file_ready != 0
        && wf_file_adapter_transfer_runs_on_caller(&wf_bridge_adapter);
}

/* Fills the record's scheduler words and clears the engine state every route
 * reads.  Called once, by submit, before any engine can see the record. */
static wf_completion_record *wf_bridge_begin(void *record) {
    wf_completion_record *held;
    if (record == NULL) {
        wf_bridge_fail(
            "a submit was given no record"
        );
    }
    wf_bridge_require();
    held = (wf_completion_record *)record;
    memset(held, 0, sizeof(*held));
    wf_completion_record_init(held);
    held->route = WF_COMPLETION_ROUTE_NONE;
    held->opened_descriptor = -1;
    held->open_outcome = WF_FILE_OPEN_SUCCEEDED;
    return held;
}

/* Hands the record to whichever engine can take it, in the one order every
 * submit uses: the ring where it has a form for this kind, then the bounded
 * adapter, and the engine here when neither applies.  Every path ends in a
 * record the runtime owns, so there is nothing to answer. */
/* Completes a socket transfer here when the host answers it without waiting:
 * the operation's outcome is the host's own, and nothing parks, wakes, or
 * crosses a ring for an answer that was already there. */
static int wf_bridge_transfer_now(wf_completion_record *record) {
    wf_file_result result;
    if (!wf_file_transfer_now(&record->request, &result)) {
        return 0;
    }
    record->route = WF_COMPLETION_ROUTE_INLINE;
    atomic_fetch_add_explicit(
        &wf_bridge_inline_executions,
        1,
        memory_order_relaxed
    );
    wf_file_complete_record(record, &result);
    return 1;
}

/* Whether a socket operation this thread would otherwise block in has to
 * wait for its descriptor's readiness instead: other contexts share the
 * thread [WAIT-2], and no ring took the operation.  Its join waits for the
 * descriptor and then makes the operation, which cannot wait by then. */
static int wf_bridge_waits_for_readiness(const wf_completion_record *record) {
    if (atomic_load_explicit(&wf_context_live, memory_order_relaxed) == 0u
        || !wf_file_readiness_supported()) {
        return 0;
    }
    switch (record->request.kind) {
        case WF_FILE_SOCKET_RECEIVE:
        case WF_FILE_SOCKET_SEND:
        case WF_FILE_SOCKET_ACCEPT:
            return 1;
        default:
            return 0;
    }
}

static void wf_bridge_dispatch(wf_completion_record *record) {
    if (wf_bridge_file_request_is_empty(&record->request)) {
        wf_bridge_complete_empty(record);
        return;
    }
    if (wf_bridge_ring_offer(record)) {
        return;
    }
    if (wf_bridge_waits_for_readiness(record)) {
        record->route = WF_COMPLETION_ROUTE_READINESS;
        return;
    }
    wf_bridge_submit_file(record);
}

void wf__completion_file_read_submit(
    int descriptor,
    void *buffer,
    uint64_t count,
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    if ((buffer == NULL && count != 0) || (uint64_t)(size_t)count != count) {
        wf_bridge_fail(
            "a read was submitted with a buffer and a count that do not describe a range"
        );
    }
    held->request.kind = WF_FILE_READ;
    held->request.operation.read.descriptor = descriptor;
    held->request.operation.read.buffer = buffer;
    held->request.operation.read.count = (size_t)count;
    wf_bridge_dispatch(held);
}

/* Publishes a refusal the host itself would have made, without asking it.
 *
 * A failed outcome and a terminated process are different things, and only
 * one of them is what an offset the target ABI cannot express deserves: the
 * writer may spell any `u64` offset, and the host answers an offset above
 * `INT64_MAX` with EINVAL.  The record is completed with exactly that answer,
 * so the ordinary linked caller builds its failed outcome from that error.
 * No host request is executed, so the inline-execution count
 * is untouched; the publication count is not, because this is one record's
 * one terminal completion. */
static void wf_bridge_complete_refused(
    wf_completion_record *record,
    int error_code
) {
    record->route = WF_COMPLETION_ROUTE_INLINE;
    record->result.kind = record->request.kind;
    record->result.value = -1;
    record->result.error_code = error_code;
    wf_completion_record_complete(record);
}

void wf__completion_file_pread_submit(
    int descriptor,
    void *buffer,
    uint64_t count,
    uint64_t file_offset,
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    if ((buffer == NULL && count != 0) || (uint64_t)(size_t)count != count) {
        wf_bridge_fail(
            "a positioned read was submitted with a buffer and a count that do not describe a range"
        );
    }
    held->request.kind = WF_FILE_PREAD;
    held->request.operation.pread.descriptor = descriptor;
    held->request.operation.pread.buffer = buffer;
    held->request.operation.pread.count = (size_t)count;
    if (file_offset > (uint64_t)INT64_MAX) {
        wf_bridge_complete_refused(held, EINVAL);
        return;
    }
    held->request.operation.pread.offset = (int64_t)file_offset;
    if (wf_bridge_file_request_is_empty(&held->request)) {
        wf_bridge_complete_empty(held);
        return;
    }
    /* The ring first, then the throughput choice below, then the queue.  A
     * positioned read the ring takes is never a candidate for running inside
     * submit: the ring has no wait for this thread to overlap. */
    if (wf_bridge_ring_offer(held)) {
        return;
    }
    if (wf_bridge_positioned_read_runs_on_caller(count)) {
        wf_bridge_execute_here(held);
        return;
    }
    wf_bridge_submit_file(held);
}

void wf__completion_file_write_submit(
    int descriptor,
    const void *buffer,
    uint64_t count,
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    if ((buffer == NULL && count != 0) || (uint64_t)(size_t)count != count) {
        wf_bridge_fail(
            "a write was submitted with a buffer and a count that do not describe a range"
        );
    }
    /* write_once is an unpositioned OutputStream operation. The native adapter's
     * write request fixes an explicit offset, which would change regular
     * file-offset semantics and is not a meaningful stream offset. Until a
     * Linux request kind is qualified for exact write(2) current-position and
     * append/stream behavior, this stays on the bounded typed adapter, which
     * `wf_linux_io_uring_carries` answers by having no form for WF_FILE_WRITE. */
    held->request.kind = WF_FILE_WRITE;
    held->request.operation.write.descriptor = descriptor;
    held->request.operation.write.buffer = buffer;
    held->request.operation.write.count = (size_t)count;
    wf_bridge_dispatch(held);
}

/* The one place an ABI the emitter emits per target reaches this unit.
 *
 * A Windows open has to know which resource the descriptor will become before
 * it opens, because the access and the create options it asks the namespace
 * for differ by that answer; no other target's open does, and every other
 * target's leaf ignores the field the argument fills.  So the emitter emits
 * one more argument on that target alone (`emitter/completion.rs`,
 * COMPLETION_WINDOWS_RUNTIME_DECLARATIONS) and this signature follows it.
 * Nothing else in this unit is `#if`-forked, and this is a difference in an
 * ABI rather than a fork of any logic: both arms fill the same record and both
 * fall into the same routing below. */
void wf__completion_file_open_at_submit(
    int directory,
    const char *path,
    int flags,
    unsigned mode,
    unsigned has_mode,
    unsigned expected_kind,
#if defined(_WIN32)
    unsigned descriptor_class,
#endif
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    if (path == NULL || has_mode > 1u
        || expected_kind > WF_FILE_EXPECT_DIRECTORY) {
        wf_bridge_fail(
            "an open was submitted with no path, or with a mode or expected kind out of range"
        );
    }
    /* The name is the submitting frame's own and stays live until the join,
     * so nothing is copied and no length can refuse the completion path
     * (design §5). */
    held->request.kind = WF_FILE_OPEN_AT;
    held->request.operation.open_at.directory = directory;
    held->request.operation.open_at.path = path;
    held->request.operation.open_at.flags = flags;
    held->request.operation.open_at.mode = mode;
    held->request.operation.open_at.has_mode = has_mode;
    held->request.operation.open_at.expected_kind =
        (enum wf_file_expected_kind)expected_kind;
#if defined(_WIN32)
    held->request.operation.open_at.descriptor_class = descriptor_class;
#endif
    wf_bridge_dispatch(held);
}

void wf__completion_file_close_submit(
    int descriptor,
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    held->request.kind = WF_FILE_CLOSE;
    held->request.operation.close.descriptor = descriptor;
    wf_bridge_dispatch(held);
}

/* The six TCP submits.
 *
 * Each fills one arm of the request union and falls into the one routing
 * `wf_bridge_dispatch` performs for every kind: the ring where it has a form,
 * the bounded adapter otherwise, and the engine here when neither applies.
 * The addresses arrive as the three scalars an emitted `SocketAddress` value
 * is and are stored in the record in that form; whichever engine takes the
 * operation converts them into the host's own record, because the shape of
 * that record is the leaf's business and not this unit's. */
static void wf_bridge_socket_endpoint(
    wf_completion_record *held,
    uint64_t address_low,
    uint64_t address_high,
    uint32_t port_and_family
) {
    held->request.operation.endpoint.descriptor = -1;
    held->request.operation.endpoint.address_length = 0u;
    held->request.operation.endpoint.address.portable.words[0] = address_low;
    held->request.operation.endpoint.address.portable.words[1] = address_high;
    held->request.operation.endpoint.address.portable.port_and_family =
        port_and_family;
}

void wf__completion_socket_listen_submit(
    uint64_t address_low,
    uint64_t address_high,
    uint32_t port_and_family,
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    held->request.kind = WF_FILE_SOCKET_LISTEN;
    wf_bridge_socket_endpoint(held, address_low, address_high, port_and_family);
    wf_bridge_dispatch(held);
}

void wf__completion_socket_accept_submit(
    int listener,
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    if (listener < 0) {
        wf_bridge_fail(
            "an accept was submitted with no listener"
        );
    }
    held->request.kind = WF_FILE_SOCKET_ACCEPT;
    held->request.operation.accept.descriptor = listener;
    /* The whole of the storage the host may write, so a peer of either family
     * fits and the host reports back what it actually used. */
    held->request.operation.accept.peer_length =
        (unsigned)sizeof(held->request.operation.accept.peer.native);
    wf_bridge_dispatch(held);
}

void wf__completion_socket_connect_submit(
    uint64_t address_low,
    uint64_t address_high,
    uint32_t port_and_family,
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    held->request.kind = WF_FILE_SOCKET_CONNECT;
    wf_bridge_socket_endpoint(held, address_low, address_high, port_and_family);
    wf_bridge_dispatch(held);
}

void wf__completion_socket_receive_submit(
    int descriptor,
    void *buffer,
    uint64_t count,
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    if ((buffer == NULL && count != 0) || (uint64_t)(size_t)count != count) {
        wf_bridge_fail(
            "a receive was submitted with a buffer and a count that do not describe a range"
        );
    }
    held->request.kind = WF_FILE_SOCKET_RECEIVE;
    held->request.operation.receive.descriptor = descriptor;
    held->request.operation.receive.buffer = buffer;
    held->request.operation.receive.count = (size_t)count;
    /* With other contexts live and a ring to wait in, the receive goes to the
     * ring at once: the scheduler submits it with every other staged
     * operation in one entry, where a first attempt here costs a system call
     * of its own and, when the peer has not answered yet, gains nothing
     * (`WAITS.md`, Experiment 2). */
    if (!(atomic_load_explicit(&wf_context_live, memory_order_relaxed) != 0u
          && wf_bridge_ring_ready())
        && wf_bridge_transfer_now(held)) {
        return;
    }
    wf_bridge_dispatch(held);
}

void wf__completion_socket_send_submit(
    int descriptor,
    const void *buffer,
    uint64_t count,
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    if ((buffer == NULL && count != 0) || (uint64_t)(size_t)count != count) {
        wf_bridge_fail(
            "a send was submitted with a buffer and a count that do not describe a range"
        );
    }
    held->request.kind = WF_FILE_SOCKET_SEND;
    held->request.operation.send.descriptor = descriptor;
    held->request.operation.send.buffer = buffer;
    held->request.operation.send.count = (size_t)count;
    if (wf_bridge_transfer_now(held)) {
        return;
    }
    wf_bridge_dispatch(held);
}

void wf__completion_socket_shutdown_submit(
    int descriptor,
    unsigned direction,
    void *record
) {
    wf_completion_record *held = wf_bridge_begin(record);
    if (direction > (unsigned)WF_SOCKET_DIRECTION_SEND) {
        wf_bridge_fail(
            "a half-close was submitted with a direction this contract cannot mean"
        );
    }
    held->request.kind = WF_FILE_SOCKET_SHUTDOWN;
    held->request.operation.shutdown.descriptor = descriptor;
    held->request.operation.shutdown.direction =
        (enum wf_socket_direction)direction;
    wf_bridge_dispatch(held);
}

void wf__completion_directory_next_submit(
    int descriptor,
    void *buffer,
    uint64_t count,
    int64_t *position,
    void *record
) {
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
    wf_completion_record *held = wf_bridge_begin(record);
    if (position == NULL || (buffer == NULL && count != 0)
        || (uint64_t)(size_t)count != count) {
        wf_bridge_fail(
            "a directory read was submitted with no position, or with a buffer and a count that do not describe a range"
        );
    }
    held->request.kind = WF_FILE_DIRECTORY_NEXT;
    held->request.operation.directory_next.descriptor = descriptor;
    held->request.operation.directory_next.buffer = buffer;
    held->request.operation.directory_next.count = (size_t)count;
    held->request.operation.directory_next.position = position;
    wf_bridge_dispatch(held);
#else
    /* This private engine build has no enumeration request kind. A linked
     * library must not call an engine facility absent from its build. */
    (void)descriptor;
    (void)buffer;
    (void)count;
    (void)position;
    (void)record;
    wf_bridge_fail(
        "this target has no directory enumeration facility and no such request may reach this entry"
    );
#endif
}

/* ------------------------------------------------------- the statistics */

uint64_t wf__completion_wait_announcements(void) {
    return atomic_load_explicit(&wf_bridge_wake_ready, memory_order_acquire) == 0
        ? 0
        : atomic_load_explicit(&wf_bridge_runtime.stat_parks, memory_order_relaxed);
}

uint64_t wf__completion_wait_signals(void) {
    return atomic_load_explicit(&wf_bridge_wake_ready, memory_order_acquire) == 0
        ? 0
        : atomic_load_explicit(&wf_bridge_runtime.stat_wake_signals, memory_order_relaxed);
}

uint64_t wf__completion_file_submissions(void) {
    uint64_t submissions = wf_bridge_file_ready == 0
        ? 0
        : wf_file_adapter_statistics_snapshot(&wf_bridge_adapter).submissions;
    return submissions + wf_bridge_ring_submissions();
}

uint64_t wf__completion_file_fallback_submissions(void) {
    return wf_bridge_file_ready == 0
        ? 0
        : wf_file_adapter_statistics_snapshot(&wf_bridge_adapter).submissions;
}

/* Calls this process made to carry staged submissions to the kernel, where the
 * ring defers its doorbell.  With `io_uring`'s doorbell deferred this stays far
 * below the submission count, and the distance between the two is what
 * deferring bought; a ring that carries each request inside the call that
 * issues it, as the completion port does, answers zero. */
uint64_t wf__completion_native_ring_submission_enters(void) {
    return wf_bridge_ring_submission_enters();
}

uint64_t wf__completion_file_helper_executions(void) {
    return wf_bridge_file_ready == 0
        ? 0
        : wf_file_adapter_statistics_snapshot(&wf_bridge_adapter)
            .helper_executions;
}

uint64_t wf__completion_target_helper_count(void) {
    return (uint64_t)wf_file_adapter_helper_count(&wf_bridge_adapter);
}

uint64_t wf__completion_target_helper_executions(void) {
    return wf__completion_file_helper_executions();
}

uint64_t wf__completion_publications(void) {
    return atomic_load_explicit(&wf_bridge_publications, memory_order_relaxed);
}

uint64_t wf__completion_inline_executions(void) {
    return atomic_load_explicit(
        &wf_bridge_inline_executions,
        memory_order_relaxed
    );
}

uint64_t wf__completion_native_ring_submissions(void) {
    return wf_bridge_ring_submissions();
}

/* The ceiling of this bridge's own WF_IO_HELPERS setting, answered for the
 * core's entry so that a setting this runtime cannot mean ends the run before
 * the program body rather than at its first operation (`sched/entry.h`). */
unsigned long wf__sched_helper_ceiling(void) {
    return (unsigned long)WF_BRIDGE_MAX_HELPERS;
}
