//! A stopped AOF rewrite leaves the named log whole on the next invocation.
//! This kills a process, not its host; no power-failure durability is inferred.
//! Settings exercise default engine selection and native-disabled execution;
//! namespace requests use the shared adapter in both.

use std::process::{Command, Stdio};

use super::support::{build_program, compile_program, fixture_directory};
use crate::support::{PROGRAM_DEADLINE, ProgramChild, run_command};

#[test]
fn a_log_rewrite_restarts_at_each_namespace_boundary_on_both_routes() {
    let llvm = compile_program("file_replacement.wf");
    let program = build_program(&llvm);
    for native in [true, false] {
        for (stage, expected) in [("1", b"old\n"), ("2", b"new\n"), ("3", b"new\n")] {
            let directory = fixture_directory();
            let log = directory.path().join("log");
            std::fs::write(&log, b"old\n").expect("write the log");
            // A leftover temporary file must be truncated before the rewrite.
            let temporary = directory.path().join("tmp");
            std::fs::write(&temporary, b"a stale, longer rewrite\n").expect("write the leftover");
            let command = || {
                let mut command = Command::new(program.executable());
                command
                    .current_dir(directory.path())
                    .env_remove("WF_REQUIRE_WINDOWS_IOCP")
                    .env_remove("WF_IO_NO_NATIVE_RING");
                if !native {
                    command.env("WF_IO_NO_NATIVE_RING", "1");
                }
                command
            };
            let mut rewrite = command();
            rewrite
                .args(["log", "tmp", stage])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let (mut child, reached) =
                ProgramChild::spawn_signalling_first_output(&mut rewrite).expect("start rewrite");
            let input = child.take_stdin().expect("keep checkpoint input open");
            reached
                .recv_timeout(PROGRAM_DEADLINE)
                .expect("rewrite must reach its selected checkpoint");
            assert!(child.try_wait().expect("inspect rewrite").is_none());
            // Drop kills and reaps the owned process without letting it execute
            // cleanup or the next rewrite step. The input pipe stays open.
            drop(child);
            drop(input);
            assert_eq!(std::fs::read(&log).expect("log remains named"), expected);
            if stage == "1" {
                assert_eq!(std::fs::read(&temporary).expect("temporary"), b"new\n");
            } else {
                assert!(!temporary.exists(), "rename consumes the temporary name");
            }
            let restart = run_command(command().args(["log", "tmp", "0"]));
            assert!(
                restart.status.success(),
                "route {native}, stage {stage}: {restart:?}"
            );
            assert!(restart.stdout.is_empty() && restart.stderr.is_empty());

            // The restart oracle refuses absent, partial and trailing data.
            for wrong in [&b""[..], &b"ne"[..], &b"bad\n"[..], &b"old\njunk"[..]] {
                std::fs::write(&log, wrong).expect("install broken log");
                let refused = run_command(command().args(["log", "tmp", "0"]));
                assert!(!refused.status.success(), "accepted partial log {wrong:?}");
            }
            std::fs::remove_file(&log).expect("remove log");
            let absent = run_command(command().args(["log", "tmp", "0"]));
            assert!(!absent.status.success(), "accepted absent log");
        }
    }
}
