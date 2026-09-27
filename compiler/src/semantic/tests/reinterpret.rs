use std::fmt::Write;

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::model::{
    CheckedExpression, CheckedNumericType, CheckedStatement, FloatType, IntegerType,
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
fn retains_every_equal_width_reinterpret_pair() {
    let mut source = String::new();
    let mut expected = Vec::new();
    for (source_name, source_type) in NUMERIC_TYPES {
        for (destination_name, destination_type) in NUMERIC_TYPES {
            if !source_type.reinterprets_to(destination_type) {
                continue;
            }
            writeln!(
                source,
                "fn reinterpret_{source_name}_{destination_name}(value: {source_name}) -> result: {destination_name} pure {{\n  return reinterpret::<{source_name}, {destination_name}>(value);\n}}\n"
            )
            .expect("write reinterpret function");
            expected.push((source_type, destination_type));
        }
    }
    source
        .push_str("fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n");

    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("complete reinterpret family must check: {outcome:?}");
        };
        assert_eq!(expected.len(), 16);
        assert_eq!(
            checked
                .data
                .functions
                .iter()
                .filter(|function| function.body.is_some())
                .count(),
            expected.len() + 1
        );
        for (function, (expected_source, expected_destination)) in
            checked.data.functions.iter().zip(expected)
        {
            let [
                CheckedStatement::Return {
                    value:
                        CheckedExpression::Reinterpret {
                            source,
                            destination,
                            ..
                        },
                    ..
                },
            ] = function.body.as_deref().expect("WF body")
            else {
                panic!("function must retain one reinterpret operation");
            };
            assert_eq!(
                (*source, *destination),
                (expected_source, expected_destination)
            );
        }
    });
}

#[test]
fn reinterpret_shape_pair_and_operand_failures_keep_their_rule_owners() {
    for source in [
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = reinterpret::<i32, i32>(1_i32);\n  return std::process::exit_status(code: 0_u8);\n}\n".as_slice(),
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = reinterpret::<i8, u16>(1_i8);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = reinterpret::<f32, f64>(1.0_f32);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = reinterpret::<i32>(1_i32);\n  return std::process::exit_status(code: 0_u8);\n}\n",
    ] {
        assert_rule(
            source,
            SemanticRule::Op1,
            SemanticIssueKind::InvalidOperation,
        );
    }
    assert_rule_kind(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = reinterpret::<i32, u32>(1_u32);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}
