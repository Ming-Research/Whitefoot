//! [GRAM-6] type-driven conditional control: a Bool condition takes `if`, an
//! enum scrutinee takes `match`, and each is the sole form for its class. The
//! two `else` spellings the rule refuses are checked here at the exact nodes
//! it names, together with the cases it deliberately leaves to [GIVE-1].

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::{assert_rule, assert_rule_at, assert_rule_kind, with_semantics};
use crate::semantic::model::{CheckedStatement, ValueInitializerKind};

#[test]
fn a_bool_scrutinee_match_is_a_gram6_rejection_at_the_scrutinee() {
    // The Bool `match` is this test's whole subject: any mechanical rewrite
    // into the `if` [GRAM-6] demands leaves a source that checks clean and an
    // assertion that no longer asserts anything.
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let flag = True();
  match flag {
    True() => {
      return std::process::exit_status(code: 0_u8);
    }
    False() => {
      return std::process::exit_status(code: 0_u8);
    }
  }
}
"#;
    assert_rule_at(source, SemanticRule::Gram6, "flag");
}

#[test]
fn an_empty_else_is_a_gram6_rejection_at_the_if() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let flag = True();
  if flag {
    return std::process::exit_status(code: 0_u8);
  } else {
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_at(
        source,
        SemanticRule::Gram6,
        "if flag {\n    return std::process::exit_status(code: 0_u8);\n  } else {\n  }",
    );
}

#[test]
fn an_unflattened_else_if_is_a_gram6_rejection_at_the_nested_if() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let flag = True();
  if flag {
    return std::process::exit_status(code: 0_u8);
  } else {
    if flag {
      return std::process::exit_status(code: 0_u8);
    } else {
      return std::process::exit_status(code: 0_u8);
    }
  }
}
"#;
    assert_rule_at(
        source,
        SemanticRule::Gram6,
        "if flag {\n      return std::process::exit_status(code: 0_u8);\n    } else {\n      return std::process::exit_status(code: 0_u8);\n    }",
    );
}

#[test]
fn a_non_bool_condition_is_a_gram6_rejection_at_the_condition() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let count = 3_u64;
  if count {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_at(source, SemanticRule::Gram6, "count");
}

#[test]
fn checked_value_initializers_retain_their_source_production_kind() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let flag = True();
  let from_if = if flag {
    give 1_i32;
  } else {
    give 2_i32;
  }
  let outcome = Ok<i32, Overflow>(value: 1_i32);
  let from_match = match outcome {
    Ok(value: payload) => {
      give payload;
    }
    Err(error: problem) => {
      give 0_i32;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("value initializer kinds must check: {outcome:?}");
        };
        let body = &checked.data.functions[0].body.as_deref().expect("WF body");
        let CheckedStatement::ValueMatchLet { kind, .. } = &body[1] else {
            panic!("second statement must be the value-if initializer");
        };
        assert_eq!(*kind, ValueInitializerKind::ValueIf);
        let CheckedStatement::ValueMatchLet { kind, .. } = &body[3] else {
            panic!("fourth statement must be the value-match initializer");
        };
        assert_eq!(*kind, ValueInitializerKind::ValueMatch);
    });
}

#[test]
fn a_value_if_holds_its_deliveries_to_one_exact_type() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let flag = True();
  let picked = if flag {
    give 1_i32;
  } else {
    give 2_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Give1, |kind| {
        matches!(kind, SemanticIssueKind::TypeMismatch { .. })
    });
}

/// [GIVE-1] owns the undelivering `value_if`, not GRAM-6: a `value_if`'s
/// `else` is mandatory by grammar, so an empty one is an empty delivery set
/// rather than the else-free form GRAM-6 asks for.
#[test]
fn an_empty_value_if_else_is_a_give1_rejection() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let flag = True();
  let picked = if flag {
    give 1_i32;
  } else {
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(source, SemanticRule::Give1, SemanticIssueKind::InvalidGive);
}
