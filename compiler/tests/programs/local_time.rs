//! PRE-2: `Inputs.process_id` is the identifier the host gave the process it
//! started, and `local_offset_seconds` answers the local time the host's
//! zone data gives for the second at or before an instant, under the zone
//! the `TZ` environment variable names. The expected offsets are the
//! published IANA rules, independent of the runtime and of the C library the
//! runtime calls.

use std::process::{Command, Stdio};

use super::support::{CompiledProgram, ProgramChild, build_program, compile_program};

/// Runs the witness with `TZ` set to `zone` and returns the identifier of the
/// process the host started together with the lines the witness printed.
fn run_in_zone(program: &CompiledProgram, zone: &str) -> (u32, Vec<String>) {
    let mut command = Command::new(program.executable());
    command
        .env("TZ", zone)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = ProgramChild::spawn(&mut command).expect("start the witness");
    let started = child.id();
    let output = child.wait_with_output().expect("witness output");
    assert_eq!(output.status.code(), Some(0), "TZ={zone}: {output:?}");
    assert!(output.stderr.is_empty(), "TZ={zone}: {output:?}");
    let text = String::from_utf8(output.stdout).expect("decimal witness output");
    (started, text.lines().map(str::to_owned).collect())
}

/// The witness prints its process identifier, then the offsets at
/// 2026-01-15T12:00Z, at 2026-07-01T12:00Z and one nanosecond before
/// 1969-10-26T06:00Z, when New York left daylight saving time.
///
/// A field filled with a constant, a parent's or a thread's identifier fails
/// the first line. An offset with the wrong sign, one that ignores `TZ`, or
/// one that ignores daylight saving time fails the zone lines; New York's
/// summer line fails a runtime that applies standard time all year. The last
/// instant's second is 1969-10-26T05:59:59Z, still daylight time, so a
/// runtime that truncates the negative nanoseconds toward zero converts
/// 06:00:00Z instead and prints -18000 for New York.
#[test]
fn process_id_and_local_offsets_match_the_host() {
    let program = build_program(&compile_program("local_time.wf"));
    for (zone, offsets) in [
        ("UTC", ["0", "0", "0"]),
        ("Asia/Shanghai", ["28800", "28800", "28800"]),
        ("America/New_York", ["-18000", "-14400", "-14400"]),
    ] {
        let (started, lines) = run_in_zone(&program, zone);
        assert_eq!(lines.len(), 4, "TZ={zone}: {lines:?}");
        assert_eq!(lines[0], started.to_string(), "TZ={zone}: process id");
        assert_eq!(lines[1..], offsets, "TZ={zone}: offsets");
    }
}
