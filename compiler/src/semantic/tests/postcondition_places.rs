//! [ENT-2, FN-9] the places a postcondition forms at its selected returns:
//! each subscript owes [OP-4] there, judged over the formal offset's entry
//! value and, for a base denoting an entry image, that image's length at
//! entry, and each caller substitutes its actual for a formal offset.

use crate::{SemanticOutcome, SemanticRule};

use super::postconditions::{assert_complete, assert_fn9_unproved};
use super::{assert_rule_at, with_semantics_dark};
/// The [OP-4] bounds outcomes `function` retains, after a complete check.
fn bounds_outcomes(source: &[u8], function: &str) -> Vec<bool> {
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("bounds fixture must check completely: {outcome:?}");
        };
        checked
            .data
            .functions
            .iter()
            .find(|candidate| candidate.name == function)
            .unwrap_or_else(|| panic!("function {function} must exist"))
            .entailment
            .obligations
            .iter()
            .filter(|outcome| {
                outcome.family == crate::semantic::entailment::ObligationFamily::Bounds
            })
            .map(|outcome| outcome.discharged)
            .collect()
    })
}

/// [ENT-2, FN-9] a postcondition forms its places at every return that
/// selects it, so the subscript of `rows^[i]` owes [OP-4] at each of the two
/// returns, beside the body's own subscript, and a requirement discharges
/// both. Without one the place is unformed and the clause is refused at that
/// subscript, where before nothing judged it.
#[test]
fn a_postcondition_place_owes_its_subscript_at_every_selected_return() {
    let bounded =
        br#"fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64) -> r: u64 reads(rows) contract {
  requires i < 4_u64;
  ensures r <= rows^[i].len;
} {
  if rows^[0_u64].len > 9_u64 {
    return 0_u64;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(bounds_outcomes(bounded, "width"), vec![true, true, true]);
    let unbounded = String::from_utf8(bounded.to_vec())
        .expect("fixture is text")
        .replace("  requires i < 4_u64;\n", "");
    assert_rule_at(unbounded.as_bytes(), SemanticRule::Op4, "[i]");
    // A body whose requirements contradict still judges each place, under
    // the contradiction, so its records are answered.
    let uninhabited = String::from_utf8(bounded.to_vec())
        .expect("fixture is text")
        .replace(
            "  requires i < 4_u64;\n",
            "  requires i > 9_u64;\n  requires i < 4_u64;\n",
        );
    assert_complete(uninhabited.as_bytes());
}

/// [FN-9, ENT-3.S12] a formal-valued subscript is the actual of that formal
/// at each call, at both the callee's selected returns and the caller: the
/// body proves `r == rows^[i].len` from its own read of `rows^[i].len`, and
/// of two calls only the one at offset two publishes a relation over
/// element two. Before, the offset was an unknown index on both sides, so
/// the body could not prove the relation and the caller received none.
#[test]
fn a_postcondition_formal_subscript_is_the_actual_at_each_call() {
    let source = |returned: &str| {
        format!(
            r#"fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64) -> r: u64 reads(rows) contract {{
  requires i < 4_u64;
  ensures r == rows^[i].len;
}} {{
  return rows^[i].len;
}}

fn second(rows: &Array<Slots<u8, 8>, 4>) -> r: u64 reads(rows) contract {{
  ensures r == rows^[2_u64].len;
}} {{
  let first = width(rows: rows, i: 1_u64);
  let other = width(rows: rows, i: 2_u64);
  return {returned};
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#
        )
    };
    assert_complete(source("other").as_bytes());
    assert_fn9_unproved(source("first").as_bytes());
}

/// [FN-9, ENT-3.S12] a named const actual is a value-determined offset like
/// a literal, so the relation a call publishes is over the element the const
/// names, and only that element: it proves a clause over the const and not
/// one over another element. Before, a named const actual gave no offset and
/// the call published no relation.
#[test]
fn a_named_const_actual_is_the_offset_of_a_postcondition_place() {
    let source = |element: &str| {
        format!(
            r#"const second_row: u64 = 2_u64;

fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64) -> r: u64 reads(rows) contract {{
  requires i < 4_u64;
  ensures r == rows^[i].len;
}} {{
  return rows^[i].len;
}}

fn second(rows: &Array<Slots<u8, 8>, 4>) -> r: u64 reads(rows) contract {{
  ensures r == rows^[{element}].len;
}} {{
  let w = width(rows: rows, i: second_row);
  return w;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#
        )
    };
    assert_complete(source("second_row").as_bytes());
    assert_complete(source("2_u64").as_bytes());
    assert_fn9_unproved(source("1_u64").as_bytes());
}

/// [FN-9, ENT-2, MSR-3] the offset of a postcondition place is its formal's
/// entry value: the requirement bounds that value, so the subscript is
/// discharged at every return even after the body rebinds `i` past the
/// length, but the rebinding kills the entry image the relation reads, so
/// the clause is unproved at every return. Without the requirement the
/// entry value is unbounded and [DIAG-1] reports the subscript first.
#[test]
fn a_rebound_offset_formal_leaves_its_postcondition_place_unavailable() {
    let bounded =
        br#"fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64) -> r: u64 reads(rows) contract {
  requires i < 4_u64;
  ensures r <= rows^[i].len;
} {
  set i = 9_u64;
  if rows^[0_u64].len > 9_u64 {
    return 0_u64;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_fn9_unproved(bounded);
    let unbounded = String::from_utf8(bounded.to_vec())
        .expect("fixture is text")
        .replace("  requires i < 4_u64;\n", "");
    assert_rule_at(unbounded.as_bytes(), SemanticRule::Op4, "[i]");
}

/// [FN-9, ENT-2, MSR-3] a place over an entry image is judged against that
/// image's length at function entry, not the length at the return: an
/// element the body appends does not exist in `entry(rows)^` or in an owned
/// parameter's image, so an index equal to the entry length is refused, and
/// an element the body removes still exists there, so an index one below
/// the entry length is discharged. Before, the append was accepted.
#[test]
fn an_entry_image_place_is_judged_against_its_entry_length() {
    let appended =
        br#"fn grow_claim(rows: &Slots<Slots<u8, 8>, 4>, i: u64) -> r: u64 writes(rows.next), writes(rows.len) contract {
  requires i == rows^.len;
  requires rows^.len < rows^.cap;
  ensures r <= entry(rows)^[i].len;
} {
  let fresh = slots_new::<u8, 8>();
  place_back(window: rows, value: move fresh);
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_at(appended, SemanticRule::Op4, "[i]");
    let owned = br#"fn grow_own(rows: Slots<Slots<u8, 8>, 4>, i: u64) -> r: u64 pure contract {
  requires i == rows.len;
  requires rows.len < rows.cap;
  ensures r <= rows[i].len;
} {
  let fresh = slots_new::<u8, 8>();
  place_back(window: &rows, value: move fresh);
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_at(owned, SemanticRule::Op4, "[i]");
    assert_complete(
        br#"fn shrink_claim(rows: &Slots<Slots<u8, 8>, 4>, i: u64) -> r: u64 writes(rows.last), writes(rows.len) contract {
  requires rows^.len > 0_u64;
  requires i + 1_u64 == rows^.len;
  ensures r <= entry(rows)^[i].len;
} {
  let gone = take_back(window: rows);
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
}
