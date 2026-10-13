//! A TCP receive releases a context computing without waits inside one atomic
//! statement. A link-time observer records the submitting ring and the physical
//! thread that parks the receive, then the thread that enters the computation
//! after acquiring its shared object. Equal thread IDs prove that this driver
//! parked the receive before starting the nonwaiting computation. The observer
//! never services a ring or changes scheduling. Sixteen runs also include a
//! split control whose source spins until the child publishes entry; that
//! control must record distinct threads. Both placements must occur, and every
//! receive must release the computation within 100 ms of the peer's send.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#include <unistd.h>

static _Thread_local wf_completion_record *receive_record;
static _Thread_local wf_linux_io_uring_adapter *receive_adapter;

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
    enum wf_linux_io_uring_submit_result result =
        __real_wf_linux_io_uring_submit(adapter, record);
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
        trace_event("parked", (uintptr_t)receive_adapter);
        receive_record = NULL;
        receive_adapter = NULL;
    }
    return suspended;
}

uint32_t __real_wf__claim_written(void *cell);
uint32_t __wrap_wf__claim_written(void *cell) {
    uint32_t verdict = __real_wf__claim_written(cell);
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
            "-Wl,--wrap=wf_linux_io_uring_submit",
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
            .env("WF_BUSY_RING_TRACE", &trace_path);
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
        // Send only after observing both the native receive's suspension and
        // the child's acquired atomic statement. Equal thread IDs also prove
        // that the native resume which suspended the root returned before the
        // same driver entered the child's nonwaiting loop.
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
        let sent = Instant::now();
        peer.write_all(b"\n")
            .expect("send one byte through the TCP connection");
        while child.try_wait().expect("inspect receive progress").is_none() {
            assert!(
                sent.elapsed() < RECEIVE_LIMIT,
                "run {run}: the receive did not release the atomic computation within {RECEIVE_LIMIT:?}; placement: {trace}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            sent.elapsed() < RECEIVE_LIMIT,
            "run {run}: exit exceeded {RECEIVE_LIMIT:?}; placement: {trace}"
        );
        drop(peer);
        let output = child
            .wait_with_output()
            .expect("collect the completed witness");
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
        let final_trace = std::fs::read_to_string(&trace_path).expect("read the final trace");
        assert_eq!(
            placement(&final_trace),
            Some((park_thread, compute_thread)),
            "run {run}: placement changed after the peer sent: {final_trace}"
        );
        // Keep successful placement evidence in CI output too, after the
        // timed interval. The temporary trace is removed with the program.
        writeln!(
            std::io::stderr().lock(),
            "busy-ring receive run {run} split={split}: {}",
            final_trace.trim_end().replace('\n', "; ")
        )
        .expect("report native placement evidence");
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
