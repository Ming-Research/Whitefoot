//! [SHARE-3] which statements holding a map's state hold their keys' entries
//! instead (`semantic::key_twins`). A statement that held entries while its
//! block reached another would run beside a statement on that entry, so each
//! way a block's keys can escape their twin is a case that must hold the
//! whole map, beside the shapes that hold their entries.

use crate::{IrInstruction, IrOperation};

use super::{function, with_ir};

/// How the one statement holding a map's state in `name` holds it: its
/// keys' entries, or the whole map.
#[derive(Debug, Eq, PartialEq)]
enum Held {
    Entries,
    Map,
}

fn held(source: &str, name: &str) -> Held {
    with_ir(source.as_bytes(), |program| {
        let operations = function(program, name)
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
            .filter_map(|instruction| match instruction {
                IrInstruction::Define { operation, .. } => Some(operation),
                _ => None,
            })
            .collect::<Vec<_>>();
        let entries = operations
            .iter()
            .filter(|operation| matches!(operation, IrOperation::SharedMapHoldKeys { .. }))
            .count();
        let maps = operations
            .iter()
            .filter(|operation| matches!(operation, IrOperation::SharedMapHold { .. }))
            .count();
        match (entries, maps) {
            (1, 0) => Held::Entries,
            (0, 1) => Held::Map,
            other => panic!("{name} holds one map's state once: {other:?}"),
        }
    })
}

const PRELUDE: &str = r#"const names: Array<u8, 4> =[119_u8, 120_u8, 121_u8, 122_u8];

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

fn program(functions: &str) -> String {
    format!("{PRELUDE}{functions}")
}

#[test]
fn keys_computed_from_a_loop_binder_hold_their_entries() {
    let source = program(
        r#"
fn fills(map: &SharedMap<u8>) -> result: unit reads(map) waits {
  atomic state = &map^ {
    for @fill (at in 1_u64..5_u64) {
      atomic slot = &state^[&names[0_u64..at]] {
        set slot^ = Some<u8>(value: 2_u8);
      }
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "fills"), Held::Entries);
}

#[test]
fn a_block_that_writes_a_local_no_key_reads_holds_its_entries() {
    let source = program(
        r#"
fn sums(map: &SharedMap<u8>) -> result: u8 reads(map) waits {
  let total = 0_u8;
  atomic state = &map^ {
    for @read (at in 1_u64..5_u64) {
      atomic slot = &state^[&names[0_u64..at]] {
        match slot^ {
          Some(value: seen) => {
            set total = total +wrap seen^;
          }
          None() => {
          }
        }
      }
    }
    if total > 9_u8 {
      set total = 9_u8;
    }
  }
  return total;
}
"#,
    );
    assert_eq!(held(&source, "sums"), Held::Entries);
}

#[test]
fn a_key_under_a_match_on_an_unwritten_value_holds_its_entry() {
    let source = program(
        r#"
fn fills_some(map: &SharedMap<u8>, wide: u64) -> result: unit reads(map) waits {
  atomic state = &map^ {
    if wide == 1_u64 {
      atomic slot = &state^[&names[0_u64..2_u64]] {
        set slot^ = Some<u8>(value: 2_u8);
      }
    } else {
      atomic slot = &state^[&names[0_u64..1_u64]] {
        set slot^ = Some<u8>(value: 1_u8);
      }
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "fills_some"), Held::Entries);
}

#[test]
fn a_block_that_counts_the_map_holds_the_map() {
    let source = program(
        r#"
fn counts(map: &SharedMap<u8>) -> result: u64 reads(map) waits {
  let counted = 0_u64;
  atomic state = &map^ {
    for @fill (at in 1_u64..5_u64) {
      atomic slot = &state^[&names[0_u64..at]] {
        set slot^ = Some<u8>(value: 2_u8);
      }
    }
    set counted = shared_map_count::<u8>(state: state);
  }
  return counted;
}
"#,
    );
    assert_eq!(held(&source, "counts"), Held::Map);
}

#[test]
fn a_block_that_copies_its_state_holds_the_map() {
    let source = program(
        r#"
fn copies(map: &SharedMap<u8>) -> result: unit reads(map) waits {
  atomic state = &map^ {
    let same = state;
    atomic slot = &same^[&names[0_u64..1_u64]] {
      set slot^ = Some<u8>(value: 6_u8);
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "copies"), Held::Map);
}

#[test]
fn a_key_under_a_match_on_a_value_the_block_writes_holds_the_map() {
    let source = program(
        r#"
fn follows(map: &SharedMap<u8>) -> result: unit reads(map) waits {
  let found = 0_u64;
  atomic state = &map^ {
    atomic slot = &state^[&names[0_u64..1_u64]] {
      match slot^ {
        Some(value: seen) => {
          set found = 1_u64;
        }
        None() => {
        }
      }
    }
    if found == 1_u64 {
      atomic slot = &state^[&names[0_u64..2_u64]] {
        set slot^ = Some<u8>(value: 2_u8);
      }
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "follows"), Held::Map);
}

#[test]
fn a_loop_bound_the_block_writes_holds_the_map() {
    let source = program(
        r#"
fn grows(map: &SharedMap<u8>) -> result: unit reads(map) waits {
  let limit = 2_u64;
  atomic state = &map^ {
    atomic slot = &state^[&names[0_u64..1_u64]] {
      match slot^ {
        Some(value: seen) => {
          set limit = 5_u64;
        }
        None() => {
        }
      }
    }
    if limit <= 5_u64 {
      for @fill (at in 1_u64..limit) {
        atomic slot = &state^[&names[0_u64..at]] {
          set slot^ = Some<u8>(value: 2_u8);
        }
      }
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "grows"), Held::Map);
}

#[test]
fn a_block_an_edge_leaves_holds_the_map() {
    let source = program(
        r#"
fn stops(map: &SharedMap<u8>) -> result: unit reads(map) waits {
  atomic state = &map^ {
    for @fill (at in 1_u64..5_u64) {
      atomic slot = &state^[&names[0_u64..at]] {
        match slot^ {
          Some(value: seen) => {
            return unit;
          }
          None() => {
            set slot^ = Some<u8>(value: 2_u8);
          }
        }
      }
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "stops"), Held::Map);
}

#[test]
fn a_block_of_one_object_statement_holds_its_entries() {
    let source = program(
        r#"
fn logs(map: &SharedMap<u8>, total: &Shared<u64>) -> result: unit reads(map), reads(total) waits {
  atomic state = &map^ {
    for @fill (at in 1_u64..5_u64) {
      atomic slot = &state^[&names[0_u64..at]] {
        set slot^ = Some<u8>(value: 2_u8);
      }
    }
    atomic sum = &total^ {
      set sum^ = sum^ +wrap 1_u64;
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "logs"), Held::Entries);
}

#[test]
fn a_block_of_two_object_statements_holds_the_map() {
    let source = program(
        r#"
fn adds(map: &SharedMap<u8>, total: &Shared<u64>) -> result: unit reads(map), reads(total) waits {
  let seen = 0_u64;
  atomic state = &map^ {
    atomic slot = &state^[&names[0_u64..1_u64]] {
      set slot^ = Some<u8>(value: 2_u8);
    }
    atomic sum = &total^ {
      set seen = sum^;
    }
    atomic sum = &total^ {
      set sum^ = seen +wrap 1_u64;
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "adds"), Held::Map);
}

#[test]
fn an_object_statement_in_a_loop_holds_the_map() {
    let source = program(
        r#"
fn logs_each(map: &SharedMap<u8>, total: &Shared<u64>) -> result: unit reads(map), reads(total) waits {
  atomic state = &map^ {
    for @fill (at in 1_u64..5_u64) {
      atomic slot = &state^[&names[0_u64..at]] {
        set slot^ = Some<u8>(value: 2_u8);
      }
      atomic sum = &total^ {
        set sum^ = sum^ +wrap 1_u64;
      }
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "logs_each"), Held::Map);
}

#[test]
fn a_key_under_a_call_that_reads_the_clock_holds_the_map() {
    let source = program(
        r#"
fn tick(wall: &std::time::WallClock) -> result: Bool reads(wall) {
  let reading = std::time::unix_nanoseconds(clock: wall);
  let half = reading / 2_i64;
  let twice = half *wrap 2_i64;
  return reading == twice;
}

fn picks(map: &SharedMap<u8>, wall: &std::time::WallClock) -> result: unit reads(map), reads(wall) waits {
  atomic state = &map^ {
    let even = tick(wall: wall);
    if even {
      atomic slot = &state^[&names[0_u64..1_u64]] {
        set slot^ = Some<u8>(value: 1_u8);
      }
    } else {
      atomic slot = &state^[&names[0_u64..2_u64]] {
        set slot^ = Some<u8>(value: 2_u8);
      }
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "picks"), Held::Map);
}

#[test]
fn a_key_under_a_call_that_answers_the_same_again_holds_its_entry() {
    let source = program(
        r#"
fn wide(count: u64) -> result: Bool pure {
  return count == 1_u64;
}

fn picks_again(map: &SharedMap<u8>, count: u64) -> result: unit reads(map) waits {
  atomic state = &map^ {
    let both = wide(count: count);
    if both {
      atomic slot = &state^[&names[0_u64..2_u64]] {
        set slot^ = Some<u8>(value: 2_u8);
      }
    } else {
      atomic slot = &state^[&names[0_u64..1_u64]] {
        set slot^ = Some<u8>(value: 1_u8);
      }
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "picks_again"), Held::Entries);
}

#[test]
fn a_call_kept_after_the_object_statement_holds_the_map() {
    let source = program(
        r#"
fn wide(count: u64) -> result: Bool pure {
  return count == 1_u64;
}

fn logs_first(map: &SharedMap<u8>, total: &Shared<u64>, count: u64) -> result: unit reads(map), reads(total) waits {
  atomic state = &map^ {
    atomic sum = &total^ {
      set sum^ = sum^ +wrap 1_u64;
    }
    let both = wide(count: count);
    if both {
      atomic slot = &state^[&names[0_u64..2_u64]] {
        set slot^ = Some<u8>(value: 2_u8);
      }
    } else {
      atomic slot = &state^[&names[0_u64..1_u64]] {
        set slot^ = Some<u8>(value: 1_u8);
      }
    }
  }
  return unit;
}
"#,
    );
    assert_eq!(held(&source, "logs_first"), Held::Map);
}
