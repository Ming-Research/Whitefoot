//! Integer selection header and coverage judgments; fact flow is covered by
//! the entailment suites and normative conformance programs.

use super::{assert_accepts, assert_rule_kind, with_resolution, with_semantics};
use crate::semantic::model::{
    CheckedMatchSelection, CheckedMatchType, CheckedStatement, CheckedType, IntegerType,
};
use crate::{ResolutionOutcome, ResolutionRule, SemanticIssueKind, SemanticOutcome, SemanticRule};

#[test]
fn integer_dispatch_and_give_accept_the_normative_cases() {
    for source in [
        include_bytes!("../../../../tests/conformance/cases/gram12-pos-constant-dispatch-run.wf")
            .as_slice(),
        include_bytes!(
            "../../../../tests/conformance/cases/gram12-pos-integer-types-and-generic-run.wf"
        )
        .as_slice(),
        include_bytes!("../../../../tests/conformance/cases/give1-pos-integer-value-match-run.wf")
            .as_slice(),
        include_bytes!("../../../../tests/conformance/cases/err2-pos-integer-full-u8-run.wf")
            .as_slice(),
    ] {
        assert_accepts(source);
    }
}

#[test]
fn integer_headers_and_coverage_fail_for_their_named_rules() {
    for source in [
        include_bytes!("../../../../tests/conformance/cases/gram12-neg-duplicate-const-value.wf")
            .as_slice(),
        include_bytes!(
            "../../../../tests/conformance/cases/gram12-neg-duplicate-character-value.wf"
        )
        .as_slice(),
    ] {
        assert_rule_kind(source, SemanticRule::Gram12, |issue| {
            matches!(issue, SemanticIssueKind::DuplicateIntegerMatchValue { .. })
        });
    }
    for source in [
        include_bytes!("../../../../tests/conformance/cases/gram12-neg-const-label-type.wf")
            .as_slice(),
        include_bytes!("../../../../tests/conformance/cases/gram12-neg-literal-label-type.wf")
            .as_slice(),
        include_bytes!("../../../../tests/conformance/cases/gram12-neg-else-not-final.wf")
            .as_slice(),
        include_bytes!("../../../../tests/conformance/cases/gram12-neg-float-label.wf").as_slice(),
        include_bytes!("../../../../tests/conformance/cases/gram12-neg-variant-label.wf")
            .as_slice(),
        include_bytes!("../../../../tests/conformance/cases/gram12-neg-else-only.wf").as_slice(),
    ] {
        assert_rule_kind(source, SemanticRule::Gram12, |issue| {
            matches!(issue, SemanticIssueKind::InvalidIntegerMatchArm { .. })
        });
    }
    for source in [
        include_bytes!("../../../../tests/conformance/cases/err2-neg-integer-missing-else.wf")
            .as_slice(),
        include_bytes!(
            "../../../../tests/conformance/cases/err2-neg-integer-complete-with-else.wf"
        )
        .as_slice(),
    ] {
        assert_rule_kind(source, SemanticRule::Err2, |issue| {
            matches!(issue, SemanticIssueKind::InvalidIntegerMatchCoverage { .. })
        });
    }
    assert_rule_kind(
        include_bytes!("../../../../tests/conformance/cases/gram6-neg-bool-match.wf"),
        SemanticRule::Gram6,
        |issue| matches!(issue, SemanticIssueKind::InvalidConditionalForm { .. }),
    );
}

#[test]
fn identifier_arm_admits_named_consts_alone() {
    for source in [
        include_bytes!("../../../../tests/conformance/cases/gram12-neg-nonconst-label.wf")
            .as_slice(),
        include_bytes!("../../../../tests/conformance/cases/gram12-neg-const-generic-label.wf")
            .as_slice(),
    ] {
        with_resolution(source, |outcome| {
            let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("non-const arm must fail lexical lookup: {outcome:?}");
            };
            assert_eq!(issue.rule(), ResolutionRule::Gram12);
        });
    }
}

#[test]
fn symbolic_int_identity_labels_use_the_declared_suffix_name() {
    let source = b"fn dispatch<N: Int>(code: N) -> result: u8 pure {\n  match code {\n    0_N => {\n      return 0_u8;\n    }\n    1_N => {\n      return 1_u8;\n    }\n    else => {\n      return 2_u8;\n    }\n  }\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let result = dispatch::<i16>(code: 0_i16);\n  return std::process::exit_status(code: result);\n}\n";
    assert_accepts(source);
}

#[test]
fn checked_integer_labels_keep_the_full_mathematical_u64_value() {
    let source = b"fn select(code: u64) -> result: u8 pure {\n  match code {\n    18446744073709551615_u64 => {\n      return 1_u8;\n    }\n    else => {\n      return 0_u8;\n    }\n  }\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("u64 maximum must be accepted: {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "select")
            .unwrap();
        let CheckedStatement::Match {
            match_type, arms, ..
        } = &function.body.as_ref().unwrap()[0]
        else {
            panic!("source match must remain a checked match");
        };
        assert_eq!(
            *match_type,
            CheckedMatchType::Integer(CheckedType::Integer(IntegerType::U64))
        );
        assert_eq!(
            arms[0].selection,
            CheckedMatchSelection::Integer(18446744073709551615_i128)
        );
        assert_eq!(arms[1].selection, CheckedMatchSelection::Else);
        assert!(
            arms.iter()
                .all(|arm| arm.binders.is_empty() && arm.covered.is_empty())
        );
    });
}

#[test]
fn generic_float_scrutinee_is_a_type5_rejection() {
    let source = b"fn dispatch<N: Float>(code: N) -> result: u8 pure {\n  match code {\n    0_N => {\n      return 0_u8;\n    }\n    else => {\n      return 1_u8;\n    }\n  }\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";
    assert_rule_kind(source, SemanticRule::Type5, |issue| {
        matches!(issue, SemanticIssueKind::TypeMismatch { .. })
    });
}

#[test]
fn integer_match_requires_an_owned_value_not_a_reference_holder() {
    let source = b"fn select(code: &u8) -> result: u8 pure {\n  match code {\n    0_u8 => {\n      return 0_u8;\n    }\n    else => {\n      return 1_u8;\n    }\n  }\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";
    assert_rule_kind(source, SemanticRule::Type7, |issue| {
        matches!(issue, SemanticIssueKind::MissingDereference { .. })
    });
}

#[test]
fn generic_float_identity_header_is_refused_by_gram12() {
    let source = b"fn dispatch<N: Float>(code: u8) -> result: u8 pure {\n  match code {\n    0_N => {\n      return 0_u8;\n    }\n    else => {\n      return 1_u8;\n    }\n  }\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";
    assert_rule_kind(source, SemanticRule::Gram12, |issue| {
        matches!(issue, SemanticIssueKind::InvalidIntegerMatchArm { .. })
    });
}
