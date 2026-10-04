//! Deliberately damaged lowering must make the amendment-A witnesses fail.
//! These checks qualify the conformance observations independently of the
//! emitted instruction sequences they damage.

use super::{compile, compile_and_run};

fn witness(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/conformance/cases")
            .join(format!("share-pos-table-{name}.wf")),
    )
    .expect("read the normative witness")
}

#[test]
fn whole_table_witnesses_detect_read_selection_in_write_position() {
    for (name, expected) in [
        ("whole-binding-entry", 22),
        ("whole-binding-alias", 22),
        ("whole-binding-count-sees-own-writes", 21),
        ("whole-binding-passed-to-callee", 1),
        ("whole-binding-guard", 1),
    ] {
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
}

#[test]
fn entries_witness_detects_selection_of_one_slot_for_every_set_index() {
    let llvm = compile(&witness("whole-binding-entries-over-set"));
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
    let llvm = compile(&witness("whole-binding-swapped"));
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
    assert_ne!(compile_and_run(&damaged).status.code(), Some(2));
}

#[test]
fn plain_row_witness_is_distinguished_from_a_row_naming_the_table() {
    let source = witness("row-names-no-table");
    let source = String::from_utf8(source).expect("canonical source");
    let damaged = source.replace("writes(env.count)", "writes(env)");
    assert_ne!(damaged, source, "the row change was applied");
    assert_eq!(
        super::compile_rejection(damaged.as_bytes()).rule_id(),
        Some("SHARE-2")
    );
}

#[test]
fn local_table_selections_settle_before_shared_publication() {
    let source = br#"struct Store {
  map: KeyedTable<u8>;
}

const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let local = Store(map: move table);
  let first = &names[0_u64..1_u64];
  let second = &names[1_u64..2_u64];
  let t = &local.map;
  set t^[first] = Some<u8>(value: 9_u8);
  set t^[second] = None<u8>();
  let store = shared_new::<Store>(value: move local);
  let count = 0_u64;
  atomic s = &store, slot = &s^.map[first] {
    match slot^ {
      Some(value: held) => {
        if held^ != 9_u8 {
          return std::process::exit_status(code: 2_u8);
        }
      }
      None() => {
        return std::process::exit_status(code: 3_u8);
      }
    }
  }
  atomic s = &store, shared_table = &s^.map {
    set count = keyed_table_count::<u8>(table: shared_table);
  }
  let code = cvt.wrap::<u64, u8>(count);
  return std::process::exit_status(code: code);
}
"#;
    assert_eq!(compile_and_run(&compile(source)).status.code(), Some(1));
}
