//! [WIN-3] owned places: there is no take operation and no hole.
//!
//! The v0.59 subject of this module was the `box` referent read-out, the
//! multi-target commit `set (a, b) = ...;`, and holder suspension. All three
//! are retired in v0.60, and so are the `replace` statement and the inline
//! view the module's remaining cases rested on. Each retirement has a named
//! successor:
//!
//! - [LIV-2]'s `box` referent read-out retires. The successor is [WIN-3] with
//!   [TYPE-9]: a `Box`'s content is its field `inner`, `let n = move b.inner;`
//!   consumes the whole cell rather than reading one referent out of a live
//!   owner, the owner's other affine parts take their compiler-derived release
//!   [STOR-3], and a remaining linear part is the partial-consume refusal
//!   exercised in `a_partial_consume_that_abandons_a_linear_part_is_refused`
//!   below.
//! - [LIV-2]'s multi-target commit retires with [GRAM-4]'s `set_stmt`, which
//!   writes exactly one place. The successor spelling is one `set` per place,
//!   and `swap(first: &a, second: &b)` [OP-11] for a two-place exchange.
//! - [OWN-5]'s loan-conflict matrix and holder suspension retire with the
//!   loans. The successor is [REF-2] reference validity together with
//!   [EFF-5]'s clause 3, whose coverage is `tests/references.rs`.
//! - [SET-2]'s `replace` statement and its affine element exchange retire. The
//!   successor is [WIN-3]'s commit disposition: assigning over an owned place
//!   releases an affine old value and is a hard error when the old value is
//!   linear, exercised in `assigning_over_a_linear_owned_place_is_refused`
//!   below.
//! - [VIEW-2] retires with the views; the successor is [REF-4]'s range
//!   reference `&v[lo..hi]`, whose coverage is `tests/references.rs`.
//! - [STOR-4] retires with regions and arenas; there is no successor and no
//!   replacement spelling.
//! - The `BoxReferentMove` and `BorrowedBufferDescriptorMutation` capability
//!   stops retire with their subjects: an affine referent moved out of a live
//!   owning indirection, and rebinding a borrowed buffer descriptor, are both
//!   v0.59 mechanisms v0.60 does not have.
//!
//! What survives is the part of this module that was never about the read-out:
//! [OWN-1]'s whole-binding kill, which no index separation keeps alive, and
//! the terminal type a field step selects from an element. Both are retargeted
//! below onto the already-ported v0.60 conformance cases and v0.60 spellings.

// One-line record of each retired test of this module, naming the rule it
// exercised and that rule's successor:
//
// Retired with [LIV-2]: `box_referent_read_out_spends_the_selected_storage_once` had the referent read-out and the multi-target commit as its whole subject; the successor is [WIN-3]'s whole-owner consume `let n = move b.inner;` and [GRAM-4]'s one place per `set_stmt`.
// Retired with [LIV-2]: `box_referent_target_does_not_turn_an_owner_move_into_a_read_out` distinguished a read-out from an owner move; v0.60 has no read-out to distinguish, the successor being [WIN-3]'s single consume rule.
// Retired with [OWN-5]: `box_referent_read_out_preserves_shared_and_non_target_boundaries` asserted the loan-conflict matrix and the `BoxReferentMove` capability stop; the successor is [REF-2] validity plus [EFF-5] clause 3.
// Retired with [OWN-5]: `box_referent_read_out_preserves_holder_suspension` had holder suspension as its subject; the successor is [REF-2] validity plus [EFF-5] clause 3.
// Retired with [OWN-5]: `box_referent_descendant_extraction_keeps_its_cleanup_capability_boundary` asserted the `BoxReferentMove` capability stop over a descendant read-out; the successor is [WIN-3]'s whole-owner consume with [PROV-6]'s partial-consume refusal.
// Retired with [LIV-2]: `box_referent_read_out_rejects_a_later_reborrow_at_its_use` asserted a reborrow at a spent read-out target; the successor is [REF-2]'s use of an invalidated reference.
// Retired with [LIV-2]: `direct_field_index_reads_and_scalar_commits_use_the_terminal_type` paired the multi-target commit with an [OP-4] bound; the successor is [GRAM-4]'s one place per `set_stmt`, and [OP-4]'s subscript obligation keeps its own coverage in `tests/arrays.rs`.
// Retired with [LIV-2]: `proved_dynamic_indices_separate_one_commit_targets` separated two targets of one commit; the successor is [GRAM-4]'s one place per `set_stmt` and `swap` [OP-11] for a two-place exchange, with [OWN-7]'s index-step separation now answered at a call by [EFF-5].
// Retired with [OWN-5]: `a_rhs_cannot_mutate_an_index_captured_by_dynamic_commit_targets` asserted a loan conflict between a commit's captured indices and its right-hand side; the successor is [SET-1]'s target evaluation before the right-hand side, with [REF-2] deciding the reference half.
// Retired with [LIV-2]: `repeated_affine_element_read_out_keeps_type2_diagnostic_priority` ordered two diagnostics of one multi-target commit; the successor is [WIN-3], which owns the element move-out on its own.
// Retired with [SET-2]: `affine_nested_fields_are_exchanged_or_read_out_once` had `replace` and the multi-target commit as its subject; the successor is [WIN-3]'s commit disposition.
// Retired with [VIEW-2]: `exclusive_inline_view_retains_bounds_and_owner_loan` had the exclusive inline view and its owner loan as its subject; the successor is [REF-4]'s range reference with [REF-2] validity.
// Retired with [SET-2]: `borrowed_buffer_descriptor_replacement_is_an_explicit_capability_stop` asserted the `BorrowedBufferDescriptorMutation` stop over a `replace` through a borrow; the successor is [WIN-3]'s commit disposition, there being no borrowed descriptor to rebind.
// Retired with [SET-2]: `borrowed_aggregate_buffer_replacement_cannot_bypass_the_capability_stop` asserted the same stop through an aggregate; the successor is the same [WIN-3] commit disposition.

use crate::{SemanticIssueKind, SemanticRule};

use super::{assert_accepts, assert_rule_at, assert_rule_kind};

/// [OWN-1] one consuming use kills the whole binding that rooted the place, so
/// a proved-distinct candidate index pair does not keep a later subscript
/// live. This is the v0.60 form of the cross-path case: the pair is separated
/// by [OWN-7] and the second read is still rooted in a dead binding.
#[test]
fn a_consuming_use_kills_the_whole_binding_a_separated_index_pair_included() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/own1-neg-an-unproved-candidate-index-pair-does-not-separate-a-cross-path.wf"
    );
    assert_rule_kind(source, SemanticRule::Own1, |kind| {
        matches!(kind, SemanticIssueKind::UseAfterMove { .. })
    });
}

/// [TYPE-5] a field step selects a declared field of its base's terminal type.
/// A subscript selects the element type, so a field written on a scalar
/// element selects nothing, in read position and at a `set` target alike.
///
/// The v0.59 source built its storage with `buffer_new(1_u64, 0_u8)`; the
/// v0.60 spelling is `array_filled::<u8, n>(value: v)` [OP-13], whose result
/// is the constant-capacity `Array<u8, n>` [TYPE-9].
#[test]
fn a_field_step_on_a_scalar_element_selects_no_declared_field() {
    for body in [
        "  let value = values[0_u64].missing;\n",
        "  set values[0_u64].missing = 1_u8;\n",
    ] {
        let source = format!(
            "fn main() -> status: std::process::ExitStatus pure {{\n  let values = array_filled::<u8, 2>(value: 0_u8);\n{body}  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        assert_rule_kind(source.as_bytes(), SemanticRule::Type5, |kind| {
            matches!(kind, SemanticIssueKind::TypeMismatch { .. })
        });
    }
}

/// Runtime-capacity Array element suffixes are ordinary typed place steps.
/// Resolving the complete suffix preserves TYPE-5 priority over the compiler's
/// internal flat-buffer representation.
#[test]
fn runtime_array_element_suffixes_retain_source_diagnostics() {
    let invalid_field = br#"struct Row {
  left: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let seed = Row(left: 3_u64);
  let rows = box_array_filled::<Row>(count: 1_u64, value: seed);
  let bad = rows.inner[0_u64].missing;
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(invalid_field, SemanticRule::Type5, |kind| {
        matches!(kind, SemanticIssueKind::TypeMismatch { .. })
    });

    let unproved_bound = br#"struct Row {
  left: u64;
}

fn read(rows: &Box<Array<Row>>, index: u64) -> result: u64 reads(rows) {
  return deref(rows).inner[index].left;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(unproved_bound, SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

/// [WIN-3] assigning over any owned place releases the old value when it is
/// affine and is a hard error at the target `place` when it is linear: a
/// linear value has no release, so the writer takes it out and consumes it
/// first.
///
/// This is the successor of the `replace` cases above. v0.59 stated the demand
/// over the *new* value and the target's region-freedom; v0.60 states it over
/// the old value's linearity alone.
#[test]
fn assigning_over_a_linear_owned_place_is_refused() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/win3-neg-overwrite-linear-place.wf");
    assert_rule_kind(source, SemanticRule::Win3, |kind| {
        matches!(kind, SemanticIssueKind::LinearAssignmentTarget { .. })
    });
}

/// [WIN-3, PROV-6] a move out of a field consumes the whole owner, so a
/// consume of a proper sub-place of a value one of whose remaining parts is
/// linear abandons a residual no compiler-derived release reclaims. The
/// refusal cites PROV-6 at the consumed `place` and its restructuring is the
/// destructuring consume of the whole value.
///
/// This is the successor of the referent read-out cases: v0.59 let an affine
/// part leave a live owner through the read-out, and v0.60 has one consume
/// rule for a field and for `Box` content alike.
#[test]
fn a_partial_consume_that_abandons_a_linear_part_is_refused() {
    let source = br#"nodrop struct Token {
  value: u64;
}

struct Pair {
  kept: Token;
  spare: Token;
}

fn split(pair: Pair) -> result: Token pure {
  return move pair.kept;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Prov6, |kind| {
        matches!(kind, SemanticIssueKind::LinearValuePartiallyConsumed { .. })
    });
}

/// [PROV-6, WIN-3] the selected field leaves with its caller. Its linearity
/// therefore imposes no drop obligation on an empty or affine residual.
#[test]
fn a_field_consume_judges_only_its_unselected_residual() {
    for fields in [
        "  value: T;\n",
        "  before: Box<u64>;\n  value: T;\n  after: Box<u64>;\n",
    ] {
        let source = format!(
            "struct Holder<T> {{\n{fields}}}\n\nfn take<T>(holder: Holder<T>) -> value: T pure {{\n  return move holder.value;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        assert_accepts(source.as_bytes());
    }
}

/// The residual inventory must retain an opaque generic member and a
/// fieldless nodrop nominal even though neither has a runtime drop action.
#[test]
fn a_field_consume_cannot_hide_a_linear_residual_without_a_drop_action() {
    for (declarations, generic, residual) in [
        ("", "<T>", "T"),
        ("nodrop enum Token {\n  Live();\n}\n\n", "", "Token"),
        ("nodrop struct Token {\n}\n\n", "", "Token"),
    ] {
        let source = format!(
            "{declarations}struct Holder{generic} {{\n  value: Box<u64>;\n  residual: {residual};\n}}\n\nfn take{generic}(holder: Holder{generic}) -> value: Box<u64> pure {{\n  return move holder.value;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        assert_rule_kind(source.as_bytes(), SemanticRule::Prov6, |kind| {
            matches!(kind, SemanticIssueKind::LinearValuePartiallyConsumed { .. })
        });
    }
}

/// [PROV-6, DIAG-1] the complete consumed `place` excludes its `move`
/// wrapper, even when the checked expression retains that wrapper's carrier.
#[test]
fn boxed_field_consumes_judge_linear_residuals_before_empty_releases() {
    for (declaration, residual) in [
        ("nodrop enum Token {\n  Live();\n}\n\n", "Token"),
        ("nodrop struct Token {\n}\n\n", "Token"),
        ("nodrop struct Token {\n  value: u64;\n}\n\n", "Token"),
    ] {
        let source = format!(
            "{declaration}struct Holder {{\n  value: Box<u64>;\n  residual: {residual};\n}}\n\nfn take(holder: Box<Holder>) -> value: Box<u64> pure {{\n  return move holder.inner.value;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        assert_rule_at(source.as_bytes(), SemanticRule::Prov6, "holder.inner.value");
        assert_rule_kind(source.as_bytes(), SemanticRule::Prov6, |kind| {
            matches!(
                kind,
                SemanticIssueKind::LinearValuePartiallyConsumed {
                    mechanical_fix: "destructure the whole value with let N(f: a, ...) = move v;",
                    ..
                }
            )
        });
    }
}

/// [WIN-3, DIAG-1] the refused element move cites its `place` operand and
/// offers the boundary operations that preserve initialized storage.
#[test]
fn moving_an_array_or_window_element_still_reports_a_hole() {
    for (ty, requires) in [
        ("Array<Box<u64>, 1>", ""),
        (
            "Slots<Box<u64>, 1>",
            " contract {\n  requires values.len > 0_u64;\n}",
        ),
    ] {
        let source = format!(
            "fn take(values: {ty}) -> value: Box<u64> pure{requires} {{\n  return move values[0_u64];\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        assert_rule_at(source.as_bytes(), SemanticRule::Win3, "values[0_u64]");
        assert_rule_kind(source.as_bytes(), SemanticRule::Win3, |kind| {
            matches!(
                kind,
                SemanticIssueKind::MoveOutOfSlot {
                    mechanical_fix: "use take_back, remove_at, or swap [OP-10, OP-11]",
                }
            )
        });
    }
}
