//! Stop requests cross the real host boundary into a compiled waiting context.
//! The C supervisor owns an isolated console on Windows and uses kill on POSIX;
//! it checks host-default status, listener-free entry return, exact stdout,
//! synchronized request order, and cancellation before any host request.
//! Native open-listener return and missing launcher coverage belong to the
//! completion harness.

use std::process::Command;

use super::support::{build_program, compile_program, fixture_directory};
use crate::support::{CLANG, run_command};

#[test]
fn stop_requests_and_default_termination_on_both_host_routes() {
    let llvm = compile_program("stop_signals.wf");
    let program = build_program(&llvm);
    let directory = fixture_directory();
    let source = directory.path().join("stop_signals_driver.c");
    let driver = directory
        .path()
        .join(format!("stop_signals_driver{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&source, include_str!("stop_signals_driver.c")).expect("host oracle source");
    let built = run_command(
        Command::new(CLANG)
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-Wpedantic"])
            .arg(&source)
            .arg("-o")
            .arg(&driver),
    );
    assert!(built.status.success(), "host oracle construction: {built:?}");
    #[cfg(windows)]
    let lifecycle = {
        use crate::conformance::corpus::{self, Expectation};
        use super::support::compile_sources;
        let cases = corpus::load();
        let case = cases
            .iter()
            .find(|case| case.id == "sysstop-run-lifecycle")
            .expect("stop-listener manifest case");
        assert!(matches!(case.expect, Expectation::Run(0)));
        let source = case.source();
        build_program(&compile_sources(&[("case.wf", &source)]))
    };
    for native in [true, false] {
        let mut command = Command::new(&driver);
        command
            .arg(program.executable())
            .current_dir(directory.path())
            .env("WF_DRIVERS", "3")
            .env("WF_WORKERS", "4")
            .env_remove("WF_REQUIRE_WINDOWS_IOCP")
            .env_remove("WF_IO_NO_NATIVE_RING");
        if !native {
            command.env("WF_IO_NO_NATIVE_RING", "1");
        }
        let output = run_command(&mut command);
        assert!(output.status.success(), "stop requests, route {native}: {output:?}");
        assert!(output.stdout.is_empty() && output.stderr.is_empty(), "{output:?}");
        #[cfg(windows)]
        {
            // Reuse the isolated-console supervisor for the normative case.
            // Inputs itself remains usable by processes with no console.
            let mut command = Command::new(&driver);
            command
                .arg(lifecycle.executable())
                .arg("lifecycle")
                .env_remove("WF_REQUIRE_WINDOWS_IOCP")
                .env_remove("WF_IO_NO_NATIVE_RING");
            if !native {
                command.env("WF_IO_NO_NATIVE_RING", "1");
            }
            let result = run_command(&mut command);
            assert!(result.status.success(), "stop lifecycle, route {native}: {result:?}");
            assert!(result.stdout.is_empty() && result.stderr.is_empty(), "{result:?}");
        }
    }
}
