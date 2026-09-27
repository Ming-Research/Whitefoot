//! [REF-4] range references, and [CALL-3]'s transport of a write through one.
//!
//! This module replaces `tests/slices.rs` (30 tests), whose subject was
//! v0.59's slice and view values. Every rule it exercised is retired, and each
//! retirement has a named successor:
//!
//! - [VIEW-1] view formation and the `slice_of` / `mut_slice_of` formers
//!   retire. The successor is [REF-4]'s `&x[lo..hi]`, a place form under the
//!   obligation `lo <= hi` and `hi <= x.len`, and re-slicing
//!   `&deref(part)[a..b]`.
//! - [VIEW-2] range separation retires as a judgment of its own. The successor
//!   is [OWN-7]'s range-step relation, whose four non-strict orderings are
//!   submitted by [EFF-5] at a call and reported as
//!   `UndischargedCallSeparation` citing EFF-5.
//! - [VIEW-4] the slice type and [VIEW-6] the slice return ceiling retire.
//!   `Slice<T>` and `MutSlice<T>` are not types at all: `&[T]` is a reference
//!   KIND admitted only in parameter position [TYPE-8, GRAM-2], is never a
//!   stored value, never a result and never a generic type argument, and a
//!   returned reference is refused by [REF-3].
//! - The exclusive/shared distinction over a view retires with the permission
//!   markers: there is one reference kind, and whether a callee writes through
//!   it is stated by its effect row [EFF-1].
//! - Region-scoped loan lifetimes retire with regions [OWN-3, OWN-4, OWN-10,
//!   FORM-8]; the successor is [REF-2] validity, exercised in
//!   `tests/references.rs`.
//!
//! The [OP-4] cases slices.rs carried — the data-dependent scatter and the
//! scalar count bound — keep their live subject rule and belong with the other
//! subscript tests, not with the retired view apparatus.

use crate::{CallRequirementDisposition, SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::{assert_accepts, assert_rule_kind, with_semantics};

/// Asserts that one source is refused by [FN-8] with the exact disposition and
/// the exact instantiated goal text the call owes.
fn assert_call_goal(source: &[u8], disposition: CallRequirementDisposition, goal: &str) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the call requirement must be refused: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn8);
        let SemanticIssueKind::UndischargedCallRequirement(detail) = issue.kind() else {
            panic!("FN-8 must identify the failing requirement: {issue:?}");
        };
        assert_eq!(detail.instantiated_goal, goal);
        assert_eq!(detail.disposition, disposition);
    });
}

/// [REF-4] `&x[lo..hi]` over an indexable place, re-slicing over another range
/// reference, and the kind's one measure `len` equal to `hi - lo`.
#[test]
fn a_range_reference_forms_and_reslices() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/ref4-pos-range-reference-and-reslice.wf"
    ));
}

/// [REF-4] a range reference over a `Ring` is a hard error: a wrapped window is
/// two extents and `&[T]` has one `len`.
#[test]
fn a_range_reference_over_a_ring_is_refused() {
    let source = include_bytes!("../../../../tests/conformance/cases/ref4-neg-range-over-ring.wf");
    assert_rule_kind(source, SemanticRule::Ref4, |kind| {
        matches!(kind, SemanticIssueKind::RangeOverRing { .. })
    });
}

/// [ENT-2, OP-15] a measure place may carry an ordinary subscript projection,
/// including one through a range reference. [OP-4] discharges the element
/// selection before the measure is read. The nested case keeps the real
/// `Box.inner` projection between the selected range element and the `Slots`
/// descriptor; borrowing the element into a separate reference must not be
/// required just to name either measure.
#[test]
fn a_range_element_measure_is_an_ordinary_subscripted_measure_place() {
    let source = br#"fn direct(items: &[Slots<u64, 2>]) -> length: u64 reads(items) contract {
  requires 0_u64 < deref(items).len;
} {
  return deref(items)[0_u64].len;
}

fn nested(items: &[Box<Slots<u64, 2>>]) -> length: u64 reads(items) contract {
  requires 0_u64 < deref(items).len;
} {
  return deref(items)[0_u64].inner.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// [REF-4, OP-4, SET-1] a range may contain composite elements. Every
/// subscript below the selected outer element is an ordinary typed place step:
/// its offset is evaluated in source order, its own bound is discharged, and
/// read, write and borrow all name the same final scalar storage.
#[test]
fn nested_range_element_subscripts_are_complete_places() {
    let source = br#"fn exercise(rows: &[Array<u64, 2>], outer: u64, inner: u64) -> result: u64 writes(rows) contract {
  requires outer < deref(rows).len;
  requires inner < 2_u64;
} {
  let before = deref(rows)[outer][inner];
  let changed = before +wrap 1_u64;
  set deref(rows)[outer][inner] = changed;
  let selected = &deref(rows)[outer][inner];
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// Both positions of a nested range element path owe their own [OP-4]
/// judgment. The outer failure is reported before the inner position is
/// considered, following the source's base-outward evaluation order.
#[test]
fn an_out_of_bounds_outer_nested_range_index_is_an_op4_rejection() {
    let source = br#"fn invalid(rows: &[Array<u64, 2>]) -> result: u64 reads(rows) contract {
  requires deref(rows).len == 1_u64;
} {
  return deref(rows)[1_u64][0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

/// Discharging the outer range position does not authorize a nested Array
/// position. The inner suffix keeps its own base type and obligation.
#[test]
fn an_out_of_bounds_inner_nested_range_index_is_an_op4_rejection() {
    let source = br#"fn invalid(rows: &[Array<u64, 2>]) -> result: u64 reads(rows) contract {
  requires 0_u64 < deref(rows).len;
} {
  return deref(rows)[0_u64][2_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

/// [REF-1, ENT-2, ENT-3.S1] a nested path may extend a joined range after
/// the selected holder's own length has been proved sufficient. The branch
/// supplies that fact without equating either input's measure to the joined
/// holder or reducing the holder to one possible origin.
#[test]
fn a_nested_range_element_path_preserves_joined_origins() {
    let source = br#"fn inspect(left: &[Array<u64, 2>], right: &[Array<u64, 2>], flag: Bool) -> result: u64 reads(left), reads(right) {
  let rows = if flag {
    give left;
  } else {
    give right;
  }
  if deref(rows).len > 0_u64 {
    return deref(rows)[0_u64][1_u64];
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// [ENT-2, ENT-3, ENT-6] the two incoming range measures and the joined
/// holder's measure are distinct terms. The fixed fact sources have no
/// reference-valued delivery rule that transports the former bounds to the
/// latter. Preserve this source as an unproved OP-4 control: mathematical
/// safety alone must not authorize an extra alias-based proof route.
#[test]
fn incoming_range_bounds_do_not_invent_a_joined_holder_length_fact() {
    let source = br#"fn inspect(left: &[Array<u64, 2>], right: &[Array<u64, 2>], flag: Bool) -> result: u64 reads(left), reads(right) contract {
  requires 0_u64 < deref(left).len;
  requires 0_u64 < deref(right).len;
} {
  let rows = if flag {
    give left;
  } else {
    give right;
  }
  return deref(rows)[0_u64][1_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

/// [REF-2] replacing the outer composite element writes a proper prefix of a
/// reference to one of its nested scalars. Keeping only the outer range index
/// or dropping the suffix would incorrectly leave this reference valid.
#[test]
fn replacing_a_range_element_invalidates_a_nested_element_reference() {
    let source = br#"fn invalid(rows: &[Array<u64, 2>]) -> result: u64 writes(rows) contract {
  requires 0_u64 < deref(rows).len;
} {
  let selected = &deref(rows)[0_u64][0_u64];
  let replacement = array_filled::<u64, 2>(value: 9_u64);
  set deref(rows)[0_u64] = replacement;
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Ref2, |_| true);
}

/// The subscript inside a measure place owes the same [OP-4] bound as every
/// other subscript. A one-element range cannot admit element one, even though
/// the selected element's `len` would itself be a total [OP-15] read.
#[test]
fn an_out_of_bounds_range_element_measure_is_an_op4_rejection() {
    let source = br#"fn invalid(items: &[Slots<u64, 2>]) -> length: u64 reads(items) contract {
  requires deref(items).len == 1_u64;
} {
  return deref(items)[1_u64].len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

/// [REF-1, MSR-2] a measure through a joined range holder reads the element
/// selected at run time. Its own guard is sufficient for a downstream use on
/// either incoming path. Without that guard, a fact about the first possible
/// referent must not hide the second referent's contradictory length.
#[test]
fn a_joined_range_element_measure_checks_every_possible_target() {
    let source = |check: &str| {
        format!(
            r#"fn needs_one(value: u64) -> result: unit pure contract {{
  requires value == 1_u64;
}} {{
  return unit;
}}

fn examine(flag: Bool) -> result: unit pure {{
  let left_row = slots_new::<u64, 2>();
  place_back(window: &left_row, value: 11_u64);
  let right_row = slots_new::<u64, 2>();
  let left = slots_new::<Slots<u64, 2>, 1>();
  place_back(window: &left, value: move left_row);
  let right = slots_new::<Slots<u64, 2>, 1>();
  place_back(window: &right, value: move right_row);
  let items = if flag {{
    give &left[0_u64..1_u64];
  }} else {{
    give &right[0_u64..1_u64];
  }}
{check}
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#,
        )
    };
    let guarded = source(
        "  if 0_u64 < deref(items).len {\n    let observed = deref(items)[0_u64].len;\n    if observed == 1_u64 {\n      let answer = needs_one(value: deref(items)[0_u64].len);\n    }\n  }",
    );
    assert_accepts(guarded.as_bytes());
    let conflicting = source(
        "  if 0_u64 < deref(items).len {\n    if left[0_u64].len == 1_u64 {\n      if right[0_u64].len == 0_u64 {\n        let answer = needs_one(value: deref(items)[0_u64].len);\n      }\n    }\n  }",
    );
    assert_rule_kind(conflicting.as_bytes(), SemanticRule::Fn8, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallRequirement(_))
    });
}

/// [MSR-2, ENT-5] replacing a measured range element through a second alias
/// kills the guarded fact about that element's old descriptor. The range
/// itself remains a valid view of the outer window, but its newly empty inner
/// window cannot use the stale length to discharge a downstream requirement
/// [FN-8].
#[test]
fn a_range_element_measure_dies_on_a_write_through_an_alias() {
    let source = br#"fn needs_one(value: u64) -> result: unit pure contract {
  requires value == 1_u64;
} {
  return unit;
}

fn clear(window: &Slots<u64, 2>) -> result: unit writes(window) {
  let empty = slots_new::<u64, 2>();
  set deref(window) = move empty;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let row = slots_new::<u64, 2>();
  place_back(window: &row, value: 17_u64);
  let outer = slots_new::<Slots<u64, 2>, 1>();
  place_back(window: &outer, value: move row);
  let items = &outer[0_u64..1_u64];
  if deref(items)[0_u64].len == 1_u64 {
    let aliased = &deref(items)[0_u64];
    let cleared = clear(window: aliased);
    let invalid = needs_one(value: deref(items)[0_u64].len);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Fn8, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallRequirement(_))
    });
}

/// [REF-4] formation submits `lo <= hi` and `hi <= x.len` to [MSR-4]. An
/// endpoint above the base's length discharges neither, and the rejection
/// carries the residual and the rule's own restructuring.
#[test]
fn an_endpoint_above_the_length_leaves_the_formation_undischarged() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u64, 4>(value: 0_u64);
  let hi = 9_u64;
  let part = &a[0_u64..hi];
  let seen = deref(part).len;
  if seen == 9_u64 {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Ref4, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedRangeFormationObligation { .. }
        )
    });
}

/// [CALL-3] a write through a range reference reaches the range's element
/// storage and no measure of the origin place itself, so after the call the
/// caller still holds both lengths and the bytes the callee wrote.
#[test]
fn a_write_through_a_range_reference_keeps_both_lengths() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/call3-pos-a-fill-through-an-exclusive-view-keeps-both-lengths.wf"
    ));
}

/// Asserts the [OP-4] rejection of the `cells[c]` read whose bound a killed
/// element-measure fact would otherwise have supplied.
///
/// The conformance verdict names only the rule. These fixtures also subscript
/// the origin in their guard and loop endpoint, so an [OP-4] rejection there,
/// from a write that wrongly killed the origin's own length, would satisfy
/// the rule alone; the residual pins the rejection to the stale element fact.
fn assert_cells_read_unproved(source: &[u8]) {
    assert_rule_kind(source, SemanticRule::Op4, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedBoundsObligation { residual, .. }
                if residual == "c < cells.len"
        )
    });
}

/// [CALL-3, EFF-5] a range formed at the call, `&rows[1..3]` or the re-slice
/// `&deref(view)[1..3]`, names its source's path extended by the formation's
/// own range step, the path a bound range reference names. Its projected write
/// therefore kills the guarded measure of an element the range may contain,
/// and the rejection lands on the read that measure bounded.
#[test]
fn a_write_through_an_inline_range_kills_the_element_measure_it_may_reach() {
    assert_cells_read_unproved(include_bytes!(
        "../../../../tests/conformance/cases/call3-neg-inline-range-write-kills-element-measure.wf"
    ));
    assert_cells_read_unproved(include_bytes!(
        "../../../../tests/conformance/cases/call3-neg-inline-reslice-write-kills-element-measure.wf"
    ));
}

/// [OWN-7] separates an index step from a range step once the index is
/// proved outside the range, here by written literals: element 1 lies before
/// the range `2..3`. A write through that range therefore keeps the guarded
/// measure of `rows[1]`, and the bound and inline spellings of the range reach
/// the same verdict; a range that contains the element still kills it.
#[test]
fn an_index_outside_a_written_range_is_separated_from_it() {
    let body = |call: &str| {
        format!(
            "fn refill(part: &[Slots<u64, 8>], at: u64) -> result: unit writes(part) contract {{
  requires at < deref(part).len;
}} {{
  let fresh = slots_new::<u64, 8>();
  set deref(part)[at] = move fresh;
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let rows = slots_new::<Slots<u64, 8>, 4>();
  let first = slots_new::<u64, 8>();
  let second = slots_new::<u64, 8>();
  let third = slots_new::<u64, 8>();
  place_back(window: &rows, value: move first);
  place_back(window: &rows, value: move second);
  place_back(window: &rows, value: move third);
  let cells = array_filled::<u64, 3>(value: 7_u64);
  let total = 0_u64;
  if rows.len == 3_u64 {{
    if rows[1_u64].len <= cells.len {{
{call}      for (c in 0_u64..rows[1_u64].len) {{
        let cell = cells[c];
        set total = total +wrap cell;
      }}
    }}
  }}
  return std::process::exit_status(code: 0_u8);
}}
"
        )
    };
    assert_accepts(body("      refill(part: &rows[2_u64..3_u64], at: 0_u64);\n").as_bytes());
    assert_accepts(
        body("      let tail = &rows[2_u64..3_u64];\n      refill(part: tail, at: 0_u64);\n")
            .as_bytes(),
    );
    assert_cells_read_unproved(
        body("      refill(part: &rows[1_u64..3_u64], at: 0_u64);\n").as_bytes(),
    );
    // Control: with no write the guard discharges the read.
    assert_accepts(body("").as_bytes());
}

/// [REF-4, MSR-2] overlapping views still carry distinct immutable range
/// descriptors. A direct element commit and a projected callee element write
/// through the wider view therefore preserve the narrower view's formed
/// length. Whole-origin replacement remains the [REF-2] rejection exercised
/// by `references::a_reslice_of_a_joined_range_is_invalidated_by_either_origin_replacement`.
#[test]
fn overlapping_range_element_writes_preserve_each_formed_length() {
    let source = br#"fn needs_two(part: &[u8]) -> result: unit reads(part) contract {
  requires deref(part).len == 2_u64;
} {
  let observed = deref(part).len;
  return unit;
}

fn write_first(part: &[u8]) -> result: unit writes(part) contract {
  requires 0_u64 < deref(part).len;
} {
  set deref(part)[0_u64] = 9_u8;
  return unit;
}

fn exercise(values: &Slots<u8, 4>) -> result: unit writes(values) contract {
  requires deref(values).len == 3_u64;
} {
  let wider = &deref(values)[0_u64..3_u64];
  let narrower = &deref(values)[1_u64..3_u64];
  set deref(wider)[1_u64] = 7_u8;
  let after_direct = needs_two(part: narrower);
  let written = write_first(part: wider);
  let after_call = needs_two(part: narrower);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

const CONDITIONAL_RANGE_SEPARATION_HELPERS: &str = r#"fn touch(left: &[Slots<u64, 2>], right: &[Slots<u64, 2>]) -> result: unit writes(left), writes(right) contract {
  requires 0_u64 < deref(left).len;
  requires 0_u64 < deref(right).len;
} {
  clear(window: &deref(left)[0_u64]);
  clear(window: &deref(right)[0_u64]);
  return unit;
}

fn clear(window: &Slots<u64, 2>) -> result: unit writes(window) {
  let empty = slots_new::<u64, 2>();
  set deref(window) = move empty;
  return unit;
}
"#;

/// [OWN-7, EFF-5] a range-separation proof is available under the guard that
/// establishes it. This is the positive control for the flow-sensitive
/// non-leak cases below.
#[test]
fn a_range_separation_is_available_in_its_dominating_guard() {
    let source = format!(
        "{CONDITIONAL_RANGE_SEPARATION_HELPERS}
fn inspect(values: &Array<Slots<u64, 2>, 2>, hi: u64, lo: u64) -> result: unit writes(values) contract {{
  requires 1_u64 <= hi;
  requires hi <= 2_u64;
  requires lo <= 1_u64;
}} {{
  let left = &deref(values)[0_u64..hi];
  let right = &deref(values)[lo..2_u64];
  if hi <= lo {{
    touch(left: left, right: right);
  }}
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    assert_accepts(source.as_bytes());
}

/// [OWN-7, ENT-5] a proof established only in one arm is unavailable after
/// the join. Otherwise the write through `right` can retain the stale length
/// of the overlapping slot selected through `left`.
#[test]
fn a_conditional_range_separation_does_not_escape_its_join() {
    let source = format!(
        "{CONDITIONAL_RANGE_SEPARATION_HELPERS}
fn inspect(values: &Array<Slots<u64, 2>, 2>, hi: u64, lo: u64) -> result: u64 writes(values) contract {{
  requires 1_u64 <= hi;
  requires hi <= 2_u64;
  requires lo <= 1_u64;
}} {{
  let left = &deref(values)[0_u64..hi];
  let right = &deref(values)[lo..2_u64];
  if hi <= lo {{
    touch(left: left, right: right);
  }}
  let selected = &deref(left)[0_u64];
  if deref(selected).len == 1_u64 {{
    clear(window: &deref(right)[0_u64]);
    return deref(selected)[0_u64];
  }}
  return 0_u64;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op4, |_| true);
}

/// The sibling arm has the opposite guard, so a proof recorded while walking
/// the first arm must not affect its overlap judgments.
#[test]
fn a_range_separation_does_not_leak_into_a_sibling_arm() {
    let source = format!(
        "{CONDITIONAL_RANGE_SEPARATION_HELPERS}
fn inspect(values: &Array<Slots<u64, 2>, 2>, hi: u64, lo: u64) -> result: u64 writes(values) contract {{
  requires 1_u64 <= hi;
  requires hi <= 2_u64;
  requires lo <= 1_u64;
}} {{
  let left = &deref(values)[0_u64..hi];
  let right = &deref(values)[lo..2_u64];
  if hi <= lo {{
    touch(left: left, right: right);
  }} else {{
    let selected = &deref(left)[0_u64];
    if deref(selected).len == 1_u64 {{
      clear(window: &deref(right)[0_u64]);
      return deref(selected)[0_u64];
    }}
  }}
  return 0_u64;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op4, |_| true);
}

/// A counted loop has a zero-trip predecessor. A separation proved in its
/// body therefore cannot be available after loop exhaustion.
#[test]
fn a_range_separation_does_not_escape_a_maybe_zero_trip_loop() {
    let source = format!(
        "{CONDITIONAL_RANGE_SEPARATION_HELPERS}
fn inspect(values: &Array<Slots<u64, 2>, 2>, hi: u64, lo: u64, count: u64) -> result: u64 writes(values) contract {{
  requires 1_u64 <= hi;
  requires hi <= 2_u64;
  requires lo <= 1_u64;
}} {{
  let left = &deref(values)[0_u64..hi];
  let right = &deref(values)[lo..2_u64];
  for (i in 0_u64..count) {{
    if hi <= lo {{
      touch(left: left, right: right);
    }}
  }}
  let selected = &deref(left)[0_u64];
  if deref(selected).len == 1_u64 {{
    clear(window: &deref(right)[0_u64]);
    return deref(selected)[0_u64];
  }}
  return 0_u64;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op4, |_| true);
}

/// [REF-4, OP-10] a range reference captures its own immutable descriptor.
/// Growing its backing Slots writes `next` and `len`, but neither write
/// changes the range's formed length or the bound for an element inside it.
#[test]
fn growing_the_backing_window_preserves_a_formed_range_length() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u8, 4>();
  place_back(window: &values, value: 11_u8);
  let part = &values[0_u64..1_u64];
  place_back(window: &values, value: 22_u8);
  let first = deref(part)[0_u64];
  return std::process::exit_status(code: first);
}
"#;
    assert_accepts(source);
}

/// [CALL-1] through a reference the callee only reads, every fact survives.
#[test]
fn a_read_only_reference_argument_keeps_every_fact() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/call1-pos-a-shared-borrow-keeps-every-fact.wf"
    ));
}

/// [OWN-7] two index steps whose offsets the fixed [ENT-6] families prove
/// distinct select two different storages, which is the acceptance half the
/// overlap relation must keep.
#[test]
fn proved_distinct_indices_do_not_overlap() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/own7-pos-distinct-noverlap.wf"
    ));
}

/// One callee whose requirement is stated over the range its parameter names.
///
/// [REF-4]: a range reference's "one measure is `len`, equal to `hi - lo`
/// [MSR-1]". [MSR-1] gives `&[T]` its own row, `len` = "range elements,
/// exact", and [OP-15] spells the read `deref(part).len`. So at every call the
/// requirement is instantiated over the actual range's own length and never
/// over the length of the storage that range was formed from: the range
/// `a[2..4]` of a four-element array has two elements, and an offset judged
/// against `a.len` would be admitted and would read outside the range.
const RANGE_OFFSET_CALLEE: &str = r#"fn at(part: &[u8], offset: u64) -> result: u8 reads(part) contract {
  requires offset < deref(part).len;
} {
  return deref(part)[offset];
}
"#;

/// A range a binding names: the requirement is that binding's own `len`, and
/// the goal names the place the writer wrote, `deref(view)` [REF-1, OP-15].
#[test]
fn a_requirement_over_a_bound_range_is_that_ranges_length() {
    let body = |offset: &str| {
        format!(
            "{RANGE_OFFSET_CALLEE}
fn main() -> status: std::process::ExitStatus pure {{
  let a = array_filled::<u8, 4>(value: 1_u8);
  let view = &a[2_u64..4_u64];
  let x = at(part: view, offset: {offset});
  return std::process::exit_status(code: x);
}}
"
        )
    };
    assert_accepts(body("1_u64").as_bytes());
    assert_call_goal(
        body("3_u64").as_bytes(),
        CallRequirementDisposition::Refuted,
        "3_u64 < deref(view).len",
    );
}

/// A range formed at the call names no binding, so it has no [ENT-2] place of
/// its own. Its goal keeps the formation's range step over the storage path,
/// which is what holds its `len` apart from that storage's own `len`
/// (compiler/checker-facts, pending: "A `&[T]` actual that forms its range at
/// the call keeps that formation's range step over the resolved storage path,
/// as one ordered projection of the goal datum"). The length is the
/// mathematical difference of the two captured endpoints [REF-4], so `a[2..4]`
/// is two elements although `a` is four.
#[test]
fn a_requirement_over_a_range_formed_at_the_call_is_that_ranges_length() {
    let body = |offset: &str| {
        format!(
            "{RANGE_OFFSET_CALLEE}
fn main() -> status: std::process::ExitStatus pure {{
  let a = array_filled::<u8, 4>(value: 1_u8);
  let x = at(part: &a[2_u64..4_u64], offset: {offset});
  return std::process::exit_status(code: x);
}}
"
        )
    };
    assert_accepts(body("1_u64").as_bytes());
    assert_call_goal(
        body("3_u64").as_bytes(),
        CallRequirementDisposition::Refuted,
        "3_u64 < a[2..4].len",
    );
}

/// [REF-4]: "Re-slicing is admitted: `&deref(part)[a..b]` under
/// `a <= b <= deref(part).len`." The inner range's `len` is its own `b - a`,
/// not the outer range's and not the array's.
#[test]
fn a_requirement_over_a_reslice_is_the_inner_ranges_length() {
    let body = |offset: &str| {
        format!(
            "{RANGE_OFFSET_CALLEE}
fn main() -> status: std::process::ExitStatus pure {{
  let a = array_filled::<u8, 4>(value: 1_u8);
  let view = &a[0_u64..4_u64];
  let sub = &deref(view)[2_u64..4_u64];
  let x = at(part: sub, offset: {offset});
  return std::process::exit_status(code: x);
}}
"
        )
    };
    assert_accepts(body("1_u64").as_bytes());
    assert_call_goal(
        body("3_u64").as_bytes(),
        CallRequirementDisposition::Refuted,
        "3_u64 < deref(sub).len",
    );
}

/// A range forwarded through a `&[T]` parameter: [REF-1] makes "passing a bare
/// reference variable where a `&T` or `&[T]` parameter is expected passes that
/// reference", so the inner call's requirement instantiates over the
/// forwarding function's own parameter and is discharged, or refuted, from
/// that function's entry facts alone [ENT-3.S4].
#[test]
fn a_requirement_over_a_forwarded_range_is_the_forwarded_ranges_length() {
    let body = |offset: &str| {
        format!(
            "{RANGE_OFFSET_CALLEE}
fn forward(part: &[u8]) -> result: u8 reads(part) contract {{
  requires deref(part).len == 2_u64;
}} {{
  return at(part: part, offset: {offset});
}}

fn main() -> status: std::process::ExitStatus pure {{
  let a = array_filled::<u8, 4>(value: 1_u8);
  let view = &a[2_u64..4_u64];
  let x = forward(part: view);
  return std::process::exit_status(code: x);
}}
"
        )
    };
    assert_accepts(body("1_u64").as_bytes());
    assert_call_goal(
        body("3_u64").as_bytes(),
        CallRequirementDisposition::Refuted,
        "3_u64 < deref(part).len",
    );
}

/// [ENT-3.S6]: "`let part = &P[lo..hi];` for a tracked P establishes
/// `deref(part).len = hi - lo` after [REF-4]'s domain goals discharge, over
/// the exact current-value images captured where the endpoints are evaluated."
/// It is an ordinary fact of the state, so a caller that has just formed the
/// range discharges an equality over its length and refutes a different one.
/// `&table[4_u64..24_u64]` is twenty elements however long `table` is.
#[test]
fn a_range_formation_establishes_its_length_equality() {
    let body = |length: &str| {
        format!(
            "fn expects(part: &[u8]) -> result: u64 reads(part.len) contract {{
  requires deref(part).len == {length};
}} {{
  return deref(part).len;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let table = array_filled::<u8, 32>(value: 0_u8);
  let view = &table[4_u64..24_u64];
  let n = expects(part: view);
  return std::process::exit_status(code: 0_u8);
}}
"
        )
    };
    assert_accepts(body("20_u64").as_bytes());
    assert_call_goal(
        body("32_u64").as_bytes(),
        CallRequirementDisposition::Refuted,
        "deref(view).len == 32_u64",
    );
}

/// [ENT-3.S6] carries the current endpoint images into an inline range actual,
/// so an ordinary ordering fact proves that the formed range is nonempty.
#[test]
fn dynamic_inline_range_length_discharge_uses_its_endpoint_ordering() {
    let source = br#"fn nonempty(part: &[u8]) -> result: u64 reads(part.len) contract {
  requires 0_u64 < deref(part).len;
} {
  return deref(part).len;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 4>(value: 0_u8);
  let lo = 1_u64;
  let hi = 3_u64;
  if lo < hi {
    let n = nonempty(part: &a[lo..hi]);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// Rebinding a range reference replaces its captured measure. The requirement
/// after the assignment therefore sees the new four-element range.
#[test]
fn rebinding_a_range_reference_replaces_its_captured_length() {
    let source = format!(
        "{RANGE_OFFSET_CALLEE}
fn main() -> status: std::process::ExitStatus pure {{
  let a = array_filled::<u8, 4>(value: 0_u8);
  let part = &a[2_u64..3_u64];
  set part = &a[0_u64..4_u64];
  let x = at(part: part, offset: 3_u64);
  return std::process::exit_status(code: x);
}}
"
    );
    assert_accepts(source.as_bytes());
}

/// [REF-4, ENT-5] assigning a new range to the holder replaces its captured
/// descriptor. A window-part preservation rule must not retain the old
/// one-element length across this whole-holder write.
#[test]
fn rebinding_a_range_reference_does_not_retain_the_old_length() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 2>(value: 0_u8);
  let part = &a[0_u64..1_u64];
  set part = &a[0_u64..0_u64];
  let first = deref(part)[0_u64];
  return std::process::exit_status(code: first);
}
"#;
    assert_rule_kind(source, SemanticRule::Op4, |_| true);
}

/// Endpoint bindings are evaluated when a range is formed. Later assignments
/// cannot rewrite the length already stored in the reference descriptor.
#[test]
fn a_bound_range_keeps_the_endpoint_images_captured_at_formation() {
    let body = |length: &str| {
        format!(
            "fn expects(part: &[u8]) -> result: u64 reads(part.len) contract {{
  requires deref(part).len == {length};
}} {{
  return deref(part).len;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let a = array_filled::<u8, 6>(value: 0_u8);
  let lo = 1_u64;
  let hi = 3_u64;
  let part = &a[lo..hi];
  set lo = 0_u64;
  set hi = 6_u64;
  let n = expects(part: part);
  return std::process::exit_status(code: 0_u8);
}}
"
        )
    };
    assert_accepts(body("2_u64").as_bytes());
    assert_call_goal(
        body("6_u64").as_bytes(),
        CallRequirementDisposition::Refuted,
        "deref(part).len == 6_u64",
    );
}

/// Two inline ranges reuse the same endpoint binding spellings but evaluate
/// them at different times, so their call goals retain distinct lengths.
#[test]
fn inline_ranges_separated_by_endpoint_mutation_keep_distinct_lengths() {
    let source = br#"fn below_three(part: &[u8]) -> result: u64 reads(part.len) contract {
  requires deref(part).len < 3_u64;
} {
  return deref(part).len;
}

fn above_three(part: &[u8]) -> result: u64 reads(part.len) contract {
  requires deref(part).len > 3_u64;
} {
  return deref(part).len;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 6>(value: 0_u8);
  let lo = 1_u64;
  let hi = 3_u64;
  let first = below_three(part: &a[lo..hi]);
  set lo = 0_u64;
  set hi = 4_u64;
  let second = above_three(part: &a[lo..hi]);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// [OWN-7, REF-2] a reference into an element outside a range a callee
/// writes survives the call once the call's entry state proves the index at
/// or after the range's end; a reference the entry state may put inside the
/// range dies with the element the callee may replace.
#[test]
fn a_reference_outside_a_written_range_survives_the_write() {
    let program = |bound: &str| {
        format!(
            "struct Pair {{
  x: u64;
  y: u64;
}}

fn zero(part: &[Pair], at: u64) -> result: unit writes(part) contract {{
  requires at < deref(part).len;
}} {{
  let fresh = Pair(x: 0_u64, y: 0_u64);
  set deref(part)[at] = fresh;
  return unit;
}}

fn run(values: &Array<Pair, 8>, lo: u64, hi: u64, k: u64) -> result: u64 writes(values) contract {{
  requires lo < hi;
  requires {bound};
  requires k < 8_u64;
}} {{
  let held = &deref(values)[k].x;
  zero(part: &deref(values)[lo..hi], at: 0_u64);
  let seen = deref(held);
  return seen;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
        )
    };
    assert_accepts(program("hi <= k").as_bytes());
    assert_rule_kind(
        program("hi <= 8_u64").as_bytes(),
        SemanticRule::Ref2,
        |kind| matches!(kind, SemanticIssueKind::InvalidReferenceUse { .. }),
    );
}
