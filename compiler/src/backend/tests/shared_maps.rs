//! Shared-map emission and atomic target-group lock-ledger reporting.
//! Deliberately damaged lowering must make the amendment-S witnesses fail.
//! These checks qualify the conformance observations independently of the
//! emitted instruction sequences they damage.

use super::{compile, compile_and_run, compile_link_and_run};

/// Keep every ledger witness on the ordinary source-to-emission path used by
/// the dispatch ledger. The entry is used on every path; only the body's
/// placement of the metadata read changes between witnesses.
const LOCK_LEDGER_SOURCE: &str = r#"const ledger_names: Array<u8, 1> =[97_u8];

struct Meta {
  count: u64;
}

fn lock_run(keys: Shared<ConcurrentHashMap<u8>>, meta: Shared<STATE>, held: Bool) -> result: unit pure waits {
  let key = &ledger_names[0_u64..1_u64];
  atomic slot = &keys[key], m = &metaGUARD {
BODY
    set slot^ = None<u8>();
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let keys = shared_map_new::<u8>(capacity: 1_u64);
  let initial = INITIAL;
  let meta = shared_new::<STATE>(value: initial);
  let held = True();
  lock_run(keys: move keys, meta: move meta, held: held);
  return std::process::exit_status(code: 0_u8);
}
"#;

fn lock_ledger_source(state: &str, initial: &str, guard: &str, body: &str) -> String {
    LOCK_LEDGER_SOURCE
        .replace("STATE", state)
        .replace("INITIAL", initial)
        .replace("GUARD", guard)
        .replace("BODY", body)
}

fn lock_ledger_position(source: &str, needle: &str) -> String {
    let offset = source
        .find(needle)
        .expect("the witness contains the source position");
    let before = &source[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before.len(), |(_, tail)| tail.len())
        + 1;
    format!("test.wf:{line}:{column}")
}

fn lock_ledger_lines(module: &str) -> Vec<&str> {
    module
        .lines()
        .filter_map(|line| line.strip_prefix(crate::LOCK_LEDGER_PREFIX))
        .filter(|line| line.starts_with("LOCK lock_run  "))
        .collect()
}

fn metadata_lock_line(source: &str, state: &str, timing: &str, first_use: &str) -> String {
    format!(
        "LOCK lock_run  {}  targets(m): {state}  {timing}; first use {}",
        lock_ledger_position(source, "atomic slot"),
        lock_ledger_position(source, first_use),
    )
}

/// A host display path must remain within one comment, with literal
/// backslashes distinct from escapes for control characters.
#[test]
fn lock_ledger_escapes_control_characters_in_host_display_paths() {
    let source = lock_ledger_source(
        "Meta",
        "Meta(count: 1_u64)",
        "",
        "    let observed = m^.count;\n    set m^.count = observed;",
    );
    let inputs = [crate::SourceInput::from_host_path(
        "test.wf",
        "host\nledger_tail\r\t\u{1}\\test.wf",
        source.as_bytes(),
    )];
    let module = crate::native_test_support::timed("whitefoot-compile", || {
        crate::compile(&inputs, crate::CompilerLimits::default())
            .expect("normal compiler pipeline must emit")
            .into_string()
    });
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per target group: {lines:?}");
    let expected = metadata_lock_line(&source, "Meta", "on every path", "let observed")
        .replace("test.wf", r"host\nledger_tail\r\t\u{1}\\test.wf");
    let comment = format!("{}{expected}", crate::LOCK_LEDGER_PREFIX);
    assert_eq!(
        module
            .lines()
            .filter(|line| *line == comment.as_str())
            .count(),
        1,
        "the escaped metadata path stays on one lock comment: {lines:?}"
    );
    assert!(
        !module.lines().any(|line| line.starts_with("ledger_tail")),
        "host text after a newline must not become a module line"
    );
}

/// An unconditional first read must not be described as conditional merely
/// because the emitter checks the lazy group's taken word.
#[test]
fn lock_ledger_reports_a_metadata_read_on_every_path() {
    let source = lock_ledger_source(
        "Meta",
        "Meta(count: 1_u64)",
        "",
        "    let observed = m^.count;\n    set m^.count = observed;\n    if held {\n      set slot^ = Some<u8>(value: 1_u8);\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per target group: {lines:?}");
    let expected = metadata_lock_line(&source, "Meta", "on every path", "let observed");
    assert!(
        lines.contains(&expected.as_str()),
        "an unconditional read takes Meta on every path: {lines:?}"
    );
}

/// The metadata group is reached only through the true successor of `held`.
/// A report that promotes that use to every path must fail this assertion.
#[test]
fn lock_ledger_reports_a_metadata_read_on_some_paths() {
    let source = lock_ledger_source(
        "Meta",
        "Meta(count: 1_u64)",
        "",
        "    if held {\n      let observed = m^.count;\n      set m^.count = observed;\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per target group: {lines:?}");
    let expected = metadata_lock_line(&source, "Meta", "on some paths", "let observed");
    assert!(
        lines.contains(&expected.as_str()),
        "the false successor never uses Meta: {lines:?}"
    );
}

/// Primitive states precede the entry's nominal map state. The conditional
/// primitive read does not postpone its lock: acquiring the entry takes both.
#[test]
fn lock_ledger_names_the_entry_that_takes_an_earlier_primitive_group() {
    let source = lock_ledger_source(
        "u64",
        "1_u64",
        "",
        "    if held {\n      let observed = m^;\n      set m^ = observed;\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per target group: {lines:?}");
    let entry = lock_ledger_position(&source, "slot =");
    let expected = format!(
        "{}; taken with targets(slot) at {entry}",
        metadata_lock_line(&source, "u64", "before the guard", "slot ="),
    );
    assert!(
        lines.contains(&expected.as_str()),
        "the entry takes u64 before the guard and reports why: {lines:?}"
    );
}

/// The guard's read requires the metadata state before the guard, even though
/// the block itself uses it only under a condition.
#[test]
fn lock_ledger_reports_a_guard_read_before_the_guard() {
    let source = lock_ledger_source(
        "Meta",
        "Meta(count: 1_u64)",
        " when m^.count != 0_u64",
        "    if held {\n      let observed = m^.count;\n      set m^.count = observed;\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per target group: {lines:?}");
    let expected = metadata_lock_line(&source, "Meta", "before the guard", "m^.count !=");
    assert!(
        lines.contains(&expected.as_str()),
        "a guard read takes Meta before testing the guard: {lines:?}"
    );
}

/// A guard over a local value reads no target. Its watch fallback takes all
/// groups before the guard, including metadata used only under a condition.
#[test]
fn lock_ledger_reports_all_groups_before_a_guard_that_reads_no_target() {
    let source = lock_ledger_source(
        "Meta",
        "Meta(count: 1_u64)",
        " when held",
        "    if held {\n      let observed = m^.count;\n      set m^.count = observed;\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per target group: {lines:?}");
    let expected = metadata_lock_line(&source, "Meta", "before the guard", "atomic slot");
    assert!(
        lines.contains(&expected.as_str()),
        "a guard that reads no target takes Meta before testing the guard: {lines:?}"
    );
}

/// Both emitted successors acquire Meta. This catches a report that treats
/// all acquisitions within source `if` statements as only some paths.
#[test]
fn lock_ledger_reports_a_metadata_read_in_both_branches_on_every_path() {
    let source = lock_ledger_source(
        "Meta",
        "Meta(count: 1_u64)",
        "",
        "    if held {\n      let observed = m^.count;\n      set m^.count = observed;\n    } else {\n      let other = m^.count;\n      set m^.count = other;\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per target group: {lines:?}");
    let metadata = lines
        .iter()
        .find(|line| line.contains("targets(m): Meta"))
        .expect("the metadata group is reported");
    assert!(metadata.contains("  on every path; first use "), "{metadata}");
    assert!(
        metadata.contains(&lock_ledger_position(&source, "let observed")),
        "{metadata}"
    );
    assert!(
        metadata.contains(&lock_ledger_position(&source, "let other")),
        "{metadata}"
    );
}

/// An unconditional use of a later source state also takes an earlier state
/// whose own use is conditional. The ledger must name the actual trigger.
#[test]
fn lock_ledger_names_a_later_lazy_group_that_takes_an_earlier_group() {
    let source = lock_ledger_source(
        "Meta",
        "Meta(count: 1_u64)",
        "",
        "    let observed_tail = z^.count;\n    set z^.count = observed_tail;\n    if held {\n      let observed = m^.count;\n      set m^.count = observed;\n    }",
    )
    .replace(
        "struct Meta {\n  count: u64;\n}",
        "struct Meta {\n  count: u64;\n}\n\nstruct Zeta {\n  count: u64;\n}",
    )
    .replace("held: Bool)", "tail: Shared<Zeta>, held: Bool)")
    .replace("m = &meta", "m = &meta, z = &tail")
    .replace(
        "  let held = True();",
        "  let initial_tail = Zeta(count: 1_u64);\n  let tail = shared_new::<Zeta>(value: initial_tail);\n  let held = True();",
    )
    .replace(
        "meta: move meta, held: held",
        "meta: move meta, tail: move tail, held: held",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 3, "one line per target group: {lines:?}");
    let trigger = lock_ledger_position(&source, "let observed_tail");
    let expected = format!(
        "{}; taken with targets(z) at {trigger}",
        metadata_lock_line(&source, "Meta", "on every path", "let observed_tail"),
    );
    assert!(
        lines.contains(&expected.as_str()),
        "Zeta's unconditional use takes Meta too: {lines:?}"
    );
}

fn witness(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/conformance/cases")
            .join(format!("share-pos-map-{name}.wf")),
    )
    .expect("read the normative witness")
}

/// Compiles a whole-target witness, turns its write selections into read
/// selections, and requires the damaged program to leave its exit code.
/// Each witness is its own test so the harness runs them in parallel.
fn assert_detects_read_selection_in_write_position(name: &str, expected: i32) {
    let llvm = compile(&witness(name));
    let mut changes = 0;
    let damaged = llvm
        .lines()
        .map(|line| {
            if line.contains("call ptr @wf__table_held_entry(") && line.ends_with("i32 1)") {
                changes += 1;
                line.replace("i32 1)", "i32 0)")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        changes > 0,
        "{name}: the write-selection damage was applied"
    );
    let output = compile_and_run(&damaged);
    assert_ne!(output.status.code(), Some(expected), "{name}");
}

#[test]
fn whole_target_entry_detects_read_selection_in_write_position() {
    assert_detects_read_selection_in_write_position("whole-target-entry", 22);
}

#[test]
fn whole_target_alias_detects_read_selection_in_write_position() {
    assert_detects_read_selection_in_write_position("whole-target-alias", 22);
}

#[test]
fn whole_target_count_detects_read_selection_in_write_position() {
    assert_detects_read_selection_in_write_position("whole-target-count-sees-own-writes", 21);
}

#[test]
fn whole_target_callee_detects_read_selection_in_write_position() {
    assert_detects_read_selection_in_write_position("whole-target-passed-to-callee", 1);
}

#[test]
fn whole_target_guard_detects_read_selection_in_write_position() {
    assert_detects_read_selection_in_write_position("whole-target-guard", 1);
}

#[test]
fn entries_witness_detects_selection_of_one_slot_for_every_set_index() {
    let llvm = compile(&witness("whole-target-entries-over-set"));
    let mut changes = 0;
    let damaged = llvm
        .lines()
        .map(|line| {
            if line.contains("call ptr @wf__table_hold_slot(") {
                let (prefix, _) = line.rsplit_once(", i64 ").expect("slot call's index");
                changes += 1;
                format!("{prefix}, i64 0)")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(changes > 0, "the slot-selection damage was applied");
    assert_ne!(compile_and_run(&damaged).status.code(), Some(3));
}

#[test]
fn whole_swap_witness_detects_a_missing_swap() {
    let name = "swapped";
    let llvm = compile(&witness(name));
    let mut changes = 0;
    let damaged = llvm
        .lines()
        .map(|line| {
            if line.contains("call void @wf__keyed_table_swap(") {
                changes += 1;
                "  ; deliberately omitted table swap".to_owned()
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(changes > 0, "the swap damage was applied");
    assert_ne!(compile_and_run(&damaged).status.code(), Some(22), "{name}");
}

#[test]
fn map_tag_readers_use_one_byte_without_reading_neighboring_payload() {
    let source = br#"const tag_names: Array<u8, 2> =[97_u8, 98_u8];

struct NeighborBytes {
  first: u8;
  second: u8;
  third: u8;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<NeighborBytes>(capacity: 2_u64);
  let fresh = shared_map_new::<NeighborBytes>(capacity: 2_u64);
  let first = &tag_names[0_u64..1_u64];
  let second = &tag_names[1_u64..2_u64];
  let value = NeighborBytes(first: 129_u8, second: 193_u8, third: 251_u8);
  let initial = 0_u64;
  let remaining = 0_u64;
  let bounded_keys = key_set_new(capacity: 0_u64);
  let bounded_count = 0_u64;
  let bounded_failed = False();
  atomic t = &store {
    set t^[first] = Some<NeighborBytes>(value: value);
    set t^[second] = Some<NeighborBytes>(value: value);
    set initial = map_count::<NeighborBytes>(map: t);
    set t^[first] = None<NeighborBytes>();
    set remaining = map_count::<NeighborBytes>(map: t);
    let bounded = map_scan_within::<NeighborBytes>(map: t, cursor: 0_u64, count: 2_u64, limit: 0_u64, keys: &bounded_keys);
    let required = 0_u64;
    match bounded {
      Next(next: end) => {
        set bounded_failed = True();
      }
      Needs(bytes: wanted) => {
        set required = wanted;
      }
    }
    if required > 0_u64 {
      let admitted = map_scan_within::<NeighborBytes>(map: t, cursor: 0_u64, count: 2_u64, limit: required, keys: &bounded_keys);
      match admitted {
        Next(next: end) => {
          if end != 0_u64 {
            set bounded_failed = True();
          }
        }
        Needs(bytes: wanted) => {
          set bounded_failed = True();
        }
      }
    } else {
      set bounded_failed = True();
    }
    set bounded_count = bounded_keys.len;
  }
  if initial != 2_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if remaining != 1_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if bounded_failed {
    return std::process::exit_status(code: 6_u8);
  }
  if bounded_count != 1_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  atomic slot = &store[second] {
    set slot^ = Some<NeighborBytes>(value: value);
  }
  let keys = key_set_new(capacity: 2_u64);
  let cursor = 0_u64;
  atomic t = &store {
    set cursor = map_scan::<NeighborBytes>(map: t, cursor: 0_u64, count: 2_u64, keys: &keys);
  }
  if keys.len != 1_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  let moved = 0_u64;
  let cleared = 1_u64;
  atomic t = &store, f = &fresh {
    swap(first: t, second: f);
    set moved = map_count::<NeighborBytes>(map: f);
    map_clear::<NeighborBytes>(map: f);
    set cleared = map_count::<NeighborBytes>(map: f);
  }
  if moved != 1_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if cleared != 0_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    for (symbol, opcode) in [
        ("wf__keyed_table_count", "call i64"),
        ("wf__keyed_table_scan", "call i64"),
        ("wf__keyed_table_scan_within", "call i64"),
        ("wf__keyed_table_clear", "call void"),
        ("wf__keyed_table_swap", "call void"),
        ("wf__table_hold_release", "call void"),
        ("wf__atomic_group_release", "call void"),
    ] {
        let calls: Vec<_> = llvm.lines().filter(|line| {
            line.contains(&format!("{opcode} @{symbol}("))
        }).collect();
        assert!(!calls.is_empty(), "{symbol} is exercised");
        assert!(calls.iter().all(|line| line.contains(", i64 0, i32 1, i64 0")),
            "{symbol} receives tag offset 0, byte width 1 and None tag 0: {calls:?}");
    }
    let option = super::nominal_type("Option<NeighborBytes>");
    let payload = super::nominal_type("NeighborBytes");
    assert!(llvm.contains(&format!("{option} = type {{ i8, {payload} }}")), "{llvm}");
    assert!(llvm.contains(&format!("{payload} = type {{ i8, i8, i8 }}")), "{llvm}");
    assert!(llvm.contains(".tag = load i8, ptr "), "the entry unlock reads one byte: {llvm}");
    assert!(llvm.contains(".some = icmp ne i8 "), "the entry unlock compares one byte: {llvm}");
    assert!(llvm.lines().any(|line| line.contains(".present = zext i1 ") && line.ends_with(" to i32"))
        && llvm.lines().any(|line| line.contains("call void @wf__table_unlock_entry(ptr ")
            && line.contains(", i32 ")),
        "the runtime receives its unchanged i32 presence argument: {llvm}");
    // The two WF calls share one monomorphized row body. Count runtime
    // calls, and trace each one's separate needs pointer within that body.
    let mut bounded_runtime_calls = 0;
    for body in llvm.split("\n}\n") {
        for call in body.lines().filter(|line| line.contains("call i64 @wf__keyed_table_scan_within(")) {
            bounded_runtime_calls += 1;
            let (_, arguments) = call.split_once("@wf__keyed_table_scan_within(").unwrap();
            let needs = arguments.split(", ").nth(5).and_then(|arg| arg.strip_prefix("ptr "))
                .expect("the runtime's sixth argument is its separate needs pointer");
            assert!(body.contains(&format!("{needs} = alloca i32, align 4")),
                "the needs output has its unchanged i32 storage: {body}");
            let loads = body.lines().filter_map(|line| {
                line.trim().strip_suffix(&format!(" = load i32, ptr {needs}"))
            }).collect::<Vec<_>>();
            assert_eq!(loads.len(), 1, "each bounded runtime call reads its i32 needs output once: {body}");
            assert!(body.contains(&format!(" = icmp eq i32 {}, 0", loads[0])),
                "the bounded runtime call compares its separate i32 needs result: {body}");
        }
    }
    assert!(bounded_runtime_calls > 0, "the bounded scan runtime is exercised");
    // All four bytes belong to the slot. Dirtying the inactive payload of
    // None is semantically unobservable, but a stale four-byte tag reader
    // mistakes those nonzero neighbors for a present entry.
    let observer = r#"#include <stdint.h>
#include <stdio.h>
extern void *wf__table_held_entry(void *, const unsigned char *, uint64_t, uint32_t);
extern uint64_t wf__keyed_table_count(void *, uint64_t, uint32_t, uint64_t);
uint64_t wf_test_narrow_count(void *map, uint64_t offset, uint32_t width, uint64_t none) {
    static const unsigned char key[] = {97};
    unsigned char *slot = wf__table_held_entry(map, key, 1, 0);
    if (slot != NULL && slot[0] == 0) {
        slot[1] = 129;
        slot[2] = 193;
        slot[3] = 251;
        fputs("D;", stdout);
    }
    return wf__keyed_table_count(map, offset, width, none);
}
"#;
    let observed = llvm.replace("call i64 @wf__keyed_table_count(", "call i64 @wf_test_narrow_count(");
    assert_ne!(observed, llvm);
    let observed = format!("{observed}\ndeclare i64 @wf_test_narrow_count(ptr, i64, i32, i64)\n");
    let output = compile_link_and_run(&observed, Some(observer), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"D;", "the inactive neighboring bytes were dirtied: {output:?}");
    // A representative wrong width must reach the intended count mismatch,
    // with no out-of-bounds read: this slot has exactly four initialized bytes.
    let damaged = observed.lines().map(|line| {
        if line.contains("call i64 @wf_test_narrow_count(") {
            line.replace(", i64 0, i32 1, i64 0)", ", i64 0, i32 4, i64 0)")
        } else {
            line.to_owned()
        }
    }).collect::<Vec<_>>().join("\n");
    assert_ne!(damaged, observed);
    let output = compile_link_and_run(&damaged, Some(observer), &[]);
    assert_eq!(output.status.code(), Some(2), "the widened read sees inactive payload bytes: {output:?}");
    assert_eq!(output.stdout, b"D;", "{output:?}");
    // Isolate the bounded scan's entry-width argument. Its runtime reads the
    // same dirty None before this hold is released; count stays correct.
    let mut scans = 0;
    let damaged = observed.lines().map(|line| {
        if line.contains("call i64 @wf__keyed_table_scan_within(") {
            scans += 1;
            let replacement = line.replace(", i64 0, i32 1, i64 0)", ", i64 0, i32 4, i64 0)");
            assert_ne!(replacement, line, "the bounded scan's byte width was damaged");
            replacement
        } else {
            line.to_owned()
        }
    }).collect::<Vec<_>>().join("\n");
    assert_eq!(scans, bounded_runtime_calls,
        "every bounded scan runtime call, shared by both WF call sites, has its entry width damaged");
    let output = compile_link_and_run(&damaged, Some(observer), &[]);
    assert_eq!(output.status.code(), Some(7),
        "the widened bounded scan includes the inactive None as a second key: {output:?}");
    assert_eq!(output.stdout, b"D;", "{output:?}");
}
