//! [SHARE-3] The wake test a guard's watch registers and the write flag a
//! release passes, as the backend emits them for the completion bridge's
//! `wf_wake_test` (compiler/waiting-contexts/state-locks).

use super::{compile, emitted_function};

fn guarded(guard: &str, block: &str) -> String {
    format!(
        r#"struct Gate {{
  turn: u64;
  stop: u64;
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let start = Gate(turn: 0_u64, stop: 0_u64);
  let state = shared_new::<Gate>(value: start);
  let seen = 0_u64;
  atomic g = &state when {guard} {{
    {block}
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

/// The one line of `body` that calls `entry`.
fn call_line<'body>(body: &'body str, entry: &str) -> &'body str {
    let mut lines = body
        .lines()
        .filter(|line| line.contains(&format!("@{entry}(")) && !line.starts_with("declare"));
    let line = lines
        .next()
        .unwrap_or_else(|| panic!("one call of {entry}: {body}"));
    assert!(lines.next().is_none(), "one call of {entry}: {body}");
    line
}

#[test]
fn a_kept_comparison_is_registered_as_its_bytes() {
    let llvm = compile(guarded("g^.stop == 3_u64", "set g^.turn = seen;").as_bytes());
    let main = emitted_function(&llvm, "main");
    let watch = call_line(main, "wf__shared_watch");
    let Some((_, test)) = watch.rsplit_once(", ptr ") else {
        panic!("the watch passes its test last: {watch}");
    };
    let test = test.trim_end_matches(')');
    assert!(test.starts_with("%wf.wake.v"), "{watch}");
    assert!(
        main.contains(&format!("{test} = alloca [104 x i8], align 8")),
        "the test is frame storage of WF_WAKE_TEST_SIZE bytes: {main}"
    );
    // Header: one code, the first atom's.
    assert!(
        main.contains(&format!(
            "store [8 x i8] [i8 1, i8 1, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0], ptr {test}, align 8"
        )),
        "{main}"
    );
    // The atom's width (8 bytes), flags (none) and comparison (equal).
    assert!(
        main.contains("store [8 x i8] [i8 8, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0]"),
        "{main}"
    );
    // `stop` is the second u64: its offset is computed from the state's
    // address, not written as a number the emitter assumed.
    assert!(main.contains("ptrtoint ptr"), "{main}");
    // The statement wrote: its release wakes watchers.
    call_line(main, "wf__shared_unlock");
    assert!(!main.contains("@wf__shared_unlock_unwritten("), "{main}");
}

#[test]
fn a_guard_with_no_kept_part_registers_no_test() {
    // The guard reads only a local, which no write changes.
    let llvm = compile(guarded("seen == 3_u64", "set g^.turn = seen;").as_bytes());
    let main = emitted_function(&llvm, "main");
    let watch = call_line(main, "wf__shared_watch");
    assert!(watch.ends_with(", ptr null)"), "{watch}");
    assert!(!main.contains("%wf.wake.v"), "{main}");
}

#[test]
fn a_read_only_statement_releases_unwritten() {
    let llvm = compile(guarded("g^.stop == 3_u64", "set seen = g^.turn;").as_bytes());
    let main = emitted_function(&llvm, "main");
    let release = call_line(main, "wf__shared_unlock_unwritten");
    assert!(release.ends_with(", i32 1)"), "the hold stays a write hold: {release}");
    assert!(!main.contains("@wf__shared_unlock("), "{main}");
    assert!(
        llvm.lines().any(|line| line.starts_with("declare")
            && line.contains("@wf__shared_unlock_unwritten(ptr")),
        "the entry is declared: {llvm}"
    );
}

#[test]
fn the_emitted_codes_are_the_bridge_s_own() {
    let header = crate::COMPLETION_BRIDGE_HEADER;
    for line in [
        "#define WF_WAKE_ATOMS 4u",
        "#define WF_WAKE_PROGRAM 7u",
        "enum { WF_WAKE_AND = 8, WF_WAKE_OR = 9 };",
        "enum { WF_WAKE_EQUAL, WF_WAKE_NOT_EQUAL, WF_WAKE_LESS, WF_WAKE_LESS_EQUAL,",
        "       WF_WAKE_GREATER, WF_WAKE_GREATER_EQUAL };",
        "enum { WF_WAKE_SIGNED = 1, WF_WAKE_BIT = 2, WF_WAKE_FIELD = 4 };",
    ] {
        assert!(
            header.lines().any(|candidate| candidate == line),
            "bridge.h keeps `{line}`, which emitter/wake.rs encodes"
        );
    }
}
