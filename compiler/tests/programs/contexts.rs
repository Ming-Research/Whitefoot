//! The context runtime's part of [WAIT-2]'s progress promise: while every
//! context keeps reaching a wait, a context whose wait has ended proceeds, and
//! a program that can take no further step is stopped with a report. Each
//! program here stopped, or ran on without end, on the runtime before the
//! change that makes it pass, on the route or driver count it names
//! (`research/investigations/io-model/CONCURRENCY-MODEL.md`, section 10.4).

use super::support::{build_program, compile_program, compile_sources};

/// The routes and driver counts a program runs under: the kernel ring on
/// one driver and on four, and the shared file adapter, where one driver
/// runs. A host with no ring runs every one of them on the adapter.
const ROUTES: [&[(&str, &str)]; 3] = [
    &[("WF_DRIVERS", "1")],
    &[("WF_DRIVERS", "4")],
    &[("WF_IO_NO_NATIVE_RING", "1")],
];

/// R1: two contexts joined by one pipe. The reader starts first, and the
/// writer asks for four times what a pipe holds in one request, so the
/// write finishes only while the reader's context runs. Before operations
/// the ring does not carry went to helper threads once contexts run, the
/// writer's call blocked the thread every context shared, on every route.
#[test]
fn a_pipe_between_two_contexts_drains_on_every_route() {
    let program = build_program(&compile_program("pipe_contexts.wf"));
    for settings in ROUTES {
        for round in 0..2 {
            let output = program.run_with_own_pipe(settings);
            assert_eq!(
                output.status.code(),
                Some(0),
                "{settings:?}, round {round}: {output:?}"
            );
        }
    }
}

/// R2: the entry polls an object for the write of a context it spawned.
/// Every poll is an atomic statement the object answers at once, so the
/// entry never suspends; before a run of such waits yielded to a ready
/// context, one driver never ran the producer.
#[test]
fn a_context_polling_an_object_lets_the_producer_run_on_one_driver() {
    let program = build_program(&compile_program("poll_contexts.wf"));
    for drivers in ["1", "4"] {
        for round in 0..3 {
            let output = program.run_with_settings(None, &[("WF_DRIVERS", drivers)]);
            assert_eq!(
                output.status.code(),
                Some(0),
                "drivers {drivers}, round {round}: {output:?}"
            );
        }
    }
}

/// R3: two contexts hand a guard back and forth and stop only after a third
/// context's read completes. The two are never both waiting, so a driver
/// that looked for host completions only when nothing was ready never
/// reaped that read, on every route.
#[test]
fn a_host_completion_reaches_its_context_while_others_stay_ready() {
    let program = build_program(&compile_program("busy_contexts.wf"));
    for settings in ROUTES {
        for round in 0..2 {
            let output = program.run_with_file_input_and_settings(b"ready\n", settings);
            assert_eq!(
                output.status.code(),
                Some(0),
                "{settings:?}, round {round}: {output:?}"
            );
        }
    }
}

const GUARD_CYCLE: &[u8] = b"struct Flags {
  left: u64;
  right: u64;
}

fn left_side(flags: Shared<Flags>) -> result: unit pure waits {
  atomic state = &flags when state^.right != 0_u64 {
    set state^.left = 1_u64;
  }
  return unit;
}

fn right_side(flags: Shared<Flags>) -> result: unit pure waits {
  atomic state = &flags when state^.left != 0_u64 {
    set state^.right = 1_u64;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let start = Flags(left: 0_u64, right: 0_u64);
  let flags = shared_new::<Flags>(value: start);
  let first = shared_share::<Flags>(shared: &flags);
  let second = shared_share::<Flags>(shared: &flags);
  spawn left_side(flags: move first);
  spawn right_side(flags: move second);
  return std::process::exit_status(code: 0_u8);
}
";

const OWN_GUARD: &[u8] = b"fn wait_then_set(cell: Shared<u64>) -> result: unit pure waits {
  atomic value = &cell when value^ != 0_u64 {
    set value^ = 2_u64;
  }
  atomic value = &cell {
    set value^ = 1_u64;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let cell = shared_new::<u64>(value: 0_u64);
  let handle = shared_share::<u64>(shared: &cell);
  spawn wait_then_set(cell: move handle);
  return std::process::exit_status(code: 0_u8);
}
";

/// R4: a program in which every context waits for a guard or for another
/// context, with no host operation outstanding, can take no further step,
/// and the runtime stops it with its report on one driver and on four.
/// Before the stop was found on every driver, the entry's driver reported it
/// only while it ran alone, and four drivers waited without end.
#[test]
fn a_program_that_can_take_no_step_stops_with_a_report_on_every_driver_count() {
    for (name, source) in [("guard_cycle.wf", GUARD_CYCLE), ("own_guard.wf", OWN_GUARD)] {
        let program = build_program(&compile_sources(&[(name, source)]));
        for drivers in ["1", "4"] {
            let output = program.run_with_settings(None, &[("WF_DRIVERS", drivers)]);
            assert_ne!(
                output.status.code(),
                Some(0),
                "{name}, drivers {drivers}: {output:?}"
            );
            let report = String::from_utf8_lossy(&output.stderr);
            assert!(
                report.contains("every context waits for a guard or for another context"),
                "{name}, drivers {drivers}: {report}"
            );
        }
    }
}
