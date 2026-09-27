use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::entailment::{DerivationNode, ObligationFamily};
use super::super::goal::{GoalExpression, GoalOperation};
use super::super::model::{CheckedExpression, CheckedIntegerOperation, CheckedStatement};
use super::{assert_rule, assert_rule_kind, with_semantics};

#[test]
fn retains_the_complete_nonfloating_integer_family() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let a = 8_i32 / 2_i32;
  let b = 9_i32 % 2_i32;
  let c = iand(a, b);
  let d = ior(a, b);
  let e = ixor(a, b);
  let f = inot(a);
  let g = ishl.wrap(a, 1_u32);
  let h = ishr.wrap(a, 1_u32);
  let i = ishl(a, 1_u32);
  let j = ishr(a, 1_u32);
  let left_is_defined = ishl.defined(a, 1_u32);
  let right_is_defined = ishr.defined(a, 1_u32);
  let k = irotl(a, 1_u32);
  let l = irotr(a, 1_u32);
  let m = ipopcount(a);
  let n = iclz(a);
  let o = ictz(a);
  let p = ibswap(a);
  let q = imulhi(a, b);
  let r = a +sat b;
  let s = a -sat b;
  let t = a *sat b;
  let u = imin(a, b);
  let v = imax(a, b);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("complete integer family must check: {outcome:?}");
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
        assert_eq!(operations.len(), 24);
        assert_eq!(operations[0], CheckedIntegerOperation::DivideExact);
        assert_eq!(operations[6], CheckedIntegerOperation::ShiftLeftWrap);
        assert_eq!(operations[10], CheckedIntegerOperation::ShiftLeftDefined);
        assert_eq!(operations[11], CheckedIntegerOperation::ShiftRightDefined);
        assert_eq!(operations[14], CheckedIntegerOperation::PopulationCount);
        assert_eq!(operations[21], CheckedIntegerOperation::MultiplySaturating);
        assert_eq!(operations[23], CheckedIntegerOperation::Maximum);
    });

    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = ibswap(1_i8);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
    assert_rule_kind(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = ishl.wrap(1_i8, 1_i8);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

#[test]
fn exhaustion_invariant_proves_exact_shift_counts_below_the_value_width() {
    let source = br#"fn shift_prefix(limit: u64) -> result: unit pure contract {
  requires limit <= 31_u64;
} {
  let amount = 0_u32;
  for @items (
    i in 0_u64..limit,
    invariant consumed: amount <= i
  ) {
    set amount = amount + 1_u32;
  }
  let left = ishl(1_u32, amount);
  let right = ishr(1_u32, amount);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the exhaustion invariant must bound both exact shifts: {outcome:?}");
        };
        let function = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "shift_prefix")
            .expect("shift_prefix function exists");
        let shifts = function
            .entailment
            .obligations
            .iter()
            .filter(|outcome| {
                outcome.family == ObligationFamily::IntegerDomain
                    && matches!(
                        &outcome.canonical_goal,
                        Some(GoalExpression::Operation {
                            row: GoalOperation::Integer {
                                operation: CheckedIntegerOperation::ShiftLeftDefined
                                    | CheckedIntegerOperation::ShiftRightDefined,
                                ..
                            },
                            ..
                        })
                    )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            shifts.len(),
            2,
            "left and right shifts retain one OP-2 each"
        );
        for shift in shifts {
            assert_eq!(shift.components.len(), 1);
            assert!(shift.discharged);
            assert!(shift.residual.is_none());

            let root = shift
                .derivation
                .expect("the accepted exact shift retains a derivation root");
            let mut seen = vec![false; function.entailment.derivations.nodes.len()];
            let mut stack = vec![root];
            let mut used_exhaustion = false;
            while let Some(node) = stack.pop() {
                let index = node.0 as usize;
                if seen[index] {
                    continue;
                }
                seen[index] = true;
                let retained = &function.entailment.derivations.nodes[index];
                used_exhaustion |= matches!(
                    retained,
                    DerivationNode::AffineConsequence {
                        premises,
                        ..
                    } if !premises.is_empty()
                );
                stack.extend(retained.parent_ids());
            }
            assert!(
                used_exhaustion,
                "each exact-shift proof must consume the exported source invariant"
            );
        }
    });
}
