//! A completion that waits in the ring of a driver whose thread is computing
//! reaches its context within 100 ms (one second in a sanitized build),
//! whether or not another driver is idle.
//!
//! `ring_assist.wf` accepts two loopback connections and serves each in a
//! context of its own. One peer asks for a recurrence that never waits; the
//! other sends four bytes 100 ms later and times their echo. With two
//! drivers the entry starts both contexts on its own driver, so the echo's
//! receive completes in the ring of the computing driver while the other
//! driver is idle and has nothing to take: before the monitor handed such a
//! role to a spare, the echo came back only after the whole computation.
//! The client cannot know the runner's step rate in advance, so a run whose
//! computation answered before an echo that stayed within the bound shows
//! neither outcome: a stalled echo behind a computation too short to push it
//! past the bound looks the same as a computation that ended before the echo
//! was served. Such a run is run again with twice the steps, and only a run
//! whose computation outlasts the echo decides the arm. The split arm makes
//! the entry compute briefly after the first spawn, which leaves that context
//! ready long enough for the idle driver to take it, so the echo is served on
//! a driver of its own; it is the control that the client and the timing
//! detect the stall and nothing else.
//! Each run's placement is read from its `WF_SCHED_REPORT=2` lines, never
//! assumed, and a run whose placement differs from its arm's is run again.
//! A run is split when the idle driver took a context and the computing
//! driver's role was never handed over for ring work; every other run is
//! same-driver. The handoff makes the echo's context ready on the computing
//! driver's role, where the idle driver may then take it, so a steal alone
//! does not show that the echo was served on a driver of its own.

use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use super::support::{CompiledProgram, build_program, compile_program};

/// Recurrence steps of the computing peer's first run. On the hosted runners
/// they last at most about 170 to 240 ms from the request, so with the echo
/// sent 100 ms after the request a stall can stay under the bound; a run
/// that cannot decide is run again with twice the steps.
const STEPS: u64 = 1_500_000_000;
/// Doublings of the steps an arm may take before a computation that still
/// answers before the echo fails the arm: sixteen times the first run's
/// steps last seconds at the hosted runners' step rate.
const DOUBLINGS: u32 = 4;
/// Steps the entry computes after the first spawn in the split arm, a few
/// milliseconds at the hosted runners' step rate against the idle driver's
/// 1 ms look for ready contexts; a run where the idle driver took nothing is
/// misplaced and run again.
const SPLIT_SPIN: u64 = 20_000_000;

/// The bound on an echo's return. Without the ring trigger the echo waits for
/// the whole computation, and a computation that ends before the bound decides
/// nothing and is run again with more steps, so a longer bound tells the
/// stall from the fix just as well. A sanitized run executes the runtime far
/// more slowly, and the sanitized corpus shares its CPUs among many programs,
/// so its bound is one second.
fn echo_limit() -> Duration {
    if crate::support::sanitizer().is_some() {
        Duration::from_secs(1)
    } else {
        Duration::from_millis(100)
    }
}
/// Runs an arm may take to show its placement. The idle driver can take the
/// echo's context before it parks, which makes that run split; on 2 CPUs three
/// runs sometimes held none that stayed on the entry's driver. Every run still
/// has to keep its echo inside the bound.
const ATTEMPTS: usize = 8;

/// The recurrence `mix` in `ring_assist.wf` computes, advanced by squaring
/// the step's affine map instead of stepping it, so the oracle is independent
/// of the program's loop and costs nothing.
fn mix(seed: u64, steps: u64) -> u64 {
    let mut multiplier = 6_364_136_223_846_793_005_u64;
    let mut increment = 1_442_695_040_888_963_407_u64;
    let (mut total_multiplier, mut total_increment) = (1_u64, 0_u64);
    let mut remaining = steps;
    while remaining != 0 {
        if remaining & 1 == 1 {
            total_multiplier = total_multiplier.wrapping_mul(multiplier);
            total_increment = total_increment.wrapping_mul(multiplier).wrapping_add(increment);
        }
        increment = multiplier.wrapping_add(1).wrapping_mul(increment);
        multiplier = multiplier.wrapping_mul(multiplier);
        remaining >>= 1;
    }
    total_multiplier.wrapping_mul(seed).wrapping_add(total_increment)
}

#[test]
fn the_recurrence_oracle_matches_stepping() {
    let mut stepped = 7_u64;
    for steps in 0..300_u64 {
        assert_eq!(mix(7, steps), stepped, "{steps} steps");
        stepped = stepped
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("reserve a loopback port")
        .local_addr()
        .expect("the reserved port's address")
        .port()
}

fn connect_when_ready(port: u16) -> TcpStream {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
            Ok(stream) => {
                stream.set_read_timeout(Some(Duration::from_secs(20))).expect("bound peer reads");
                stream.set_write_timeout(Some(Duration::from_secs(10))).expect("bound peer writes");
                return stream;
            }
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("the program never listened on {port}: {error}"),
        }
    }
}

/// One driver's report line, by field name; a field the line lacks reads as
/// zero, so an older runtime's report still parses.
struct DriverReport(Vec<(String, u64)>);

impl DriverReport {
    fn field(&self, name: &str) -> u64 {
        self.0
            .iter()
            .find(|(field, _)| field == name)
            .map_or(0, |(_, value)| *value)
    }
}

fn driver_reports(stderr: &str) -> Vec<DriverReport> {
    let mut reports = Vec::new();
    for line in stderr.lines() {
        if line.starts_with("compute: ") {
            continue;
        }
        let fields = line
            .strip_prefix("driver: ")
            .unwrap_or_else(|| panic!("unexpected diagnostic line: {line:?}"));
        let pairs = fields
            .split(' ')
            .map(|pair| {
                let (name, value) = pair.split_once('=').expect("a report field");
                (name.to_owned(), value.parse().expect("a report count"))
            })
            .collect::<Vec<(String, u64)>>();
        let report = DriverReport(pairs);
        assert_eq!(report.field("index"), reports.len() as u64, "{stderr}");
        reports.push(report);
    }
    reports
}

struct Run {
    echo: Duration,
    /// The computation's reply was already readable when the echo returned.
    computed_first: bool,
    reports: Vec<DriverReport>,
    stderr: String,
}

/// One run of the witness: `None` when the host refused the kernel ring, so
/// the program ran one driver and no ring exists for the trigger to watch.
fn run_once(program: &CompiledProgram, steps: u64, spin: u64) -> Option<Run> {
    let port = free_port().to_string();
    let steps_argument = steps.to_string();
    let spin = spin.to_string();
    let child = program.spawn_on_route_with(
        true,
        &[("WF_DRIVERS", "2"), ("WF_SCHED_REPORT", "2")],
        &[port.as_bytes(), steps_argument.as_bytes(), spin.as_bytes()],
    );
    let port = port.parse().expect("the reserved port");
    let mut echo = connect_when_ready(port);
    let mut compute = connect_when_ready(port);
    compute.write_all(b"g").expect("ask for the computation");
    std::thread::sleep(Duration::from_millis(100));
    let rings = std::fs::read_dir(format!("/proc/{}/fd", child.id()))
        .expect("list the program's descriptors")
        .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
        .filter(|target| target.to_string_lossy().contains("io_uring"))
        .count();
    if rings == 0 {
        assert_ne!(
            std::env::var("WF_REQUIRE_LINUX_IO_URING").as_deref(),
            Ok("1"),
            "the host refused the kernel ring this case requires"
        );
        drop(echo);
        drop(compute);
        let output = child.wait_with_output().expect("wait for the program");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        return None;
    }
    assert_eq!(rings, 2, "two drivers hold one ring each");
    let sent = Instant::now();
    echo.write_all(b"ping").expect("send the echo's bytes");
    let mut returned = [0_u8; 4];
    echo.read_exact(&mut returned).expect("read the echo");
    let latency = sent.elapsed();
    assert_eq!(&returned, b"ping");
    compute.set_nonblocking(true).expect("look at the computation's reply without waiting");
    let mut first = [0_u8; 1];
    let computed_first = match compute.peek(&mut first) {
        Err(error) if error.kind() == ErrorKind::WouldBlock => false,
        Ok(read) if read > 0 => true,
        other => panic!("the computation's connection failed before the echo returned: {other:?}"),
    };
    compute.set_nonblocking(false).expect("wait for the computation's reply");
    let mut value = [0_u8; 8];
    compute.read_exact(&mut value).expect("read the computation's reply");
    assert_eq!(u64::from_le_bytes(value), mix(1, steps), "the computed value");
    echo.shutdown(Shutdown::Write).expect("end the echo's sending");
    let mut rest = Vec::new();
    echo.read_to_end(&mut rest).expect("the echo connection closes");
    assert!(rest.is_empty(), "nothing more is echoed: {rest:?}");
    drop(echo);
    drop(compute);
    let output = child.wait_with_output().expect("wait for the program");
    let stderr = String::from_utf8(output.stderr).expect("a text report");
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    let reports = driver_reports(&stderr);
    assert_eq!(reports.len(), 2, "{stderr}");
    Some(Run {
        echo: latency,
        computed_first,
        reports,
        stderr,
    })
}

/// Runs one arm until a run shows the arm's placement, checking the echo's
/// bound on every run before anything else, so a late echo always fails as
/// one, and returns the run that showed the placement. A run whose
/// computation answered before an echo within the bound decides nothing and
/// is run again with twice the steps, at most `DOUBLINGS` times; it does not
/// count as an attempt at the placement.
fn run_arm(arm: &str, spin: u64, placed: impl Fn(&[DriverReport]) -> bool) -> Option<Run> {
    let program = build_program(&compile_program("ring_assist.wf"));
    let mut misplaced = String::new();
    let mut steps = STEPS;
    let echo_limit = echo_limit();
    let mut doublings = 0;
    let mut attempt = 0;
    let mut placements = 0;
    while placements < ATTEMPTS {
        attempt += 1;
        let run = run_once(&program, steps, spin)?;
        assert!(
            run.echo < echo_limit,
            "{arm} arm, attempt {attempt}: the echo took {:?}, over {echo_limit:?}, beside a computation of {steps} steps:\n{}",
            run.echo,
            run.stderr
        );
        assert_eq!(
            run.reports[0].field("reserve_misses"),
            0,
            "{arm} arm, attempt {attempt}: a spare was free:\n{}",
            run.stderr
        );
        if run.computed_first {
            assert!(
                doublings < DOUBLINGS,
                "{arm} arm, attempt {attempt}: the computation of {steps} steps still answered before an echo of {:?}, so it did not run beside the echo:\n{}",
                run.echo,
                run.stderr
            );
            println!(
                "{arm} arm, attempt {attempt}: the computation of {steps} steps answered before an echo of {:?}; run again with {} steps",
                run.echo,
                steps * 2
            );
            steps *= 2;
            doublings += 1;
            continue;
        }
        placements += 1;
        let shown = placed(&run.reports);
        println!(
            "{arm} arm, attempt {attempt}: echo {:?}, {}",
            run.echo,
            if shown { "the arm's placement" } else { "misplaced" }
        );
        if shown {
            return Some(run);
        }
        misplaced.push_str(&format!("attempt {attempt}, echo {:?}:\n{}", run.echo, run.stderr));
    }
    panic!("{arm} arm: no run of {ATTEMPTS} showed the arm's placement:\n{misplaced}");
}

/// The idle driver took a ready context from the entry's driver, and that
/// driver's role was never handed over for ring work, so the echo's
/// context was taken before its receive was submitted.
fn split_drivers(reports: &[DriverReport]) -> bool {
    reports[1].field("stolen_contexts") >= 1 && reports[0].field("ring_reassignments") == 0
}

/// Every run that is not split: either the idle driver took no context, or
/// the computing driver's role was handed over for ring work, after which
/// the idle driver may take the contexts the handoff made ready.
fn same_driver(reports: &[DriverReport]) -> bool {
    !split_drivers(reports)
}

/// Same driver: the idle driver takes no context before both contexts park
/// on the entry's driver, so the echo's receive completes in the computing
/// driver's ring. The echo returns within 100 ms, and the report attributes
/// it to the ring trigger, which handed the computing driver's role over;
/// the idle driver may take the echo's context once the handoff made it
/// ready. Without the trigger the echo waits for the whole computation and
/// fails the bound on its first same-driver run whose computation outlasts
/// the bound.
#[test]
fn an_echo_beside_a_computation_on_its_driver_returns_within_100_ms() {
    let Some(run) = run_arm("same-driver", 0, same_driver) else {
        return;
    };
    assert!(
        run.reports[0].field("ring_reassignments") >= 1,
        "a fast echo on the computing driver without a ring reassignment is not attributed:\n{}",
        run.stderr
    );
}

/// Split: the idle driver takes the echo's context while the entry computes,
/// so the echo is served on a driver of its own on every runtime. It shows
/// that the client and its timing find the stall only where the same-driver
/// arm places it.
#[test]
fn an_echo_on_an_idle_driver_returns_within_100_ms_beside_a_computation() {
    let _ = run_arm("split", SPLIT_SPIN, split_drivers);
}
