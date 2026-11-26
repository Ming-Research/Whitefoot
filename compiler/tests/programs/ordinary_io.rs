//! Complete file/pipe behavior through ordinary WF calls and the shipped ABI.
//! Reuse each image across fixture inputs; private interposition stays in the
//! compiler tests, and source-language rejections belong to conformance.

use std::process::Output;

use super::support::{
    CompiledProgram, build_program, compile_program, compile_program_with_overlap,
    fixture_directory,
};

fn run(program: &CompiledProgram, files: &[(&str, &[u8])], arguments: &[&[u8]]) -> Output {
    let fixture = fixture_directory();
    for (name, bytes) in files {
        fixture.write_nested(name, bytes);
    }
    program.run(fixture.path(), arguments)
}

fn read_status(output: Output, expected: i32) {
    assert_eq!(output.status.code(), Some(expected), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn short_exact_and_empty_files_report_progress_then_end() {
    let program = build_program(&compile_program("io_chunked_read.wf"));
    // A short success is progress, not EOF. An exact-capacity file also needs
    // a following request to observe EOF. The status encodes bytes and reads.
    for (bytes, expected) in [(b"abcde".as_slice(), 52), (b"abc", 31), (b"", 0)] {
        read_status(run(&program, &[("input", bytes)], &[b"input"]), expected);
    }
}

#[test]
fn acquisition_propagates_the_owner_or_error_under_both_lowerings() {
    for module in [
        compile_program("io_propagate_open.wf"),
        compile_program_with_overlap("io_propagate_open.wf"),
    ] {
        let program = build_program(&module);
        read_status(run(&program, &[("input", b"abc")], &[b"input"]), 3);
        read_status(run(&program, &[], &[b"missing"]), 203);
    }
}

#[test]
fn read_preserves_the_requested_prefix_and_untouched_buffer_bytes() {
    let program = build_program(&compile_program("io_exact_prefix.wf"));
    // The WF receiver compares all eight bytes, including both untouched
    // sides. A truncated digest could hide a wrong byte behind a collision.
    read_status(run(&program, &[("input", b"abcde")], &[b"input"]), 0);
}

#[test]
fn write_publishes_its_range_and_absolute_endpoint() {
    let program = build_program(&compile_program("io_write_prefix.wf"));
    let output = run(&program, &[], &[]);
    assert_eq!(output.stdout, b"xy");
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stderr.is_empty());
}

#[test]
fn output_calls_preserve_source_order() {
    let program = build_program(&compile_program("io_ordered_writes.wf"));
    let output = run(&program, &[], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"ABC");
    assert!(output.stderr.is_empty());
}

#[test]
fn open_uses_the_directory_argument_and_reports_path_refusals() {
    let program = build_program(&compile_program("io_open_and_read.wf"));
    read_status(run(&program, &[("input", b"hello")], &[b"input"]), 5);
    read_status(
        run(
            &program,
            &[("inner/input", b"hello")],
            &[b"./inner/./input"],
        ),
        5,
    );
    // A parent component is refused at conversion, before any open [PRE-2].
    read_status(
        run(
            &program,
            &[("inner/input", b"hello")],
            &[b"./inner/../inner/input"],
        ),
        204,
    );
    read_status(run(&program, &[], &[b"missing"]), 203);
    read_status(run(&program, &[], &[b"/absent"]), 204);
}

#[test]
fn a_zero_length_read_preserves_the_following_transfer() {
    let program = build_program(&compile_program("io_vacant_read.wf"));
    read_status(run(&program, &[("input", b"abcde")], &[b"input"]), 5);
    read_status(run(&program, &[("input", b"")], &[b"input"]), 213);
}

#[test]
fn the_file_argument_read_and_output_chain_preserves_both_channels() {
    let program = build_program(&compile_program("io_complete_first_slice.wf"));
    let bytes = b"one line and then a longer second line\n";
    let output = run(&program, &[("page.txt", bytes)], &[b"page.txt"]);
    assert_eq!(output.stdout, bytes);
    assert_eq!(output.stderr, b"page.txt");
    assert_eq!(output.status.code(), Some(39));
}

#[test]
fn a_pipe_closed_before_start_reports_broken_pipe_on_the_first_write() {
    let program = build_program(&compile_program("io_broken_pipe.wf"));
    let fixture = fixture_directory();
    let (status, stderr) = program.run_with_closed_output(fixture.path(), &[]);
    assert_eq!(status.code(), Some(42));
    assert!(stderr.is_empty());
}
