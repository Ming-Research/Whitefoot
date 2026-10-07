//! The file-replacement and writable-subdirectory witnesses on both Windows host routes.
//! The Unix corpus adapter already builds and runs every manifest case;
//! programs::file_replacement also covers replacement on the native-disabled route.
//! Windows's ordinary corpus adapter does not yet run the full corpus.
//! Default engine selection and native-disabled execution both carry these
//! namespace requests through the shared file adapter.

#![cfg(windows)]

use std::process::Command;

use super::corpus::{self, Expectation};
use crate::programs::support::{build_program, compile_sources, fixture_directory};
use crate::support::run_command;

#[test]
fn namespace_cases_on_both_host_routes() {
    let cases = corpus::load();
    for id in [
        "sysreplace-run-rename-replaces",
        "sysreplace-run-rename-missing",
        "sysreplace-run-remove-twice",
        "sysreplace-run-open-handles",
        "sysreplace-run-sync-directory",
        "sysubdir-run-create",
        "sysubdir-run-existing",
        "sysubdir-run-file-error",
        "sysubdir-run-namespace",
    ] {
        let case = cases
            .iter()
            .find(|case| case.id == id)
            .expect("manifest case");
        let Expectation::Run(expected) = case.expect else {
            panic!("{id} must retain its executable verdict");
        };
        let source = case.source();
        let llvm = compile_sources(&[("case.wf", &source)]);
        let program = build_program(&llvm);
        let arrangement = case.arrange.as_ref().expect("native argument names");
        let argv = arrangement.argv.as_ref().expect("argument vector");
        // These ASCII fixture names are host strings, copied by the WF case
        // into POSIX bytes or Windows UTF-16 bytes through host_copy_bytes.
        let arguments: Vec<_> = argv
            .iter()
            .skip(1)
            .map(|bytes| std::str::from_utf8(bytes).expect("textual fixture argument"))
            .collect();
        assert!(
            arrangement.files.is_empty()
                && arrangement.stdin.is_none()
                && arrangement.redirect.is_empty()
        );
        for native in [true, false] {
            let directory = fixture_directory();
            let mut command = Command::new(program.executable());
            command
                .current_dir(directory.path())
                .args(&arguments)
                .env_remove("WF_REQUIRE_WINDOWS_IOCP")
                .env_remove("WF_IO_NO_NATIVE_RING");
            if !native {
                command.env("WF_IO_NO_NATIVE_RING", "1");
            }
            let output = run_command(&mut command);
            assert_eq!(
                output.status.code(),
                Some(expected),
                "{id}, route {native}: {output:?}"
            );
            assert!(output.stdout.is_empty() && output.stderr.is_empty());
        }
    }
}
