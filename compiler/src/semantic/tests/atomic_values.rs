//! Atomic-value direct use, path composition and ordinary effects [SHARE-2].

use super::with_semantics;
use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

const MAIN: &str = r#"
fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

fn check(source: &str, expected: Option<SemanticRule>) {
    let source = format!("{source}{MAIN}");
    with_semantics(source.as_bytes(), |outcome| match (outcome, expected) {
        (SemanticOutcome::Complete(_), None) => {}
        (SemanticOutcome::SourceIssue { issue, .. }, Some(rule)) => {
            assert_eq!(issue.rule(), rule, "{issue:?}");
            if rule == SemanticRule::Share2 {
                assert!(
                    matches!(issue.kind(), SemanticIssueKind::AtomicValueUse { .. }),
                    "{issue:?}"
                );
            }
        }
        (outcome, expected) => panic!("expected {expected:?}, got {outcome:?}"),
    });
}

#[test]
fn mutually_exclusive_operations_across_distinct_targets_are_one_per_path() {
    check(
        r#"
fn choose(first: &Shared<AtomicValue>, second: &Shared<AtomicValue>, flag: Bool) -> result: unit reads(first), reads(second) waits {
  atomic a = &first^, b = &second^ {
    if flag {
      atomic_add(cell: a, value: 1_u64);
    } else {
      atomic_raise(cell: b, value: 19_u64);
    }
  }
  return unit;
}
"#,
        None,
    );
}

#[test]
fn a_returning_arm_does_not_spend_the_continuing_paths_operation() {
    check(
        r#"
fn choose(first: &Shared<AtomicValue>, flag: Bool) -> result: unit reads(first) waits {
  atomic a = &first^ {
    if flag {
      atomic_add(cell: a, value: 1_u64);
      return unit;
    }
    atomic_load(cell: a);
  }
  return unit;
}
"#,
        None,
    );
}

#[test]
fn atomic_statements_inside_a_loop_get_separate_operation_counts() {
    check(
        r#"
fn repeat(first: &Shared<AtomicValue>) -> result: unit reads(first) waits {
  for (position in 0_u64..2_u64) {
    atomic a = &first^ {
      atomic_add(cell: a, value: 1_u64);
    }
  }
  return unit;
}
"#,
        None,
    );
}

#[test]
fn a_loop_before_the_operation_keeps_the_fallthrough_path() {
    check(
        r#"
fn work(first: &Shared<AtomicValue>, flag: Bool) -> result: unit reads(first) waits {
  atomic a = &first^ {
    loop {
      if flag { break; }
      return unit;
    }
    atomic_load(cell: a);
  }
  return unit;
}
"#,
        None,
    );
}

#[test]
fn sequential_calls_and_branch_then_call_are_refused() {
    for body in [
        "atomic_load(cell: a); atomic_add(cell: a, value: 1_u64);",
        "if flag { atomic_load(cell: a); } atomic_add(cell: a, value: 1_u64);",
    ] {
        check(
            &format!(
                r#"
fn work(first: &Shared<AtomicValue>, flag: Bool) -> result: unit reads(first) waits {{
  atomic a = &first^ {{ {body} }}
  return unit;
}}
"#
            ),
            Some(SemanticRule::Share2),
        );
    }
}

#[test]
fn distinct_atomic_targets_share_one_path_count() {
    check(
        r#"
fn work(first: &Shared<AtomicValue>, second: &Shared<AtomicValue>) -> result: unit reads(first), reads(second) waits {
  atomic a = &first^, b = &second^ {
    atomic_load(cell: a);
    atomic_add(cell: b, value: 1_u64);
  }
  return unit;
}
"#,
        Some(SemanticRule::Share2),
    );
}

#[test]
fn a_value_initializer_give_keeps_the_operation_for_its_continuation() {
    check(
        r#"
fn work(first: &Shared<AtomicValue>, flag: Bool) -> result: unit reads(first) waits {
  atomic a = &first^ {
    let previous = if flag {
      let value = atomic_load(cell: a);
      give value;
    } else {
      give 0_u64;
    }
    atomic_raise(cell: a, value: previous);
  }
  return unit;
}
"#,
        Some(SemanticRule::Share2),
    );
}

#[test]
fn even_a_single_iteration_loop_cannot_contain_an_operation() {
    check(
        r#"
fn work(first: &Shared<AtomicValue>) -> result: unit reads(first) waits {
  atomic a = &first^ {
    for (position in 0_u64..1_u64) {
      atomic_load(cell: a);
    }
  }
  return unit;
}
"#,
        Some(SemanticRule::Share2),
    );
}

#[test]
fn aliases_and_helper_arguments_cannot_hide_an_operation() {
    check(
        r#"
fn work(first: &Shared<AtomicValue>) -> result: unit reads(first) waits {
  atomic a = &first^ {
    let alias_value = a;
    atomic_load(cell: alias_value);
  }
  return unit;
}
"#,
        Some(SemanticRule::Share2),
    );
    check(
        r#"
fn observe(value: &AtomicValue) -> result: unit pure { return unit; }
fn work(first: &Shared<AtomicValue>) -> result: unit reads(first) waits {
  atomic a = &first^ {
    observe(value: a);
  }
  return unit;
}
"#,
        Some(SemanticRule::Share2),
    );
}

#[test]
fn operations_are_refused_in_helpers_and_guards() {
    check(
        r#"
fn observe(value: &AtomicValue) -> result: u64 reads(value) {
  return atomic_load(cell: value);
}
"#,
        Some(SemanticRule::Share2),
    );
    check(
        r#"
fn atomic_guard(value: &AtomicValue) -> result: Bool pure {
  return True();
}
fn work(first: &Shared<AtomicValue>) -> result: unit reads(first) waits {
  atomic a = &first^ when atomic_guard(value: a) {
    atomic_raise(cell: a, value: 1_u64);
  }
  return unit;
}
"#,
        Some(SemanticRule::Share2),
    );
}

#[test]
fn scalar_result_contracts_discharge_later_bounded_raises() {
    check(
        r#"
fn work(first: &Shared<AtomicValue>, second: &Shared<AtomicValue>) -> result: unit reads(first), reads(second) waits {
  let observed = 0_u64;
  atomic a = &first^ { set observed = atomic_load(cell: a); }
  atomic b = &second^ { atomic_raise(cell: b, value: observed); }
  return unit;
}
"#,
        None,
    );
}

#[test]
fn arguments_beyond_the_protocol_bounds_are_refused_by_contracts() {
    check(
        r#"
fn work() -> result: unit pure {
  let first = shared_atomic_new(value: 9223372036854775809_u64);
  return unit;
}
"#,
        Some(SemanticRule::Fn8),
    );
    for call in [
        "atomic_add(cell: a, value: 4294967297_u64);",
        "atomic_raise(cell: a, value: 9223372036854775809_u64);",
    ] {
        check(
            &format!(
                r#"
fn work(first: &Shared<AtomicValue>) -> result: unit reads(first) waits {{
  atomic a = &first^ {{ {call} }}
  return unit;
}}
"#
            ),
            Some(SemanticRule::Fn8),
        );
    }
}

#[test]
fn atomic_state_cannot_be_embedded_inline() {
    check(
        r#"
struct BadState { cell: AtomicValue; }
"#,
        Some(SemanticRule::Type9),
    );
}

#[test]
fn atomic_value_bindings_are_not_proof_expressions() {
    check(
        r#"
fn work(first: &Shared<AtomicValue>) -> result: unit reads(first) waits {
  atomic a = &first^ {
    invariant invalid_state: a == a;
    atomic_load(cell: a);
  }
  return unit;
}
"#,
        Some(SemanticRule::Share2),
    );
}

#[test]
fn checked_atomic_targets_retain_their_kind_and_ordinary_read_authority() {
    use crate::semantic::model::{CheckedStatement, CheckedTargetKind};
    let source = format!(
        r#"
fn observe_shared(first: &SharedRead<AtomicValue>) -> result: unit reads(first) waits {{
  atomic a = &first^ {{ atomic_load(cell: a); }}
  return unit;
}}
fn update_shared(first: &Shared<AtomicValue>) -> result: unit reads(first) waits {{
  atomic a = &first^ {{ atomic_add(cell: a, value: 1_u64); }}
  return unit;
}}
{MAIN}"#
    );
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("atomic target classification must check: {outcome:?}");
        };
        for (name, reads) in [("observe_shared", true), ("update_shared", false)] {
            let function = checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .expect("the requested function exists");
            let Some(CheckedStatement::Atomic { targets, .. }) =
                function.body.as_ref().and_then(|body| body.first())
            else {
                panic!("{name} starts with an atomic statement");
            };
            assert_eq!(targets.len(), 1);
            assert!(matches!(targets[0].kind, CheckedTargetKind::AtomicValue));
            assert_eq!(targets[0].reads, reads);
            assert_eq!(targets[0].state.measured(), None);
        }
    });
}
