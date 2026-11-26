//! PRE-2: `open_files_limit` reads the soft and hard `RLIMIT_NOFILE` limits
//! the shell's `ulimit` reports, a raise above the hard limit is refused, and
//! a raise grows the handle budget so the program holds more files open at
//! once than its starting soft limit allowed.

use std::process::{Command, Stdio};

use super::support::{ProgramChild, build_program, compile_program, fixture_directory};

/// The shell lowers the soft limit to 128 and the hard limit to 4096, prints
/// both with `ulimit` as the independent oracle, then runs the witness, which
/// prints its own reading of both, its soft limit after asking for 4097,
/// and both limits after raising the soft limit to 512, then holds 300 files
/// open at once.
///
/// A reading that differs from `ulimit` fails the first comparison. A raise
/// above the hard limit that is not refused exits 21. A raise that sets the
/// host limit without adding handle credits leaves the budget at 64, so the
/// 65th nested open fails and the witness exits 40; a raise that does not
/// set the host limit prints 128 instead of 512.
#[test]
fn open_files_limit_reads_raises_and_grows_the_handle_budget() {
    let program = build_program(&compile_program("open_files.wf"));
    let directory = fixture_directory();
    std::fs::write(directory.path().join("held"), b"held\n").expect("write the held file");
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "ulimit -Sn 128 && ulimit -Hn 4096 && ulimit -Sn && ulimit -Hn && exec \"$0\"",
        ])
        .arg(program.executable())
        .current_dir(directory.path())
        .env_remove("WF_IO_NO_NATIVE_RING")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = ProgramChild::spawn(&mut command)
        .expect("start the witness")
        .wait_with_output()
        .expect("witness output");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let text = String::from_utf8(output.stdout).expect("decimal witness output");
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 7, "{lines:?}");
    assert_eq!(lines[..2], ["128", "4096"], "the shell's own limits");
    assert_eq!(lines[2..4], lines[..2], "the witness's reading against ulimit");
    assert_eq!(lines[4..], ["128", "512", "4096"], "after the refusal and the raise");
}
