//! WAIT-2, PRE-2: a call of `std::process::exit_now` ends the execution with
//! its status while other contexts still wait, one on a guard that never
//! holds and one in an hour-long sleep, so no join, release or scope exit
//! runs. The same program given an argument writes the same line and skips
//! only the exit call, and keeps waiting: its outstanding sleep keeps the
//! runtime's stop for an execution that can take no step from ending it.
//! Both runs are timed from the program's line, written just before the exit
//! call, because a loaded host may delay a spawn without bound.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::support::{build_program, compile_corpus_module_program};
use crate::support::ProgramChild;

const LIMIT: Duration = Duration::from_secs(1);
/// How long a run may take to reach its line, which only start-up delays.
const START: Duration = Duration::from_secs(60);

#[test]
fn an_exit_ends_the_program_at_once_while_the_same_program_without_it_keeps_waiting() {
    let program = build_program(&compile_corpus_module_program("process_exit", "process_exit"));
    let run = |arguments: &[&str]| {
        let mut command = Command::new(program.executable());
        command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    };
    let (mut leaving, leaving_line) = ProgramChild::spawn_signalling_first_output(&mut run(&[]))
        .expect("start the program that exits");
    let (mut staying, staying_line) =
        ProgramChild::spawn_signalling_first_output(&mut run(&["stay"]))
            .expect("start the program that stays");

    // The exiting run is polled from its line on, before the staying run's line
    // is awaited, so a late line from the other run cannot delay seeing the
    // exit. The staying run's line is read later than it arrived, if anything,
    // which only lengthens the time it is shown to keep running.
    leaving_line
        .recv_timeout(START)
        .expect("the exiting program writes its line before it exits");
    let leaving_wrote = Instant::now();
    let status = loop {
        if let Some(status) = leaving.try_wait().expect("poll the exiting program") {
            break status;
        }
        let elapsed = leaving_wrote.elapsed();
        assert!(
            elapsed <= LIMIT,
            "the program had not ended {elapsed:?} after the line written before its exit call"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    let ended = leaving_wrote.elapsed();
    assert!(ended <= LIMIT, "the program ended {ended:?} after its line");
    assert_eq!(status.code(), Some(1), "the exit's status, {ended:?} after the line");
    let output = leaving.wait_with_output().expect("collect the exiting program's output");
    assert_eq!(output.stdout, b"ending\n", "{output:?}");

    staying_line
        .recv_timeout(START)
        .expect("the staying program writes its line where the exit would be");
    let staying_wrote = Instant::now();
    while staying_wrote.elapsed() < LIMIT {
        let state = staying.try_wait().expect("poll the staying program");
        assert!(
            state.is_none(),
            "the program without the exit call ended with {state:?} {:?} after its line",
            staying_wrote.elapsed()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        staying.try_wait().expect("poll the staying program").is_none(),
        "the program without the exit call ended {:?} after its line",
        staying_wrote.elapsed()
    );
    // Dropping the child stops its process group.
    drop(staying);
}

/// WAIT-2: when several contexts call `exit_now`, the first call to execute
/// ends the execution with its status, and a host operation whose call has
/// completed keeps its effect; where a context executes is not observable, so
/// the outcome holds on one driver and on four alike. Four contexts released
/// by one atomic statement race their exits, statuses 11 to 14, beside a
/// context waiting on a guard that never holds; the entry writes its line
/// before the release and would return status 0. Every run must end with one
/// of the four statuses and keep the line. The race is repeated on four
/// drivers, where the exits run on different threads at once.
#[test]
fn of_several_racing_exits_one_ends_the_program_with_its_status_on_one_driver_and_on_four() {
    let program = build_program(&compile_corpus_module_program(
        "process_exit_race",
        "process_exit_race",
    ));
    for (drivers, runs) in [("1", 2), ("4", 16)] {
        for run in 0..runs {
            let mut command = Command::new(program.executable());
            command
                .env_remove("WF_WORKERS")
                .env("WF_DRIVERS", drivers)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let started = Instant::now();
            let output = ProgramChild::spawn(&mut command)
                .expect("start the racing exits")
                .wait_with_output()
                .expect("the racing exits end the program");
            let code = output.status.code();
            assert!(
                matches!(code, Some(11..=14)),
                "drivers {drivers}, run {run}: the status must be one exit's, after {:?}: {output:?}",
                started.elapsed()
            );
            assert_eq!(output.stdout, b"go\n", "drivers {drivers}, run {run}: {output:?}");
        }
    }
}
