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

/// Guards use the projected read row, including calls on whole states and
/// selected shards, rather than looking only for a direct dereference.
#[test]
fn guards_cannot_read_shards_through_callee_rows() {
    for (referent, expression, target) in [
        ("u8", "value^ != 0_u8", "part = &stats^[mine]"),
        ("Shards<u8>", "value^.len > 1_u64", "part = &stats^"),
    ] {
        let source = format!(
            r#"fn ready(value: &{referent}) -> result: Bool reads(value) {{
  return {expression};
}}

fn inspect(stats: &Shared<Shards<u8>>, mine: AnyShard) -> result: unit reads(stats) waits {{
  atomic {target} when ready(value: part) {{
    let seen = part;
  }}
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("a callee read of shards must be refused: {outcome:?}");
            };
            assert_eq!(issue.rule(), crate::SemanticRule::Share2);
            assert!(matches!(
                issue.kind(),
                crate::SemanticIssueKind::AtomicGuardReadsShards { mechanical_fix }
                    if mechanical_fix.contains("without a guard")
            ));
            let coordinate = issue.location().coordinate();
            let start = usize::try_from(coordinate.start().value()).expect("source offset");
            let end = usize::try_from(coordinate.end().value()).expect("source offset");
            assert_eq!(&source[start..end], "ready(value: part)");
        });
    }
}

/// Forming a reference to a shard is no state read when the guard's callee
/// has no read row. The target retains the selector expression for lowering.
#[test]
fn a_guard_that_does_not_read_its_shard_argument_checks() {
    let source = r#"fn ready(value: &u8) -> result: Bool pure {
  return True();
}

fn inspect(stats: &Shared<Shards<u8>>, mine: AnyShard) -> result: unit reads(stats) waits {
  atomic part = &stats^[mine] when ready(value: part) {
    set part^ = 1_u8;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a guard with no shard read must check: {outcome:?}");
        };
        let function = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "inspect")
            .expect("inspect is checked");
        let CheckedStatement::Atomic { targets, .. } =
            &function.body.as_ref().expect("inspect has a body")[0]
        else {
            panic!("inspect starts with an atomic statement");
        };
        assert_eq!(targets.len(), 1);
        assert!(matches!(
            targets[0].kind,
            super::super::model::CheckedTargetKind::Shard(_)
        ));
        assert_eq!(
            targets[0].referent,
            super::super::model::CheckedType::Integer(super::super::model::IntegerType::U8),
        );
    });
}

/// An owned noncopy index is refused by SHARE-2 before OWN-1 asks for move.
#[test]
fn a_shard_index_must_be_a_selector_before_ownership_is_checked() {
    let source = r#"fn inspect(stats: &Shared<Shards<u8>>, bad: Box<u8>) -> result: unit reads(stats) waits {
  atomic part = &stats^[bad] {
    set part^ = 1_u8;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("a Box shard selector must be refused: {outcome:?}");
        };
        assert_eq!(issue.rule(), crate::SemanticRule::Share2);
        assert!(matches!(
            issue.kind(),
            crate::SemanticIssueKind::AtomicTargetNotShared { mechanical_fix, .. }
                if mechanical_fix.contains("let mine = any_shard();")
        ));
        let coordinate = issue.location().coordinate();
        let start = usize::try_from(coordinate.start().value()).expect("source offset");
        let end = usize::try_from(coordinate.end().value()).expect("source offset");
        assert_eq!(&source[start..end], "bad");
    });
}

/// Phase 1 checks the fixed run fixtures independently of their lowering.
#[test]
fn all_shards_run_cases_pass_the_checker() {
    for source in [
        include_bytes!("../../../../tests/conformance/cases/share-pos-shards-count.wf").as_slice(),
        include_bytes!(
            "../../../../tests/conformance/cases/share-pos-shards-start-from-maker.wf"
        )
        .as_slice(),
        include_bytes!("../../../../tests/conformance/cases/share-pos-shards-owned-buffers.wf")
            .as_slice(),
        include_bytes!("../../../../tests/conformance/cases/share-pos-shards-maximum.wf").as_slice(),
        include_bytes!(
            "../../../../tests/conformance/cases/share-pos-shards-beside-map-entry.wf"
        )
        .as_slice(),
        include_bytes!("../../../../tests/conformance/cases/share-pos-shards-whole-to-callee.wf")
            .as_slice(),
    ] {
        with_semantics(source, |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{outcome:?}"
            );
        });
    }
}

/// Every way a block writes through its binder makes the statement hold the
/// entry alone.
#[test]
fn a_statement_that_writes_through_its_binder_holds_its_entry_alone() {
    let source = format!(
        "{PRELUDE}{}",
        r#"
fn replaces(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
    set slot^ = Some<u8>(value: 1_u8);
  }
  return unit;
}

fn writes_a_payload(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
    clear(observed: slot);
  }
  return unit;
}

fn writes_on_one_branch(store: &Shared<ConcurrentHashMap<u8>>, flag: Bool) -> result: unit reads(store) waits {
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
    if flag {
      set slot^ = None<u8>();
    }
  }
  return unit;
}

fn writes_in_a_loop(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
    let same = slot;
    set same^ = Some<u8>(value: 1_u8);
  }
  return unit;
}

fn writes_through_a_copied_payload(store: &Shared<ConcurrentHashMap<u8>>) -> result: unit reads(store) waits {
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
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
  let observed = &key[0_u64..2_u64];
  atomic slot = &store^[observed] {
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

/// Both source and internally formed prelude nominals follow PRE-1's order.
#[test]
fn atomic_prelude_type_order_keeps_enums_after_shards_and_the_selector() {
    with_semantics(
        br#"fn inspect(shards: &Shared<Shards<u8>>, selector: &Shared<AnyShard>, flag: &Shared<Bool>, option: &Shared<Option<u8>>, outcome: &Shared<Result<u8, u8>>, overflow: &Shared<Overflow>, division: &Shared<DivError>, narrow: &Shared<NarrowError>, scan: &Shared<ScanStep>) -> result: unit reads(shards), reads(selector), reads(flag), reads(option), reads(outcome), reads(overflow), reads(division), reads(narrow), reads(scan) waits {
  atomic a = &shards^, b = &selector^, c = &flag^, d = &option^, e = &outcome^, f = &overflow^, g = &division^, h = &narrow^, i = &scan^ {
    let count = a^.len;
    let selected = b^;
    let boolean = c^;
    let optional = d^;
    let returned = e^;
    let overflowed = f^;
    let divided = g^;
    let narrowed = h^;
    let scanned = i^;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("the prelude states must check: {outcome:?}");
            };
            let inspect = checked
                .data
                .executable_functions()
                .find(|function| function.name == "inspect")
                .expect("inspect");
            let targets = inspect
                .body
                .as_deref()
                .expect("body")
                .iter()
                .find_map(|statement| match statement {
                    CheckedStatement::Atomic { targets, .. } => Some(targets),
                    _ => None,
                })
                .expect("targets");
            let ranks = targets
                .iter()
                .map(|target| target.lock_order[0].as_str())
                .collect::<Vec<_>>();
            assert_eq!(
                ranks,
                ["023", "024", "025", "026", "027", "028", "029", "030", "031"]
            );
        },
    );
}

/// The implementation's type order compares const arguments by numeric value.
#[test]
fn atomic_type_order_uses_const_values() {
    with_semantics(
        br#"fn make_box<T: drop>(value: T) -> result: Box<T> pure {
  return box_new::<T>(value: move value);
}

struct Sized<const n: u64> {
  value: u8;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let two_value = Sized<2>(value: 0_u8);
  let ten_value = Sized<10>(value: 0_u8);
  let two = shared_new::<Sized<2>>(value: two_value);
  let ten = shared_new::<Sized<10>>(value: ten_value);
  let boxed = make_box::<u8>(value: 0_u8);
  let cell = shared_new::<Box<u8>>(value: move boxed);
  atomic a = &ten, b = &two, c = &cell {
    set a^.value = 1_u8;
    set b^.value = 2_u8;
    set c^.inner = 3_u8;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("must check: {outcome:?}");
            };
            // [TYPE-9] neither a symbolic Box<T> nor its concrete Box<u8>
            // instance carries a region that could separate shared-state roots.
            let mut boxes = 0;
            for nominal in &checked.data.nominals {
                if let super::super::model::CheckedNominalKind::Box { region, .. } = nominal.kind {
                    assert!(region.is_none(), "Box carries no brand");
                    assert!(checked.data.nominal_confinement[nominal.id.0 as usize].is_empty());
                    boxes += 1;
                }
            }
            assert!(boxes >= 2, "symbolic and concrete Boxes were checked");
            let main = checked
                .data
                .functions
                .iter()
                .find(|f| f.name == "main")
                .expect("main");
            let targets = main
                .body
                .as_deref()
                .expect("body")
                .iter()
                .find_map(|s| {
                    if let CheckedStatement::Atomic { targets, .. } = s {
                        Some(targets)
                    } else {
                        None
                    }
                })
                .expect("targets");
            assert!(
                targets[1].lock_order < targets[0].lock_order,
                "capacity 2 must precede 10"
            );
            assert!(
                targets[2].lock_order < targets[1].lock_order,
                "prelude Box must precede a source nominal"
            );
        },
    );
}

/// Host-module nominals share the module-path/name order with source nominals.
#[test]
fn atomic_type_order_uses_host_module_path_then_name() {
    with_semantics(
        br#"alias ExitStatus = std::process::ExitStatus;
alias ArgError = std::text::ArgError;

fn main() -> status: ExitStatus pure waits {
  let process_value = std::process::exit_status(code: 0_u8);
  let process = shared_new::<ExitStatus>(value: move process_value);
  let error_value = ArgError::InvalidIndex();
  let environment = shared_new::<ArgError>(value: error_value);
  atomic p = &process, e = &environment {
    set p^ = std::process::exit_status(code: 1_u8);
    let error = e^;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("host states must check: {outcome:?}");
            };
            let main = checked
                .data
                .functions
                .iter()
                .find(|f| f.name == "main")
                .expect("main");
            let targets = main
                .body
                .as_deref()
                .expect("body")
                .iter()
                .find_map(|s| {
                    if let CheckedStatement::Atomic { targets, .. } = s {
                        Some(targets)
                    } else {
                        None
                    }
                })
                .expect("targets");
            assert_eq!(targets[0].lock_order, ["100", "std.process", "ExitStatus"]);
            assert_eq!(targets[1].lock_order, ["100", "std.text", "ArgError"]);
            assert!(targets[0].lock_order < targets[1].lock_order);
        },
    );
}
