//! ENT-2/3/4 struct equality's retained field goals and signed proofs.

use super::{assert_rule_at, with_semantics};
use crate::SemanticOutcome;
use crate::semantic::entailment::{FunctionEntailment, GoalSign};
use crate::semantic::goal::{GoalExpression, GoalOperation};

const TYPES: &str = "struct FieldLeaf {\n  count: u8;\n  ready: Bool;\n}\n\nstruct FieldRoot {\n  first: FieldLeaf;\n  tail: u8;\n}\n\n";
const MAIN: &str = "fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";

fn summary(source: &str, name: &str) -> FunctionEntailment {
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("{outcome:?}");
        };
        program
            .data
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .entailment
            .clone()
    })
}

#[test]
fn struct_fields_are_ordered_recursive_goals_with_only_integer_projections() {
    for operator in ["==", "!="] {
        for binding in [false, true] {
            let condition = if binding {
                "condition".to_owned()
            } else {
                format!("a^ {operator} b^")
            };
            let origin = if binding {
                format!("  let condition = a^ {operator} b^;\n")
            } else {
                String::new()
            };
            let source = format!(
                "{TYPES}fn probe(a: &FieldRoot, b: &FieldRoot) -> result: unit reads(a), reads(b) {{\n{origin}  if {condition} {{\n    return unit;\n  }}\n  return unit;\n}}\n\n{MAIN}"
            );
            let result = summary(&source, "probe");
            let positive = if operator == "==" {
                GoalSign::Positive
            } else {
                GoalSign::Negative
            };
            for entry in result.boolean_decompositions.iter().filter(|entry| {
                matches!(
                    result.inventory.goals[entry.parent.0 as usize].expression,
                    GoalExpression::Operation {
                        row: GoalOperation::ValueEquality { .. },
                        ..
                    }
                )
            }) {
                if entry.sign != positive {
                    assert!(
                        entry.members.is_empty(),
                        "a disequality establishes no chosen field"
                    );
                    continue;
                }
                assert_eq!(
                    entry.members.len(),
                    4,
                    "first, first.count, first.ready, tail"
                );
                let projected: Vec<_> = entry
                    .members
                    .iter()
                    .map(|(goal, sign)| {
                        assert_eq!(*sign, GoalSign::Positive);
                        result.inventory.goals[goal.0 as usize].projection.is_some()
                    })
                    .collect();
                assert_eq!(projected, [false, true, false, true]);
            }
            assert!(
                result
                    .boolean_decompositions
                    .iter()
                    .any(|entry| entry.sign == positive && entry.members.len() == 4)
            );
            if binding {
                assert!(
                    result
                        .boolean_decompositions
                        .iter()
                        .any(|entry| entry.sign == positive
                            && entry.members.len() == 4
                            && matches!(
                                result.inventory.goals[entry.parent.0 as usize].expression,
                                GoalExpression::Datum(_)
                            )),
                    "the own-Bool origin decomposes as the direct condition does"
                );
            }
        }
    }
}

#[test]
fn signed_struct_reconstruction_uses_all_positive_or_any_negative_field() {
    for requirement in [
        "  requires a == b;",
        "  define comparison = a != b;\n  requires bnot(comparison);",
    ] {
        let source = format!(
            "{TYPES}fn need(a: FieldRoot, b: FieldRoot) -> result: unit pure contract {{\n{requirement}\n}} {{\n  return unit;\n}}\n\nfn probe(a: FieldRoot, b: FieldRoot) -> result: unit pure {{\n  if a.first.count == b.first.count {{\n    if a.first.ready == b.first.ready {{\n      if a.tail == b.tail {{\n        need(a: a, b: b);\n      }}\n    }}\n  }}\n  return unit;\n}}\n\n{MAIN}"
        );
        summary(&source, "probe");
        let missing = source.replace("if a.tail == b.tail", "if a.first.count == b.first.count");
        assert_rule_at(
            missing.as_bytes(),
            crate::SemanticRule::Fn8,
            "need(a: a, b: b)",
        );
    }
    for requirement in [
        "  requires a != b;",
        "  define comparison = a == b;\n  requires bnot(comparison);",
    ] {
        let source = format!(
            "{TYPES}fn need(a: FieldRoot, b: FieldRoot) -> result: unit pure contract {{\n{requirement}\n}} {{\n  return unit;\n}}\n\nfn probe(a: FieldRoot, b: FieldRoot) -> result: unit pure {{\n  if a.first.count != b.first.count {{\n    need(a: a, b: b);\n  }}\n  return unit;\n}}\n\n{MAIN}"
        );
        summary(&source, "probe");
    }
}

#[test]
fn enum_and_array_equalities_remain_exact_roots() {
    for ty in ["Option<u8>", "Array<u8, 2>"] {
        for operator in ["==", "!="] {
            let source = format!(
                "fn probe(a: {ty}, b: {ty}) -> result: unit pure {{\n  if a {operator} b {{\n    return unit;\n  }}\n  return unit;\n}}\n\n{MAIN}"
            );
            let result = summary(&source, "probe");
            // ENT-3 retains either sign of enum/Array equality as an exact
            // root with no children; only nonempty decompositions are recorded.
            assert!(result.boolean_decompositions.is_empty());
            let roots: Vec<_> = result
                .inventory
                .goals
                .iter()
                .filter(|goal| {
                    matches!(
                        goal.expression,
                        GoalExpression::Operation {
                            row: GoalOperation::ValueEquality { equal, .. },
                            ..
                        } if equal == (operator == "==")
                    )
                })
                .collect();
            assert!(!roots.is_empty(), "the exact equality root is retained");
            assert!(
                roots.iter().all(|goal| goal.projection.is_none()),
                "enum/Array equality has no integer comparison projection"
            );
        }
    }
}

#[test]
fn struct_requirement_fields_supply_the_exact_operation_domain() {
    let source =
        include_str!("../../../../tests/conformance/cases/ent3-pos-struct-equality-fields.wf");
    let result = summary(source, "increment");
    assert!(
        result
            .boolean_decompositions
            .iter()
            .any(|entry| entry.members.len() == 2)
    );
}
