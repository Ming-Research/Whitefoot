//! PRE-2's execution input must match the runtime roles actually started.
//! The WF witness reads the field before its first spawn, prints it on both
//! sides of the join, and forces lazy driver startup. The old library rejects
//! its field access; a configured-value or late-recomputed implementation
//! disagrees with these outputs and the separately published driver reports.

use std::process::Command;

#[cfg(target_os = "linux")]
use super::support::CompiledProgram;
use super::support::{build_program, compile_program};
use crate::support::run_command;

/// A host without the ring runs one driver even when two were requested.
/// This fails a field that reports the configured count or zero; today's
/// pre-amendment compiler cannot construct the witness because drivers is absent.
#[test]
fn inputs_report_one_driver_without_the_kernel_ring() {
    let program = build_program(&compile_program("driver_count.wf"));
    let output = run_command(
        Command::new(program.executable())
            .env("WF_DRIVERS", "2")
            .env("WF_IO_NO_NATIVE_RING", "1")
            .env("WF_IO_HELPERS", "0")
            .env_remove("WF_WORKERS")
            .env_remove("WF_SCHED_REPORT"),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"01\n01\n", "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[cfg(target_os = "linux")]
fn three_affinity_cpus() -> String {
    let status = std::fs::read_to_string("/proc/self/status").expect("read allowed CPU list");
    let allowed = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .expect("kernel CPU affinity list");
    let mut cpus = Vec::new();
    for interval in allowed.trim().split(',') {
        let mut ends = interval.split('-');
        let first: u32 = ends
            .next()
            .expect("CPU interval start")
            .parse()
            .expect("CPU number");
        let last: u32 = ends
            .next()
            .map_or(first, |end| end.parse().expect("CPU range end"));
        cpus.extend((first..=last).take(3 - cpus.len()));
        if cpus.len() == 3 {
            break;
        }
    }
    assert_eq!(
        cpus.len(),
        3,
        "io-hosts driver criterion requires three available CPUs: {allowed}"
    );
    cpus.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(target_os = "linux")]
fn affinity_command(program: &CompiledProgram, cpus: &str, drivers: Option<&str>) -> Command {
    let mut command = Command::new("taskset");
    command
        .args(["--cpu-list", cpus])
        .arg(program.executable())
        .env_remove("WF_DRIVERS")
        .env_remove("WF_IO_NO_NATIVE_RING")
        .env_remove("WF_WORKERS")
        .env("WF_IO_HELPERS", "0")
        .env("WF_SCHED_REPORT", "2");
    if let Some(count) = drivers {
        command.env("WF_DRIVERS", count);
    }
    command
}

/// firn's criterion on a required io_uring CI host: explicit two, affinity
/// default three, and zero/default three must equal actual published driver
/// roles. WF_SCHED_REPORT=2 emits each role only after its executor starts,
/// independently of the field's frozen selection. The negative values must
/// fail with the existing parser diagnostic before the entry prints anything.
/// A missing field fails construction on today's compiler; a count selected
/// from host-wide CPUs or echoing WF_DRIVERS disagrees with the native oracle.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires io_uring and three affinity CPUs; completion-linux runs this case"]
fn driver_count_matches_started_roles_under_three_cpu_affinity() {
    let program = build_program(&compile_program("driver_count.wf"));
    let cpus = three_affinity_cpus();
    for (setting, expected) in [(Some("2"), 2), (None, 3), (Some("0"), 3)] {
        let output = run_command(&mut affinity_command(&program, &cpus, setting));
        assert_eq!(
            output.status.code(),
            Some(0),
            "WF_DRIVERS={setting:?}: {output:?}"
        );
        let text = String::from_utf8(output.stdout.clone()).expect("decimal count output");
        let counts = text
            .lines()
            .map(|line| line.parse::<u64>().expect("driver count"))
            .collect::<Vec<_>>();
        assert_eq!(
            counts,
            [expected, expected],
            "WF_DRIVERS={setting:?}: {output:?}"
        );
        let reports = String::from_utf8(output.stderr.clone()).expect("driver reports");
        let indices = reports
            .lines()
            .filter_map(|line| line.strip_prefix("driver: index="))
            .map(|line| {
                line.split_whitespace()
                    .next()
                    .expect("driver index")
                    .parse::<u64>()
                    .expect("numeric driver index")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            indices,
            (0..expected).collect::<Vec<_>>(),
            "missing native ring or wrong started count, WF_DRIVERS={setting:?}: {output:?}"
        );
        assert_eq!(
            counts[0],
            indices.len() as u64,
            "field differs from started roles"
        );
    }
    for setting in ["-1", "65"] {
        let output = run_command(&mut affinity_command(&program, &cpus, Some(setting)));
        assert_eq!(
            output.status.code(),
            Some(1),
            "WF_DRIVERS={setting}: {output:?}"
        );
        assert!(
            output.stdout.is_empty(),
            "invalid count reached entry: {output:?}"
        );
        assert_eq!(
            output.stderr,
            b"whitefoot scheduler: WF_DRIVERS must be an integer from 0 through 64\n",
            "WF_DRIVERS={setting}: {output:?}"
        );
    }
}
