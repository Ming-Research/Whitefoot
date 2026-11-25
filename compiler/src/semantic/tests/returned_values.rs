//! Postconditions proved over a returned computed integer expression: the
//! return's returned value receives the image an ordinary `let` of the same
//! expression gives its binding [FN-9, ENT-2, ENT-3].

use crate::{SemanticIssueKind, SemanticRule};

use super::assert_rule;
use super::postconditions::{assert_complete, assert_fn9_refuted, assert_fn9_unproved};

/// [FN-9, ENT-3] a returned computed integer expression proves a clause over
/// the return's returned value, which receives the image an ordinary `let`
/// of the same expression gives its binding: the offset to an operand, the
/// row's result bounds and an integer `cvt`'s equality. Each selected return
/// is proved on its own, and each result ordinal of one return has its own
/// returned value.
#[test]
fn a_returned_computed_integer_proves_what_a_let_of_it_proves() {
    assert_complete(
        br#"fn bump(value: u64, up: Bool) -> result: u64 pure contract {
  requires value <= 999999999999999999_u64;
  ensures result <= 1000000000000000000_u64;
} {
  if up {
    return value + 1_u64;
  }
  return value;
}
"#,
    );
    assert_complete(
        br#"fn next(value: u64) -> result: u64 pure contract {
  requires value < 18446744073709551615_u64;
  ensures result == value + 1_u64;
} {
  return value + 1_u64;
}
"#,
    );
    assert_complete(
        br#"fn digit(value: u64) -> result: u64 pure contract {
  ensures result < 10_u64;
} {
  return value % 10_u64;
}
"#,
    );
    assert_complete(
        br#"fn widen(value: u8) -> result: u64 pure contract {
  ensures result <= 255_u64;
} {
  return cvt::<u8, u64>(value);
}
"#,
    );
    assert_complete(
        br#"fn split(value: u64) -> (low: u64, high: u64) pure contract {
  requires value <= 100_u64;
  ensures low < 10_u64;
  ensures high <= 10_u64;
} {
  return value % 10_u64, value / 10_u64;
}
"#,
    );
}

/// [FN-9, ENT-6, ENT-3.S7] the returned value also holds the affine value
/// image and the captured division a `let` binding of the same expression
/// holds, so each clause below is proved by both spellings: an L0-only
/// returned value proves neither, since `low <= value + step` relates three
/// values and `half < count` needs the scaled image `2*half <= count`. The
/// two-result returns read no whole-expression image.
#[test]
fn a_returned_computed_integer_holds_the_affine_image_of_a_let_of_it() {
    for body in [
        "return value + step, 0_u64;",
        "let low = value + step;\n  return low, 0_u64;",
    ] {
        assert_complete(
            format!(
                "fn pair(value: u64, step: u64) -> (low: u64, high: u64) pure contract {{
  requires value <= 1000_u64;
  requires step <= 1000_u64;
  ensures low <= value + step;
}} {{
  {body}
}}
"
            )
            .as_bytes(),
        );
    }
    for body in [
        "return count / 2_u64, count;",
        "let half = count / 2_u64;\n  return half, count;",
    ] {
        assert_complete(
            format!(
                "fn halve(count: u64) -> (half: u64, whole: u64) pure contract {{
  requires count >= 2_u64;
  ensures half < whole;
}} {{
  {body}
}}
"
            )
            .as_bytes(),
        );
    }
}

/// [FN-9] the returned value carries only what the expression's let image
/// gives: an offset of one refutes equality to the operand, a product is
/// related to no operand, the two ordinals of one return are not confused,
/// and a call result is still no computed integer expression.
#[test]
fn a_returned_computed_integer_proves_nothing_a_let_of_it_would_not() {
    assert_fn9_refuted(
        br#"fn next(value: u64) -> result: u64 pure contract {
  requires value < 18446744073709551615_u64;
  ensures result == value;
} {
  return value + 1_u64;
}
"#,
    );
    assert_fn9_unproved(
        br#"fn product(a: u64, b: u64) -> result: u64 pure contract {
  requires a >= 1_u64;
  requires a <= 10_u64;
  requires b >= 1_u64;
  requires b <= 10_u64;
  ensures result >= a;
} {
  return a * b;
}
"#,
    );
    assert_fn9_unproved(
        br#"fn split(value: u64) -> (low: u64, high: u64) pure contract {
  requires value <= 100_u64;
  ensures high < 10_u64;
} {
  return value % 10_u64, value / 10_u64;
}
"#,
    );
    assert_rule(
        br#"fn same(value: u64) -> result: u64 pure contract {
  ensures result == value;
} {
  return value;
}

fn relay(value: u64) -> result: u64 pure contract {
  ensures result == value;
} {
  return same(value: value);
}
"#,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionReturn,
    );
}
