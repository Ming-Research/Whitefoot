//! [TYPE-5] and [GIVE-1] derivation: a `let` binder's mode and type come
//! from its selected right-hand side, never from a written annotation, and a
//! value initializer's come from its delivery set.

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::{assert_rule, assert_rule_at, assert_rule_kind, with_semantics};

/// The derived type is the operand's own, so a right-hand side that produces
/// a different type than the use site wants is still caught — by the
/// consuming construct's rule ([FN-1] at `return`), not by the vanished
/// annotation.
#[test]
fn a_derived_binding_still_faces_its_consumer_s_exactness_rule() {
    assert_rule(
        br#"fn answer() -> result: i32 pure {
  let value = 40_i64;
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Fn1,
        SemanticIssueKind::ReturnMismatch,
    );
}

/// [GIVE-1] derivation is agreement over the closed delivery set, never a
/// join or a widening, and the citation lands on the *later* `give`.
#[test]
fn a_second_give_of_another_type_rejects_at_that_give() {
    assert_rule_kind(
        br#"fn choose(flag: Option<i32>) -> result: unit pure {
  let picked = match flag {
    Some(value: inner) => {
      give inner;
    }
    None() => {
      give 0_i64;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Give1,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

/// [GIVE-1] an empty delivery set — every arm leaves by `return` — has no
/// type to derive, and its repair is the statement form with the binding
/// dropped, so the citation is at the `let_stmt` rather than at any arm. The
/// statements after it are unreachable once the binding is gone [FN-1], so
/// the repair deletes them too [DIAG-1].
#[test]
fn an_empty_delivery_set_rejects_at_the_let_statement() {
    assert_rule(
        br#"fn choose(flag: Option<i32>) -> result: i32 pure {
  let picked = match flag {
    Some(value: inner) => {
      return inner;
    }
    None() => {
      return 0_i32;
    }
  }
  return picked;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Give1,
        SemanticIssueKind::EmptyDeliverySet {
            binding: "picked".to_owned(),
            mechanical_fix: "every arm leaves by `return` or `break`, so no value reaches `picked`: drop `let picked =`, write the `match` as a statement, and delete the statements after it in this block, which no path reaches".to_owned(),
        },
    );
}

/// The round-3 blocker's own witness. `None()` carries no operand, and after
/// the annotation is deleted the written arguments are the only supply there
/// is; [TYPE-5] therefore makes them mandatory in every position.
#[test]
fn a_nullary_prelude_construction_types_itself_from_written_arguments() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let absent = None<Array<u8, 2>>();
  let present = Some<i32>(value: 7_i32);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("written prelude arguments must type the construction: {outcome:?}");
        };
        let names = checked
            .data
            .nominals
            .iter()
            .map(|nominal| nominal.name.as_str())
            .collect::<Vec<_>>();
        for expected in ["Option<Array<u8, 2>>", "Option<i32>"] {
            assert!(
                names.contains(&expected),
                "missing instance {expected} derived from written arguments: {names:?}"
            );
        }
    });
}

#[test]
fn a_prelude_construction_without_its_arguments_rejects_at_the_construct() {
    assert_rule_kind(
        br#"fn main() -> status: std::process::ExitStatus pure {
  let absent = None();
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

#[test]
fn a_result_construction_writes_both_of_its_arguments() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let good = Ok<i32, Overflow>(value: 1_i32);
  let flag = Overflow();
  let bad = Err<i32, Overflow>(error: flag);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("written Result arguments must type both variants: {outcome:?}");
        };
        let names = checked
            .data
            .nominals
            .iter()
            .map(|nominal| nominal.name.as_str())
            .collect::<Vec<_>>();
        assert!(
            names.contains(&"Result<i32, Overflow>"),
            "missing Result instance derived from written arguments: {names:?}"
        );
    });
}

/// [OP-2] a written type argument on a deleted-class operation cites OP-1.
/// This is the judgment that inverted: v0.22 cited FN-2 for its *absence*.
#[test]
fn a_written_type_argument_on_a_derived_operation_rejects() {
    assert_rule(
        // The written argument is the violation, so deleting it — which is
        // what A1 does to a legal call — leaves nothing to cite.
        br#"fn smaller(x: i32, y: i32) -> result: i32 pure {
  return imin::<i32>(x, y);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
}

/// [OP-2] "Operands of two different exact types are a hard error citing
/// TYPE-5 at the second operand atom in source order" — so the citation is
/// pinned to those bytes, not merely to the rule.
#[test]
fn disagreeing_operands_cite_type5_at_the_second_operand_atom() {
    assert_rule_at(
        br#"fn smaller(x: i32, y: i64) -> result: i32 pure {
  return imin(x, y);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Type5,
        "y",
    );
}

/// The first operand fixes the row, so an operand type outside the closed
/// integer set is the *selection* failing, which cites OP-1 rather than the
/// per-operand TYPE-5.
#[test]
fn a_first_operand_outside_the_closed_set_cites_op1() {
    assert_rule(
        br#"fn smaller(x: Bool, y: Bool) -> result: Bool pure {
  return imin(x, y);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
}

// Retired with v0.59's regions: `cell_content_that_bears_a_region_still_rejects_under_stor5`
// had a region-bearing `Box` content as its subject, and v0.60 [STOR-5] states
// the reference-free-storage rule instead, whose successor witnesses belong to
// the reference suites.
