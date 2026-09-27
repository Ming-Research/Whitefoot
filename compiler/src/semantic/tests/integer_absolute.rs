use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::entailment::{DerivationNode, ObligationFamily};
use super::super::goal::{GoalExpression, GoalOperation};
use super::super::model::{CheckedExpression, CheckedIntegerOperation, CheckedStatement};
use super::{assert_rule, with_semantics};

#[test]
fn retains_each_mode_and_rejects_unsigned_types() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let wrapped = iabs.wrap(-128_i8);
  let exact = iabs(-42_i16);
  let absolute_value_is_defined = iabs.defined(-42_i64);
  let absolute_result = iabs.checked(-42_i32);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("integer absolute-value family must check: {outcome:?}");
        };
        let operations = checked.data.functions[0]
            .body
            .as_deref()
            .expect("WF body")
            .iter()
            .filter_map(|statement| match statement {
                CheckedStatement::Let {
                    value: CheckedExpression::IntegerOperation { operation, .. },
                    ..
                } => Some(*operation),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            operations,
            [
                CheckedIntegerOperation::AbsoluteWrap,
                CheckedIntegerOperation::AbsoluteExact,
                CheckedIntegerOperation::AbsoluteDefined,
                CheckedIntegerOperation::AbsoluteChecked,
            ]
        );
    });

    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = iabs.wrap(1_u8);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
}

#[test]
fn active_invariant_excludes_the_signed_minimum_from_exact_absolute_value() {
    let source = br#"fn magnitudes(floor: i32, limit: u64) -> result: unit pure contract {
  requires -2147483647_i32 <= floor;
  requires floor <= 100_i32;
  requires limit <= 10_u64;
} {
  let value = floor;
  for @items (
    i in 0_u64..limit,
    invariant above_minimum: floor <= value,
    invariant progress: value <= floor + i
  ) {
    let magnitude = iabs(value);
    set value = value + 1_i32;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the active invariant must exclude i32::MIN from iabs: {outcome:?}");
        };
        let function = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "magnitudes")
            .expect("magnitudes function exists");
        let absolute = function
            .entailment
            .obligations
            .iter()
            .find(|outcome| {
                outcome.family == ObligationFamily::IntegerDomain
                    && matches!(
                        &outcome.canonical_goal,
                        Some(GoalExpression::Operation {
                            row: GoalOperation::Integer {
                                operation: CheckedIntegerOperation::AbsoluteDefined,
                                ..
                            },
                            ..
                        })
                    )
            })
            .expect("iabs retains one OP-2 obligation");
        assert_eq!(absolute.components.len(), 1);
        assert!(absolute.discharged);
        assert!(absolute.residual.is_none());

        let root = absolute
            .derivation
            .expect("the accepted iabs retains a derivation root");
        let mut seen = vec![false; function.entailment.derivations.nodes.len()];
        let mut stack = vec![root];
        let mut used_invariant = false;
        while let Some(node) = stack.pop() {
            let index = node.0 as usize;
            if seen[index] {
                continue;
            }
            seen[index] = true;
            let retained = &function.entailment.derivations.nodes[index];
            used_invariant |= matches!(
                retained,
                DerivationNode::AffineConsequence {
                    premises,
                    ..
                } if !premises.is_empty()
            );
            stack.extend(retained.parent_ids());
        }
        assert!(
            used_invariant,
            "the iabs proof must consume the active source invariant"
        );
    });
}
