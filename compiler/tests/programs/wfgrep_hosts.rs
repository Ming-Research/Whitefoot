//! `tests/programs/wfgrep.wf` on every host: one tree, one expected hit set.
//!
//! The POSIX-only module beside this one checks wfgrep against a reference
//! search and the host's `grep`; this one runs on Linux and in the Windows job
//! alike, so the same lines must come out of both. The tree holds an empty
//! directory, whose only host entries are `.` and `..`, a non-ASCII name and
//! two levels below the root: a listing that reported `.` and `..`, or names
//! that are not the same bytes on both hosts [PRE-2], changes the published
//! lines or reports a path as unreadable.

use super::support::{build_program, compile_program, fixture_directory};

#[test]
fn wfgrep_publishes_the_same_lines_on_every_host() {
    let program = build_program(&compile_program("wfgrep.wf"));
    let fixture = fixture_directory();
    fixture.directory("tree/empty");
    fixture.write_nested("tree/alpha.txt", b"needle here\nplain\n");
    fixture.write_nested("tree/sub/gamma.txt", b"deep needle\n");
    fixture.write_nested("tree/sub/\u{e9}.txt", "needle \u{e9}\n".as_bytes());
    fixture.write_nested("tree/sub/deeper/epsilon.txt", b"deepest needle\nlast\n");
    let output = program.run(fixture.path(), &[b"needle", b"tree"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "wfgrep: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "no path may be reported: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut published: Vec<&[u8]> = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    published.sort_unstable();
    let expected: [&[u8]; 4] = [
        b"tree/alpha.txt:1:needle here",
        b"tree/sub/deeper/epsilon.txt:1:deepest needle",
        b"tree/sub/gamma.txt:1:deep needle",
        "tree/sub/\u{e9}.txt:1:needle \u{e9}".as_bytes(),
    ];
    assert_eq!(published, expected);
}
