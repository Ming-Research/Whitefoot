//! O11 signed-Boolean-decomposition tests, in both directions.
//!
//! Direction one: ordinary establishment records and establishes exactly the
//! sound decomposition set — `+band` and `-bor` decompose into their signed
//! children recursively, `bnot` flips, and `-band`, `+bor`, and `bxor` on
//! either sign contribute nothing (the classic asymmetry: the other sign's
//! content is genuinely disjunctive).
//!
//! The obligations these guards protect now discharge, which is the rule's
//! purpose: the [ENT-3] members are facts at their establishment point.
//! Design: `research/investigations/o11-composition/DESIGN.md`.

use crate::SemanticOutcome;

use super::super::entailment::{
    BooleanGoalDecomposition, FunctionEntailment, GoalSign, Relation, TermKind,
};
use super::super::goal::{GoalDatum, GoalExpression, GoalOperation};
use super::super::model::CheckedBooleanOperation;
use super::{with_semantics, with_semantics_dark};

fn entailment(source: &[u8], function: &str) -> FunctionEntailment {
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("boolean-composition test source must check completely: {outcome:?}");
        };
        let function = checked
            .data
            .functions
            .iter()
            .find(|candidate| candidate.name == function)
            .unwrap_or_else(|| panic!("function {function} must exist"));
        function.entailment.clone()
    })
}

/// The recorded parent's retained expression must have the expected Boolean
/// root; returns the entry for further member assertions.
fn entry_with_root(
    summary: &FunctionEntailment,
    operation: CheckedBooleanOperation,
    sign: GoalSign,
) -> &BooleanGoalDecomposition {
    summary
        .boolean_decompositions
        .iter()
        .find(|candidate| {
            candidate.sign == sign
                && matches!(
                    &summary.inventory.goals[candidate.parent.0 as usize].expression,
                    GoalExpression::Operation {
                        row: GoalOperation::Boolean(root),
                        ..
                    } if *root == operation
                )
        })
        .unwrap_or_else(|| {
            panic!(
                "expected a recorded {operation:?}/{sign:?} decomposition, got {:?}",
                summary.boolean_decompositions
            )
        })
}

/// The recorded entry whose parent is the `own Bool` binding a guard named,
/// rather than that binding's expanded Boolean tree. Both parents are
/// established, so both are recorded; this one carries the conjuncts in the
/// operand forms their own comparison bindings recorded.
fn binding_rooted_entry(summary: &FunctionEntailment, sign: GoalSign) -> &BooleanGoalDecomposition {
    summary
        .boolean_decompositions
        .iter()
        .find(|candidate| {
            candidate.sign == sign
                && matches!(
                    &summary.inventory.goals[candidate.parent.0 as usize].expression,
                    GoalExpression::Datum(GoalDatum::Place { .. })
                )
        })
        .unwrap_or_else(|| {
            panic!(
                "expected a recorded binding-rooted {sign:?} decomposition, got {:?}",
                summary.boolean_decompositions
            )
        })
}

/// Asserts every member of one entry is a bare `own Bool` comparison binding
/// carried at the expected sign — the shape that delivers its relation from
/// `state.origins` rather than from a projection of its own.
fn assert_binding_members(
    summary: &FunctionEntailment,
    entry: &BooleanGoalDecomposition,
    sign: GoalSign,
) {
    for member in &entry.members {
        assert_eq!(member.1, sign);
        assert!(
            matches!(
                &summary.inventory.goals[member.0.0 as usize].expression,
                GoalExpression::Datum(GoalDatum::Place { projections, .. })
                    if projections.is_empty()
            ),
            "member {member:?} must be a bare Bool binding: {:?}",
            summary.inventory.goals[member.0.0 as usize]
        );
    }
}

/// Asserts one member is a comparison goal whose retained projection is the
/// exact normalized bound `place - constant <= bound` in operand order.
fn assert_comparison_member(
    summary: &FunctionEntailment,
    member: (super::super::entailment::GoalId, GoalSign),
    sign: GoalSign,
    constant: i128,
    bound: i128,
) {
    assert_eq!(member.1, sign);
    let retained = &summary.inventory.goals[member.0.0 as usize];
    assert!(
        matches!(
            &retained.expression,
            GoalExpression::Operation {
                row: GoalOperation::Integer { .. },
                ..
            }
        ),
        "member must be a comparison goal: {retained:?}"
    );
    let Some(Relation::Bound {
        left,
        right,
        bound: held,
    }) = &retained.projection
    else {
        panic!("comparison member must retain its exact projection: {retained:?}");
    };
    assert!(
        matches!(
            summary.inventory.terms[left.0 as usize],
            TermKind::Place(..)
        ),
        "projection left operand must be the tracked place"
    );
    assert_eq!(
        summary.inventory.terms[right.0 as usize],
        // A written zero is the zero term: one term per mathematical value.
        if constant == 0 {
            TermKind::Zero
        } else {
            TermKind::Constant(constant)
        }
    );
    assert_eq!(*held, bound);
}

/// S1: a true `band` guard publishes both positive conjuncts, so both guarded
/// subscripts discharge.
#[test]
fn passed_band_guard_establishes_positive_conjuncts_and_discharges_both() {
    let source = br#"const table: Array<u8, 8> =[0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8];

fn read_pair(low: u64, high: u64) -> result: u8 pure {
  let low_ok = low < 8_u64;
  let high_ok = high < 8_u64;
  let both = band(low_ok, high_ok);
  if both {
    let first = table[low];
    let second = table[high];
    return second;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read_pair");
    // Each conjunct is established, so each subscript discharges.
    assert_eq!(summary.obligations.len(), 2);
    assert!(summary.obligations.iter().all(|o| o.discharged));
    let entry = entry_with_root(&summary, CheckedBooleanOperation::And, GoalSign::Positive);
    assert_eq!(entry.members.len(), 2);
}

/// S1: the ruled-flip guard shape. The `bor` else edge establishes both
/// negative disjuncts (whose projections negate to the two-sided bound) and
/// contributes nothing on the true edge, so the guarded subscript discharges
/// on the edge the guard protects.
#[test]
fn bor_guard_false_edge_establishes_negative_disjuncts_and_discharges() {
    let source = br#"const table: Array<u8, 4> =[0_u8, 0_u8, 0_u8, 0_u8];

fn get(symbol: u64) -> result: u8 pure {
  let below = symbol < 0_u64;
  let above = symbol >= 4_u64;
  let invalid = bor(below, above);
  if invalid {
    return 0_u8;
  } else {
    return table[symbol];
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "get");
    assert_eq!(summary.obligations.len(), 1);
    assert!(summary.obligations[0].discharged);
    // Only the false edge decomposes a disjunction; +bor contributes nothing.
    // The edge establishes the named binding and its expanded tree, so the
    // false edge records both parents and the true edge still records neither.
    assert_eq!(summary.boolean_decompositions.len(), 2);
    let named = binding_rooted_entry(&summary, GoalSign::Negative);
    assert_eq!(named.members.len(), 2);
    assert_binding_members(&summary, named, GoalSign::Negative);
    let entry = entry_with_root(&summary, CheckedBooleanOperation::Or, GoalSign::Negative);
    assert_eq!(entry.members.len(), 2);
    // -symbol < 0: projection symbol - 0 <= -1, negated at activation.
    assert_comparison_member(&summary, entry.members[0], GoalSign::Negative, 0, -1);
    // -symbol >= 4: projection normalizes ge by operand swap, 4 - symbol <= 0.
    let above = &summary.inventory.goals[entry.members[1].0.0 as usize];
    let Some(Relation::Bound { left, bound, .. }) = &above.projection else {
        panic!("the ige member must retain its projection: {above:?}");
    };
    assert_eq!(
        summary.inventory.terms[left.0 as usize],
        TermKind::Constant(4)
    );
    assert_eq!(*bound, 0);
    // The ruled flip, now live: this is the shape the 2026-08-09 decision
    // disposed. The protected corpus case records the corresponding current
    // expectation independently.
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "the guarded subscript discharges from the decomposed else edge",
        );
    });
}

/// The asymmetry, pinned: `-band` and `+bor` carry only disjunctive content
/// and record nothing, and `bxor` records nothing on either sign.
#[test]
fn disjunctive_signs_and_bxor_record_nothing() {
    let source = br#"fn classify(a: u64, b: u64) -> result: u64 pure {
  let a_small = a < 16_u64;
  let b_small = b < 16_u64;
  let both = band(a_small, b_small);
  let either = bor(a_small, b_small);
  let mixed = bxor(a_small, b_small);
  if both {
    return 0_u64;
  } else if either {
    return 1_u64;
  } else if mixed {
    return 2_u64;
  } else {
    return 3_u64;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "classify");
    // Exactly the +band then-edge and the -bor false-edge entries exist, each
    // recorded at both its named binding and its expanded tree: no -band, no
    // +bor, and no bxor entry on either sign or either parent shape — the
    // `mixed` edges establish their bindings and still decompose to nothing.
    assert_eq!(summary.boolean_decompositions.len(), 4);
    let conjunction = entry_with_root(&summary, CheckedBooleanOperation::And, GoalSign::Positive);
    assert_eq!(conjunction.members.len(), 2);
    let disjunction = entry_with_root(&summary, CheckedBooleanOperation::Or, GoalSign::Negative);
    assert_eq!(disjunction.members.len(), 2);
    let named_conjunction = binding_rooted_entry(&summary, GoalSign::Positive);
    assert_eq!(named_conjunction.members.len(), 2);
    assert_binding_members(&summary, named_conjunction, GoalSign::Positive);
    let named_disjunction = binding_rooted_entry(&summary, GoalSign::Negative);
    assert_eq!(named_disjunction.members.len(), 2);
    assert_binding_members(&summary, named_disjunction, GoalSign::Negative);
    assert!(
        conjunction
            .members
            .iter()
            .all(|member| member.1 == GoalSign::Positive)
    );
    assert!(
        disjunction
            .members
            .iter()
            .all(|member| member.1 == GoalSign::Negative)
    );
}

/// `bnot` flips the sign both ways, and the De Morgan shape falls out of
/// recursion, never rewriting: `+bnot(bor(A, B))` records `-bor`, `-A`,
/// `-B`; `-bnot(bor(A, B))` flips to the disjunctive sign and stops after
/// the child.
#[test]
fn bnot_flips_recursively_without_rewriting() {
    let source = br#"const table: Array<u8, 8> =[0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8];

fn guard(index: u64) -> result: u8 pure {
  let low = index < 4_u64;
  let high = index >= 8_u64;
  let outside = bor(low, high);
  let inside = bnot(outside);
  if inside {
    return table[index];
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "guard");
    // The recursion reaches `-index < 4`, which discharges the subscript.
    assert_eq!(summary.obligations.len(), 1);
    assert!(summary.obligations[0].discharged);
    // Both edges record both parents: the named `inside` binding and its
    // expanded `bnot` tree.
    assert_eq!(summary.boolean_decompositions.len(), 4);
    let named_positive = binding_rooted_entry(&summary, GoalSign::Positive);
    assert_eq!(named_positive.members.len(), 3);
    assert_binding_members(&summary, named_positive, GoalSign::Negative);
    let named_negative = binding_rooted_entry(&summary, GoalSign::Negative);
    assert_eq!(named_negative.members.len(), 1);
    assert_binding_members(&summary, named_negative, GoalSign::Positive);
    let positive = entry_with_root(&summary, CheckedBooleanOperation::Not, GoalSign::Positive);
    assert_eq!(positive.members.len(), 3);
    let inner = &summary.inventory.goals[positive.members[0].0.0 as usize];
    assert_eq!(positive.members[0].1, GoalSign::Negative);
    assert!(matches!(
        &inner.expression,
        GoalExpression::Operation {
            row: GoalOperation::Boolean(CheckedBooleanOperation::Or),
            ..
        }
    ));
    assert_comparison_member(&summary, positive.members[1], GoalSign::Negative, 4, -1);
    assert_eq!(positive.members[2].1, GoalSign::Negative);
    // The false edge flips to +bor, which is disjunctive: one member only.
    let negative = entry_with_root(&summary, CheckedBooleanOperation::Not, GoalSign::Negative);
    assert_eq!(negative.members.len(), 1);
    assert_eq!(negative.members[0].1, GoalSign::Positive);
    assert_eq!(negative.members[0].0, positive.members[0].0);
}

/// S4 establishes a `band` requirement's conjuncts at body entry. At the
/// caller, the same true `band` guard proves the complete requirement.
#[test]
fn band_requirement_and_guard_share_the_same_conjuncts() {
    // The subscripted storage is a const `Array<u8, 8>`. Its `len` is the
    // type constant N [MSR-1, WIN-1], a standing fact of the place, which is
    // what lets the two conjuncts stay the literal bounds the members below
    // pin; a window parameter would carry `len` as a runtime number its block
    // stores and the conjuncts would have to name it.
    let source = br#"const table: Array<u8, 8> =[0_u8, 1_u8, 2_u8, 3_u8, 4_u8, 5_u8, 6_u8, 7_u8];

fn pick(low: u64, high: u64) -> result: u8 pure contract {
  define low_ok = low < 8_u64;
  define high_ok = high < 8_u64;
  define both = band(low_ok, high_ok);
  requires both;
} {
  let first = table[low];
  return first;
}

fn caller(low: u64, high: u64) -> result: u8 pure {
  let low_ok = low < 8_u64;
  let high_ok = high < 8_u64;
  let both = band(low_ok, high_ok);
  if both {
    return pick(low: low, high: high);
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let callee = entailment(source, "pick");
    assert_eq!(callee.obligations.len(), 1);
    assert!(callee.obligations[0].discharged);
    let entry = entry_with_root(&callee, CheckedBooleanOperation::And, GoalSign::Positive);
    assert_eq!(entry.members.len(), 2);
    assert_comparison_member(&callee, entry.members[0], GoalSign::Positive, 8, -1);
    assert_comparison_member(&callee, entry.members[1], GoalSign::Positive, 8, -1);
    // The caller discharges the exact conjunction from the same two guard
    // conjuncts.
    let caller = entailment(source, "caller");
    assert_eq!(caller.call_goals.len(), 1);
    assert!(
        matches!(
            caller.call_goals[0].disposition,
            super::super::entailment::CallGoalDisposition::Discharged
        ),
        "whole-tree caller evidence must discharge: {:?}",
        caller.call_goals[0]
    );
}

/// The band/derived-index discharge asymmetry, pinned. A conjunct that bounds
/// a let-bound derived value keeps the relation its own comparison binding
/// recorded, so the conjoined guard discharges exactly what the equivalent
/// pair of nested single-bound guards discharges. Before the members were read
/// through their bindings, the expanded conjunct read
/// `at +wrap 1 < deref(input).len`, whose arithmetic root has no term form,
/// and the second subscript's obligation survived while the first discharged.
///
/// The runtime length comes from a range reference [REF-4], which is what
/// v0.59's `Slice<u8>` parameter became: its one measure is `len`, read as
/// the place form `deref(input).len` [OP-15, MSR-1].
///
/// Both halves are branch guards. The guard proves the decomposition directly:
/// without the binding-read the conjoined half fails `[OP-4]` on
/// `next < deref(input).len` while the nested half still discharges.
#[test]
fn band_conjunct_over_a_derived_binding_discharges_like_the_single_bound_pair() {
    let conjoined = br#"fn read_pair(input: &[u8], at: u64) -> result: u8 reads(input) {
  let next = at +wrap 1_u64;
  let spare = deref(input).len;
  let at_ok = at < spare;
  let next_ok = next < spare;
  let both = band(at_ok, next_ok);
  if both {
    let first = deref(input)[at];
    let second = deref(input)[next];
    return first +wrap second;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let separate = br#"fn read_pair(input: &[u8], at: u64) -> result: u8 reads(input) {
  let next = at +wrap 1_u64;
  let spare = deref(input).len;
  let at_ok = at < spare;
  let next_ok = next < spare;
  if at_ok {
    if next_ok {
      let first = deref(input)[at];
      let second = deref(input)[next];
      return first +wrap second;
    }
    return 0_u8;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for source in [conjoined.as_slice(), separate.as_slice()] {
        let summary = entailment(source, "read_pair");
        assert_eq!(summary.obligations.len(), 2);
        assert!(
            summary.obligations.iter().all(|o| o.discharged),
            "both subscripts must discharge: {:?}",
            summary.obligations
        );
    }
    // The conjunct that bounds the derived value is carried as its own
    // comparison binding, not as an expanded arithmetic comparison.
    let summary = entailment(conjoined, "read_pair");
    let named = binding_rooted_entry(&summary, GoalSign::Positive);
    assert_eq!(named.members.len(), 2);
    assert_binding_members(&summary, named, GoalSign::Positive);
}

/// `if band(..)` admits both subscripts on the true edge and neither on the
/// false edge, because `-band` carries only disjunctive content.
#[test]
fn band_guard_over_a_derived_binding_admits_the_true_edge_only() {
    let source = br#"fn window(input: &[u8], at: u64) -> result: u8 reads(input) {
  let next = at +wrap 1_u64;
  let spare = deref(input).len;
  let at_ok = at < spare;
  let next_ok = next < spare;
  let both = band(at_ok, next_ok);
  if both {
    let first = deref(input)[at];
    let second = deref(input)[next];
    return first +wrap second;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "window");
    assert_eq!(summary.obligations.len(), 2);
    assert!(summary.obligations.iter().all(|o| o.discharged));
    let else_edge = br#"fn window(input: &[u8], at: u64) -> result: u8 reads(input) {
  let next = at +wrap 1_u64;
  let spare = deref(input).len;
  let at_ok = at < spare;
  let next_ok = next < spare;
  let both = band(at_ok, next_ok);
  if both {
    return 0_u8;
  } else {
    return deref(input)[next];
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(else_edge, "window");
    assert_eq!(summary.obligations.len(), 1);
    assert!(
        !summary.obligations[0].discharged,
        "a false `band` bounds neither conjunct: {:?}",
        summary.obligations[0]
    );
}

/// The negative twin the widening must keep failing: reading the conjuncts
/// through their bindings proves exactly the two bounds the band names and no
/// third one, so a subscript by an index the band never bounded still carries
/// its obligation, and a disjunction still bounds neither side. Guards for the
/// same reason as the positive case above.
#[test]
fn band_over_derived_bindings_proves_no_unnamed_bound() {
    let uncovered = br#"fn read_three(input: &[u8], at: u64) -> result: u8 reads(input) {
  let next = at +wrap 1_u64;
  let far = at +wrap 2_u64;
  let spare = deref(input).len;
  let at_ok = at < spare;
  let next_ok = next < spare;
  let both = band(at_ok, next_ok);
  if both {
    let first = deref(input)[at];
    let third = deref(input)[far];
    return first +wrap third;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(uncovered, "read_three");
    assert_eq!(summary.obligations.len(), 2);
    assert_eq!(
        summary.obligations.iter().filter(|o| o.discharged).count(),
        1,
        "only the named bound discharges: {:?}",
        summary.obligations
    );
    let disjoined = br#"fn read_pair(input: &[u8], at: u64) -> result: u8 reads(input) {
  let next = at +wrap 1_u64;
  let spare = deref(input).len;
  let at_ok = at < spare;
  let next_ok = next < spare;
  let either = bor(at_ok, next_ok);
  if either {
    return deref(input)[next];
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(disjoined, "read_pair");
    assert_eq!(summary.obligations.len(), 1);
    assert!(
        !summary.obligations[0].discharged,
        "a disjunction bounds neither side: {:?}",
        summary.obligations[0]
    );
}
