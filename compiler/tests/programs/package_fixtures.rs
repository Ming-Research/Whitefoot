//! The witness programs the release package acceptance builds with the packaged
//! compiler (`.github/toolchain/check-package.sh`, `package-acceptance.yml`).
//! They are built here with the gate's own toolchain so a language change that
//! breaks one fails the gate, not the next release run, and so the result the
//! acceptance expects is stated where a failure can be traced to the source.

use std::os::unix::process::ExitStatusExt;

use super::support::{compile_and_run, compile_sources};

const WAITING: &[u8] = include_bytes!("../../../tests/package-fixtures/waiting.wf");
const STACK_EXHAUSTION: &[u8] =
    include_bytes!("../../../tests/package-fixtures/stack-exhaustion.wf");

/// An owned enum crosses a direct waiting call and a bound spawn whose result
/// is used, so the build lowers coroutines; exit 1 or 2 names a wrong result.
#[test]
fn the_waiting_witness_runs_to_a_zero_exit() {
    let output = compile_and_run(&compile_sources(&[("waiting.wf", WAITING)]));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// Every activation holds a frame larger than a guard page, and the recursion
/// is deeper than the entry's stack, so the program ends only if each frame is
/// probed into the guard: the floor's abort, with its one record on standard
/// error. An unprobed frame would step over the guard and end in a host fault
/// with no record. Each activation sums its array after the recursive call, so
/// the optimizer must keep the whole array in the frame; a reader that takes
/// one element lets the optimizer shrink the frame, and the program then
/// returns normally.
#[test]
fn the_stack_exhaustion_witness_ends_in_the_floor_abort_and_its_record() {
    let output = compile_and_run(&compile_sources(&[(
        "stack-exhaustion.wf",
        STACK_EXHAUSTION,
    )]));
    assert_eq!(output.status.signal(), Some(6), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "{\"resource\":\"stack\"}\n",
        "{output:?}"
    );
}
