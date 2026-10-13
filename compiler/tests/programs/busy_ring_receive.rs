//! A TCP receive releases a context computing without waits inside one atomic
//! statement. A link-time observer records the submitting ring and the physical
//! thread whose wait call reports suspension, then the thread that claims the
//! computation's entry cell after acquiring its shared object. These markers
//! retain the witness's placement rule; their relationship to coroutine unwind
//! and ring service is exposed by additional timestamped diagnostics. The
//! observer never invokes ring service or scheduler operations. Sixteen runs include a
//! split control whose source spins until the child publishes entry; that
//! control must record distinct threads. Both placements must occur, and every
//! receive must release the computation within 100 ms of the peer's send.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::support::{build_program_with_driver_arguments, compile_program};
use crate::support::ProgramChild;

const RUNS: usize = 16;
const START_LIMIT: Duration = Duration::from_secs(3);
const RECEIVE_LIMIT: Duration = Duration::from_millis(100);

// GNU ld wrapping observes the ordinary runtime entry points on main and on
// this branch. These calls retain their real behavior; no observer progresses
// or flushes a ring, makes a context ready, or changes a completion record.
const PLACEMENT_OBSERVER: &str = r#"
#define _GNU_SOURCE
#include "linux_io_uring.h"
#include "bridge.h"
#include <fcntl.h>
#include <inttypes.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

static _Thread_local wf_completion_record *receive_record;
static _Thread_local wf_linux_io_uring_adapter *receive_adapter;
static _Thread_local wf_linux_io_uring_adapter *observed_reap_adapter;
static _Atomic uintptr_t observed_receive_record;
static _Atomic uintptr_t observed_receive_adapter;
static _Atomic uintptr_t driver_adapters[2];
static _Atomic unsigned initialized_rings;

static uint64_t trace_clock(clockid_t clock) {
    struct timespec time;
    if (clock_gettime(clock, &time) != 0) abort();
    return (uint64_t)time.tv_sec * UINT64_C(1000000000) + (uint64_t)time.tv_nsec;
}

/* One append preserves each record as a line. File order is observation order,
 * not proof that concurrent calls executed in that order; timestamps supply
 * the clocks. Adapter addresses stay live until the witness shuts down. */
static void diagnostic_event(
    const char *event, wf_linux_io_uring_adapter *adapter,
    const void *record, const char *format, ...
) {
    const char *path = getenv("WF_BUSY_RING_DIAGNOSTICS");
    char details[384];
    va_list arguments;
    va_start(arguments, format);
    int detail_length = vsnprintf(details, sizeof(details), format, arguments);
    va_end(arguments);
    if (path == NULL || detail_length < 0
        || (size_t)detail_length >= sizeof(details)) abort();
    int driver_index = -1;
    for (unsigned index = 0; index < 2; ++index) {
        if (adapter != NULL && atomic_load_explicit(
                &driver_adapters[index], memory_order_acquire
            ) == (uintptr_t)adapter) driver_index = (int)index;
    }
    uint64_t monotonic_ns = trace_clock(CLOCK_MONOTONIC);
    uint64_t unix_ns = trace_clock(CLOCK_REALTIME);
    char line[768];
    int length = snprintf(line, sizeof(line),
        "event=%s monotonic_ns=%" PRIu64 " unix_ns=%" PRIu64
        " tid=%ld driver_index=%d adapter=%" PRIxPTR
        " ring_fd=%d record=%" PRIxPTR " %s\n",
        event, monotonic_ns, unix_ns,
        (long)syscall(SYS_gettid), driver_index, (uintptr_t)adapter,
        adapter == NULL ? -1 : adapter->ring_descriptor, (uintptr_t)record,
        details);
    if (length <= 0 || (size_t)length >= sizeof(line)) abort();
    int descriptor = open(path, O_WRONLY | O_APPEND | O_CLOEXEC);
    if (descriptor < 0) abort();
    if (write(descriptor, line, (size_t)length) != length) abort();
    if (close(descriptor) != 0) abort();
}

int __real_wf_linux_io_uring_init(
    wf_linux_io_uring_adapter *adapter, wf_completion_runtime *runtime,
    size_t depth, size_t completions);
int __wrap_wf_linux_io_uring_init(
    wf_linux_io_uring_adapter *adapter, wf_completion_runtime *runtime,
    size_t depth, size_t completions
) {
    int result = __real_wf_linux_io_uring_init(adapter, runtime, depth, completions);
    if (result == 0) {
        unsigned index = atomic_fetch_add_explicit(
            &initialized_rings, 1, memory_order_relaxed);
        if (index < 2) atomic_store_explicit(
            &driver_adapters[index], (uintptr_t)adapter, memory_order_release);
        /* Root retention initializes index 0 before lazy driver startup;
         * this two-ring witness then initializes exactly index 1. */
        diagnostic_event("ring_init", adapter, NULL,
            "result=%d successful_init_ordinal=%u", result, index);
    } else {
        diagnostic_event("ring_init", NULL, NULL,
            "result=%d requested_adapter=%" PRIxPTR, result, (uintptr_t)adapter);
    }
    return result;
}

static int trace_ring_service(void) {
    const char *enabled = getenv("WF_TRACE_RING_SERVICE");
    return enabled != NULL && enabled[0] == '1' && enabled[1] == '\0';
}

/* GNU --wrap catches references from other objects. Calls to progress from
 * within linux_io_uring.c may bypass it. receive_complete still identifies
 * the actual completing thread and the ring registered at submission. */
int __real_wf_linux_io_uring_progress(
    wf_linux_io_uring_adapter *adapter, size_t budget, unsigned wait,
    size_t *published);
int __wrap_wf_linux_io_uring_progress(
    wf_linux_io_uring_adapter *adapter, size_t budget, unsigned wait,
    size_t *published
) {
    if (!trace_ring_service() || adapter == NULL || adapter->initialized == 0
        || published == NULL || budget == 0) {
        return __real_wf_linux_io_uring_progress(adapter, budget, wait, published);
    }
    diagnostic_event("reap_begin", adapter, NULL,
        "budget=%zu wait=%u sq_head=%u sq_tail=%u cq_head=%u cq_tail=%u",
        budget, wait,
        __atomic_load_n(adapter->submission_head, __ATOMIC_ACQUIRE),
        __atomic_load_n(adapter->submission_tail, __ATOMIC_ACQUIRE),
        __atomic_load_n(adapter->completion_head, __ATOMIC_ACQUIRE),
        __atomic_load_n(adapter->completion_tail, __ATOMIC_ACQUIRE));
    wf_linux_io_uring_adapter *previous = observed_reap_adapter;
    observed_reap_adapter = adapter;
    int result = __real_wf_linux_io_uring_progress(adapter, budget, wait, published);
    observed_reap_adapter = previous;
    diagnostic_event("reap_end", adapter, NULL,
        "budget=%zu wait=%u published=%zu result=%d sq_head=%u sq_tail=%u cq_head=%u cq_tail=%u",
        budget, wait, *published, result,
        __atomic_load_n(adapter->submission_head, __ATOMIC_ACQUIRE),
        __atomic_load_n(adapter->submission_tail, __ATOMIC_ACQUIRE),
        __atomic_load_n(adapter->completion_head, __ATOMIC_ACQUIRE),
        __atomic_load_n(adapter->completion_tail, __ATOMIC_ACQUIRE));
    return result;
}

void __real_wf_completion_record_complete(wf_completion_record *record);
void __wrap_wf_completion_record_complete(wf_completion_record *record) {
    uintptr_t expected = (uintptr_t)record;
    if (record->request.kind == WF_FILE_SOCKET_RECEIVE
        && atomic_compare_exchange_strong_explicit(
            &observed_receive_record, &expected, 0,
            memory_order_acq_rel, memory_order_acquire)) {
        wf_linux_io_uring_adapter *adapter = (wf_linux_io_uring_adapter *)
            atomic_load_explicit(&observed_receive_adapter, memory_order_acquire);
        /* Publication can immediately end this record's frame lifetime.
         * All reads and the diagnostic write precede real publication. */
        diagnostic_event("receive_complete", adapter, record,
            "kind=%u route=%u value=%" PRId64 " error=%d reap_origin=%s reap_adapter=%" PRIxPTR,
            (unsigned)record->request.kind, (unsigned)record->route,
            (int64_t)record->result.value, record->result.error_code,
            observed_reap_adapter == NULL ? "unwrapped_or_trace_off" : "wrapped_progress",
            (uintptr_t)observed_reap_adapter);
    }
    __real_wf_completion_record_complete(record);
}

static void trace_event(const char *event, uintptr_t ring) {
    const char *path = getenv("WF_BUSY_RING_TRACE");
    char line[128];
    int length = snprintf(line, sizeof(line), "%s %ld %" PRIxPTR "\n",
        event, (long)syscall(SYS_gettid), ring);
    if (path == NULL || length <= 0 || (size_t)length >= sizeof(line)) abort();
    int descriptor = open(path, O_WRONLY | O_APPEND | O_CLOEXEC);
    if (descriptor < 0) abort();
    if (write(descriptor, line, (size_t)length) != length) abort();
    if (close(descriptor) != 0) abort();
}

enum wf_linux_io_uring_submit_result __real_wf_linux_io_uring_submit(
    wf_linux_io_uring_adapter *adapter, wf_completion_record *record);

enum wf_linux_io_uring_submit_result __wrap_wf_linux_io_uring_submit(
    wf_linux_io_uring_adapter *adapter, wf_completion_record *record) {
    int is_receive = record->request.kind == WF_FILE_SOCKET_RECEIVE;
    if (is_receive) {
        atomic_store_explicit(&observed_receive_adapter,
            (uintptr_t)adapter, memory_order_release);
        atomic_store_explicit(&observed_receive_record,
            (uintptr_t)record, memory_order_release);
        diagnostic_event("receive_submit_begin", adapter, record, "");
    }
    enum wf_linux_io_uring_submit_result result =
        __real_wf_linux_io_uring_submit(adapter, record);
    if (is_receive) {
        diagnostic_event("receive_submit_end", adapter, record, "result=%d", result);
        if (result != WF_LINUX_IO_URING_TARGET_OWNS) {
            uintptr_t expected = (uintptr_t)record;
            (void)atomic_compare_exchange_strong_explicit(
                &observed_receive_record, &expected, 0,
                memory_order_acq_rel, memory_order_acquire);
        }
    }
    if (result == WF_LINUX_IO_URING_TARGET_OWNS
        && record->request.kind == WF_FILE_SOCKET_RECEIVE) {
        receive_record = record;
        receive_adapter = adapter;
    }
    return result;
}

int __real_wf__context_wait(void *operation, void *frame);
int __wrap_wf__context_wait(void *operation, void *frame) {
    int suspended = __real_wf__context_wait(operation, frame);
    wf_completion_record *record = operation;
    if (suspended == 1 && record == receive_record
        && record->route == WF_COMPLETION_ROUTE_LINUX_IO_URING
        && record->request.kind == WF_FILE_SOCKET_RECEIVE) {
        diagnostic_event("wait_return", receive_adapter, record,
            "suspended=%d kind=%u route=%u state=%u issued=%u",
            suspended, (unsigned)record->request.kind, (unsigned)record->route,
            atomic_load_explicit(&record->state, memory_order_acquire),
            atomic_load_explicit(&record->issued, memory_order_acquire));
        trace_event("parked", (uintptr_t)receive_adapter);
        receive_record = NULL;
        receive_adapter = NULL;
    }
    return suspended;
}

uint32_t __real_wf__claim_written(void *cell);
uint32_t __wrap_wf__claim_written(void *cell) {
    uint32_t verdict = __real_wf__claim_written(cell);
    diagnostic_event("computing", NULL, NULL,
        "cell=%" PRIxPTR " verdict=%" PRIu32, (uintptr_t)cell, verdict);
    trace_event("computing", 0);
    return verdict;
}
"#;

// Return the park and computation thread IDs only once both complete events
// have arrived. A partial final line is retried rather than parsed as evidence.
fn placement(trace: &str) -> Option<(u64, u64)> {
    if !trace.ends_with('\n') {
        return None;
    }
    let mut parked = None;
    let mut computing = None;
    for line in trace.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        assert_eq!(fields.len(), 3, "invalid placement event: {trace}");
        let thread = fields[1].parse::<u64>().expect("a physical thread ID");
        assert_ne!(thread, 0, "invalid placement thread: {trace}");
        let ring = u64::from_str_radix(fields[2], 16).expect("a ring address");
        match fields[0] {
            "parked" => {
                assert_ne!(ring, 0, "the receive must name a native ring: {trace}");
                assert!(parked.replace(thread).is_none(), "duplicate park: {trace}");
            }
            "computing" => {
                assert_eq!(ring, 0, "invalid computation event: {trace}");
                assert!(
                    computing.replace(thread).is_none(),
                    "duplicate computation: {trace}"
                );
            }
            _ => panic!("unknown placement event: {trace}"),
        }
    }
    Some((parked?, computing?))
}

fn native_ring_count(child: &ProgramChild) -> usize {
    std::fs::read_dir(format!("/proc/{}/fd", child.id()))
        .expect("list the witness's descriptors")
        .map(|entry| {
            let path = entry.expect("read a witness descriptor entry").path();
            std::fs::read_link(path).expect("read a witness descriptor target")
        })
        .filter(|target| target.to_string_lossy().contains("io_uring"))
        .count()
}

fn unix_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the current Unix timestamp")
        .as_nanos()
}

// Declared before the child, so unwinding drops the child first and then
// preserves whatever records exist when an earlier assertion stops the run.
struct PanicObservations {
    run: usize,
    split: bool,
    diagnostic_path: std::path::PathBuf,
    placement_path: std::path::PathBuf,
}

impl Drop for PanicObservations {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            return;
        }
        let collected_unix_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut stderr = std::io::stderr().lock();
        let run = self.run;
        let split = self.split;
        let _ = writeln!(
            stderr,
            "busy-ring receive run={run} split={split} collected_unix_ns={collected_unix_ns} panic_snapshot available_trace_records_only WF_TRACE_RING_SERVICE={}",
            std::env::var("WF_TRACE_RING_SERVICE").unwrap_or_else(|_| "unset".to_owned())
        );
        for (order_name, path) in [
            ("record_order", &self.diagnostic_path),
            ("placement_order", &self.placement_path),
        ] {
            match std::fs::read_to_string(path) {
                Ok(trace) => {
                    for (order, line) in trace.lines().enumerate() {
                        let _ = writeln!(
                            stderr,
                            "busy-ring receive run={run} split={split} collected_unix_ns={collected_unix_ns} panic_snapshot {order_name}={order} {line}"
                        );
                    }
                }
                Err(error) => {
                    let _ = writeln!(
                        stderr,
                        "busy-ring receive run={run} split={split} collected_unix_ns={collected_unix_ns} panic_snapshot {order_name} unavailable={error}"
                    );
                }
            }
        }
    }
}

fn report_observations(
    run: usize,
    split: bool,
    diagnostic_path: &std::path::Path,
    placement_trace: &str,
    scheduler_report: &str,
) {
    let diagnostics =
        std::fs::read_to_string(diagnostic_path).expect("read ring-service observations");
    let mut stderr = std::io::stderr().lock();
    let collected_unix_ns = unix_ns();
    writeln!(
        stderr,
        "busy-ring receive run={run} split={split} collected_unix_ns={collected_unix_ns} config WF_TRACE_RING_SERVICE={} ring_mapping=successful_init_ordinal(driver0_then_driver1)",
        std::env::var("WF_TRACE_RING_SERVICE").unwrap_or_else(|_| "unset".to_owned())
    )
    .expect("report trace configuration");
    for (order, line) in diagnostics.lines().enumerate() {
        writeln!(
            stderr,
            "busy-ring receive run={run} split={split} record_order={order} {line}"
        )
        .expect("report ring-service observation");
    }
    for (order, line) in placement_trace.lines().enumerate() {
        writeln!(
            stderr,
            "busy-ring receive run={run} split={split} collected_unix_ns={collected_unix_ns} placement_order={order} {line}"
        )
        .expect("report native placement evidence");
    }
    for line in scheduler_report.lines() {
        writeln!(
            stderr,
            "busy-ring receive run={run} split={split} collected_unix_ns={collected_unix_ns} child_stderr {line}"
        )
        .expect("report driver evidence");
    }
}

#[test]
fn a_tcp_receive_releases_an_atomic_computation_with_an_idle_driver() {
    let program = build_program_with_driver_arguments(
        &compile_program("busy_ring_receive.wf"),
        Some(PLACEMENT_OBSERVER),
        &[
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wno-override-module",
            "-Wl,--wrap=wf_linux_io_uring_init",
            "-Wl,--wrap=wf_linux_io_uring_submit",
            "-Wl,--wrap=wf_linux_io_uring_progress",
            "-Wl,--wrap=wf_completion_record_complete",
            "-Wl,--wrap=wf__context_wait",
            "-Wl,--wrap=wf__claim_written",
        ],
    );
    let mut same_driver = false;
    let mut split_driver = false;
    for run in 0..RUNS {
        let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve a loopback port");
        let address = reservation
            .local_addr()
            .expect("the reserved loopback address");
        drop(reservation);
        let split = run % 2 != 0;
        let trace_path = program.working_directory().join(format!("placement-{run}"));
        std::fs::write(&trace_path, b"").expect("create the placement trace");
        let diagnostic_path = program.working_directory().join(format!("diagnostics-{run}"));
        std::fs::write(&diagnostic_path, b"").expect("create the diagnostic trace");
        let _panic_observations = PanicObservations {
            run,
            split,
            diagnostic_path: diagnostic_path.clone(),
            placement_path: trace_path.clone(),
        };
        let run_started = Instant::now();
        let mut command = Command::new(program.executable());
        command.arg(address.port().to_string());
        if split {
            command.arg("split");
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_remove("WF_IO_NO_NATIVE_RING")
            .env_remove("WF_WORKERS")
            .env("WF_DRIVERS", "2")
            .env("WF_SCHED_REPORT", "2")
            .env("WF_BUSY_RING_TRACE", &trace_path)
            .env("WF_BUSY_RING_DIAGNOSTICS", &diagnostic_path);
        let (mut child, ready) = ProgramChild::spawn_signalling_first_output(&mut command)
            .expect("start the busy-ring witness");
        ready.recv_timeout(START_LIMIT).unwrap_or_else(|error| {
            panic!("run {run}: the TCP listener never became ready: {error}")
        });
        // Readiness follows listen but precedes accept and spawn. The root
        // ring is retained before entry; secondary drivers start lazily.
        let rings = native_ring_count(&child);
        if rings == 0 {
            assert_ne!(
                std::env::var("REQUIRE_NATIVE").as_deref(),
                Ok("1"),
                "run {run}: the host refused the kernel ring this witness requires"
            );
            assert_ne!(
                std::env::var("WF_REQUIRE_LINUX_IO_URING").as_deref(),
                Ok("1"),
                "run {run}: the host refused the kernel ring this witness requires"
            );
            // Release the fallback accept. Only an independently observed
            // absent ring may skip; status 77 alone is never enough.
            let peer = TcpStream::connect_timeout(&address, START_LIMIT)
                .expect("release the unavailable-ring witness's accept");
            let deadline = Instant::now() + START_LIMIT;
            while child
                .try_wait()
                .expect("inspect unavailable-ring exit")
                .is_none()
            {
                assert!(
                    Instant::now() < deadline,
                    "run {run}: unavailable-ring exit timed out"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            let output = child
                .wait_with_output()
                .expect("collect unavailable-ring exit");
            drop(peer);
            let trace = std::fs::read_to_string(&trace_path).expect("read fallback placement");
            let stderr = String::from_utf8_lossy(&output.stderr);
            report_observations(run, split, &diagnostic_path, &trace, &stderr);
            assert_eq!(output.status.code(), Some(77), "run {run}: {output:?}");
            assert_eq!(output.stdout, b"r", "run {run}: {output:?}");
            // Write directly so libtest capture cannot hide the skip notice.
            writeln!(
                std::io::stderr().lock(),
                "busy-ring receive io_uring: skipped; unavailable on this host (not native evidence)"
            )
            .expect("report unavailable native evidence");
            return;
        }
        assert_eq!(rings, 1, "run {run}: the retained pre-spawn root ring");
        let mut peer = TcpStream::connect_timeout(&address, START_LIMIT)
            .expect("connect to the witness's TCP listener");
        peer.set_write_timeout(Some(RECEIVE_LIMIT))
            .expect("bound the peer's write");
        // Preserve the witness's placement criterion: the wait entry returned
        // suspended and the child claimed its entry cell. Diagnostic markers
        // identify those calls, not completed coroutine unwind or loop entry.
        let start = Instant::now();
        let (park_thread, compute_thread, trace) = loop {
            let trace = std::fs::read_to_string(&trace_path).expect("read placement evidence");
            if let Some((park_thread, compute_thread)) = placement(&trace) {
                break (park_thread, compute_thread, trace);
            }
            assert!(
                child.try_wait().expect("inspect before sending").is_none(),
                "run {run}: the witness exited before both placement events: {trace}"
            );
            assert!(
                start.elapsed() < START_LIMIT,
                "run {run}: placement did not become observable: {trace}"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        if park_thread == compute_thread {
            assert!(
                trace.starts_with("parked "),
                "run {run}: receive did not park before computation: {trace}"
            );
            same_driver = true;
        }
        if split {
            assert_ne!(
                park_thread, compute_thread,
                "run {run}: split source spin did not separate the drivers: {trace}"
            );
            split_driver = true;
        }
        // Distinct threads in a direct run are not placement evidence: a
        // whole-role handoff can replace a driver's physical thread.
        assert_eq!(
            native_ring_count(&child),
            2,
            "run {run}: two native driver rings; placement: {trace}"
        );
        let send_unix_ns = unix_ns();
        let send_since_run_start_ns = run_started.elapsed().as_nanos();
        let sent = Instant::now();
        peer.write_all(b"\n")
            .expect("send one byte through the TCP connection");
        while child.try_wait().expect("inspect receive progress").is_none() {
            if sent.elapsed() >= RECEIVE_LIMIT {
                let final_trace = std::fs::read_to_string(&trace_path)
                    .expect("read placement at receive timeout");
                report_observations(run, split, &diagnostic_path, &final_trace, "");
                writeln!(
                    std::io::stderr().lock(),
                    "busy-ring receive run={run} split={split} parent_timeout unix_ns={} send_unix_ns={send_unix_ns} send_since_run_start_ns={send_since_run_start_ns} send_to_observation_ns={}",
                    unix_ns(), sent.elapsed().as_nanos()
                )
                .expect("report receive timeout observation");
            }
            assert!(
                sent.elapsed() < RECEIVE_LIMIT,
                "run {run}: the receive did not release the atomic computation within {RECEIVE_LIMIT:?}; placement: {trace}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let exit_elapsed = sent.elapsed();
        if exit_elapsed >= RECEIVE_LIMIT {
            let final_trace = std::fs::read_to_string(&trace_path)
                .expect("read placement at late exit");
            report_observations(run, split, &diagnostic_path, &final_trace, "");
            writeln!(
                std::io::stderr().lock(),
                "busy-ring receive run={run} split={split} parent_late_exit unix_ns={} send_unix_ns={send_unix_ns} send_since_run_start_ns={send_since_run_start_ns} send_to_exit_ns={}",
                unix_ns(), exit_elapsed.as_nanos()
            )
            .expect("report late exit observation");
        }
        assert!(
            sent.elapsed() < RECEIVE_LIMIT,
            "run {run}: exit exceeded {RECEIVE_LIMIT:?}; placement: {trace}"
        );
        let exit_unix_ns = unix_ns();
        let exit_since_run_start_ns = run_started.elapsed().as_nanos();
        drop(peer);
        let output = child
            .wait_with_output()
            .expect("collect the completed witness");
        let final_trace = std::fs::read_to_string(&trace_path).expect("read the final trace");
        report_observations(
            run,
            split,
            &diagnostic_path,
            &final_trace,
            &String::from_utf8_lossy(&output.stderr),
        );
        writeln!(
            std::io::stderr().lock(),
            "busy-ring receive run={run} split={split} parent_send unix_ns={send_unix_ns} since_run_start_ns={send_since_run_start_ns}\nbusy-ring receive run={run} split={split} parent_exit unix_ns={exit_unix_ns} since_run_start_ns={exit_since_run_start_ns} send_to_exit_ns={}",
            exit_elapsed.as_nanos()
        )
        .expect("report send-to-exit observation");
        assert_eq!(output.status.code(), Some(0), "run {run}: {output:?}");
        assert_eq!(output.stdout, b"r", "run {run}: {output:?}");
        let stderr = String::from_utf8(output.stderr).expect("a text driver report");
        let mut drivers = 0;
        for line in stderr.lines().filter(|line| line.starts_with("driver: ")) {
            let field = |name: &str| {
                line.split_whitespace()
                    .find_map(|field| field.strip_prefix(name))
                    .unwrap_or_else(|| panic!("run {run}: missing {name} in {line:?}"))
                    .parse::<u64>()
                    .expect("a driver report count")
            };
            assert_eq!(field("index="), drivers, "run {run}: {stderr}");
            drivers += 1;
        }
        assert_eq!(drivers, 2, "run {run}: {stderr}");
        assert_eq!(
            placement(&final_trace),
            Some((park_thread, compute_thread)),
            "run {run}: placement changed after the peer sent: {final_trace}"
        );
    }
    assert!(
        same_driver,
        "no run of {RUNS} exercised the computing driver's ring"
    );
    assert!(
        split_driver,
        "no run of {RUNS} placed the receive on a driver separate from the computation"
    );
}
