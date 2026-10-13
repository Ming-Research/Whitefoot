//! A TCP receive releases a context computing without waits inside an atomic
//! statement. With two drivers, the root submits the receive before running
//! its single ready child locally; an idle driver may steal that child first.
//! Sixteen runs alternate this direct receive and a split control that spins
//! without waiting until the child's independent entry verdict is published.
//! Every receive must finish within 100 ms. Both placements must occur: zero
//! aggregate steals proves the same driver, while the split control's entry
//! spin proves the child ran elsewhere. Placement uses counters present on
//! main too. Before ring assist, a same-driver run cannot finish: only the
//! receive can kill the computation, while only the computing driver's
//! unserved ring can deliver the receive.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::support::{build_program, compile_program};
use crate::support::ProgramChild;

const RUNS: usize = 16;
const START_LIMIT: Duration = Duration::from_secs(3);
const RECEIVE_LIMIT: Duration = Duration::from_millis(100);

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
    let program = build_program(&compile_program("busy_ring_receive.wf"));
    let mut same_driver = false;
    let mut split_driver = false;
    for run in 0..RUNS {
        let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve a loopback port");
        let address = reservation
            .local_addr()
            .expect("the reserved loopback address");
        drop(reservation);
        let split = run % 2 != 0;
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
            .env("WF_SCHED_REPORT", "2");
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
        // Let the child enter its atomic statement and the root submit its
        // receive. Source checks entry independently, so an early send cannot
        // turn a receive that preceded computation into a successful run.
        std::thread::sleep(Duration::from_millis(100));
        assert!(child.try_wait().expect("inspect before sending").is_none());
        assert_eq!(
            native_ring_count(&child),
            2,
            "run {run}: two native driver rings"
        );
        let sent = Instant::now();
        peer.write_all(b"\n")
            .expect("send one byte through the TCP connection");
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
        drop(peer);
        let output = child
            .wait_with_output()
            .expect("collect the completed witness");
        assert_eq!(output.status.code(), Some(0), "run {run}: {output:?}");
        assert_eq!(output.stdout, b"r", "run {run}: {output:?}");
        let stderr = String::from_utf8(output.stderr).expect("a text driver report");
        let mut drivers = 0;
        let mut stolen = 0_u64;
        for line in stderr.lines().filter(|line| line.starts_with("driver: ")) {
            let field = |name: &str| {
                line.split_whitespace()
                    .find_map(|field| field.strip_prefix(name))
                    .unwrap_or_else(|| panic!("run {run}: missing {name} in {line:?}"))
                    .parse::<u64>()
                    .expect("a driver report count")
            };
            assert_eq!(field("index="), drivers, "run {run}: {stderr}");
            stolen += field("stolen_contexts=");
            drivers += 1;
        }
        assert_eq!(drivers, 2, "run {run}: {stderr}");
        // Zero steals is conservative: a later steal excludes a valid same-
        // driver run rather than falsely counting it. Latency was required
        // above regardless of how this run's placement is classified.
        same_driver |= stolen == 0;
        if split {
            assert!(
                stolen > 0,
                "run {run}: split entry spin had no context steal: {stderr}"
            );
            split_driver = true;
        }
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
