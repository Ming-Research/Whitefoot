//! A context that computes without waiting must not hold back the other
//! contexts of its driver [WAIT-2]: a read whose byte has arrived returns, and
//! a read whose deadline has passed ends, while the computation goes on.
//!
//! Each witness starts a context that computes for up to three seconds
//! without reaching a wait, then reads standard input. Its exit status is the
//! read's own duration in tens of milliseconds (250 for 2.5 s or more), so a
//! read held until the computation ends reads as 250 and one served on time
//! as a small number. With two drivers the computation runs beside the read on
//! the root's driver unless the idle driver stole it; the scheduler report
//! (`WF_SCHED_REPORT=2`) names the contexts driver 1 stole, and only runs in
//! which it stole none put both contexts on one driver.

use std::time::Duration;

use super::support::{CompiledProgram, build_program, compile_program};

const RUNS: usize = 6;

/// A counter of driver 1 in a scheduler report on standard error, or `None`
/// when the report has no driver 1.
fn second_driver_count(stderr: &[u8], counter: &str) -> Option<u64> {
    let text = String::from_utf8_lossy(stderr);
    let prefix = format!("{counter}=");
    text.lines()
        .filter(|line| line.starts_with("driver: index=1 "))
        .find_map(|line| {
            line.split_whitespace()
                .find_map(|field| field.strip_prefix(prefix.as_str()))
                .map(|count| count.parse::<u64>().expect("a driver counter"))
        })
}

/// Whether driver 1 took the computing context: it stole it while it was
/// ready, or completed its first sleep by a borrow and so queued it on itself.
fn computation_moved(stderr: &[u8]) -> bool {
    second_driver_count(stderr, "stolen_contexts").unwrap_or(0) != 0
        || second_driver_count(stderr, "borrowed_sleeps").unwrap_or(0) != 0
}

/// Runs `program` `RUNS` times and returns, for each run, its exit status and
/// whether both contexts shared one driver.
fn runs(
    program: &CompiledProgram,
    native_ring: bool,
    drivers: &str,
    late_byte: Duration,
) -> Vec<(Option<i32>, bool, String)> {
    (0..RUNS)
        .map(|_| {
            let output = program.run_with_input_after_first_output(
                b"z",
                late_byte,
                native_ring,
                &[("WF_DRIVERS", drivers), ("WF_SCHED_REPORT", "2")],
            );
            let shared = drivers == "1" || !computation_moved(&output.stderr);
            (
                output.status.code(),
                shared,
                String::from_utf8_lossy(&output.stderr).into_owned(),
            )
        })
        .collect()
}

/// Describes the runs of one configuration, and answers a failure when no
/// run put both contexts on one driver or a run that did ended over `limit`.
fn judge(case: &str, results: &[(Option<i32>, bool, String)], limit: i32) -> Option<String> {
    let summary = results
        .iter()
        .map(|(code, shared, stderr)| {
            let drivers = stderr
                .lines()
                .filter(|line| line.starts_with("driver: "))
                .collect::<Vec<_>>()
                .join(" | ");
            format!("{code:?}{} [{drivers}]", if *shared { "" } else { " (split)" })
        })
        .collect::<Vec<_>>()
        .join("\n  ");
    eprintln!("{case}:\n  {summary}");
    if !results.iter().any(|(_, shared, _)| *shared) {
        return Some(format!("{case}: no run put both contexts on one driver: {summary}"));
    }
    results
        .iter()
        .find(|(code, shared, _)| {
            *shared && !matches!(code, Some(value) if (0..=limit).contains(value))
        })
        .map(|(code, _, stderr)| {
            format!("{case}: a run on one driver ended with {code:?}, limit {limit}: {summary}\n{stderr}")
        })
}

/// The byte comes 100 ms after the program reports it is about to read; a
/// read served while the computation goes on returns within 260 ms.
#[test]
fn a_read_whose_byte_arrives_returns_beside_a_computation() {
    let program = build_program(&compile_program("read_beside_computation.wf"));
    let mut failures = Vec::new();
    for native_ring in [true, false] {
        for drivers in ["1", "2"] {
            let results = runs(&program, native_ring, drivers, Duration::from_millis(100));
            failures.extend(judge(
                &format!("read, native ring {native_ring}, drivers {drivers}"),
                &results,
                25,
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The read's deadline is 50 ms away and the byte comes a second later; a
/// deadline served while the computation goes on ends the read within
/// 150 ms, 100 ms after the deadline.
#[test]
fn a_read_whose_deadline_passes_ends_beside_a_computation() {
    let program = build_program(&compile_program("deadline_beside_computation.wf"));
    let mut failures = Vec::new();
    for native_ring in [true, false] {
        for drivers in ["1", "2"] {
            let results = runs(&program, native_ring, drivers, Duration::from_secs(1));
            failures.extend(judge(
                &format!("deadline, native ring {native_ring}, drivers {drivers}"),
                &results,
                15,
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
