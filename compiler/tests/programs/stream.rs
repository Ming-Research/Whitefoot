//! The invocation's ordinary standard input stream, end to end [PRE-2].
//!
//! `stdin_echo.wf` reads the stream in `Inputs.stdin` to its end with `read_next` and
//! publishes every byte it observed, so one run exercises both halves of the
//! stream pair against a real host. The two shapes a standard input takes are
//! different runtime paths and both are run here: a pipe, whose end the writer
//! decides and whose reads may be short, and a redirected regular file, which
//! the kernel completes without a readiness wait.
//!
//! Each shape is run on both routes the host has. The shipped default reaches
//! the Linux completion ring, which carries the stream read as a read at
//! offset -1; `WF_IO_NO_NATIVE_RING` runs the same program through the shared
//! file adapter's own `read`. The two must agree byte for byte, because the
//! route is an implementation choice and not a language one.

use super::support::{build_program, compile_program, emitted_function};

/// Larger than the program's 4096-byte chunk, so every run makes several
/// `read_next` calls and the program's own loop, not one lucky call, is what
/// reaches the end.
fn payload() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(10_000);
    for index in 0..10_000_u32 {
        bytes.push(u8::try_from(index % 251).expect("a byte"));
    }
    bytes
}

#[test]
fn the_stream_uses_ordinary_linked_calls_and_an_ordinary_inputs_argument() {
    let llvm = compile_program("stdin_echo.wf");
    // The source calls ordinary PRE-2 signatures. Native submission and join
    // belong to their linked bodies and cannot select a compiler call path.
    for (caller, callee) in [("main", "read_next"), ("publish_all", "write_once")] {
        let body = emitted_function(&llvm, caller);
        assert_eq!(
            body.matches(&format!("call void @wf_std.io.{callee}("))
                .count(),
            1
        );
        assert_eq!(
            llvm.lines()
                .filter(|line| line.starts_with(&format!("declare void @wf_std.io.{callee}(")))
                .count(),
            1
        );
    }
    assert!(!llvm.contains("call void @wf_std.fs.read_at("));
    assert!(!llvm.contains("@wf__completion_"));
    // Build initialization supplies one ordinary Inputs owner, then receives
    // the ordinary opaque ExitStatus through its result destination. The
    // launcher does not open either standard stream. The heap is ambient and
    // has no source spelling [STOR-8], so the entry call carries no store
    // argument beside the inputs.
    let entry = emitted_function(&llvm, "_main_body");
    assert!(entry.contains("call i32 @wf__ordinary_inputs(ptr %inputs, i32 %argc, ptr %argv)"));
    assert!(entry.contains("call void @\"wf_main\"(ptr %status, ptr %inputs)"));
    assert!(!entry.contains("@open"));
}

#[test]
fn a_piped_standard_input_is_echoed_to_its_end_on_both_routes() {
    let llvm = compile_program("stdin_echo.wf");
    let program = build_program(&llvm);
    let bytes = payload();
    for native_ring in [true, false] {
        let output = program.run_with_piped_input(&bytes, native_ring);
        assert!(
            output.status.success(),
            "the echo must reach the end of a pipe (native ring: {native_ring}): {:?}",
            output.status
        );
        assert_eq!(output.stdout, bytes, "native ring: {native_ring}");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn a_redirected_file_is_echoed_to_its_end_on_both_routes() {
    let llvm = compile_program("stdin_echo.wf");
    let program = build_program(&llvm);
    let bytes = payload();
    for native_ring in [true, false] {
        let output = program.run_with_file_input(&bytes, native_ring);
        assert!(
            output.status.success(),
            "the echo must reach the end of a file (native ring: {native_ring}): {:?}",
            output.status
        );
        assert_eq!(output.stdout, bytes, "native ring: {native_ring}");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn an_empty_standard_input_reaches_its_end_without_publishing() {
    let llvm = compile_program("stdin_echo.wf");
    let program = build_program(&llvm);
    for native_ring in [true, false] {
        let output = program.run_with_piped_input(&[], native_ring);
        assert!(output.status.success(), "native ring: {native_ring}");
        assert!(output.stdout.is_empty(), "native ring: {native_ring}");
    }
}
