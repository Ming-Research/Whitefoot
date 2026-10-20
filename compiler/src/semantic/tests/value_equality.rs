//! OP-16's type judgment, including symbolic and substituted nominal parts.

use super::{assert_rule_at, assert_rule_kind, with_semantics};
use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

const DECLARATIONS: &str = "struct EqualityWrap<T: copy> {\n  value: T;\n}\n\nenum EqualityChoice<T: copy> {\n  EqualityAbsent();\n  EqualityPresent(value: T);\n}\n\n";
const MAIN: &str = "fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";

#[test]
fn equality_types_are_structural_after_nominal_substitution() {
    for ty in [
        "unit",
        "Bool",
        "u8",
        "i64",
        "Option<u32>",
        "Result<u32, u8>",
        "Array<u8, 0>",
        "Array<EqualityWrap<Option<u32>>, 4>",
        "EqualityWrap<EqualityChoice<Array<u16, 3>>>",
    ] {
        let source = format!(
            "{DECLARATIONS}fn same(left: {ty}, right: {ty}) -> result: Bool pure {{\n  return left == right;\n}}\n\n{MAIN}"
        );
        with_semantics(source.as_bytes(), |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{ty}: {outcome:?}"
            );
        });
    }
    for (ty, part) in [
        ("f64", "operand type"),
        ("Box<u32>", "operand type"),
        ("Slots<u32, 4>", "operand type"),
        ("Ring<u32, 4>", "operand type"),
        ("EqualityWrap<f64>", "field `value`"),
        (
            "EqualityChoice<f32>",
            "payload field `EqualityPresent.value`",
        ),
        ("Option<f64>", "payload field `Some.value`"),
        ("Array<EqualityWrap<f64>, 0>", "element type, field `value`"),
    ] {
        let source = format!(
            "{DECLARATIONS}fn same(left: {ty}, right: {ty}) -> result: Bool pure {{\n  return left == right;\n}}\n\n{MAIN}"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("{ty}: {outcome:?}")
            };
            assert_eq!(issue.rule(), SemanticRule::Op1, "{ty}: {issue:?}");
            let SemanticIssueKind::InvalidEqualityType { mechanical_fix } = issue.kind() else {
                panic!("{ty}: {issue:?}")
            };
            assert!(mechanical_fix.contains(part), "{ty}: {mechanical_fix}");
        });
    }
}

#[test]
fn equality_reports_the_first_bad_part_in_declaration_order() {
    let source = format!(
        "struct EqualityBad {{\n  first: Array<f32, 2>;\n  second: f64;\n}}\n\nfn same(left: EqualityBad, right: EqualityBad) -> result: Bool pure {{\n  return left != right;\n}}\n\n{MAIN}"
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op1, |kind| {
        matches!(kind, SemanticIssueKind::InvalidEqualityType { mechanical_fix }
            if mechanical_fix.contains("field `first`, element type") && mechanical_fix.contains("f32") && !mechanical_fix.contains("second"))
    });
    let source = format!(
        "enum EqualityBad {{\n  EqualityFirst(ok: u32, bad: f32);\n  EqualitySecond(earlier: f64);\n}}\n\nfn same(left: EqualityBad, right: EqualityBad) -> result: Bool pure {{\n  return left == right;\n}}\n\n{MAIN}"
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op1, |kind| {
        matches!(kind, SemanticIssueKind::InvalidEqualityType { mechanical_fix }
            if mechanical_fix.contains("payload field `EqualityFirst.bad`") && !mechanical_fix.contains("EqualitySecond"))
    });
}

#[test]
fn symbolic_equality_requires_int_or_eq_even_inside_a_nominal() {
    for ty in ["T", "EqualityWrap<T>", "EqualityChoice<T>", "Array<T, 4>"] {
        for bound in ["Int", "Eq", "copy", "Float"] {
            let source = format!(
                "{DECLARATIONS}fn same<T: {bound}>(left: {ty}, right: {ty}) -> result: Bool pure {{\n  return left == right;\n}}\n\n{MAIN}"
            );
            with_semantics(source.as_bytes(), |outcome| {
                if matches!(bound, "Int" | "Eq") {
                    assert!(
                        matches!(outcome, SemanticOutcome::Complete(_)),
                        "{ty}: {outcome:?}"
                    );
                } else {
                    let SemanticOutcome::SourceIssue { issue } = outcome else {
                        panic!("{ty}: {outcome:?}")
                    };
                    assert_eq!(issue.rule(), SemanticRule::Op1, "{ty}: {issue:?}");
                    assert!(matches!(
                        issue.kind(),
                        SemanticIssueKind::InvalidEqualityType { .. }
                    ));
                }
            });
        }
    }
}

#[test]
fn reference_kinds_are_not_values_but_copy_referents_compare() {
    let source = format!(
        "fn same(left: &u32, right: &u32) -> result: Bool pure {{\n  return left == right;\n}}\n\n{MAIN}"
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op1, |kind| {
        matches!(kind, SemanticIssueKind::InvalidEqualityType { .. })
    });
    let source = format!(
        "{DECLARATIONS}fn same(left: &EqualityWrap<u32>, right: &EqualityWrap<u32>) -> result: Bool reads(left), reads(right) {{\n  return left^ == right^;\n}}\n\n{MAIN}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        )
    });
}

#[test]
fn retired_enum_operation_names_are_ordinary_user_identifiers() {
    let source = format!(
        "fn eeq(left: u32, right: u32) -> result: Bool pure {{\n  return left == right;\n}}\n\nfn ene(left: u32, right: u32) -> result: Bool pure {{\n  return left != right;\n}}\n\nfn uses_names(left: u32, right: u32) -> result: Bool pure {{\n  let equal_value = eeq(left: left, right: right);\n  let unequal_value = ene(left: left, right: right);\n  return bxor(equal_value, unequal_value);\n}}\n\n{MAIN}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        )
    });
}

#[test]
fn equality_type_mismatch_names_the_offending_operand_even_outside_the_domain() {
    let source = format!(
        "struct EqualityLeft {{\n  value: u32;\n}}\n\nstruct EqualityRight {{\n  value: u32;\n}}\n\nfn same(left: EqualityLeft, right: EqualityRight) -> result: Bool pure {{\n  return left == right;\n}}\n\n{MAIN}"
    );
    assert_rule_at(source.as_bytes(), SemanticRule::Type5, "right");
    let source = format!(
        "fn same(left: f32, right: f64) -> result: Bool pure {{\n  return left != right;\n}}\n\n{MAIN}"
    );
    assert_rule_at(source.as_bytes(), SemanticRule::Type5, "right");
}

#[test]
fn equality_preflight_resolves_generic_numeric_literal_types() {
    let source = format!(
        "fn same<T: Int>(left: T) -> result: Bool pure {{\n  return left == 0_T;\n}}\n\n{MAIN}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
    let source = format!(
        "fn same<T: Int>(left: f32) -> result: Bool pure {{\n  return left == 0_T;\n}}\n\n{MAIN}"
    );
    assert_rule_at(source.as_bytes(), SemanticRule::Type5, "0_T");
    let source = format!(
        "fn same<T: Float>(left: T) -> result: Bool pure {{\n  return left == 0_T;\n}}\n\n{MAIN}"
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op1, |kind| {
        matches!(kind, SemanticIssueKind::InvalidEqualityType { .. })
    });
}

#[test]
fn eq_bound_argument_rejection_names_the_argument_and_first_bad_field() {
    let source = include_bytes!("../../../../tests/conformance/cases/fn2-neg-eq-bound-float.wf");
    assert_rule_at(source, SemanticRule::Fn2, "BoundFloat");
    assert_rule_kind(source, SemanticRule::Fn2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidEqualityType { mechanical_fix }
            if mechanical_fix.contains("field `fraction`") && mechanical_fix.contains("f64"))
    });
}

#[test]
fn eq_bound_is_copy_and_supplies_symbolic_contract_goal_origins() {
    let source = format!(
        "fn need<T: Eq>(left: T, right: T) -> result: unit pure contract {{\n  requires left == right;\n}} {{\n  return unit;\n}}\n\nfn forward<T: Eq>(left: T, right: T) -> result: Bool pure {{\n  let saved = left;\n  let equal = saved == right;\n  if equal {{\n    need::<T>(left: left, right: right);\n  }}\n  return equal;\n}}\n\n{MAIN}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

#[test]
fn opaque_copy_structs_and_instant_are_equality_types() {
    for source in [
        include_str!("../../../../tests/conformance/cases/op16-pos-opaque-struct.wf").to_owned(),
        format!(
            "fn same(left: std::time::Instant, right: std::time::Instant) -> result: Bool pure {{\n  return left == right;\n}}\n\n{MAIN}"
        ),
    ] {
        with_semantics(source.as_bytes(), |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{outcome:?}"
            );
        });
    }
}
