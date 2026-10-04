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
            CheckedStatement::Atomic { entries, body, .. } => {
                out.extend(entries.iter().map(|entry| entry.reads));
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

struct Bytes {
  map: KeyedTable<u8>;
}

struct Pairs {
  map: KeyedTable<Pair>;
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
fn replaces(store: &Shared<Bytes>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
    set slot^ = Some<u8>(value: 1_u8);
  }
  return unit;
}

fn writes_a_payload(store: &Shared<Bytes>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
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

fn calls_a_payload_writer(store: &Shared<Bytes>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
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

fn calls_an_entry_writer(store: &Shared<Bytes>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
    clear(held: slot);
  }
  return unit;
}

fn writes_on_one_branch(store: &Shared<Bytes>, flag: Bool) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
    if flag {
      set slot^ = None<u8>();
    }
  }
  return unit;
}

fn writes_in_a_loop(store: &Shared<Bytes>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
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

fn writes_a_field(store: &Shared<Pairs>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
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

fn writes_through_a_copy(store: &Shared<Bytes>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
    let same = slot;
    set same^ = Some<u8>(value: 1_u8);
  }
  return unit;
}

fn writes_through_a_copied_payload(store: &Shared<Bytes>) -> result: unit reads(store) waits {
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
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
fn copies_out(store: &Shared<Bytes>, out: &u8) -> result: unit reads(store), writes(out) waits {
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
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

fn calls_a_reader(store: &Shared<Bytes>) -> result: u8 reads(store) waits {
  let total = 0_u8;
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
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

fn reads_a_field(store: &Shared<Pairs>) -> result: u8 reads(store) waits {
  let right = 0_u8;
  let held = &key[0_u64..2_u64];
  atomic s = &store^, slot = &s^.map[held] {
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

/// A prepared source exposes its ordinary readonly owner and uses the
/// original span count to bound its entry binding.
#[test]
fn prepared_keys_keep_the_source_and_publish_its_span_count() {
    let source = r#"fn restore(prepared: PreparedKeys) -> source: KeySource pure {
  return move prepared.source;
}

fn discard(prepared: PreparedKeys) -> result: unit pure {
  return unit;
}

fn invalid_span(source: KeySource) -> result: KeyPrepareError pure {
  return InvalidSpan(source: move source, index: 0_u64);
}

fn duplicate(source: KeySource) -> result: KeyPrepareError pure {
  return Duplicate(source: move source, first: 0_u64, second: 1_u64);
}

fn inspect(store: &Shared<KeyedTable<u8>>, prepared: &PreparedKeys) -> result: unit reads(store), reads(prepared) waits {
  atomic state = &store^, entries = &state^[prepared^] {
    invariant count: entries^.len == prepared^.source.spans.inner.len;
    for @read (at in 0_u64..prepared^.source.spans.inner.len) {
      match entries^[at] {
        Some(value: seen) => {
          let observed = seen^;
        }
        None() => {
        }
      }
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let bytes = box_slots_new::<u8>(capacity: 0_u64);
  let spans = box_slots_new::<KeySpan>(capacity: 0_u64);
  let source = KeySource(bytes: move bytes, spans: move spans);
  let outcome = key_prepare(source: move source);
  match move outcome {
    Ok(value: prepared) => {
      let returned = restore(prepared: move prepared);
    }
    Err(error: failure) => {
      match move failure {
        InvalidSpan(source: returned, index: bad) => {
        }
        Duplicate(source: returned, first: first_index, second: second_index) => {
        }
      }
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("ordinary source storage and prepared keys check: {outcome:?}");
        };
        let restore = checked
            .data
            .executable_functions()
            .find(|function| function.name == "restore")
            .expect("restore checks");
        let [
            CheckedStatement::Return {
                value:
                    crate::semantic::CheckedExpression::Project {
                        consume_root,
                        residual_drops,
                        ..
                    },
                ..
            },
        ] = restore.body.as_deref().expect("restore body")
        else {
            panic!("return moves the prepared source");
        };
        assert!(
            *consume_root,
            "moving the source consumes the prepared owner"
        );
        let [order] = residual_drops.as_slice() else {
            panic!("only the hidden order remains: {residual_drops:?}");
        };
        assert!(order.prepared_order && order.fields.is_empty());
        let discard = checked
            .data
            .executable_functions()
            .find(|function| function.name == "discard")
            .expect("discard checks");
        let [CheckedStatement::Return { drops, .. }] =
            discard.body.as_deref().expect("discard body")
        else {
            panic!("discard has one return");
        };
        let [whole] = drops.as_slice() else {
            panic!("a complete prepared owner has one release: {drops:?}");
        };
        assert!(whole.fields.is_empty());

        assert!(
            checked
                .data
                .executable_functions()
                .find(|function| function.name == "key_prepare")
                .expect("key_prepare checks")
                .allocates
        );
    });
    let wrong_count = source.replace(
        "invariant count: entries^.len ==",
        "invariant count: entries^.len <",
    );
    super::assert_rule_kind(wrong_count.as_bytes(), crate::SemanticRule::Inv1, |_| true);
    let after_move = source.replace("  return move prepared.source;", "  let returned_source = move prepared.source;\n  let reused = prepared.source.spans.inner.len;\n  return move returned_source;");
    super::assert_rule_kind(after_move.as_bytes(), crate::SemanticRule::Own1, |_| true);
}

/// Prepared sources cannot be fabricated or changed through their readonly
/// field; preparation is the constructor that establishes the key invariants.
#[test]
fn prepared_keys_refuse_fabrication_and_source_writes() {
    let source = r#"fn fabricate(source: KeySource) -> prepared: PreparedKeys pure {
  return PreparedKeys(source: move source);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    super::assert_rule_at(
        source.as_bytes(),
        crate::SemanticRule::Type2,
        "PreparedKeys(source: move source)",
    );
    let write = r#"fn rewrite(prepared: &PreparedKeys, source: KeySource) -> result: unit writes(prepared.source) {
  set prepared^.source = move source;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    super::assert_rule_kind(write.as_bytes(), crate::SemanticRule::Type2, |_| true);
}

/// Preparing an already owned source takes only private runtime storage. It
/// keeps the EFF-3 allocation bit without using the program heap [STOR-8].
#[test]
fn key_preparation_marks_runtime_allocation_without_requiring_the_program_heap() {
    let source = br#"program no_heap;

fn prepare(source: KeySource) -> result: Result<PreparedKeys, KeyPrepareError> pure {
  return key_prepare(source: move source);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("preparation uses no program allocator: {outcome:?}");
        };
        for name in ["key_prepare", "prepare"] {
            let function = checked
                .data
                .executable_functions()
                .find(|function| function.name == name)
                .expect("preparation checks");
            assert!(
                function.allocates,
                "{name} retains runtime allocation metadata"
            );
        }
    });
}
