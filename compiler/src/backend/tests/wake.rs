//! [SHARE-3] The wake test a guard's watch registers and the write flag a
//! release passes, as the backend emits them for the completion bridge's
//! `wf_wake_test`, and the parking take of a statement's first type group,
//! which a guard's reservation can refuse
//! (compiler/waiting-contexts/state-locks).

use super::{compile, emit_lowered, emitted_function};

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
fn helper_and_forwarded_guards_emit_the_inline_comparison_bytes() {
    const HELPERS: &str = r#"fn covered(gate: &Gate, target: u64) -> ready: Bool reads(gate.stop) {
  return gate^.stop >= target;
}

fn forwarded(limit: u64, state: &Gate) -> ready: Bool reads(state.stop) {
  return covered(gate: state, target: limit);
}

"#;
    for guard in [
        "g^.stop >= 3_u64",
        "covered(gate: g, target: 3_u64)",
        "forwarded(limit: 3_u64, state: g)",
    ] {
        let source = guarded(guard, "set g^.turn = seen;")
            .replace("fn main()", &format!("{HELPERS}fn main()"));
        let llvm = compile(source.as_bytes());
        let main = emitted_function(&llvm, "main");
        let watch = call_line(main, "wf__shared_watch");
        assert!(
            watch.contains(", ptr %wf.wake.v"),
            "{guard}: {watch}"
        );
        assert!(main.contains(&header(&[1])), "{guard}: {main}");
        assert!(
            main.contains(&atom_tail(8, 0, GREATER_EQUAL)),
            "{guard}: {main}"
        );
        let stores_target = main
            .lines()
            .filter_map(|line| {
                let (name, expression) = line.trim().split_once(" = ")?;
                (expression == "select i1 true, i64 3, i64 3").then_some(name)
            })
            .any(|name| main.contains(&format!("store i64 {name}, ptr")));
        assert!(
            stores_target,
            "{guard}: the captured target must be the same value as the inline operand: {main}"
        );
        assert_eq!(
            main.matches("store [8 x i8]").count(),
            2,
            "{guard}: exactly one header and one comparison atom: {main}"
        );
    }
}

#[test]
fn a_multi_statement_nested_guard_registers_the_callback_and_callers_sequence() {
    let source = br#"struct Progress {
  written: u64;
  unrelated: u64;
  failure: Option<u64>;
  stopping: Bool;
}

fn reply_ready(progress: &Progress, sequence: u64) -> ready: Bool reads(progress.written), reads(progress.failure) {
  let covered = progress^.written >= sequence;
  if covered {
    return True();
  }
  match progress^.failure {
    None() => {
      return False();
    }
    Some(..) => {
      return True();
    }
  }
}

fn rebuilding_ready(progress: &Progress, sequence: u64) -> ready: Bool reads(progress.stopping), reads(progress.written), reads(progress.failure) {
  let stopping = progress^.stopping;
  if stopping {
    return True();
  }
  return reply_ready(progress: progress, sequence: sequence);
}

fn main() -> status: std::process::ExitStatus pure waits {
  let clear = None<u64>();
  let no = False();
  let initial = Progress(written: 0_u64, unrelated: 0_u64, failure: clear, stopping: no);
  let state = shared_new::<Progress>(value: initial);
  let sequence = 19_u64;
  atomic progress = &state when rebuilding_ready(progress: progress, sequence: sequence) {
    set progress^.unrelated = 1_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let main = emitted_function(&llvm, "main");
    let watch = call_line(main, "wf__shared_watch");
    let (_, test) = watch
        .rsplit_once(", ptr ")
        .expect("test is the last watch argument");
    let test = test.trim_end_matches(')');
    assert!(test.starts_with("%wf.wake.v"), "{watch}");
    assert!(
        main.contains(&format!("{test} = alloca [24 x i8], align 8")),
        "the callback descriptor has its own 24-byte ABI: {main}"
    );
    assert!(
        main.contains("store [8 x i8] [i8 255, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0]")
            && main.contains(&format!("ptr {test}, i64 0")),
        "callback discriminator: {main}"
    );

    let adapter_symbol = main
        .lines()
        .find_map(|line| {
            let (_, rest) = line.split_once("store ptr @")?;
            let (symbol, _) = rest.split_once(", ptr ")?;
            symbol.ends_with(".wake_filter").then_some(symbol)
        })
        .expect("registration stores a pointer to the generated C ABI adapter");
    let thunk_symbol = adapter_symbol
        .strip_suffix(".wake_filter")
        .expect("adapter suffix");
    let adapter = emitted_function(
        &llvm,
        adapter_symbol.strip_prefix("wf_").expect("source symbol"),
    );
    assert!(
        adapter.lines().next().is_some_and(|line| {
            line.contains("i32 ") && line.contains("(ptr ") && line.matches("ptr ").count() == 2
        }),
        "C ABI takes state and captures and returns i32: {adapter}"
    );
    assert!(
        adapter.contains("load i64"),
        "the adapter loads its captured sequence: {adapter}"
    );
    assert!(
        adapter.contains(&format!("@{thunk_symbol}(")),
        "the adapter invokes the ordinary guard thunk: {adapter}"
    );
    assert!(
        adapter.contains("zext i1") && adapter.contains("ret i32"),
        "the Bool answer crosses the runtime ABI explicitly: {adapter}"
    );

    let thunk = emitted_function(&llvm, thunk_symbol.strip_prefix("wf_").expect("thunk symbol"));
    assert!(
        thunk.contains("@wf_rebuilding_ready("),
        "the full guard is called: {thunk}"
    );
    let rebuilding = emitted_function(&llvm, "rebuilding_ready");
    assert!(
        rebuilding.contains("@wf_reply_ready("),
        "nested helper call: {rebuilding}"
    );
    let reply = emitted_function(&llvm, "reply_ready");
    assert!(
        reply.contains("icmp uge i64") && (reply.contains("br i1") || reply.contains("switch i1")),
        "the real helper keeps its comparison and branches: {reply}"
    );

    let capture_value = main
        .lines()
        .find_map(|line| {
            let (name, expression) = line.trim().split_once(" = ")?;
            (expression == "select i1 true, i64 19, i64 19").then_some(name)
        })
        .expect("the caller defines sequence 19");
    let capture_place = main
        .lines()
        .find_map(|line| {
            let (name, expression) = line.trim().split_once(" = ")?;
            (expression == format!("getelementptr inbounds i8, ptr {test}.captures, i64 0"))
                .then_some(name)
        })
        .expect("the callback has a slot for the captured sequence");
    assert!(
        main.contains(&format!("store i64 {capture_value}, ptr {capture_place}, align 8")),
        "the callback captures the waiter's actual sequence: {main}"
    );
    assert!(
        main.contains(&format!("ptr {test}, i64 8"))
            && main.contains(&format!("ptr {test}, i64 16"))
            && main.contains(&format!("store ptr {test}.captures, ptr")),
        "function and capture pointers occupy ABI offsets 8 and 16: {main}"
    );
    assert_eq!(
        main.matches("store [8 x i8]").count(),
        1,
        "the callback has only its header, no comparison atoms: {main}"
    );
}

#[test]
fn a_callback_over_an_overlapping_helper_calls_the_sequential_clone() {
    // `overlapped_ready` has two pure sibling calls, which an overlap build
    // offers to the scheduler. A writer running the guard under its hold must
    // not enter the scheduler, so the adapter calls the guard thunk's
    // sequential clone, which reaches the helper's sequential clone.
    let source = br#"struct Progress {
  written: u64;
  unrelated: u64;
}

fn weight(value: u64) -> result: u64 pure {
  return value +wrap 1_u64;
}

fn overlapped_ready(progress: &Progress, sequence: u64) -> ready: Bool reads(progress.written) {
  let current = progress^.written;
  let left = weight(value: current);
  let right = weight(value: sequence);
  return left >= right;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let initial = Progress(written: 0_u64, unrelated: 0_u64);
  let state = shared_new::<Progress>(value: initial);
  let sequence = 19_u64;
  atomic progress = &state when overlapped_ready(progress: progress, sequence: sequence) {
    set progress^.unrelated = 1_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = emit_lowered(source, crate::OverlapLowering::On);
    assert!(
        emitted_function(&llvm, "overlapped_ready").contains("@wf__par_"),
        "the helper offers its first call to the scheduler: {llvm}"
    );
    let main = emitted_function(&llvm, "main");
    let adapter_symbol = main
        .lines()
        .find_map(|line| {
            let (_, rest) = line.split_once("store ptr @")?;
            let (symbol, _) = rest.split_once(", ptr ")?;
            symbol.ends_with(".wake_filter").then_some(symbol)
        })
        .expect("registration stores a pointer to the generated C ABI adapter");
    let thunk_symbol = adapter_symbol
        .strip_suffix(".wake_filter")
        .expect("adapter suffix");
    let thunk = thunk_symbol.strip_prefix("wf_").expect("source symbol");
    let adapter = emitted_function(
        &llvm,
        adapter_symbol.strip_prefix("wf_").expect("source symbol"),
    );
    let clone = format!("@wf__par_seq_{thunk}(");
    assert!(
        adapter.contains(&clone),
        "the adapter calls the clone: {adapter}"
    );
    assert!(
        !adapter.contains(&format!("@{thunk_symbol}(")),
        "the adapter never calls the overlapping original: {adapter}"
    );
    assert!(
        llvm.lines()
            .any(|line| line.starts_with("define") && line.contains(&clone)),
        "the clone is defined: {llvm}"
    );
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

/// A program whose one guarded statement waits on `guard` over a `Dial`,
/// whose fields have each scalar form an atom reads, with the locals
/// `limit`, `shut` and `high` in scope.
fn scalars(guard: &str) -> String {
    format!(
        r#"enum Door {{
  Open();
  Shut();
}}

enum Speed {{
  Low();
  Mid();
  High();
}}

struct Dial {{
  turn: u64;
  level: i32;
  small: u8;
  half: u16;
  word: u32;
  door: Door;
  speed: Speed;
  ready: Bool;
  open: Bool;
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let no = False();
  let opened = Door::Open();
  let low = Speed::Low();
  let start = Dial(turn: 0_u64, level: 0_i32, small: 0_u8, half: 0_u16, word: 0_u32, door: opened, speed: low, ready: no, open: no);
  let state = shared_new::<Dial>(value: start);
  let seen = 0_u64;
  let limit = -2_i32;
  let shut = Door::Shut();
  let high = Speed::High();
  atomic g = &state when {guard} {{
    set g^.turn = seen;
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

/// The store of an atom's last eight bytes: its width, flags and
/// comparison (`completion/bridge.h`), then zeros.
fn atom_tail(width: u8, flags: u8, comparison: u8) -> String {
    format!(
        "store [8 x i8] [i8 {width}, i8 {flags}, i8 {comparison}, i8 0, i8 0, i8 0, i8 0, i8 0], ptr"
    )
}

/// The store of the test's header: its program's length, its codes, then
/// zeros.
fn header(codes: &[u8]) -> String {
    let mut bytes = vec![u8::try_from(codes.len()).expect("a short program")];
    bytes.extend_from_slice(codes);
    bytes.resize(8, 0);
    let items = bytes
        .iter()
        .map(|byte| format!("i8 {byte}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("store [8 x i8] [{items}], ptr %wf.wake.v")
}

/// Whether `main` extends a `from` value to 64 bits by `extension` and
/// stores the result as an atom's operand.
fn stores_extended(main: &str, extension: &str, from: &str) -> bool {
    main.lines()
        .filter_map(|line| {
            let (name, rest) = line.trim().split_once(" = ")?;
            (rest.starts_with(&format!("{extension} {from} ")) && rest.ends_with(" to i64"))
                .then_some(name)
        })
        .any(|name| main.contains(&format!("store i64 {name}, ptr")))
}

/// `main` of `scalars(guard)`, after checking that its watch passes a test.
fn watched(guard: &str) -> String {
    let llvm = compile(scalars(guard).as_bytes());
    let main = emitted_function(&llvm, "main").to_owned();
    let watch = call_line(&main, "wf__shared_watch");
    assert!(watch.contains(", ptr %wf.wake.v"), "`{guard}` registers a test: {watch}");
    main
}

/// `completion/bridge.h`'s comparison codes and flags.
const EQUAL: u8 = 0;
const LESS: u8 = 2;
const GREATER_EQUAL: u8 = 5;
const NOT_EQUAL: u8 = 1;
const SIGNED: u8 = 1;
const BIT: u8 = 2;

#[test]
fn a_signed_field_is_read_signed_and_its_operand_sign_extended() {
    // A negative constant and a captured local: the runtime compares the
    // field read as signed with the operand extended as the field is, so a
    // zero-extended -2 would be a large positive number.
    for (guard, comparison) in [("g^.level < -2_i32", LESS), ("g^.level >= limit", GREATER_EQUAL)] {
        let main = watched(guard);
        assert!(main.contains(&header(&[1])), "{guard}: {main}");
        assert!(
            main.contains(&atom_tail(4, SIGNED, comparison)),
            "{guard}: a 4-byte signed atom: {main}"
        );
        assert!(stores_extended(&main, "sext", "i32"), "{guard}: {main}");
        assert!(!stores_extended(&main, "zext", "i32"), "{guard}: {main}");
    }
}

#[test]
fn a_narrow_unsigned_field_is_read_at_its_width() {
    for (guard, width, llvm) in [
        ("g^.small == 7_u8", 1, "i8"),
        ("g^.half == 7_u16", 2, "i16"),
        ("g^.word == 7_u32", 4, "i32"),
    ] {
        let main = watched(guard);
        assert!(
            main.contains(&atom_tail(width, 0, EQUAL)),
            "{guard}: a {width}-byte unsigned atom: {main}"
        );
        assert!(stores_extended(&main, "zext", llvm), "{guard}: {main}");
        assert!(!stores_extended(&main, "sext", llvm), "{guard}: {main}");
    }
}

#[test]
fn a_tag_only_enum_field_is_read_at_its_tag_s_width() {
    // Two variants have a one-bit tag, a byte read by its low bit; three
    // have a byte tag, read whole.
    let main = watched("g^.door == shut");
    assert!(main.contains(&atom_tail(1, BIT, EQUAL)), "{main}");
    assert!(stores_extended(&main, "zext", "i1"), "{main}");
    let main = watched("g^.speed == high");
    assert!(main.contains(&atom_tail(1, 0, EQUAL)), "{main}");
    assert!(stores_extended(&main, "zext", "i8"), "{main}");
}

#[test]
fn an_and_or_an_or_of_two_atoms_is_one_postfix_program() {
    // Atoms 1 and 2, then the bridge's WF_WAKE_AND (8) or WF_WAKE_OR (9);
    // each atom is a Bool read by its low bit as "not false".
    for (guard, code) in [("band(g^.ready, g^.open)", 8), ("bor(g^.ready, g^.open)", 9)] {
        let main = watched(guard);
        assert!(main.contains(&header(&[1, 2, code])), "{guard}: {main}");
        assert_eq!(
            main.matches(&atom_tail(1, BIT, NOT_EQUAL)).count(),
            2,
            "{guard}: two Bool atoms: {main}"
        );
    }
}

#[test]
fn a_first_group_parks_and_asks_again_as_an_acquire_does() {
    let llvm = compile(
        br#"struct Alpha {
  n: u64;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let a0 = Alpha(n: 0_u64);
  let a1 = Alpha(n: 1_u64);
  let first = shared_new::<Alpha>(value: a0);
  let second = shared_new::<Alpha>(value: a1);
  atomic p = &first, q = &second {
    set p^.n = q^.n;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    let main = emitted_function(&llvm, "main");
    let call = call_line(main, "wf__atomic_group_acquire");
    let Some((parked, arguments)) = call.trim().split_once(" = call i32 @wf__atomic_group_acquire(")
    else {
        panic!("the take answers whether it parked: {call}");
    };
    assert!(
        arguments.contains(", i64 2, ptr %wf.coro.handle)"),
        "the group's two targets and the frame: {call}"
    );
    let prefix = parked
        .strip_suffix(".parked")
        .unwrap_or_else(|| panic!("{call}"));
    // As an acquire: a nonzero answer suspends, and the resumed frame asks
    // again rather than continuing, since it holds nothing yet.
    assert!(
        main.contains(&format!("icmp ne i32 {parked}, 0")),
        "{main}"
    );
    assert!(
        main.contains(&format!("[ i8 0, label {prefix}.try ")),
        "a resumed frame retries the take: {main}"
    );
    assert!(!main.contains("@wf__atomic_group_take("), "{main}");
    assert!(
        llvm.lines().any(|line| line.starts_with("declare")
            && line.contains("@wf__atomic_group_acquire(ptr")),
        "the entry is declared: {llvm}"
    );
}

/// The write flag each target of `main`'s two-target group record stores, by
/// target index, each site once.
fn group_record_flags(llvm: &str) -> Vec<(String, String)> {
    let main = emitted_function(llvm, "main");
    let lines = main.lines().collect::<Vec<_>>();
    let mut flags = lines
        .windows(2)
        .filter_map(|pair| {
            let (name, gep) = pair[0]
                .trim()
                .split_once(" = getelementptr [2 x { ptr, ptr, i32 }], ptr ")?;
            let index = gep.strip_suffix(", i32 2")?.rsplit_once(", i32 ")?.1;
            let stored = pair[1].trim().strip_prefix("store i32 ")?;
            let (value, place) = stored.split_once(", ptr ")?;
            (place == format!("{name}, align 8")).then(|| (index.to_owned(), value.to_owned()))
        })
        .collect::<Vec<_>>();
    flags.sort_unstable();
    flags.dedup();
    flags
}

fn two_alpha_group(block: &str) -> String {
    format!(
        r#"struct Alpha {{
  n: u64;
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let a0 = Alpha(n: 0_u64);
  let a1 = Alpha(n: 1_u64);
  let first = shared_new::<Alpha>(value: a0);
  let second = shared_new::<Alpha>(value: a1);
  let seen = 0_u64;
  atomic p = &first, q = &second {{
    {block}
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

#[test]
fn a_group_record_carries_each_target_s_write_flag() {
    let flag = |index: &str, value: &str| (index.to_owned(), value.to_owned());
    // Reading both: the record's third field is 0 for each target, so the
    // group's release starts no sweep.
    let llvm = compile(two_alpha_group("set seen = p^.n +wrap q^.n;").as_bytes());
    assert_eq!(group_record_flags(&llvm), [flag("0", "0"), flag("1", "0")], "{llvm}");
    // Writing p: two handles of one type may name one object, so a write
    // through p may write q's object and both are released as written.
    let llvm = compile(two_alpha_group("set p^.n = q^.n;").as_bytes());
    assert_eq!(group_record_flags(&llvm), [flag("0", "1"), flag("1", "1")], "{llvm}");
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
