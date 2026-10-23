//! Real Windows arguments, library ABI and shipped CLI construction.
//! Native worker and resource-floor observations belong to the native group.

use std::process::Command;

use super::support::{build_cli_program, fixture_directory, run_command};

#[test]
fn relative_paths_cross_the_cli_and_required_iocp_boundary() {
    let program = build_cli_program("completion_read_boundary.wf", false);
    let directory = fixture_directory();
    directory.write_nested("first.bin", b"A");
    directory.write_nested("second.bin", b"B");
    let mut command = Command::new(program.executable());
    command
        .current_dir(directory.path())
        .args(["first.bin", "second.bin"])
        .env_remove("WF_IO_NO_NATIVE_RING")
        .env("WF_REQUIRE_WINDOWS_IOCP", "1");
    let output = run_command(&mut command);
    assert!(output.status.success(), "required IOCP: {output:?}");
    assert_eq!(output.stdout, b"AB");
    assert!(output.stderr.is_empty());

    // Correct bytes through the helper cannot satisfy a required-native claim.
    command.env("WF_IO_NO_NATIVE_RING", "1");
    let refused = run_command(&mut command);
    assert!(!refused.status.success());
    assert_eq!(refused.stdout, b"AB");
    assert_eq!(refused.stderr,
        b"whitefoot completion: WF_REQUIRE_WINDOWS_IOCP was set but native IOCP was unavailable, unused, or incomplete\r\n");
}

#[test]
fn argument_name_bytes_select_the_native_components() {
    let program = build_cli_program("windows/component_open.wf", false);
    let directory = fixture_directory();
    directory.write_nested("\u{4241}", b"W");
    directory.write_nested("\u{4242}", b"X");
    directory.write_nested("AB", b"D");
    directory.write_nested("BB", b"E");
    let output = run_command(
        Command::new(program.executable())
            .current_dir(directory.path())
            .args(["\u{4241}", "\u{4242}"])
            .env_remove("WF_IO_NO_NATIVE_RING")
            .env("WF_REQUIRE_WINDOWS_IOCP", "1"),
    );
    assert!(
        output.status.success(),
        "native component arguments: {output:?}"
    );
    assert_eq!(output.stdout, b"WX");
    assert!(output.stderr.is_empty());
}

#[test]
fn both_layout_folds_preserve_the_complete_program_result() {
    for parallel in [false, true] {
        let program = build_cli_program("par_layout.wf", parallel);
        let output = program.run_with_workers(Some("4"));
        assert!(output.status.success(), "parallel {parallel}: {output:?}");
        assert_eq!(output.stdout, b"420a993efa7437a1 41fa962893d45299\n");
        assert!(output.stderr.is_empty());
    }
}
