use std::fmt::Write;

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule, StaticObligationDisposition};

use super::super::entailment::ObligationFamily;
use super::super::model::{
    CheckedConversionMode, CheckedExpression, CheckedNumericType, CheckedStatement, CheckedType,
    IntegerType,
};
use super::{assert_rule, assert_rule_kind, with_semantics};

const INTEGER_TYPES: [(&str, IntegerType); 8] = [
    ("i8", IntegerType::I8),
    ("i16", IntegerType::I16),
    ("i32", IntegerType::I32),
    ("i64", IntegerType::I64),
    ("u8", IntegerType::U8),
    ("u16", IntegerType::U16),
    ("u32", IntegerType::U32),
    ("u64", IntegerType::U64),
];

#[test]
fn every_integer_pair_has_uniform_exact_checked_and_defined_interfaces() {
    let mut source = String::new();
    let mut expected = Vec::new();
    for (source_name, source_type) in INTEGER_TYPES {
        for (destination_name, destination_type) in INTEGER_TYPES {
            writeln!(
                source,
                "fn convert_{source_name}_{destination_name}(value: {source_name}) -> result: Result<{destination_name}, NarrowError> pure contract {{\n  requires cvt.defined::<{source_name}, {destination_name}>(value);\n}} {{\n  let exact = cvt::<{source_name}, {destination_name}>(value);\n  let valid = cvt.defined::<{source_name}, {destination_name}>(value);\n  return cvt.checked::<{source_name}, {destination_name}>(value);\n}}\n"
            )
            .expect("write generated source");
            expected.push((source_type, destination_type));
        }
    }
    source
        .push_str("fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n");

    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("all integer conversion pairs must check: {outcome:?}");
        };
        assert_eq!(
            checked
                .data
                .functions
                .iter()
                .filter(|function| function.body.is_some())
                .count(),
            expected.len() + 1
        );
        assert_eq!(expected.len(), 64);
        for (function, (source_type, destination_type)) in
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
            assert_eq!(exact.ty(), CheckedType::Integer(destination_type));
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
                (
                    CheckedNumericType::Integer(source_type),
                    CheckedNumericType::Integer(destination_type)
                )
            );
            let CheckedType::Nominal(result) = result else {
                panic!("checked conversion must return Result even for total pairs");
            };
            assert_eq!(
                checked.data.nominals[result.0 as usize].name,
                format!(
                    "Result<{}, NarrowError>",
                    integer_spelling(destination_type)
                )
            );
        }
    });
}

#[test]
fn conversion_shape_and_operand_failures_keep_their_rule_owners() {
    assert_rule_kind(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = cvt::<i32, i64>(1_i16);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = cvt::<i32>(1_i32);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let flag = True();\n  let value = cvt::<Bool, i32>(flag);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
}

#[test]
fn proved_conversions_retain_domain_roots_and_share_caller_normalization() {
    let source = br#"fn bounded(value: u32) -> result: u8 pure contract {
  requires value <= 255_u32;
} {
  return cvt::<u32, u8>(value);
}

fn guarded(value: u32) -> result: u8 pure {
  if cvt.defined::<u32, u8>(value) {
    return cvt::<u32, u8>(value);
  } else {
    return 0_u8;
  }
}

fn required(value: u32) -> result: u8 pure contract {
  requires cvt.defined::<u32, u8>(value);
} {
  return cvt::<u32, u8>(value);
}

fn caller(value: u32) -> result: u8 pure contract {
  requires value <= 255_u32;
} {
  return required(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("range and signed-domain proofs must share the call path: {outcome:?}");
        };
        for name in ["bounded", "guarded", "required"] {
            let function = checked
                .data
                .functions
                .iter()
                .find(|f| f.name == name)
                .unwrap();
            let domains: Vec<_> = function
                .entailment
                .obligations
                .iter()
                .filter(|obligation| obligation.family == ObligationFamily::ConversionDomain)
                .collect();
            assert_eq!(domains.len(), 1, "one retained OP-6 obligation in {name}");
            assert!(domains[0].discharged);
            assert!(domains[0].derivation.is_some());
            super::entailment::validate_derivations(&function.entailment);
        }
        let caller = checked
            .data
            .functions
            .iter()
            .find(|f| f.name == "caller")
            .unwrap();
        super::entailment::validate_derivations(&caller.entailment);
    });
}

#[test]
fn conversion_diagnostics_distinguish_refutation_from_missing_or_stale_evidence() {
    for (body, disposition) in [
        (
            "return cvt::<u32, u8>(256_u32);",
            StaticObligationDisposition::Refuted,
        ),
        (
            "return cvt::<u32, u8>(value);",
            StaticObligationDisposition::Unproved,
        ),
        (
            "let allowed = cvt.defined::<u32, u8>(value);\n  set value = 300_u32;\n  if allowed {\n    return cvt::<u32, u8>(value);\n  } else {\n    return 0_u8;\n  }",
            StaticObligationDisposition::Unproved,
        ),
    ] {
        let source = format!(
            "fn narrow(value: u32) -> result: u8 pure {{\n  {body}\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("expected conversion-domain rejection, got {outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Op6);
            assert!(
                matches!(
                    issue.kind(),
                    SemanticIssueKind::UndischargedConversionDomainObligation { disposition: actual, residual, .. }
                        if *actual == disposition && residual.contains("cvt.defined")
                ),
                "unexpected issue: {issue:?}"
            );
        });
    }
}

const fn integer_spelling(ty: IntegerType) -> &'static str {
    match ty {
        IntegerType::I8 => "i8",
        IntegerType::I16 => "i16",
        IntegerType::I32 => "i32",
        IntegerType::I64 => "i64",
        IntegerType::U8 => "u8",
        IntegerType::U16 => "u16",
        IntegerType::U32 => "u32",
        IntegerType::U64 => "u64",
    }
}
