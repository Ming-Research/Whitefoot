//! [SHARE-3] which statements only read their state. A statement whose guard
//! and block write no path rooted at its binding may hold a map entry it uses
//! beside others that only read it; every other one holds it alone. A
//! statement classed as a reader that writes the entry would race with the
//! readers beside it, so each way of writing through the binding is a case.

use crate::SemanticOutcome;

use super::super::model::{CheckedFunction, CheckedStatement};
use super::with_semantics;

/// Whether each atomic statement in a block and the blocks it owns only
/// reads its state, in source order.
fn atomic_forms(statements: &[CheckedStatement], out: &mut Vec<bool>) {
    for statement in statements {
        match statement {
            CheckedStatement::Atomic { target, body, .. } => {
                out.push(target.reads);
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

/// Whether the one atomic statement of each named function only reads.
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
                    panic!("{name} holds one object: {forms:?}");
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

fn clear(observed: &Option<u8>) -> result: unit writes(observed) {
  set observed^ = None<u8>();
  return unit;
}

fn peek(value: &u8) -> result: u8 reads(value) {
  return value^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

/// Every way a block writes the entry through its binding makes the
/// statement hold the entry alone.
#[test]
fn a_statement_that_writes_through_its_binder_holds_its_entry_alone() {
    let source = format!(
        "{PRELUDE}{}",
        r#"
fn replaces(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    set map^[observed] = Some<u8>(value: 1_u8);
  }
  return unit;
}

fn writes_a_payload(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    match map^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    match map^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    clear(observed: &map^[observed]);
  }
  return unit;
}

fn writes_on_one_branch(store: &Shared<ConcurrentHashMap<u8>>, flag: Bool) -> result: unit reads(store) waits {
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    if flag {
      set map^[observed] = None<u8>();
    }
  }
  return unit;
}

fn writes_in_a_loop(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    for (i in 0_u64..3_u64) {
      match map^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    match map^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    let same = &map^[observed];
    set same^ = Some<u8>(value: 1_u8);
  }
  return unit;
}

fn writes_through_a_copied_payload(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    match map^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    match map^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    match map^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic map = &store^ {
    match map^[observed] {
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
