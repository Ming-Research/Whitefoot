use std::fmt::Write;

use crate::lowering::{OverlapLowering, lower_checked};
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
fn every_integer_pair_has_uniform_conversion_interfaces() {
    let mut source = String::new();
    let mut expected = Vec::new();
    for (source_name, source_type) in INTEGER_TYPES {
        for (destination_name, destination_type) in INTEGER_TYPES {
            writeln!(
                source,
                "fn convert_{source_name}_{destination_name}(value: {source_name}) -> result: Result<{destination_name}, NarrowError> pure contract {{\n  requires cvt.defined::<{source_name}, {destination_name}>(value);\n}} {{\n  let exact = cvt::<{source_name}, {destination_name}>(value);\n  let valid = cvt.defined::<{source_name}, {destination_name}>(value);\n  let wrapped = cvt.wrap::<{source_name}, {destination_name}>(value);\n  return cvt.checked::<{source_name}, {destination_name}>(value);\n}}\n"
            )
            .expect("write generated source");
            expected.push((source_type, destination_type));
        }
    }
    source
        .push_str("fn main() -> status: ExitStatus pure {\n  return exit_status(code: 0_u8);\n}\n");

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
                CheckedStatement::Let { value: wrapped, .. },
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
                panic!("conversion function must retain its four interfaces");
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
            assert!(matches!(
                wrapped,
                CheckedExpression::NumericConversion {
                    mode: CheckedConversionMode::Wrap,
                    ..
                }
            ));
            assert_eq!(wrapped.ty(), CheckedType::Integer(destination_type));
            assert_eq!(
                function
                    .entailment
                    .obligations
                    .iter()
                    .filter(|obligation| obligation.family == ObligationFamily::ConversionDomain)
                    .count(),
                1,
                "only the exact conversion requires a domain proof"
            );
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
    for operation in ["cvt", "cvt.wrap"] {
        let source = format!(
            "fn main() -> status: ExitStatus pure {{\n  let value = {operation}::<i32, i64>(1_i16);\n  return exit_status(code: 0_u8);\n}}\n"
        );
        assert_rule_kind(source.as_bytes(), SemanticRule::Type5, |kind| {
            matches!(kind, SemanticIssueKind::TypeMismatch { .. })
        });
        let source = format!(
            "fn main() -> status: ExitStatus pure {{\n  let value = {operation}(1_i32);\n  return exit_status(code: 0_u8);\n}}\n"
        );
        assert_rule(
            source.as_bytes(),
            SemanticRule::Type5,
            SemanticIssueKind::InvalidOperation,
        );
        let source = format!(
            "fn main() -> status: ExitStatus pure {{\n  let value = {operation}::<i32>(1_i32);\n  return exit_status(code: 0_u8);\n}}\n"
        );
        assert_rule(
            source.as_bytes(),
            SemanticRule::Op1,
            SemanticIssueKind::InvalidOperation,
        );
        let source = format!(
            "fn main() -> status: ExitStatus pure {{\n  let flag = True();\n  let value = {operation}::<Bool, i32>(flag);\n  return exit_status(code: 0_u8);\n}}\n"
        );
        assert_rule(
            source.as_bytes(),
            SemanticRule::Op1,
            SemanticIssueKind::InvalidOperation,
        );
    }
}

#[test]
fn wrapping_conversion_rejects_concrete_and_symbolic_float_endpoints() {
    for (source_type, destination_type) in
        [("f32", "u8"), ("u8", "f64"), ("f32", "f32"), ("f64", "f32")]
    {
        let source = format!(
            "fn invalid(value: {source_type}) -> result: {destination_type} pure {{
  return cvt.wrap::<{source_type}, {destination_type}>(value);
}}

fn main() -> status: ExitStatus pure {{
  return exit_status(code: 0_u8);
}}
"
        );
        assert_rule(
            source.as_bytes(),
            SemanticRule::Op1,
            SemanticIssueKind::InvalidOperation,
        );
    }
    for parameters in [
        "S: Float, D: Int",
        "S: Int, D: Float",
        "S: Float, D: Float",
        "S, D",
    ] {
        let source = format!(
            "fn invalid<{parameters}>(value: S) -> result: D pure {{
  return cvt.wrap::<S, D>(value);
}}

fn main() -> status: ExitStatus pure {{
  return exit_status(code: 0_u8);
}}
"
        );
        assert_rule(
            source.as_bytes(),
            SemanticRule::Op1,
            SemanticIssueKind::InvalidOperation,
        );
    }
}

#[test]
fn wrapping_conversion_is_total_for_independent_integer_parameters() {
    let source = br#"fn modular<S: Int, D: Int>(value: S) -> result: D pure {
  return cvt.wrap::<S, D>(value);
}

fn forward<A: Int, B: Int>(value: A) -> result: B pure {
  return modular::<A, B>(value: value);
}

fn main() -> status: ExitStatus pure {
  let narrowed = forward::<u16, u8>(value: 511_u16);
  let widened = forward::<i8, u32>(value: -1_i8);
  let relabeled = forward::<i32, u32>(value: -1_i32);
  let identical = forward::<u64, u64>(value: 18446744073709551615_u64);
  return exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!(
                "independent Int endpoints must check without a domain requirement: {outcome:?}"
            );
        };
        assert!(checked.data.functions.iter().all(|function| {
            function
                .entailment
                .obligations
                .iter()
                .all(|obligation| obligation.family != ObligationFamily::ConversionDomain)
        }));
        lower_checked(*checked, OverlapLowering::Off)
            .expect("concrete modular conversion instances must lower");
    });
}

#[test]
fn wrapping_conversion_is_total_in_contract_definitions_and_function_actuals() {
    let source = br#"interface BytePolicy {
  fn select(value: u16) -> result: u8 pure contract {
    define reduced = cvt.wrap::<u16, u8>(value);
    requires reduced == 1_u8;
  };
}

fn select_byte(value: u16) -> result: u8 pure contract {
  define reduced = cvt.wrap::<u16, u8>(value);
  requires reduced == 1_u8;
} {
  return cvt.wrap::<u16, u8>(value);
}

binding LowByte : BytePolicy {
  select = select_byte;
}

fn dispatch<interface BytePolicy>(value: u16) -> result: u8 pure contract {
  define reduced = cvt.wrap::<u16, u8>(value);
  requires reduced == 1_u8;
} {
  return BytePolicy::select(value: value);
}

fn main() -> status: ExitStatus pure {
  let input = 257_u16;
  let reduced = cvt.wrap::<u16, u8>(input);
  if reduced == 1_u8 {
    let selected = dispatch::<LowByte>(value: input);
  }
  return exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
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

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
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
            "fn narrow(value: u32) -> result: u8 pure {{\n  {body}\n}}\n\nfn main() -> status: ExitStatus pure {{\n  return exit_status(code: 0_u8);\n}}\n"
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
