//! [OP-1] (ii) infix resolution: the operator token selects the row, and the
//! row then takes exactly the judgment the named spelling takes.

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::model::{
    CheckedExpression, CheckedIntegerOperation, CheckedStatement, CheckedType, IntegerType,
};
use super::{assert_rule, assert_rule_at, with_semantics};

/// The operation and selected type of the one integer operation in `main`.
fn sole_operation(source: &[u8]) -> (CheckedIntegerOperation, CheckedType) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("infix source must check: {outcome:?}");
        };
        let main = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main is checked");
        let mut found = None;
        for statement in main.body.as_deref().expect("WF body") {
            let CheckedStatement::Let { value, .. } = statement else {
                continue;
            };
            if let CheckedExpression::IntegerOperation {
                operation,
                operand_type,
                ..
            } = value
            {
                assert!(found.is_none(), "exactly one integer operation");
                found = Some((*operation, *operand_type));
            }
        }
        found.expect("one integer operation")
    })
}

#[test]
fn every_operator_token_selects_its_row() {
    for (operator, expected) in [
        ("+", CheckedIntegerOperation::AddExact),
        ("+wrap", CheckedIntegerOperation::AddWrap),
        ("+defined", CheckedIntegerOperation::AddDefined),
        ("+checked", CheckedIntegerOperation::AddChecked),
        ("+sat", CheckedIntegerOperation::AddSaturating),
        ("-", CheckedIntegerOperation::SubtractExact),
        ("-wrap", CheckedIntegerOperation::SubtractWrap),
        ("-defined", CheckedIntegerOperation::SubtractDefined),
        ("-checked", CheckedIntegerOperation::SubtractChecked),
        ("-sat", CheckedIntegerOperation::SubtractSaturating),
        ("*", CheckedIntegerOperation::MultiplyExact),
        ("*wrap", CheckedIntegerOperation::MultiplyWrap),
        ("*defined", CheckedIntegerOperation::MultiplyDefined),
        ("*checked", CheckedIntegerOperation::MultiplyChecked),
        ("*sat", CheckedIntegerOperation::MultiplySaturating),
        ("/", CheckedIntegerOperation::DivideExact),
        ("/defined", CheckedIntegerOperation::DivideDefined),
        ("/checked", CheckedIntegerOperation::DivideChecked),
        ("%", CheckedIntegerOperation::RemainderExact),
        ("%defined", CheckedIntegerOperation::RemainderDefined),
        ("%checked", CheckedIntegerOperation::RemainderChecked),
    ] {
        // Proof-required exact rows are statically discharged for these
        // constant operands and therefore contribute no runtime effect.
        let source = format!(
            "fn main() -> status: std::process::ExitStatus pure {{\n  let c = 6_i32 {operator} 7_i32;\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        let (operation, operand_type) = sole_operation(source.as_bytes());
        assert_eq!(operation, expected, "operator {operator:?} selects its row");
        assert_eq!(
            operand_type,
            CheckedType::Integer(IntegerType::I32),
            "operator {operator:?} derives its selected type from the operands",
        );
    }
}

/// [OP-2] the selection comes from the first operand, and every later operand
/// is held to the row's argument type for it — so the second atom is where a
/// disagreement is reported.
#[test]
fn a_disagreeing_second_operand_is_a_type5_rejection_at_that_operand() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let a = 1_i32;
  let b = 2_u64;
  let c = a + b;
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_at(source, SemanticRule::Type5, "b");
}

/// No integer row accepts a Bool, so the selection itself fails and [OP-1]
/// reports it at the whole expression rather than at one operand.
#[test]
fn an_operand_type_outside_every_row_is_an_op1_rejection() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let f = True();
  let g = False();
  let h = f + g;
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_at(source, SemanticRule::Op1, "f + g");
}

/// A bare exact operator is proof-required for every operand shape and adds no
/// runtime effect. Unknown parameters therefore leave a static [OP-2, ENT-6]
/// obligation instead of adding a fallback path.
#[test]
fn bare_arithmetic_is_a_static_obligation_without_a_runtime_effect() {
    let source = br#"fn add(a: i32, b: i32) -> result: i32 pure {
  return a + b;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an unproved exact operation must be rejected statically: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::UndischargedIntegerDomainObligation { .. }
        ));
    });
}

/// [GRAM-5]'s complete set of positions taking a bare `expr`, enumerated from
/// the grammar rather than from whichever tests happened to fail: the `if_stmt`
/// and `value_if` conditions, `ordinary_let_rhs`, `propagate_let_rhs`,
/// `set_stmt`, `return_stmt`, `give_stmt`, and the `match_stmt` and
/// `value_match` scrutinees. `expr_stmt := call ";"` takes a `call`, so
/// infix cannot be written there and it is deliberately absent. v0.33 has no
/// `check_stmt`; contract clauses are not statements and are covered by the
/// contract tests instead.
///
/// Each source writes one infix over `a` and `b` at the named position and
/// binds the second operand with the exact line [`DISAGREEING_OPERAND`]
/// rewrites, which is what turns every entry into its own negative case.
const EXPRESSION_POSITIONS: [(&str, &str); 9] = [
    (
        "ordinary_let_rhs",
        "fn main() -> status: std::process::ExitStatus pure {
  let a = 6_u64;
  let b = 7_u64;
  let c = a +wrap b;
  return std::process::exit_status(code: 0_u8);
}
",
    ),
    (
        "propagate_let_rhs",
        "fn step(a: u64) -> result: Result<u64, Overflow> pure {
  let b = 7_u64;
  let c = propagate a +checked b;
  return Ok<u64, Overflow>(value: c);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
",
    ),
    (
        "set_stmt",
        "fn main() -> status: std::process::ExitStatus pure {
  let a = 6_u64;
  let b = 7_u64;
  set a = a +wrap b;
  return std::process::exit_status(code: 0_u8);
}
",
    ),
    (
        "return_stmt",
        "fn add(a: u64) -> result: u64 pure {
  let b = 7_u64;
  return a +wrap b;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
",
    ),
    (
        "give_stmt",
        "fn main() -> status: std::process::ExitStatus pure {
  let a = 6_u64;
  let b = 7_u64;
  let f = True();
  let c = if f {
    give a +wrap b;
  } else {
    give a;
  }
  return std::process::exit_status(code: 0_u8);
}
",
    ),
    (
        "match_stmt scrutinee",
        "fn main() -> status: std::process::ExitStatus pure {
  let a = 6_u64;
  let b = 7_u64;
  match a +checked b {
    Ok(value: v) => {
      return std::process::exit_status(code: 0_u8);
    }
    Err(error: e) => {
      return std::process::exit_status(code: 0_u8);
    }
  }
}
",
    ),
    (
        "value_match scrutinee",
        "fn main() -> status: std::process::ExitStatus pure {
  let a = 6_u64;
  let b = 7_u64;
  let c = match a +checked b {
    Ok(value: v) => {
      give 1_u64;
    }
    Err(error: e) => {
      give 2_u64;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
",
    ),
    (
        "if_stmt condition",
        "fn main() -> status: std::process::ExitStatus pure {
  let a = 6_u64;
  let b = 7_u64;
  if a +defined b {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
",
    ),
    (
        "value_if condition",
        "fn main() -> status: std::process::ExitStatus pure {
  let a = 6_u64;
  let b = 7_u64;
  let c = if a +defined b {
    give 1_u64;
  } else {
    give 2_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
",
    ),
];

/// The second operand's binding, and the disagreeing type that replaces it.
const AGREEING_OPERAND: &str = "let b = 7_u64;";
const DISAGREEING_OPERAND: &str = "let b = 7_i32;";

/// [OP-2]'s operand judgment runs at every position, not only in the `let`
/// initializer.
///
/// A position that merely stopped failing the tree could still be skipping the
/// judgment, so each source is rewritten to disagree on its second operand and
/// must report TYPE-5 at exactly that operand — the same citation the `let`
/// path produces. A position that did not check the infix cannot produce it.
#[test]
fn a_disagreeing_operand_is_reported_at_that_operand_from_every_position() {
    for (position, source) in EXPRESSION_POSITIONS {
        assert_eq!(
            source.matches(AGREEING_OPERAND).count(),
            1,
            "{position} must bind its second operand with the rewritten line",
        );
        let source = source.replace(AGREEING_OPERAND, DISAGREEING_OPERAND);
        assert_rule_at(source.as_bytes(), SemanticRule::Type5, "b");
    }
}

/// The `return` position has two structural queries, reached under
/// complementary conditions, and each broke on infix independently.
///
/// The declared result type disagrees with both the infix and a plain atom.
/// Both paths must reach FN-1's ordinary return judgment rather than treating
/// the infix expression as an internal structural failure.
#[test]
fn an_infix_returned_at_a_disagreeing_result_type_is_an_fn1_rejection() {
    let infix = br#"fn pick(a: u64) -> result: i32 pure {
  let b = 7_u64;
  return a +wrap b;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(infix, SemanticRule::Fn1, SemanticIssueKind::ReturnMismatch);
    let plain = br#"fn pick(a: u64) -> result: i32 pure {
  return a;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(plain, SemanticRule::Fn1, SemanticIssueKind::ReturnMismatch);
}
