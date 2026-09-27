use crate::lowering::{OverlapLowering, lower_checked};
use crate::{DeclarationRole, SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::entailment::{CallGoalDisposition, CallGoalEvidence};
use super::super::goal::{GoalDatum, GoalExpression, GoalOperation, GoalProjection};
use super::super::model::{
    CheckedConst, CheckedExpression, CheckedIntegerOperation, CheckedNominalKind, CheckedStatement,
    CheckedType, CheckedValue, IntegerType, MeasuredKind, WindowShape,
};
use super::{assert_rule, with_resolved_semantics, with_semantics, with_semantics_dark};

/// A reference is a local name for a path and its validity is a fact [REF-1,
/// REF-2]; the value fact a requirement needs is killed exactly when a write
/// reaches that path [EFF-5, ENT-5]. Without the intervening write the entry
/// requirement carries to the inner call; with it the inner requirement is
/// undischarged.
///
/// This is the v0.60 statement of what the retiring `&uniq` whole-result case
/// stated: v0.59 routed the referent through a region-parameterized function
/// returning `&uniq 'r u64`, which [REF-3] now forbids outright, so the write
/// is exhibited by an ordinary callee whose row declares it.
#[test]
fn a_write_through_a_reference_kills_the_value_fact_a_requirement_needs() {
    for borrowed in [false, true] {
        let (mode, term, read, effects, actual) = if borrowed {
            ("&", "deref(value)", "deref(value)", "reads(value)", "value")
        } else {
            ("", "value", "value", "pure", "seen")
        };
        let bind = if borrowed {
            String::new()
        } else {
            "  let seen = deref(value);\n".to_owned()
        };
        for changed in [false, true] {
            // [EFF-1] `writes(p)` states every access at or below `p`, so the
            // writing row declares the write alone.
            let (write, forward_effect) = if changed {
                ("  overwrite(cell: value);\n", "writes(value)")
            } else {
                ("", "reads(value)")
            };
            let source = format!(
                "fn overwrite(cell: &u64) -> result: unit writes(cell) {{\n  set deref(cell) = 9_u64;\n  return unit;\n}}\n\nfn indexed(value: {mode}u64) -> result: u64 {effects} contract {{\n  requires {term} < 1_u64;\n}} {{\n  let rows = array_filled::<u64, 1>(value: 7_u64);\n  let index = {read};\n  return rows[index];\n}}\n\nfn forward(value: &u64) -> result: u64 {forward_effect} contract {{\n  requires deref(value) < 1_u64;\n}} {{\n{write}{bind}  return indexed(value: {actual});\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
            );
            if changed {
                super::assert_rule_kind(source.as_bytes(), SemanticRule::Fn8, |kind| {
                    matches!(kind, SemanticIssueKind::UndischargedCallRequirement(_))
                });
            } else {
                with_semantics(source.as_bytes(), |outcome| {
                    assert!(
                        matches!(outcome, SemanticOutcome::Complete(_)),
                        "an unwritten referent preserves its established fact: {outcome:?}"
                    );
                });
            }
        }
    }
}

// Retired with v0.59's loans and regions: `borrow_result_loan_ceilings_do_not_transfer_value_requirements`
// had FN-1's borrow-result loan ceiling as its subject, and every source it
// wrote returned a reference from `select`. [REF-3] now refuses a returned
// reference outright: "a `return_stmt` whose selected expression is a
// reference is that violation, and [FN-1] forms no candidate there", so no
// v0.60 source can form the transfer this case refused.

#[test]
fn a_non_bool_requires_predicate_cites_op5() {
    assert_rule(
        br#"fn invalid(value: i32) -> out: i32 pure contract {
  requires value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Op5,
        SemanticIssueKind::InvalidPredicateCondition,
    );
}

#[test]
fn plural_requires_keep_every_source_occurrence_at_the_call() {
    let source = br#"fn exact(value: i32) -> out: i32 pure contract {
  requires value == 1_i32;
  requires value == 1_i32;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = 1_i32;
  let observed = exact(value: value);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("both requirement occurrences must discharge: {outcome:?}");
        };
        let exact = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "exact")
            .expect("exact function");
        assert_eq!(exact.requirements.len(), 2);
        let main = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("ordinary main");
        let CheckedStatement::Let {
            value: CheckedExpression::UserCall { requirements, .. },
            ..
        } = &main.body.as_deref().expect("WF body")[1]
        else {
            panic!("the second statement must retain the user call");
        };
        assert_eq!(requirements.len(), 2);
        assert_eq!(main.entailment.call_goals.len(), 2);
        assert_ne!(
            main.entailment.call_goals[0].requires_clause,
            main.entailment.call_goals[1].requires_clause
        );
    });
}

#[test]
fn a_later_requires_clause_is_not_dropped_after_an_earlier_success() {
    let source = br#"fn exact(value: i32) -> out: i32 pure contract {
  requires value == 1_i32;
  requires value == 2_i32;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = 1_i32;
  let observed = exact(value: value);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("the refuted second requirement must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn8);
        let SemanticIssueKind::UndischargedCallRequirement(detail) = issue.kind() else {
            panic!("FN-8 must identify the failing requirement clause: {issue:?}");
        };
        assert_eq!(
            detail.disposition,
            crate::CallRequirementDisposition::Refuted
        );
        assert!(!detail.requires_clause.path().components().is_empty());
    });
}

fn instantiated_call_goal_arguments(call: &CheckedExpression) -> &[GoalExpression] {
    let CheckedExpression::UserCall { requirements, .. } = call else {
        panic!("call must carry its instantiated requirement");
    };
    let [requirement] = requirements.as_slice() else {
        panic!("call must carry exactly one instantiated requirement");
    };
    let GoalExpression::Operation { arguments, .. } = &requirement.goal.root else {
        panic!("call requirement must remain an operation goal");
    };
    arguments
}

#[test]
fn requires_retains_one_static_goal_without_a_second_expression_tree() {
    let source = br#"fn bounded(x: i32) -> result: i32 pure contract {
  define permitted = x >= 0_i32;
  requires permitted;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let x = 7_i32;
  let value = bounded(x: x);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("valid checked requirement must be implemented: {outcome:?}");
        };
        let function = &checked.data.functions[0];
        let requirement = function
            .requirements
            .first()
            .expect("checked requires retains its boundary metadata");
        assert!(!requirement.clause.components().is_empty());
    });
}

#[test]
fn requires_rejects_user_calls_and_partial_operations() {
    assert_rule(
        include_bytes!("../../../../tests/conformance/cases/fn8-neg-requires-user-call.wf"),
        SemanticRule::Fn8,
        SemanticIssueKind::InvalidRequires,
    );
    assert_rule(
        include_bytes!("../../../../tests/conformance/cases/fn8-neg-requires-partial-op.wf"),
        SemanticRule::Fn8,
        SemanticIssueKind::InvalidRequires,
    );
}

#[test]
fn requires_is_static_and_keeps_op5_typing() {
    with_semantics(
        br#"fn admitted(value: i32) -> result: i32 pure contract {
  requires value >= 0_i32;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            let SemanticOutcome::Complete(_) = outcome else {
                panic!("a requirement is not an executed EFF-2 occurrence: {outcome:?}");
            };
        },
    );
    assert_rule(
        include_bytes!("../../../../tests/conformance/cases/fn8-neg-requires-non-bool-check.wf"),
        SemanticRule::Op5,
        SemanticIssueKind::InvalidPredicateCondition,
    );
}

/// [FN-8] admits a clause computation in either spelling, and holds both to
/// the same subset.
///
/// The infix shape is the one that hides operands: an `expr`'s own atom is the
/// left operand and the tail carries the operator and the right, so a pass
/// that stops at the first `atom` child validates a third of the expression
/// and admits a proof-required row, a `move`, a borrow, or a subscript in the
/// rest.
#[test]
fn requires_holds_an_infix_row_to_the_same_subset_as_its_named_spelling() {
    with_semantics(
        b"fn f(a: u64) -> result: u64 pure contract {\n  \
          define doubled = a *wrap 2_u64;\n  \
          requires doubled <= 16_u64;\n} {\n  \
          return a;\n}\n\n\
          fn main() -> status: std::process::ExitStatus pure {\n  \
          return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| {
            let SemanticOutcome::Complete(_) = outcome else {
                panic!("an infix clause let spells an admitted row: {outcome:?}");
            };
        },
    );
    // The exact affine rows are read mathematically in a clause, the carve-out
    // INV-1 already gives an affine expression, so a bare `+` states a relation
    // rather than requesting an operation.
    with_semantics(
        b"fn f(x: i32) -> result: i32 pure contract {\n  \
          define raised = x + 1_i32;\n  \
          requires raised > x;\n} {\n  \
          return x;\n}\n\n\
          fn main() -> status: std::process::ExitStatus pure {\n  \
          return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| {
            let SemanticOutcome::Complete(_) = outcome else {
                panic!("an exact affine row is admitted in a clause: {outcome:?}");
            };
        },
    );
    // Every other exact row stays inadmissible: division has an input no
    // relation can state its way out of, so admitting it would put a partial
    // operation where no domain obligation discharges it.
    assert_rule(
        b"fn f(x: u64) -> result: u64 pure contract {\n  \
          define half = x / 2_u64;\n  \
          requires half <= x;\n} {\n  \
          return x;\n}\n\n\
          fn main() -> status: std::process::ExitStatus pure {\n  \
          return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Fn8,
        SemanticIssueKind::InvalidRequires,
    );
    // Reached only through the infix tail's own atom, never the expr's.
    assert_rule(
        b"fn f(xs: Slots<u64, 4>, a: u64) -> result: u64 pure contract {\n  \
          define sum = a +wrap xs[1_u64];\n  \
          requires sum <= 8_u64;\n} {\n  \
          return a;\n}\n\n\
          fn main() -> status: std::process::ExitStatus pure {\n  \
          return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Fn8,
        SemanticIssueKind::InvalidRequires,
    );
    // A non-consuming datum is itself an admitted definition expression;
    // alpha-expansion substitutes the parameter datum into the requirement.
    with_semantics(
        b"fn f(x: i32) -> result: i32 pure contract {\n  \
          define candidate = x;\n  \
          requires candidate > 0_i32;\n} {\n  \
          return x;\n}\n\n\
          fn main() -> status: std::process::ExitStatus pure {\n  \
          return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| {
            let SemanticOutcome::Complete(_) = outcome else {
                panic!("a non-consuming datum definition is admitted: {outcome:?}");
            };
        },
    );
}

/// [FN-8]'s clause local is an "own **copy** value", and after the v0.23
/// annotation deletion the only thing that can say so is the derived type.
///
/// The written type used to carry this. Nothing else does: every row below is
/// pure, total and non-trapping, so the admitted-spelling filter passes it,
/// and the value it yields is still not a copy type.
#[test]
fn requires_holds_a_clause_local_to_a_copy_type() {
    // Non-copy by shape: `slots_new` is the reachable aggregate [OP-13] row,
    // since a range reference is formed by a `borrow_expr` the clause subset
    // already rejects [REF-4].
    assert_rule(
        b"fn f(a: u64) -> result: u64 pure contract {\n  \
          define xs = slots_new::<i32, 4>();\n  \
          requires a < 8_u64;\n} {\n  \
          return a;\n}\n\n\
          fn main() -> status: std::process::ExitStatus pure {\n  \
          return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Fn8,
        SemanticIssueKind::InvalidRequires,
    );
    // The symbolic generic pass must intern the checked-arithmetic Result.
    // Its copy payload and copy error make the structural Result copy too.
    with_semantics(
        br#"fn invalid<T: Int>(x: T) -> result: T pure contract {
  define raised = x +checked 1_T;
  requires x > 0_T;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{outcome:?}"
            )
        },
    );
    // A payload variant remains copy when every payload is copy.
    with_semantics(
        b"fn f(x: i32) -> result: i32 pure contract {\n  \
          define raised = x +checked 1_i32;\n  \
          requires x > 0_i32;\n} {\n  \
          return x;\n}\n\n\
          fn main() -> status: std::process::ExitStatus pure {\n  \
          return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{outcome:?}"
            )
        },
    );
    // The positive control: a copy-typed clause local is still admitted, or
    // the gate above has over-rejected into every clause `let`.
    with_semantics(
        b"fn f(a: u64) -> result: u64 pure contract {\n  \
          define ok = a < 8_u64;\n  \
          requires ok;\n} {\n  \
          return a;\n}\n\n\
          fn main() -> status: std::process::ExitStatus pure {\n  \
          return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("a Bool clause local is a copy value: {outcome:?}");
            };
            let requirement = checked.data.functions[0]
                .requirements
                .first()
                .expect("f carries its admitted requirement");
            let GoalExpression::Operation {
                row:
                    GoalOperation::Integer {
                        operation: CheckedIntegerOperation::Less,
                        operand_type: CheckedType::Integer(IntegerType::U64),
                    },
                arguments,
                result: CheckedType::Bool,
                ..
            } = &requirement.template.root
            else {
                panic!(
                    "the admitted template must retain exactly the written comparison: {:?}",
                    requirement.template.root
                );
            };
            assert_eq!(arguments.len(), 2);
        },
    );
}

#[test]
fn ordinary_prelude_calls_in_requires_are_fn8_source_rejections() {
    assert_rule(
        br#"fn invalid() -> result: std::process::ExitStatus pure contract {
  define status = std::process::exit_status(code: 0_u8);
  requires 0_u8 == 0_u8;
} {
  return std::process::exit_status(code: 0_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Fn8,
        SemanticIssueKind::InvalidRequires,
    );
}

#[test]
fn requires_locals_are_distinct_from_same_named_body_locals() {
    let source = br#"fn increment(x: i32) -> result: i32 pure contract {
  define value = x >= 0_i32;
  requires value;
} {
  let value = x +wrap 1_i32;
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let x = 7_i32;
  let value = increment(x: x);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("requires and body scopes must remain disjoint: {outcome:?}");
        };
        let function = &checked.data.functions[0];
        assert!(!function.requirements.is_empty());
        assert!(matches!(
            function.body.as_deref().expect("WF body")[0],
            CheckedStatement::Let { .. }
        ));
    });
}

#[test]
fn goal_templates_ignore_clause_spelling_and_local_sharing() {
    let source = br#"fn shared(a: u64, b: u64) -> result: u64 pure contract {
  define sum = a +wrap b;
  define below = sum < 100_u64;
  define above = sum > 0_u64;
  define bounded = band(below, above);
  requires bounded;
} {
  return a;
}

fn duplicated(left: u64, right: u64) -> result: u64 pure contract {
  define first_sum = left +wrap right;
  define low_half = first_sum < 100_u64;
  define second_sum = left +wrap right;
  define high_half = second_sum > 0_u64;
  define complete = band(low_half, high_half);
  requires complete;
} {
  return left;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("alpha-equivalent requirements must check: {outcome:?}");
        };
        let requirement = |name: &str| {
            checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .and_then(|function| function.requirements.first())
                .unwrap_or_else(|| panic!("missing requirement for {name}"))
        };
        let shared = requirement("shared");
        let duplicated = requirement("duplicated");
        assert_eq!(shared.template, duplicated.template);
        assert_ne!(shared.clause, duplicated.clause);
    });
}

#[test]
fn goal_templates_retain_order_row_and_named_const_identity() {
    let source = br#"const first_limit: u64 = 8_u64;

const second_limit: u64 = 8_u64;

fn baseline(value: u64) -> result: u64 pure contract {
  requires value < first_limit;
} {
  return value;
}

fn swapped(value: u64) -> result: u64 pure contract {
  requires first_limit < value;
} {
  return value;
}

fn different_row(value: u64) -> result: u64 pure contract {
  requires value <= first_limit;
} {
  return value;
}

fn different_const(value: u64) -> result: u64 pure contract {
  requires value < second_limit;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("identity controls must check before comparison: {outcome:?}");
        };
        let template = |name: &str| {
            &checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .and_then(|function| function.requirements.first())
                .unwrap_or_else(|| panic!("missing requirement for {name}"))
                .template
        };
        let baseline = template("baseline");
        assert_ne!(baseline, template("swapped"));
        assert_ne!(baseline, template("different_row"));
        assert_ne!(baseline, template("different_const"));
    });
}

#[test]
fn goal_cell_deref_projection_retains_the_selected_referent_type() {
    // [TYPE-9] a `Box` carries no brand and no measure, and its content is the
    // field `inner` rather than a `deref` spelling; the content step still
    // resolves to the one dereference projection, so the retained goal datum
    // is unchanged.
    let source = br#"fn positive(owner: Box<i32>) -> result: Box<i32> pure contract {
  requires owner.inner > 0_i32;
} {
  return move owner;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("an admitted cell-referent goal must check: {outcome:?}");
        };
        let root = &checked.data.functions[0]
            .requirements
            .first()
            .expect("positive has a requirement")
            .template
            .root;
        let GoalExpression::Operation { arguments, .. } = root else {
            panic!("comparison must remain an operation goal");
        };
        let GoalExpression::Datum(GoalDatum::Parameter {
            projections, ty, ..
        }) = &arguments[0]
        else {
            panic!("cell referent must remain the first formal datum");
        };
        assert_eq!(projections, &[GoalProjection::Deref]);
        assert_eq!(*ty, CheckedType::Integer(IntegerType::I32));
    });
}

#[test]
fn goal_field_projection_retains_the_selected_run_type() {
    let source = br#"struct Envelope {
  values: Slots<u8, 2>;
}

fn measured(envelope: Envelope) -> result: Envelope pure contract {
  define size = envelope.values.len;
  requires size == size;
} {
  return move envelope;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("an admitted projected-run goal must check: {outcome:?}");
        };
        let GoalExpression::Operation { arguments, .. } = &checked.data.functions[0]
            .requirements
            .first()
            .expect("measured has a requirement")
            .template
            .root
        else {
            panic!("comparison must remain an operation goal");
        };
        let GoalExpression::Operation {
            arguments: length_arguments,
            ..
        } = &arguments[0]
        else {
            panic!("expanded local must retain its len operation");
        };
        let GoalExpression::Datum(GoalDatum::Parameter {
            projections, ty, ..
        }) = &length_arguments[0]
        else {
            panic!("projected run must remain the formal datum");
        };
        assert_eq!(projections, &[GoalProjection::Field(0)]);
        let CheckedType::Window {
            shape,
            element,
            capacity,
        } = *ty
        else {
            panic!("projected type must remain a constant-capacity window");
        };
        assert_eq!(shape, WindowShape::Slots);
        assert_eq!(capacity, Some(CheckedConst::Value(2)));
        assert_eq!(
            checked.element_type(element),
            Some(CheckedType::Integer(IntegerType::U8))
        );
    });
}

#[test]
fn concrete_equal_const_arguments_produce_equal_goal_templates() {
    let source =
        br#"fn left<const n: u64>(value: Slots<u8, n>) -> result: Slots<u8, n> pure contract {
  define size = value.len;
  requires size == size;
} {
  return move value;
}

fn right<const count: u64>(input: Slots<u8, count>) -> result: Slots<u8, count> pure contract {
  define extent = input.len;
  requires extent == extent;
} {
  return move input;
}

fn different<const width: u64>(items: Slots<u8, width>) -> result: Slots<u8, width> pure contract {
  define amount = items.len;
  requires amount == amount;
} {
  return move items;
}

fn main() -> status: std::process::ExitStatus pure {
  let left_input = slots_new::<u8, 2>();
  let left_output = left::<2>(value: move left_input);
  let right_input = slots_new::<u8, 2>();
  let right_output = right::<2>(input: move right_input);
  let different_input = slots_new::<u8, 3>();
  let different_output = different::<3>(items: move different_input);
  let left_size = left_output.len;
  let right_size = right_output.len;
  let different_size = different_output.len;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("generic requirements must form concrete templates: {outcome:?}");
        };
        let template = |name: &str| {
            &checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .and_then(|function| function.requirements.first())
                .unwrap_or_else(|| panic!("missing concrete requirement for {name}"))
                .template
        };
        assert_eq!(template("left"), template("right"));
        assert_ne!(template("left"), template("different"));
    });
}

#[test]
fn unused_generic_requirement_is_retained_symbolically_without_a_concrete_function() {
    let source = br#"fn positive<T: Int>(value: T) -> result: T pure contract {
  requires value > 0_T;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_resolved_semantics(source, |resolved, outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("unused generic requirement must survive symbolic checking: {outcome:?}");
        };
        assert_eq!(
            checked
                .data
                .functions
                .iter()
                .filter(|function| function.body.is_some())
                .count(),
            1
        );
        assert_eq!(checked.data.functions[0].name, "main");
        let positive = resolved
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.role() == DeclarationRole::Function
                    && declaration.spelling() == "positive"
            })
            .expect("positive source declaration")
            .id();
        let user_requirements = checked
            .data
            .generic_requirements
            .iter()
            .filter(|symbolic| symbolic.declaration == positive)
            .collect::<Vec<_>>();
        let [symbolic] = user_requirements.as_slice() else {
            panic!(
                "exactly one user GenericInt requirement must remain symbolic: {user_requirements:#?}"
            );
        };
        let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operand_type: CheckedType::GenericInt(_),
                    ..
                },
            ..
        } = &symbolic.requirement.template.root
        else {
            panic!("retained generic requirement must remain symbolic");
        };
    });
}

#[test]
fn a_nominal_bearing_generic_requirement_survives_the_symbolic_checkpoint_as_metadata() {
    // v0.59 reached the symbolic nominal through `buffer_fits::<Pair<T>>(n)`.
    // [OP-9]'s allocation-size predicate "has no writer-callable spelling" in
    // v0.60, so a measure over `Slots<Pair<T>, 1>` retains the nominal as the
    // exact element type of its ContainerMeasure row.
    let source = br#"struct Pair<T: Int> {
  left: T;
  right: T;
}

fn need<T: Int>(pairs: Slots<Pair<T>, 1>) -> result: unit pure contract {
  requires pairs.len <= 1_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_resolved_semantics(source, |resolved, outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("symbolic nominal requirements must remain valid metadata: {outcome:?}");
        };
        let need = resolved
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.role() == DeclarationRole::Function && declaration.spelling() == "need"
            })
            .expect("need source declaration")
            .id();
        let user_requirements = checked
            .data
            .generic_requirements
            .iter()
            .filter(|symbolic| symbolic.declaration == need)
            .collect::<Vec<_>>();
        let [symbolic] = user_requirements.as_slice() else {
            panic!(
                "exactly one nominal-bearing user requirement must remain symbolic: {user_requirements:#?}"
            );
        };
        let requirement = &symbolic.requirement;
        let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation: CheckedIntegerOperation::LessEqual,
                    operand_type: CheckedType::Integer(IntegerType::U64),
                },
            arguments,
            ..
        } = &requirement.template.root
        else {
            panic!("the retained requirement must preserve its exact symbolic goal");
        };
        let GoalExpression::Operation {
            row:
                GoalOperation::ContainerMeasure {
                    measure: super::super::model::CheckedMeasure::Length,
                    measured: MeasuredKind::ConstantSlots,
                    element: Some(element),
                    constant: Some(CheckedConst::Value(1)),
                },
            arguments: measure_arguments,
            ..
        } = &arguments[0]
        else {
            panic!("the left operand must retain the exact Slots measure row");
        };
        assert!(matches!(
            measure_arguments.as_slice(),
            [GoalExpression::Datum(GoalDatum::Parameter { projections, .. })]
                if projections.is_empty()
        ));
        assert!(matches!(
            &arguments[1],
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer {
                ty: IntegerType::U64,
                bits: 1,
            }))
        ));
        let CheckedType::Nominal(nominal) = checked.data.elements[element.index()] else {
            panic!("the Slots element must retain Pair<T>'s nominal identity");
        };
        let index = nominal.0 as usize;
        let retained = checked
            .data
            .nominals
            .get(index)
            .expect("Pair<T>'s nominal identity must address checked metadata");
        assert!(retained.name.starts_with("Pair<"));
        assert!(
            index >= checked.data.executable_nominal_count,
            "metadata-only symbolic nominals must follow the executable prefix"
        );
        let CheckedNominalKind::Struct { fields } = &retained.kind else {
            panic!("Pair<T> must retain its checked struct shape");
        };
        assert_eq!(fields.len(), 2);
        assert!(
            fields
                .iter()
                .all(|field| matches!(field.ty, CheckedType::GenericInt(_)))
        );
        lower_checked(*checked, OverlapLowering::Off)
            .expect("metadata-only symbolic nominals must not reach lowering");
    });
}

#[test]
fn a_derived_const_in_generic_requirement_has_checked_program_owned_structure() {
    let source = br#"fn need<const n: u64>(value: Slots<u8, n + 1>) -> result: Slots<u8, n + 1> pure contract {
  define size = value.len;
  requires size == size;
} {
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_resolved_semantics(source, |resolved, outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the symbolic const requirement must be retained: {outcome:?}");
        };
        assert_eq!(checked.data.derived_consts.len(), 1);
        let derived = checked.data.derived_consts[0];
        assert!(matches!(derived.left, CheckedConst::Parameter(_)));
        assert_eq!(derived.right, CheckedConst::Value(1));
        let need = resolved
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.role() == DeclarationRole::Function && declaration.spelling() == "need"
            })
            .expect("need source declaration")
            .id();
        let user_requirements = checked
            .data
            .generic_requirements
            .iter()
            .filter(|symbolic| symbolic.declaration == need)
            .collect::<Vec<_>>();
        let [symbolic] = user_requirements.as_slice() else {
            panic!(
                "exactly one user requirement must retain a derived const: {user_requirements:#?}"
            );
        };
        let rendered = format!("{:#?}", symbolic.requirement.template.root);
        assert!(
            rendered.contains("DerivedConstId") && rendered.contains("0,"),
            "the retained goal must name the checked-program-owned table entry: {rendered}"
        );
        lower_checked(*checked, OverlapLowering::Off)
            .expect("metadata-only derived consts must not enter executable lowering");
    });
}

#[test]
fn called_generic_keeps_concrete_instances_and_one_symbolic_requirement() {
    let source = br#"fn positive<T: Int>(value: T) -> result: T pure contract {
  requires value > 0_T;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let narrow = 1_i32;
  let narrow_result = positive::<i32>(value: narrow);
  let wide = 1_i64;
  let wide_result = positive::<i64>(value: wide);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("concrete generic calls and symbolic metadata must coexist: {outcome:?}");
        };
        let concrete = checked
            .data
            .functions
            .iter()
            .filter(|function| function.name == "positive")
            .collect::<Vec<_>>();
        assert_eq!(concrete.len(), 2);
        let user_requirements = checked
            .data
            .generic_requirements
            .iter()
            .filter(|requirement| requirement.declaration == concrete[0].declaration)
            .collect::<Vec<_>>();
        let [symbolic] = user_requirements.as_slice() else {
            panic!(
                "the called user generic must retain exactly one symbolic requirement: {user_requirements:#?}"
            );
        };
        assert_eq!(symbolic.declaration, concrete[0].declaration);
        assert!(concrete.iter().all(|function| {
            function
                .requirements
                .first()
                .is_some_and(|requirement| requirement.template.root.ty() == CheckedType::Bool)
        }));
    });
}

#[test]
fn generic_to_generic_discovery_does_not_duplicate_symbolic_requirements() {
    let source = br#"fn inner<T: Int>(value: T) -> result: T pure contract {
  requires value > 0_T;
} {
  return value;
}

fn outer<T: Int>(value: T) -> result: T pure contract {
  requires value > 0_T;
} {
  return inner::<T>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_resolved_semantics(source, |resolved, outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("transitive symbolic validation must retain canonical entries: {outcome:?}");
        };
        assert_eq!(
            checked
                .data
                .functions
                .iter()
                .filter(|function| function.body.is_some())
                .count(),
            1
        );
        let inner = resolved
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.role() == DeclarationRole::Function && declaration.spelling() == "inner"
            })
            .expect("inner source declaration")
            .id();
        let outer = resolved
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.role() == DeclarationRole::Function && declaration.spelling() == "outer"
            })
            .expect("outer source declaration")
            .id();
        let user_requirements = checked
            .data
            .generic_requirements
            .iter()
            .filter(|symbolic| symbolic.declaration == inner || symbolic.declaration == outer)
            .collect::<Vec<_>>();
        assert_eq!(user_requirements.len(), 2, "{user_requirements:#?}");
        assert_eq!(
            user_requirements
                .iter()
                .filter(|requirement| requirement.declaration == inner)
                .count(),
            1
        );
        assert_eq!(
            user_requirements
                .iter()
                .filter(|requirement| requirement.declaration == outer)
                .count(),
            1
        );
    });
}

/// [MSR-6, FN-2, FN-8] a const generic used as an ordinary value actual is
/// the same symbolic constant that selects the callee instance. The source
/// schema must preserve that identity through an alpha-renamed generic call,
/// while the concrete replay folds both occurrences to the selected integer.
#[test]
fn const_generic_values_discharge_requirements_through_transitive_forwarding() {
    let source = br#"fn accept<const expected: u64>(value: u64) -> result: unit pure contract {
  requires value == expected;
} {
  return unit;
}

fn relay<const forwarded: u64>() -> result: unit pure {
  let accepted = accept::<forwarded>(value: forwarded);
  return unit;
}

fn outer<const ceiling: u64>() -> result: unit pure {
  let relayed = relay::<ceiling>();
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let completed = outer::<7>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("symbolic and concrete const forwarding must both check: {outcome:?}");
        };
        let relay = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "relay")
            .expect("concrete relay instance");
        let CheckedStatement::Let {
            value: call @ CheckedExpression::UserCall { arguments, .. },
            ..
        } = &relay.body.as_deref().expect("relay body")[0]
        else {
            panic!("relay must retain the checked accept call");
        };
        assert!(matches!(
            &arguments[0],
            CheckedExpression::Constant(CheckedValue::Integer {
                ty: IntegerType::U64,
                bits: 7,
            })
        ));
        let goal_arguments = instantiated_call_goal_arguments(call);
        assert_eq!(goal_arguments[0], goal_arguments[1]);
        assert!(matches!(
            &goal_arguments[0],
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer {
                ty: IntegerType::U64,
                bits: 7,
            }))
        ));
    });
}

/// Two const parameters are separate [ENT-2] terms. Treating any checked
/// const-generic value actual as the callee's selected const would make this
/// requirement spuriously true; with no written relation it remains FN-8
/// unproved in the symbolic schema.
#[test]
fn independent_const_generic_values_do_not_become_equal_call_datums() {
    let source = br#"fn accept<const expected: u64>(value: u64) -> result: unit pure contract {
  requires value == expected;
} {
  return unit;
}

fn invalid<const actual: u64, const expected: u64>() -> result: unit pure {
  let denied = accept::<expected>(value: actual);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    super::assert_rule_kind(source, SemanticRule::Fn8, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedCallRequirement(detail)
                if detail.disposition == crate::CallRequirementDisposition::Unproved
        )
    });
}

/// [MSR-4] applies its affine-left/L0-right bridge to an FN-8 numeric goal.
/// Here `larger` publishes `doubled <= widened`, while the caller's entry
/// requirement and nonzero edge prove `length < doubled`. Neither half alone
/// proves the call requirement.
fn scalar_growth_bridge_program(
    length_requirement: bool,
    nonzero_guard: bool,
    total_postcondition: bool,
    nested_requirement: bool,
) -> String {
    let length_contract = if length_requirement {
        " contract {\n  requires length <= capacity;\n}"
    } else {
        ""
    };
    let nonzero_guard = if nonzero_guard {
        "  if capacity == 0_u64 {\n    return unit;\n  }\n"
    } else {
        ""
    };
    let total_postcondition = if total_postcondition {
        "  ensures result >= total;\n"
    } else {
        ""
    };
    let room_contract = if nested_requirement {
        "  define room = length < capacity;\n  define positive = capacity > 0_u64;\n  define complete = band(room, positive);\n  requires complete;"
    } else {
        "  requires length < capacity;"
    };
    format!(
        r#"fn larger(current: u64, total: u64) -> result: u64 pure contract {{
  ensures result >= current;
{total_postcondition}}} {{
  if current >= total {{
    return current;
  }}
  return total;
}}

fn require_room(length: u64, capacity: u64) -> result: unit pure contract {{
{room_contract}
}} {{
  return unit;
}}

fn prove_growth(length: u64, capacity: u64) -> result: unit pure{length_contract} {{
{nonzero_guard}  if capacity <= 9223372036854775807_u64 {{
    let doubled = capacity + capacity;
    let widened = larger(current: capacity, total: doubled);
    require_room(length: length, capacity: widened);
  }}
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

#[test]
fn fn8_uses_the_affine_left_l0_right_bridge_for_scalar_growth() {
    for nested_requirement in [false, true] {
        let source = scalar_growth_bridge_program(true, true, true, nested_requirement);
        with_semantics(source.as_bytes(), |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "the complete scalar growth proof must discharge FN-8: {outcome:?}"
            );
        });
    }
}

#[test]
fn fn8_growth_bridge_requires_each_written_source_fact() {
    for source in [
        scalar_growth_bridge_program(false, true, true, false),
        scalar_growth_bridge_program(true, false, true, false),
        scalar_growth_bridge_program(true, true, false, false),
    ] {
        super::assert_rule_kind(source.as_bytes(), SemanticRule::Fn8, |kind| {
            matches!(
                kind,
                SemanticIssueKind::UndischargedCallRequirement(detail)
                    if detail.disposition == crate::CallRequirementDisposition::Unproved
            )
        });
    }
}

/// The right bridge may be a live measure rather than a scalar binding. The
/// nonzero-start range captures `3 * capacity - capacity`, so its length has
/// no L0 equality to `doubled`, `tripled`, or another scalar binding. Proving
/// `length < ceiling` therefore needs the live `part.len` candidate and the
/// callee's `part.len <= ceiling` bridge.
#[test]
fn fn8_bridge_visits_a_live_measure_before_scalar_bindings() {
    let source = br#"fn range_ceiling(part: &[u8]) -> ceiling: u64 reads(part.len) contract {
  ensures ceiling >= deref(part).len;
} {
  return deref(part).len;
}

fn require_room(length: u64, ceiling: u64) -> result: unit pure contract {
  requires length < ceiling;
} {
  return unit;
}

fn caller(length: u64, capacity: u64) -> result: unit pure contract {
  requires length <= capacity;
  requires capacity <= 2_u64;
} {
  if capacity == 0_u64 {
    return unit;
  }
  let doubled = capacity + capacity;
  let tripled = doubled + capacity;
  let storage = array_filled::<u8, 6>(value: 0_u8);
  let part = &storage[capacity..tripled];
  let ceiling = range_ceiling(part: part);
  require_room(length: length, ceiling: ceiling);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "the live range measure must be available to the shared MSR-4 bridge: {outcome:?}"
        );
    });
}

#[test]
fn forward_calls_retain_paths_and_exact_literal_place_and_named_const_images() {
    let source = br#"const requirement_limit: u64 = 8_u64;

const equal_value_other_const: u64 = 8_u64;

fn main() -> status: std::process::ExitStatus pure {
  let local = 3_u64;
  let from_place = below(value: local);
  let from_literal = below(value: 4_u64);
  let from_named = below(value: equal_value_other_const);
  let from_same_named = below(value: requirement_limit);
  return std::process::exit_status(code: 0_u8);
}

fn below(value: u64) -> result: u64 pure contract {
  requires value < requirement_limit;
} {
  return value;
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("forward call goal metadata must check: {outcome:?}");
        };
        let main = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main function");
        let CheckedStatement::Let { binding: local, .. } =
            &main.body.as_deref().expect("WF body")[0]
        else {
            panic!("main must bind the place actual");
        };
        let calls = main.body.as_deref().expect("WF body")[1..=4]
            .iter()
            .map(|statement| match statement {
                CheckedStatement::Let {
                    value: call @ CheckedExpression::UserCall { .. },
                    ..
                } => call,
                other => panic!("expected retained user call, got {other:?}"),
            })
            .collect::<Vec<_>>();

        for call in &calls {
            let CheckedExpression::UserCall {
                call,
                argument_nodes,
                requirements,
                ..
            } = call
            else {
                unreachable!();
            };
            let [requirement] = requirements.as_slice() else {
                panic!("call must retain exactly one requirement");
            };
            assert_eq!(argument_nodes.len(), 1);
            assert_ne!(*call, argument_nodes[0]);
            assert_ne!(*call, requirement.requires_clause);
        }
        let call_paths = calls
            .iter()
            .map(|call| match call {
                CheckedExpression::UserCall { call, .. } => call,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_ne!(call_paths[0], call_paths[1]);
        assert_ne!(call_paths[1], call_paths[2]);

        assert!(matches!(
            &instantiated_call_goal_arguments(calls[0])[0],
            GoalExpression::Datum(GoalDatum::Place { root, projections, ty })
                if *root == *local && projections.is_empty()
                    && *ty == CheckedType::Integer(IntegerType::U64)
        ));
        assert!(matches!(
            &instantiated_call_goal_arguments(calls[1])[0],
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer {
                ty: IntegerType::U64,
                bits: 4,
            }))
        ));
        let GoalExpression::Datum(GoalDatum::NamedConst {
            declaration: actual_const,
            ..
        }) = &instantiated_call_goal_arguments(calls[2])[0]
        else {
            panic!("named actual must retain declaration identity");
        };
        let GoalExpression::Datum(GoalDatum::NamedConst {
            declaration: requirement_const,
            ..
        }) = &instantiated_call_goal_arguments(calls[2])[1]
        else {
            panic!("requirement operand must retain declaration identity");
        };
        assert_ne!(actual_const, requirement_const);
        let CheckedExpression::UserCall {
            arguments,
            goal_arguments,
            ..
        } = calls[2]
        else {
            unreachable!();
        };
        assert!(matches!(
            &arguments[0],
            CheckedExpression::NamedConstant { declaration, .. }
                if declaration == actual_const
        ));
        assert_eq!(
            &goal_arguments[0],
            &instantiated_call_goal_arguments(calls[2])[0]
        );

        let same_arguments = instantiated_call_goal_arguments(calls[3]);
        assert_eq!(same_arguments[0], same_arguments[1]);
        assert!(matches!(
            calls[3],
            CheckedExpression::UserCall { arguments, .. }
                if matches!(
                    &arguments[0],
                    CheckedExpression::NamedConstant { declaration, .. }
                        if declaration == requirement_const
                )
        ));
        assert_eq!(main.entailment.call_goals.len(), 4);
        assert_eq!(
            main.entailment
                .call_goals
                .iter()
                .map(|outcome| outcome.disposition)
                .collect::<Vec<_>>(),
            vec![
                CallGoalDisposition::Discharged,
                CallGoalDisposition::Discharged,
                CallGoalDisposition::Refuted,
                CallGoalDisposition::Refuted,
            ]
        );
        assert_eq!(
            main.entailment.call_goals[0].evidence,
            vec![CallGoalEvidence::ExactL0Projection]
        );
        assert_eq!(
            main.entailment.call_goals[2].evidence,
            vec![CallGoalEvidence::NegatedL0Projection]
        );
    });
}

#[test]
fn call_front_end_captures_a_subscript_until_entailment_admits_its_identity() {
    let source = br#"const values: Array<u8, 2> =[3_u8, 3_u8];

fn positive(value: u8) -> result: unit pure contract {
  requires value < 10_u8;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  positive(value: values[0_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a discharged direct-subscript actual must check: {outcome:?}");
        };
        let main = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main function");
        let CheckedStatement::Evaluate {
            value: call @ CheckedExpression::UserCall { .. },
            ..
        } = &main.body.as_deref().expect("WF body")[0]
        else {
            panic!("main must retain the call expression");
        };
        let CheckedExpression::UserCall {
            call: call_path,
            argument_nodes,
            arguments,
            goal_arguments,
            requirements,
            ..
        } = call
        else {
            unreachable!();
        };
        let [requirement] = requirements.as_slice() else {
            panic!("call must retain exactly one requirement");
        };
        assert!(matches!(
            &arguments[0],
            CheckedExpression::ReadStorage { root, .. }
                if matches!(root.root, super::super::places::PlaceRoot::Constant(_))
                    && matches!(root.path.as_slice(), [super::super::model::CheckedPlaceStep::Subscript(index)]
                        if !index.obligation.components().is_empty())
        ));
        assert_eq!(argument_nodes.len(), 1);
        let GoalExpression::Datum(GoalDatum::EvaluatedValue {
            function: caller,
            occurrence:
                super::super::goal::EvaluatedValueOccurrence::CallArgument { call, argument },
            captured_type,
            projections,
            ty,
        }) = &goal_arguments[0]
        else {
            panic!("the front end must retain an occurrence-local pre-admission capture");
        };
        assert_eq!(*caller, main.id);
        assert_eq!(call, call_path);
        assert_eq!(*argument, 0);
        assert_eq!(*captured_type, CheckedType::Integer(IntegerType::U8));
        assert_eq!(*ty, *captured_type);
        assert!(projections.is_empty());
        let GoalExpression::Operation { arguments, .. } = &requirement.goal.root else {
            panic!("positive requirement must remain a comparison");
        };
        assert_eq!(&arguments[0], &goal_arguments[0]);
        assert_eq!(main.entailment.call_goals.len(), 1);
        assert_eq!(
            main.entailment.call_goals[0].disposition,
            CallGoalDisposition::Unproved
        );
        let GoalExpression::Operation { arguments, .. } = &main.entailment.call_goals[0].goal.root
        else {
            panic!("the entailment goal must retain its comparison root");
        };
        assert!(matches!(
            &arguments[0],
            GoalExpression::Operation {
                row: GoalOperation::ArrayIndex { .. },
                ..
            }
        ));
        assert!(main.entailment.call_goals[0].evidence.is_empty());
    });
}

#[test]
fn proved_subscript_actual_reuses_its_stable_goal_identity_at_fn8() {
    let source = br#"const values: Array<u8, 2> =[3_u8, 3_u8];

fn positive(value: u8) -> result: unit pure contract {
  requires value < 10_u8;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  if values[0_u64] < 10_u8 {
    positive(value: values[0_u64]);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the admitted indexed value must satisfy the matching requirement: {outcome:?}");
        };
        let main = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main function");
        let [call] = main.entailment.call_goals.as_slice() else {
            panic!("the true branch must retain one call requirement");
        };
        assert_eq!(call.disposition, CallGoalDisposition::Discharged);
        let GoalExpression::Operation { arguments, .. } = &call.goal.root else {
            panic!("the requirement must retain its comparison root");
        };
        assert!(matches!(
            &arguments[0],
            GoalExpression::Operation {
                row: GoalOperation::ArrayIndex { .. },
                ..
            }
        ));
    });
}

/// A concrete reference-root datum already denotes its referent. Substituting
/// `&deref(value)` therefore removes the callee wrapper and retains the
/// caller's reference binding as the root with no projection step.
#[test]
fn borrow_substitution_normalizes_a_reborrow_to_the_caller_reference_root() {
    let source = br#"fn observe(value: &u64) -> result: unit reads(value) contract {
  requires deref(value) > 0_u64;
} {
  let copied = deref(value);
  return unit;
}

fn proxy(value: &u64) -> result: unit reads(value) {
  observe(value: &deref(value));
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let local = 1_u64;
  observe(value: &local);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("borrow and reborrow call goals must check: {outcome:?}");
        };
        let proxy = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "proxy")
            .expect("proxy function");
        let CheckedStatement::Evaluate {
            value: CheckedExpression::UserCall { requirements, .. },
            ..
        } = &proxy.body.as_deref().expect("WF body")[0]
        else {
            panic!("proxy must retain its call requirement");
        };
        let [requirement] = requirements.as_slice() else {
            panic!("proxy call must retain exactly one requirement");
        };
        let GoalExpression::Operation { arguments, .. } = &requirement.goal.root else {
            panic!("observe requirement must remain a comparison");
        };
        assert!(matches!(
            &arguments[0],
            GoalExpression::Datum(GoalDatum::Place { root, projections, .. })
                if *root == proxy.parameters[0].binding
                    && projections.is_empty()
        ));
        assert_eq!(proxy.entailment.call_goals.len(), 1);
        assert_eq!(
            proxy.entailment.call_goals[0].disposition,
            CallGoalDisposition::Unproved
        );

        let main = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main function");
        let CheckedStatement::Let { binding: local, .. } =
            &main.body.as_deref().expect("WF body")[0]
        else {
            panic!("main local binding");
        };
        let CheckedStatement::Evaluate {
            value: CheckedExpression::UserCall { requirements, .. },
            ..
        } = &main.body.as_deref().expect("WF body")[1]
        else {
            panic!("main direct call requirement");
        };
        let [requirement] = requirements.as_slice() else {
            panic!("main call must retain exactly one requirement");
        };
        let GoalExpression::Operation { arguments, .. } = &requirement.goal.root else {
            panic!("observe requirement must remain a comparison");
        };
        assert!(matches!(
            &arguments[0],
            GoalExpression::Datum(GoalDatum::Place { root, projections, .. })
                if *root == *local && projections.is_empty()
        ));
        assert_eq!(main.entailment.call_goals.len(), 1);
        assert_eq!(
            main.entailment.call_goals[0].disposition,
            CallGoalDisposition::Discharged
        );
        assert_eq!(
            main.entailment.call_goals[0].evidence,
            vec![CallGoalEvidence::ExactL0Projection]
        );
    });
}

/// The slice half of this case is retired with v0.59's regions: it asserted
/// that a `Slice<'r, T>` formal region and its caller's region substitute to
/// two distinct regions, and v0.60 has no region parameter and no view type at
/// all ([REF-4]'s `&[T]` carries no brand). The type-and-const substitution
/// half is unchanged and is what this case now states.
#[test]
fn call_goal_substitutes_type_and_const_arguments() {
    let source = br#"fn guarded<T: Int, const n: u64>(value: T, values: Slots<u8, n>) -> result: T pure contract {
  define positive = value > 0_T;
  define size = values.len;
  define exact = size == size;
  define complete = band(positive, exact);
  requires complete;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u8, 3>();
  let result = guarded::<i32, 3>(value: 4_i32, values: move values);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("concrete generic substitutions must check: {outcome:?}");
        };
        let main = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main function");
        let CheckedStatement::Let {
            value:
                CheckedExpression::UserCall {
                    call,
                    argument_nodes,
                    goal_arguments,
                    requirements,
                    ..
                },
            ..
        } = &main.body.as_deref().expect("WF body")[1]
        else {
            panic!("guarded call metadata");
        };
        let [requirement] = requirements.as_slice() else {
            panic!("guarded call must retain exactly one requirement");
        };
        assert_eq!(argument_nodes.len(), 2);
        assert!(argument_nodes[0].components() < argument_nodes[1].components());
        assert_ne!(*call, argument_nodes[0]);
        assert_ne!(*call, argument_nodes[1]);
        assert!(matches!(
            goal_arguments[1].ty(),
            CheckedType::Window {
                shape: WindowShape::Slots,
                capacity: Some(CheckedConst::Value(3)),
                ..
            }
        ));
        let GoalExpression::Operation {
            row: GoalOperation::Boolean(_),
            arguments,
            ..
        } = &requirement.goal.root
        else {
            panic!("guarded requirement must remain the complete band goal");
        };
        let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation: CheckedIntegerOperation::Greater,
                    operand_type,
                },
            arguments: positive_arguments,
            ..
        } = &arguments[0]
        else {
            panic!("generic positive subgoal");
        };
        assert_eq!(*operand_type, CheckedType::Integer(IntegerType::I32));
        assert!(matches!(
            &positive_arguments[1],
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer {
                ty: IntegerType::I32,
                bits: 0,
            }))
        ));
        let GoalExpression::Operation {
            arguments: size_arguments,
            ..
        } = &arguments[1]
        else {
            panic!("generic size subgoal");
        };
        for length in size_arguments {
            assert!(matches!(
                length,
                GoalExpression::Operation {
                    row: GoalOperation::ContainerMeasure {
                        measured: MeasuredKind::ConstantSlots,
                        constant: Some(CheckedConst::Value(3)),
                        ..
                    },
                    ..
                }
            ));
        }
        assert_eq!(main.entailment.call_goals.len(), 1);
        assert_eq!(
            main.entailment.call_goals[0].disposition,
            CallGoalDisposition::Discharged
        );
        assert_eq!(
            main.entailment.call_goals[0].evidence,
            vec![CallGoalEvidence::BooleanIntroductionPositive]
        );
    });
}

/// The OWN-1 bare-affine rejection inside a static requirement carries the
/// clause-specific repair because [FN-8] rejects `move` inside a contract.
#[test]
fn requires_clause_bare_affine_use_carries_the_static_repair() {
    let expected_fix =
        "restate the definition or clause over copy operands or non-consuming admitted reads";
    assert_rule(
        br#"nocopy enum Holder {
  Value();
}

fn inspect(holder: Holder) -> result: unit pure contract {
  requires eeq(holder, holder);
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let holder = Holder::Value();
  let held = inspect(holder: move holder);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Own1,
        SemanticIssueKind::BareAffineUse {
            mechanical_fix: expected_fix,
        },
    );
}

#[test]
fn affine_requirements_publish_only_established_non_l0_ordering_leaves() {
    let cases = [
        ("  requires a + b <= limit;", true),
        (
            "  define sum = a + b;\n  define bound = sum <= limit;\n  define permitted = a <= 16_u64;\n  requires band(bound, permitted);",
            true,
        ),
        (
            "  define sum = a + b;\n  define overflow = sum > limit;\n  define excluded = a > 16_u64;\n  define either = bor(overflow, excluded);\n  requires bnot(either);",
            true,
        ),
        (
            "  define sum = a + b;\n  define bound = sum <= limit;\n  define permitted = a <= 16_u64;\n  requires bor(bound, permitted);",
            false,
        ),
        ("  requires a + b == limit;", false),
    ];
    for (requirement, accepted) in cases {
        // Contract definitions precede every requirement in canonical source.
        let source = format!(
            "fn room(a: u64, b: u64, limit: u64) -> result: u64 pure contract {{\n{requirement}\n  requires a <= 16_u64;\n  requires b <= 16_u64;\n}} {{\n  let total = a + b;\n  let remaining = limit - total;\n  return remaining;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_semantics(source.as_bytes(), |outcome| {
            if accepted {
                let SemanticOutcome::Complete(checked) = outcome else {
                    panic!("affine S4 image {requirement}: {outcome:?}");
                };
                let function = checked
                    .data
                    .functions
                    .iter()
                    .find(|f| f.name == "room")
                    .unwrap();
                super::entailment::validate_derivations(&function.entailment);
                assert!(
                    function
                        .entailment
                        .derivations
                        .nodes
                        .iter()
                        .any(|node| matches!(
                            node,
                            super::super::entailment::DerivationNode::RequirementAffineImage { .. }
                        ))
                );
            } else {
                let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                    panic!("an unestablished ordering leaf must remain unavailable: {outcome:?}");
                };
                assert_eq!(issue.rule(), SemanticRule::Op2);
            }
        });
    }
}

#[test]
fn affine_requirement_images_keep_copies_but_do_not_retarget_replaced_scalars() {
    for (body, accepted) in [
        (
            "  let old = a;\n  set a = 32_u64;\n  let total = old + b;",
            true,
        ),
        ("  set a = 32_u64;\n  let total = a + b;", false),
    ] {
        let source = format!(
            "fn room(a: u64, b: u64, limit: u64) -> result: u64 pure contract {{\n  requires a <= 16_u64;\n  requires b <= 16_u64;\n  requires a + b <= limit;\n}} {{\n{body}\n  let remaining = limit - total;\n  return remaining;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_semantics(source.as_bytes(), |outcome| {
            if accepted {
                let SemanticOutcome::Complete(checked) = outcome else {
                    panic!("the copied entry value keeps its requirement: {outcome:?}");
                };
                let function = checked
                    .data
                    .functions
                    .iter()
                    .find(|f| f.name == "room")
                    .unwrap();
                super::entailment::validate_derivations(&function.entailment);
            } else {
                let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                    panic!("the replacement is not the captured value: {outcome:?}");
                };
                assert_eq!(issue.rule(), SemanticRule::Op2);
            }
        });
    }
}

#[test]
fn affine_requirement_measure_observations_survive_as_values_without_retargeting() {
    for (observed, accepted) in [("old", true), ("current", false)] {
        let source = format!(
            // [OP-15] the measure is a member read, [SET-1] with [WIN-3]'s
            // disposition is the statement that replaces the window, and an
            // `own` parameter carries no effect entry [EFF-1], so the row is
            // `pure`. The subject is unchanged: the scalar copied before the
            // write keeps the bound the requirement gave it, and the measure
            // read after the write does not inherit it.
            "fn room(values: Slots<u64, 16>, extra: u64, limit: u64) -> result: u64 pure contract {{\n  requires values.len <= 16_u64;\n  requires extra <= 16_u64;\n  requires values.len + extra <= limit;\n}} {{\n  let old = values.len;\n  let seed = array_filled::<u64, 16>(value: 0_u64);\n  let fresh = slots_from_array::<u64, 16>(values: seed);\n  set values = move fresh;\n  let current = values.len;\n  let total = {observed} + extra;\n  let remaining = limit - total;\n  return remaining;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_semantics(source.as_bytes(), |outcome| {
            if accepted {
                let SemanticOutcome::Complete(checked) = outcome else {
                    panic!("an observed measure keeps its old value: {outcome:?}");
                };
                let function = checked
                    .data
                    .functions
                    .iter()
                    .find(|f| f.name == "room")
                    .unwrap();
                super::entailment::validate_derivations(&function.entailment);
            } else {
                let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                    panic!("the replacement's length has no old sum bound: {outcome:?}");
                };
                assert_eq!(issue.rule(), SemanticRule::Op2);
            }
        });
    }
}

/// A subscript inside a clause measure place names the element it indexes, and
/// a fact about a different element is a different term.
///
/// [MSR-1]: an admitted measure place is formed "with any number of
/// field-selection and enum-payload `psuffix`es, `deref` wrappings, and
/// subscripts", and "The subscript admission is what makes `table[i].len` a
/// term, so a storage whose elements are themselves storages has provable
/// operations." [ENT-2] then decides identity by spelling: "Two places are the
/// same term exactly when their roots resolve to the same declaration event
/// and their canonical source spellings are byte-identical." So a caller that
/// has proved the measure of row zero has proved nothing about row one, and
/// the call at index one is an ordinary [FN-8] failure rather than an
/// acceptance the subscript was dropped from.
///
/// The offset here is a value parameter of the callee, which the template
/// carries by ordinal and each reader substitutes: the caller by its own
/// actual, the body by that parameter's binding (compiler/checker-facts,
/// pending). The rendered goal text is not pinned: [DIAG-3] requires byte
/// identity "only where this specification explicitly fixes both selection and
/// encoding", and no rule fixes how a subscript inside a measure place prints.
#[test]
fn a_clause_subscript_names_the_element_it_indexes() {
    let program = |index: &str| {
        format!(
            r#"fn cell_at(rows: &Array<Slots<u8, 4>, 2>, i: u64, k: u64) -> result: u8 reads(rows) contract {{
  requires i < 2_u64;
  requires k < deref(rows)[i].len;
}} {{
  return deref(rows)[i][k];
}}

fn read_first(rows: &Array<Slots<u8, 4>, 2>) -> result: u8 reads(rows) contract {{
  requires 1_u64 < deref(rows)[0_u64].len;
}} {{
  return cell_at(rows: rows, i: {index}, k: 1_u64);
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#
        )
    };
    with_semantics(program("0_u64").as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "the caller's own subscripted requirement discharges the call's: {outcome:?}"
        );
    });
    with_semantics(program("1_u64").as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("a fact about row zero proves nothing about row one: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn8);
        assert!(
            matches!(
                issue.kind(),
                SemanticIssueKind::UndischargedCallRequirement(_)
            ),
            "the failure is the call's own requirement: {issue:?}"
        );
    });
}
