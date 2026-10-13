//! A pipe receive releases a context computing without waits inside an atomic
//! statement. With two drivers, the root submits the receive before running
//! its single ready child locally; an idle driver may steal that child first.
//! Sixteen runs alternate the direct receive and a split control that leaves
//! the child ready during a finite recurrence before receiving. Reports must
//! show both placements: a ring reassignment attributes the same-driver case;
//! an idle-driver steal without a ring reassignment establishes the split.
//! Repetition alone does not establish either. Before ring assist, a same-driver
//! run cannot finish: only the receive can kill the computation, while only
//! the computing driver's unserved ring can deliver the receive.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::support::{build_program, compile_program};
use crate::support::ProgramChild;

const RUNS: usize = 16;
const START_LIMIT: Duration = Duration::from_secs(3);
const RECEIVE_LIMIT: Duration = Duration::from_millis(100);

#[test]
fn a_pipe_receive_releases_an_atomic_computation_with_an_idle_driver() {
    let program = build_program(&compile_program("busy_ring_receive.wf"));
    let mut same_driver = false;
    let mut split_driver = false;
    for run in 0..RUNS {
        let mut command = Command::new(program.executable());
        if run % 2 != 0 {
            command.arg("split");
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_remove("WF_IO_NO_NATIVE_RING")
            .env_remove("WF_WORKERS")
            .env("WF_DRIVERS", "2")
            .env("WF_SCHED_REPORT", "2");
        let (mut child, ready) = ProgramChild::spawn_signalling_first_output(&mut command)
            .expect("start the busy-ring witness");
        let mut input = child.take_stdin().expect("the peer's pipe writer");
        if let Err(error) = ready.recv_timeout(START_LIMIT) {
            let deadline = Instant::now() + Duration::from_millis(100);
            while child.try_wait().expect("inspect startup").is_none() {
                assert!(
                    Instant::now() < deadline,
                    "run {run}: the witness never became ready: {error}"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            let output = child.wait_with_output().expect("collect startup failure");
            assert_eq!(output.status.code(), Some(77), "{output:?}");
            assert_ne!(
                std::env::var("WF_REQUIRE_LINUX_IO_URING").as_deref(),
                Ok("1"),
                "the host refused the kernel ring this witness requires"
            );
            return;
        }
        // Let the child enter its atomic statement and the root submit its
        // read. The source also checks the independent entry verdict, so an
        // early write cannot produce a successful run.
        std::thread::sleep(Duration::from_millis(100));
        assert!(child.try_wait().expect("inspect before writing").is_none());
        let sent = Instant::now();
        input.write_all(b"\n").expect("send one line through the pipe");
        while child.try_wait().expect("inspect receive progress").is_none() {
            assert!(
                sent.elapsed() < RECEIVE_LIMIT,
                "run {run}: the receive did not release the atomic computation within {RECEIVE_LIMIT:?}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            sent.elapsed() < RECEIVE_LIMIT,
            "run {run}: exit exceeded {RECEIVE_LIMIT:?}"
        );
        drop(input);
        let output = child.wait_with_output().expect("collect the completed witness");
        assert_eq!(output.status.code(), Some(0), "run {run}: {output:?}");
        assert_eq!(output.stdout, b"r", "run {run}: {output:?}");
        let stderr = String::from_utf8(output.stderr).expect("a text driver report");
        let mut drivers = 0;
        let mut reassigned = false;
        let mut stolen = false;
        for line in stderr.lines().filter(|line| line.starts_with("driver: ")) {
            drivers += 1;
            let reassignments = line
                .split_whitespace()
                .find_map(|field| field.strip_prefix("ring_reassignments="))
                .unwrap_or("0")
                .parse::<u64>()
                .expect("a ring reassignment count");
            reassigned |= reassignments != 0;
            if line.split_whitespace().any(|field| field == "index=1") {
                let contexts = line
                    .split_whitespace()
                    .find_map(|field| field.strip_prefix("stolen_contexts="))
                    .expect("the idle driver's placement counter")
                    .parse::<u64>()
                    .expect("a stolen context count");
                stolen = contexts != 0;
            }
        }
        assert_eq!(drivers, 2, "run {run}: {stderr}");
        same_driver |= reassigned;
        split_driver |= !reassigned && stolen;
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
