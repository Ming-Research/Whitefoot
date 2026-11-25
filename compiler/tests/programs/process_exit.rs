//! WAIT-2, PRE-2: a call of `std::process::exit_now` ends the execution with
//! its status while other contexts still wait, one on a guard that never
//! holds and one in an hour-long sleep, so no join, release or scope exit
//! runs. The same program given an argument writes the same line and skips
//! only the exit call, and keeps waiting: its outstanding sleep keeps the
//! runtime's stop for an execution that can take no step from ending it.
//! Both runs are timed from the program's line, written just before the exit
//! call, because a loaded host may delay a spawn without bound.

use std::process::Command;
use std::time::{Duration, Instant};

use super::support::{build_program, compile_corpus_module_program};
use crate::support::ProgramChild;

const LIMIT: Duration = Duration::from_secs(1);
/// How long a run may take to reach its line, which only start-up delays.
const START: Duration = Duration::from_secs(60);

#[test]
fn an_exit_ends_the_program_at_once_while_the_same_program_without_it_keeps_waiting() {
    let program = build_program(&compile_corpus_module_program("process_exit", "process_exit"));
    let (mut leaving, leaving_line) =
        ProgramChild::spawn_signalling_first_output(&mut Command::new(program.executable()))
            .expect("start the program that exits");
    let (mut staying, staying_line) = ProgramChild::spawn_signalling_first_output(
        Command::new(program.executable()).arg("stay"),
    )
    .expect("start the program that stays");

    leaving_line
        .recv_timeout(START)
        .expect("the exiting program writes its line before it exits");
    let leaving_wrote = Instant::now();
    staying_line
        .recv_timeout(START)
        .expect("the staying program writes its line where the exit would be");
    let staying_wrote = Instant::now();

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
