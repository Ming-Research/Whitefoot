//! Deliberately damaged lowering must make the amendment-S witnesses fail.
//! These checks qualify the conformance observations independently of the
//! emitted instruction sequences they damage.

use super::{compile, compile_and_run, compile_link_and_run};

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
  atomic t = &store {
    set t^[first] = Some<NeighborBytes>(value: value);
    set t^[second] = Some<NeighborBytes>(value: value);
    set initial = map_count::<NeighborBytes>(map: t);
    set t^[first] = None<NeighborBytes>();
    set remaining = map_count::<NeighborBytes>(map: t);
  }
  if initial != 2_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if remaining != 1_u64 {
    return std::process::exit_status(code: 2_u8);
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
        ("wf__keyed_table_clear", "call void"),
        ("wf__keyed_table_swap", "call void"),
        ("wf__table_hold_release", "call void"),
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
}
