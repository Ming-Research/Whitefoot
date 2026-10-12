use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::model::{
    CheckedExpression, CheckedFloatOperation, CheckedStatement, CheckedType, FloatType,
};
use super::{assert_rule, assert_rule_kind, with_semantics};

#[test]
fn retains_the_complete_direct_float_operation_family() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let a = fadd.strict(1.0_f32, 2.0_f32);
  let b = fsub.strict(a, 1.0_f32);
  let c = fmul.strict(a, b);
  let d = fdiv.strict(c, b);
  let e = feq(a, d);
  let f = flt(b, a);
  let g = fle(b, a);
  let h = fgt(a, b);
  let i = fge(a, b);
  let j = fne(a, b);
  let k = fneg(a);
  let l = fabs(k);
  let m = fcopysign(a, k);
  let n = fmin(a, b);
  let o = fmax(a, b);
  let p = ffloor(a);
  let q = fceil(a);
  let r = ftrunc(a);
  let s = froundeven(a);
  let t = frem(a, b);
  let u = fsqrt.strict(a);
  let v = ffma.strict(a, b, c);
  let w = finf::<f32>();
  let x = fnan::<f32>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("complete direct float family must check: {outcome:?}");
        };
        let operations = checked.data.functions[0]
            .body
            .as_deref()
            .expect("WF body")
            .iter()
            .filter_map(|statement| match statement {
                CheckedStatement::Let {
                    value:
                        CheckedExpression::FloatOperation {
                            operation,
                            operand_type,
                            ..
                        },
                    ..
                } => Some((*operation, *operand_type)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(operations.len(), 24);
        assert_eq!(
            operations[0],
            (
                CheckedFloatOperation::AddStrict,
                CheckedType::Float(FloatType::F32)
            )
        );
        assert_eq!(
            operations[21],
            (
                CheckedFloatOperation::FusedMultiplyAddStrict,
                CheckedType::Float(FloatType::F32)
            )
        );
        assert_eq!(
            operations[23],
            (
                CheckedFloatOperation::Nan,
                CheckedType::Float(FloatType::F32)
            )
        );
    });
}

#[test]
fn float_literal_and_operation_failures_keep_their_rule_owners() {
    // FORM-5 minimizes bytes, then ASCII lexicographic order: 500.0 and
    // 5.0e2 tie, so 5.0e2 wins; 1.0 is shortest; 1.0e-5 beats 0.00001.
    for (literal, canonical) in [
        ("0.5e3_f64", Some("5.0e2_f64")),
        ("1.00_f64", Some("1.0_f64")),
        ("0.00001_f64", Some("1.0e-5_f64")),
        ("1.0e999_f64", None),
        ("1.0e99_f32", None),
    ] {
        let source = format!(
            "fn value() -> result: {} pure {{\n  return {literal};\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n",
            if literal.ends_with("_f32") {
                "f32"
            } else {
                "f64"
            },
        );
        assert_rule(
            source.as_bytes(),
            SemanticRule::Form7,
            SemanticIssueKind::InvalidFloatLiteral {
                canonical_spelling: canonical.map(str::to_owned),
                mechanical_fix: match canonical {
                    Some(spelling) => format!("write the literal as `{spelling}`"),
                    None => "replace the literal with a canonical spelling of a finite value representable in its stated type".to_owned(),
                },
            },
        );
    }
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = fadd.strict(1_i32, 2_i32);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
    assert_rule_kind(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = fadd.strict(1.0_f64, 2_i32);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}
