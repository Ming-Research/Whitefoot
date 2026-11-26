//! A context that computes without waiting must not hold back the other
//! contexts of its driver [WAIT-2]: a read whose byte has arrived returns, a
//! read whose deadline has passed or whose watch has fired ends, and a ready
//! context starts, while the computation goes on, as long as no more contexts
//! compute than there are drivers.
//!
//! Each witness measures its own lateness inside the program and exits with it
//! in milliseconds (250 for 250 ms or more, 251 and above for an outcome other
//! than the expected one), so a context held until the computation ends reads
//! as 250 and one served on time as a small number. The runtime hands a
//! driver over within two handoff cycles of a monitor period and the time a
//! run may go on before it is asked to yield (1 ms each), so the limit is
//! those 4 ms plus an allowance for a shared hosted runner, unless a case
//! states its own.
//!
//! With two drivers the computation runs beside the waiting context on one
//! driver only if the idle driver did not take it; the scheduler report
//! (`WF_SCHED_REPORT=2`) names what driver 1 stole or borrowed. A case needs a
//! stated number of runs in which the contexts were placed as it intends,
//! makes at most twice that many attempts, and fails with fewer or with any
//! such run over its limit.

use std::process::Output;
use std::time::Duration;

use super::support::{CompiledProgram, build_program, compile_program};

/// Two handoff cycles of 2 ms, and 40 ms for a shared hosted runner.
const HANDOFF_LIMIT_MS: i32 = 4 + 40;

/// One runtime configuration: the I/O route and the driver count.
#[derive(Clone, Copy)]
struct Configuration {
    native_ring: bool,
    drivers: &'static str,
}

/// The io_uring route at one and two drivers, and the readiness route, which
/// runs one driver.
const CONFIGURATIONS: [Configuration; 3] = [
    Configuration { native_ring: true, drivers: "1" },
    Configuration { native_ring: true, drivers: "2" },
    Configuration { native_ring: false, drivers: "1" },
];

impl Configuration {
    fn describe(self) -> String {
        format!(
            "{} at {} driver(s)",
            if self.native_ring { "io_uring" } else { "no ring" },
            self.drivers
        )
    }
}

/// The sum over every driver line of the scheduler report of one counter.
fn driver_total(stderr: &[u8], counter: &str) -> u64 {
    let prefix = format!("{counter}=");
    String::from_utf8_lossy(stderr)
        .lines()
        .filter(|line| line.starts_with("driver: "))
        .filter_map(|line| {
            line.split_whitespace()
                .find_map(|field| field.strip_prefix(prefix.as_str()))
                .map(|count| count.parse::<u64>().expect("a driver counter"))
        })
        .sum()
}

/// A counter of driver 1 in the scheduler report, or zero when the report has
/// no driver 1.
fn second_driver_count(stderr: &[u8], counter: &str) -> u64 {
    let prefix = format!("{counter}=");
    String::from_utf8_lossy(stderr)
        .lines()
        .filter(|line| line.starts_with("driver: index=1 "))
        .find_map(|line| {
            line.split_whitespace()
                .find_map(|field| field.strip_prefix(prefix.as_str()))
                .map(|count| count.parse::<u64>().expect("a driver counter"))
        })
        .unwrap_or(0)
}

/// Whether the computing context stayed on the waiting context's driver:
/// driver 1 neither stole it while it was ready nor completed its first sleep
/// by a borrow, which queues the sleeper on the borrowing driver.
fn computation_stayed(stderr: &[u8]) -> bool {
    second_driver_count(stderr, "stolen_contexts") == 0
        && second_driver_count(stderr, "borrowed_sleeps") == 0
}

/// Makes runs until `wanted` of them qualify, at most twice that many, and
/// answers a failure when a qualifying run's status is not within `0..=limit`
/// or too few runs qualified.
fn require(
    case: &str,
    wanted: usize,
    limit: i32,
    mut run: impl FnMut() -> Output,
    qualifies: impl Fn(&Output) -> bool,
) -> Option<String> {
    let mut seen = Vec::new();
    let mut qualified = 0;
    for _ in 0..2 * wanted {
        let output = run();
        let counts = qualifies(&output);
        let code = output.status.code();
        let drivers = String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| line.starts_with("driver: "))
            .collect::<Vec<_>>()
            .join(" | ");
        seen.push(format!("{code:?}{} [{drivers}]", if counts { "" } else { " (not placed)" }));
        if counts {
            if !matches!(code, Some(value) if (0..=limit).contains(&value)) {
                return Some(format!(
                    "{case}: a run ended with {code:?}, limit {limit}:\n  {}\n{}",
                    seen.join("\n  "),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            qualified += 1;
            if qualified == wanted {
                eprintln!("{case}: {wanted} runs within {limit}:\n  {}", seen.join("\n  "));
                return None;
            }
        }
    }
    Some(format!(
        "{case}: {qualified} of {} runs placed as intended, {wanted} needed:\n  {}",
        seen.len(),
        seen.join("\n  ")
    ))
}

/// Requires `wanted` runs of a standard-input witness in every configuration,
/// the byte written `late_byte` after the program reports it is about to
/// read; a two-driver run qualifies when the computation stayed beside the
/// reader.
fn require_beside_reader(
    program: &CompiledProgram,
    name: &str,
    wanted: usize,
    limit: i32,
    late_byte: Duration,
) {
    let failures = CONFIGURATIONS
        .iter()
        .filter_map(|configuration| {
            require(
                &format!("{name}, {}", configuration.describe()),
                wanted,
                limit,
                || {
                    program.run_with_input_after_first_output(
                        b"z",
                        late_byte,
                        configuration.native_ring,
                        &[("WF_DRIVERS", configuration.drivers), ("WF_SCHED_REPORT", "2")],
                    )
                },
                |output| configuration.drivers == "1" || computation_stayed(&output.stderr),
            )
        })
        .collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The byte comes 100 ms after the program reports it is about to read, so
/// the read's own duration, its exit status, is 100 ms plus its lateness.
#[test]
fn a_read_whose_byte_arrives_returns_beside_a_computation() {
    let program = build_program(&compile_program("read_beside_computation.wf"));
    require_beside_reader(&program, "read", 64, 100 + HANDOFF_LIMIT_MS, Duration::from_millis(100));
}

/// The read's deadline is 50 ms away while the other context would compute for
/// 2 s, and the byte comes 300 ms later; the read ends within 100 ms after the
/// deadline.
#[test]
fn a_read_whose_deadline_passes_ends_beside_a_computation() {
    let program = build_program(&compile_program("deadline_beside_computation.wf"));
    require_beside_reader(&program, "deadline", 10, 150, Duration::from_millis(300));
}

/// The computing context fires the read's watch once the read has begun and
/// then computes; the read ends `Cancelled` within 100 ms of the firing, and
/// the byte comes 300 ms after the program reports it is about to read.
#[test]
fn a_fired_watch_ends_a_read_beside_a_computation() {
    let program = build_program(&compile_program("cancel_beside_computation.wf"));
    require_beside_reader(&program, "cancel", 10, 100, Duration::from_millis(300));
}

/// As many contexts compute as there are drivers once a guard is let go, and
/// the context waiting on it must start. On two drivers a run qualifies when
/// the report shows a role handed to a spare, since the contexts are
/// otherwise placed wherever the idle driver steals them.
#[test]
fn a_ready_context_starts_beside_computations_on_every_driver() {
    let program = build_program(&compile_program("ready_beside_computations.wf"));
    let failures = ["1", "2"]
        .into_iter()
        .filter_map(|drivers| {
            require(
                &format!("ready, {drivers} driver(s)"),
                64,
                HANDOFF_LIMIT_MS,
                || {
                    program.run_with_settings(
                        None,
                        &[("WF_DRIVERS", drivers), ("WF_SCHED_REPORT", "2")],
                    )
                },
                |output| drivers == "1" || driver_total(&output.stderr, "reassignments") > 0,
            )
        })
        .collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
