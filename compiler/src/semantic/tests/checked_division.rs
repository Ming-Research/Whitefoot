use crate::SemanticOutcome;

use super::super::model::{
    CheckedExpression, CheckedIntegerOperation, CheckedStatement, CheckedType,
};
use super::with_semantics;

#[test]
fn produces_div_error_results() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let quotient = -2147483648_i32 /checked -1_i32;
  let remainder = 42_u64 %checked 5_u64;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("checked division family must check: {outcome:?}");
        };
        let body = &checked.data.functions[0].body.as_deref().expect("WF body");
        let [
            CheckedStatement::Let {
                value:
                    CheckedExpression::IntegerOperation {
                        operation: CheckedIntegerOperation::DivideChecked,
                        result: CheckedType::Nominal(quotient_result),
                        ..
                    },
                ..
            },
            CheckedStatement::Let {
                value:
                    CheckedExpression::IntegerOperation {
                        operation: CheckedIntegerOperation::RemainderChecked,
                        result: CheckedType::Nominal(remainder_result),
                        ..
                    },
                ..
            },
            CheckedStatement::Return { .. },
        ] = *body
        else {
            panic!("checked division and remainder must retain distinct operations");
        };
        assert_eq!(
            checked.data.nominals[quotient_result.0 as usize].name,
            "Result<i32, DivError>"
        );
        assert_eq!(
            checked.data.nominals[remainder_result.0 as usize].name,
            "Result<u64, DivError>"
        );
    });
}
