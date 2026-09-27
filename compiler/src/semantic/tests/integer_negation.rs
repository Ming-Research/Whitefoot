use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::model::{CheckedExpression, CheckedIntegerOperation, CheckedStatement};
use super::{assert_rule, with_semantics};

#[test]
fn retains_each_mode_and_rejects_unsigned_types() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let wrapped = ineg.wrap(-128_i8);
  let exact = ineg(-42_i16);
  let negation_is_defined = ineg.defined(-42_i64);
  let negation_result = ineg.checked(-42_i32);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("integer negation family must check: {outcome:?}");
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
                CheckedIntegerOperation::NegateWrap,
                CheckedIntegerOperation::NegateExact,
                CheckedIntegerOperation::NegateDefined,
                CheckedIntegerOperation::NegateChecked,
            ]
        );
    });

    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = ineg.wrap(1_u8);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
}
