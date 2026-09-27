use std::fmt::Write;

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule, StaticObligationDisposition};

use super::super::model::{
    CheckedConversionMode, CheckedExpression, CheckedNumericType, CheckedStatement, CheckedType,
    FloatType, IntegerType,
};
use super::{assert_rule, assert_rule_kind, with_semantics};

const NUMERIC_TYPES: [(&str, CheckedNumericType); 10] = [
    ("i8", CheckedNumericType::Integer(IntegerType::I8)),
    ("i16", CheckedNumericType::Integer(IntegerType::I16)),
    ("i32", CheckedNumericType::Integer(IntegerType::I32)),
    ("i64", CheckedNumericType::Integer(IntegerType::I64)),
    ("u8", CheckedNumericType::Integer(IntegerType::U8)),
    ("u16", CheckedNumericType::Integer(IntegerType::U16)),
    ("u32", CheckedNumericType::Integer(IntegerType::U32)),
    ("u64", CheckedNumericType::Integer(IntegerType::U64)),
    ("f32", CheckedNumericType::Float(FloatType::F32)),
    ("f64", CheckedNumericType::Float(FloatType::F64)),
];

#[test]
fn only_whole_type_conversion_domains_need_no_operand_evidence() {
    // The OP-6 table, including identity, independently of the implementation's
    // width/sign classifier. All other pairs need an operand-domain proof.
    let destinations: [&[&str]; 10] = [
        &["i8", "i16", "i32", "i64", "f32", "f64"],
        &["i16", "i32", "i64", "f32", "f64"],
        &["i32", "i64", "f64"],
        &["i64"],
        &["i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64"],
        &["i32", "i64", "u16", "u32", "u64", "f32", "f64"],
        &["i64", "u32", "u64", "f64"],
        &["u64"],
        &["f32", "f64"],
        &["f64"],
    ];
    assert_eq!(destinations.iter().map(|row| row.len()).sum::<usize>(), 39);
    for ((source_name, _), allowed) in NUMERIC_TYPES.into_iter().zip(destinations) {
        for (destination_name, _) in NUMERIC_TYPES {
            let source = format!(
                "fn convert(value: {source_name}) -> result: {destination_name} pure {{\n  return cvt::<{source_name}, {destination_name}>(value);\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
            );
            with_semantics(source.as_bytes(), |outcome| {
                if allowed.contains(&destination_name) {
                    let SemanticOutcome::Complete(checked) = outcome else {
                        panic!("total {source_name} -> {destination_name}: {outcome:?}");
                    };
                    super::entailment::validate_derivations(&checked.data.functions[0].entailment);
                } else {
                    let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                        panic!("partial {source_name} -> {destination_name}: {outcome:?}");
                    };
                    assert_eq!(issue.rule(), SemanticRule::Op6);
                    assert!(matches!(
                        issue.kind(),
                        SemanticIssueKind::UndischargedConversionDomainObligation {
                            disposition: StaticObligationDisposition::Unproved,
                            ..
                        }
                    ));
                }
            });
        }
    }
}

#[test]
fn every_float_endpoint_pair_has_uniform_exact_checked_and_defined_interfaces() {
    let mut source = String::new();
    let mut expected = Vec::new();
    for (source_name, source_type) in NUMERIC_TYPES {
        for (destination_name, destination_type) in NUMERIC_TYPES {
            if !matches!(source_type, CheckedNumericType::Float(_))
                && !matches!(destination_type, CheckedNumericType::Float(_))
            {
                continue;
            }
            writeln!(
                source,
                "fn convert_{source_name}_{destination_name}(value: {source_name}) -> result: Result<{destination_name}, NarrowError> pure contract {{\n  requires cvt.defined::<{source_name}, {destination_name}>(value);\n}} {{\n  let exact = cvt::<{source_name}, {destination_name}>(value);\n  let valid = cvt.defined::<{source_name}, {destination_name}>(value);\n  return cvt.checked::<{source_name}, {destination_name}>(value);\n}}\n"
            )
            .expect("write conversion function");
            expected.push((source_type, destination_type, destination_name));
        }
    }
    source
        .push_str("fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n");

    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("all concrete float-endpoint conversion pairs must check: {outcome:?}");
        };
        assert_eq!(expected.len(), 36);
        assert_eq!(
            checked
                .data
                .functions
                .iter()
                .filter(|function| function.body.is_some())
                .count(),
            expected.len() + 1
        );
        for (function, (expected_source, expected_destination, destination_name)) in
            checked.data.functions.iter().zip(expected)
        {
            let [
                CheckedStatement::Let { value: exact, .. },
                CheckedStatement::Let { value: defined, .. },
                CheckedStatement::Return {
                    value:
                        CheckedExpression::NumericConversion {
                            mode,
                            source,
                            destination,
                            result,
                            ..
                        },
                    ..
                },
            ] = function.body.as_deref().expect("WF body")
            else {
                panic!("conversion function must retain its three interfaces");
            };
            assert_eq!(*mode, CheckedConversionMode::Checked);
            assert!(matches!(
                exact,
                CheckedExpression::NumericConversion {
                    mode: CheckedConversionMode::Exact,
                    ..
                }
            ));
            assert_eq!(exact.ty(), expected_destination.ty());
            assert!(matches!(
                defined,
                CheckedExpression::NumericConversion {
                    mode: CheckedConversionMode::Defined,
                    ..
                }
            ));
            assert_eq!(defined.ty(), CheckedType::Bool);
            assert_eq!(
                (*source, *destination),
                (expected_source, expected_destination)
            );
            let CheckedType::Nominal(result) = result else {
                panic!("checked conversion must return Result even for total pairs");
            };
            assert_eq!(
                checked.data.nominals[result.0 as usize].name,
                format!("Result<{destination_name}, NarrowError>")
            );
        }
    });
}

#[test]
fn float_conversion_operand_failures_keep_their_rule_owners() {
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = cvt::<f32, Bool>(1.0_f32);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
    assert_rule_kind(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = cvt::<u32, f64>(1_u16);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

#[test]
fn exact_constant_domains_and_sufficient_intervals_keep_distinct_proof_meanings() {
    let source = br#"const exact: f64 = 1.5_f64;

fn endpoints(value: u32) -> result: f32 pure contract {
  requires value <= 16777216_u32;
} {
  let larger_exact = cvt::<u32, f32>(16777218_u32);
  let from_constant = cvt::<f64, f32>(exact);
  return cvt::<u32, f32>(value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!(
                "exact constants and the sufficient interval must prove their sites: {outcome:?}"
            );
        };
        let endpoints = checked
            .data
            .functions
            .iter()
            .find(|f| f.name == "endpoints")
            .unwrap();
        super::entailment::validate_derivations(&endpoints.entailment);
    });
    for (header, value, disposition) in [
        ("", "16777217_u32", StaticObligationDisposition::Refuted),
        (
            "contract {\n  requires value == 16777218_u32;\n}",
            "value",
            StaticObligationDisposition::Unproved,
        ),
    ] {
        let header = if header.is_empty() {
            String::new()
        } else {
            format!(" {header}")
        };
        let source = format!(
            "fn narrow(value: u32) -> result: f32 pure{header} {{\n  return cvt::<u32, f32>({value});\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("expected conversion-domain rejection, got {outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Op6);
            assert!(
                matches!(
                    issue.kind(),
                    SemanticIssueKind::UndischargedConversionDomainObligation { disposition: actual, .. }
                        if *actual == disposition
                ),
                "unexpected issue: {issue:?}"
            );
        });
    }
}
