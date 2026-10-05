//! [SHARE-3] which entry bindings only read their entry. A statement whose
//! guard and block write no path rooted at an entry binder may hold that
//! entry beside others that only read it; every other one holds it alone. A
//! binding classed as a reader whose entry is written would race with the
//! readers beside it, so each way of writing through the binder is a case.

use crate::SemanticOutcome;

use super::super::model::{CheckedFunction, CheckedStatement};
use super::with_semantics;

/// Whether each entry binding of the atomic statements in a block and the
/// blocks it owns only reads, in source order.
fn atomic_forms(statements: &[CheckedStatement], out: &mut Vec<bool>) {
    for statement in statements {
        match statement {
            CheckedStatement::Atomic { targets, body, .. } => {
                out.extend(targets.iter().map(|entry| entry.reads));
                atomic_forms(body, out);
            }
            CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
                for arm in arms {
                    atomic_forms(&arm.body, out);
                }
            }
            CheckedStatement::Loop { body, .. } | CheckedStatement::CountedRange { body, .. } => {
                atomic_forms(body, out);
            }
            _ => {}
        }
    }
}

/// Whether the one entry binding of each named function only reads.
fn reads_of(source: &str, names: &[&str]) -> Vec<bool> {
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the program must check: {outcome:?}");
        };
        let functions: &[CheckedFunction] = &checked.data.functions;
        names
            .iter()
            .map(|name| {
                let function = functions
                    .iter()
                    .find(|function| function.name == *name)
                    .unwrap_or_else(|| panic!("{name} is checked"));
                let mut forms = Vec::new();
                atomic_forms(function.body.as_deref().unwrap_or_default(), &mut forms);
                let [reads] = forms.as_slice() else {
                    panic!("{name} binds one entry: {forms:?}");
                };
                *reads
            })
            .collect()
    })
}

const PRELUDE: &str = r#"const key: Array<u8, 2> =[107_u8, 49_u8];

struct Pair {
  left: u8;
  right: u8;
}

fn bump(value: &u8) -> result: unit writes(value) {
  set value^ = value^ +wrap 1_u8;
  return unit;
}

fn clear(held: &Option<u8>) -> result: unit writes(held) {
  set held^ = None<u8>();
  return unit;
}

fn peek(value: &u8) -> result: u8 reads(value) {
  return value^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

/// Every way a block writes through its binder makes the statement hold the
/// entry alone.
#[test]
fn a_statement_that_writes_through_its_binder_holds_its_entry_alone() {
    let source = format!(
        "{PRELUDE}{}",
        r#"
fn replaces(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    set slot^ = Some<u8>(value: 1_u8);
  }
  return unit;
}

fn writes_a_payload(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    match slot^ {
      Some(value: seen) => {
        set seen^ = 2_u8;
      }
      None() => {
      }
    }
  }
  return unit;
}

fn calls_a_payload_writer(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    match slot^ {
      Some(value: seen) => {
        bump(value: seen);
      }
      None() => {
      }
    }
  }
  return unit;
}

fn calls_an_entry_writer(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    clear(held: slot);
  }
  return unit;
}

fn writes_on_one_branch(store: &Shared<ConcurrentHashMap<u8>>, flag: Bool) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    if flag {
      set slot^ = None<u8>();
    }
  }
  return unit;
}

fn writes_in_a_loop(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    for (i in 0_u64..3_u64) {
      match slot^ {
        Some(value: seen) => {
          set seen^ = seen^ +wrap 1_u8;
        }
        None() => {
        }
      }
    }
  }
  return unit;
}

fn writes_a_field(store: &Shared<ConcurrentHashMap<Pair>>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    match slot^ {
      Some(value: pair) => {
        set pair^.right = 3_u8;
      }
      None() => {
      }
    }
  }
  return unit;
}

fn writes_through_a_copy(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    let same = slot;
    set same^ = Some<u8>(value: 1_u8);
  }
  return unit;
}

fn writes_through_a_copied_payload(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    match slot^ {
      Some(value: seen) => {
        let same = seen;
        set same^ = 2_u8;
      }
      None() => {
      }
    }
  }
  return unit;
}
"#
    );
    let names = [
        "replaces",
        "writes_a_payload",
        "calls_a_payload_writer",
        "calls_an_entry_writer",
        "writes_on_one_branch",
        "writes_in_a_loop",
        "writes_a_field",
        "writes_through_a_copy",
        "writes_through_a_copied_payload",
    ];
    let readers: Vec<_> = names
        .iter()
        .zip(reads_of(&source, &names))
        .filter_map(|(name, reads)| reads.then_some(*name))
        .collect();
    assert!(
        readers.is_empty(),
        "these write their entry, so each must hold it alone: {readers:?}"
    );
}

/// A block that writes only places outside the entry reads it beside other
/// such statements.
#[test]
fn a_statement_that_writes_only_other_places_reads_its_entry() {
    let source = format!(
        "{PRELUDE}{}",
        r#"
fn copies_out(store: &Shared<ConcurrentHashMap<u8>>, out: &u8) -> result: unit reads(store), writes(out) waits {
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    match slot^ {
      Some(value: seen) => {
        set out^ = seen^;
      }
      None() => {
      }
    }
  }
  return unit;
}

fn calls_a_reader(store: &Shared<ConcurrentHashMap<u8>>) -> result: u8 reads(store) waits {
  let total = 0_u8;
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    match slot^ {
      Some(value: seen) => {
        set total = peek(value: seen);
      }
      None() => {
      }
    }
  }
  return total;
}

fn reads_a_field(store: &Shared<ConcurrentHashMap<Pair>>) -> result: u8 reads(store) waits {
  let right = 0_u8;
  let held = &key[0_u64..2_u64];
  atomic slot = &store^[held] {
    match slot^ {
      Some(value: pair) => {
        set right = pair^.right;
      }
      None() => {
      }
    }
  }
  return right;
}
"#
    );
    let names = ["copies_out", "calls_a_reader", "reads_a_field"];
    let writers: Vec<_> = names
        .iter()
        .zip(reads_of(&source, &names))
        .filter_map(|(name, reads)| (!reads).then_some(*name))
        .collect();
    assert!(
        writers.is_empty(),
        "these write nothing of their entry, so each reads it: {writers:?}"
    );
}
