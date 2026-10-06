//! Deliberately damaged lowering must make the amendment-S witnesses fail.
//! These checks qualify the conformance observations independently of the
//! emitted instruction sequences they damage.

use super::{compile, compile_and_run};

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
