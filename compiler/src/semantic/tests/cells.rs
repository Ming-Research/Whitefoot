//! The cell [TYPE-9]: `Box<T>` is the prelude's opaque struct
//! `opaque struct Box<T> { inner: T; }`, whose one field `inner` is its
//! content, stored in exactly one heap object the `Box` value owns [STOR-1].
//!
//! v0.60 retired the store apparatus this module was written against. There
//! is one heap [STOR-8], so a cell carries no store brand and no region, its
//! allocation is total and returns no `Result`, and there is no `allocates`
//! row to declare. `box_new::<T>(value: v)` is the [OP-13] construction
//! record, the content is read as `b.inner` and never through `deref`
//! [TYPE-7], it is written with `set b.inner = v` [SET-1], and
//! `let n = move b.inner;` consumes the cell, yields its content and frees it
//! [WIN-3].

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::model::{
    CheckedExpression, CheckedNominalKind, CheckedOwnedTakeCleanup, CheckedPlaceStep,
    CheckedStatement, CheckedType,
};
use super::{assert_rule, assert_rule_kind, with_semantics};

#[test]
fn cell_creation_content_read_and_cleanup_are_explicit() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let value = 41_u64;
  let owner = box_new::<u64>(value: value);
  let loaded = owner.inner;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("cell creation and copy content read must check: {outcome:?}");
        };
        let main = &checked.data.functions[0];
        let body = main.body.as_deref().expect("WF body");
        let CheckedStatement::Let {
            value:
                CheckedExpression::BoxDeref {
                    nominal,
                    referent: CheckedType::Integer(_),
                    ..
                },
            ..
        } = &body[2]
        else {
            panic!("the content read must remain the field step `inner`");
        };
        // [STOR-8] there is one heap, so a cell carries no brand: the checked
        // nominal's region is `None` for every cell a v0.60 program forms.
        assert!(matches!(
            checked.data.nominals[nominal.0 as usize].kind,
            CheckedNominalKind::Box {
                referent: CheckedType::Integer(_),
                region: None,
                ..
            }
        ));
        let CheckedStatement::Return { drops, .. } = &body[3] else {
            panic!("main must end in return");
        };
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].ty, CheckedType::Nominal(*nominal));
    });
}

/// Assigning one whole cell owner over another transfers the second
/// allocation into the first binding without changing the first binding's
/// cell type, and releases the displaced affine value [WIN-3].
///
/// This was `let old = replace first = move second;` while [SET-2] existed.
/// The `replace` statement is retired: [SET-1] writes the place and [WIN-3]
/// owns the old value's disposition, which is the release of an affine one.
#[test]
fn whole_cell_assignment_preserves_the_owner_shape() {
    let source = br#"struct Pair {
  value: u64;
}

fn assign_owner() -> result: u64 pure {
  let first_value = Pair(value: 0_u64);
  let second_value = Pair(value: 1_u64);
  let first = box_new::<Pair>(value: first_value);
  let second = box_new::<Pair>(value: second_value);
  set first = move second;
  let seen = first.inner.value;
  return seen;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("whole-cell assignment must preserve the owner type: {outcome:?}");
        };
        let assign_owner = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "assign_owner")
            .expect("assign_owner function");
        let CheckedStatement::Set { target, value, .. } =
            &assign_owner.body.as_deref().expect("WF body")[4]
        else {
            panic!("the whole-owner write must remain a checked Set");
        };
        let CheckedType::Nominal(nominal) = target.ty() else {
            panic!("the target must remain the cell nominal");
        };
        assert!(matches!(
            checked.data.nominals[nominal.0 as usize].kind,
            CheckedNominalKind::Box { .. }
        ));
        assert_eq!(value.ty(), target.ty());
    });
}

/// [TYPE-9, WIN-3] `let n = move b.inner;` consumes the cell, yields its
/// content and frees the cell.
///
/// This replaces the retired `move deref(owner)` capability stop: the v0.59
/// checker answered an affine referent move with
/// `UnsupportedSemanticFeature::BoxReferentMove`, and v0.60 states the
/// unboxing outright. The one remaining refusal — a move of a
/// runtime-capacity content, which has no constant-capacity twin — is owned
/// by the conformance case `type9-neg-move-runtime-capacity-content`.
#[test]
fn unboxing_consumes_the_cell_and_yields_its_content() {
    let source = br#"nocopy struct Pair {
  value: u64;
}

fn unbox() -> result: u64 pure {
  let content = Pair(value: 7_u64);
  let cell = box_new::<Pair>(value: move content);
  let taken = move cell.inner;
  return taken.value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("unboxing an affine content must check: {outcome:?}");
        };
        let unbox = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "unbox")
            .expect("unbox function");
        // [TYPE-9, WIN-3] the consume of the content through the field step
        // `inner` is not the ordinary content read: the cell ceases to exist
        // here and is freed with the value it hands back, so the checked node
        // is the unboxing one and not `BoxDeref`.
        let CheckedStatement::Let {
            value: CheckedExpression::BoxTake { referent, .. },
            ..
        } = &unbox.body.as_deref().expect("WF body")[2]
        else {
            panic!("the consumed content must remain the field step `inner`");
        };
        assert!(
            matches!(referent, CheckedType::Nominal(_)),
            "the yielded content is the struct the cell held: {referent:?}"
        );
    });
}

#[test]
fn nested_owned_box_take_carries_an_ordered_cleanup_plan() {
    let source = br#"nocopy struct Payload {
  value: u8;
}

nocopy struct Inner {
  selected: Box<Payload>;
  tail: Box<u8>;
}

struct Outer {
  before: Box<u8>;
  head: Box<Inner>;
  other: Box<u8>;
}

fn take() -> result: u8 pure {
  let payload = Payload(value: 1_u8);
  let selected = box_new::<Payload>(value: move payload);
  let tail = box_new::<u8>(value: 2_u8);
  let inner = Inner(selected: move selected, tail: move tail);
  let head = box_new::<Inner>(value: move inner);
  let before = box_new::<u8>(value: 0_u8);
  let other = box_new::<u8>(value: 3_u8);
  let outer = Outer(before: move before, head: move head, other: move other);
  let taken = move outer.head.inner.selected.inner;
  return taken.value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("nested take must check: {outcome:?}");
        };
        let take = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "take")
            .expect("take function");
        let CheckedStatement::Let {
            value: CheckedExpression::BoxTake { cleanup, .. },
            ..
        } = &take.body.as_deref().expect("WF body")[8]
        else {
            panic!("nested move must remain one checked owner take");
        };
        let [
            CheckedOwnedTakeCleanup::Drop { path: before, .. },
            CheckedOwnedTakeCleanup::BoxShell {
                path: selected,
                nominal: selected_box,
                ..
            },
            CheckedOwnedTakeCleanup::Drop { path: tail, .. },
            CheckedOwnedTakeCleanup::BoxShell {
                path: head,
                nominal: head_box,
                ..
            },
            CheckedOwnedTakeCleanup::Drop { path: other, .. },
        ] = cleanup.as_slice()
        else {
            panic!("complete ordered cleanup plan: {cleanup:?}");
        };
        assert_eq!(before, &[CheckedPlaceStep::Field(0)]);
        assert_eq!(head, &[CheckedPlaceStep::Field(1)]);
        assert_eq!(other, &[CheckedPlaceStep::Field(2)]);
        assert_eq!(
            selected,
            &[
                CheckedPlaceStep::Field(1),
                CheckedPlaceStep::BoxReferent(*head_box),
                CheckedPlaceStep::Field(0),
            ]
        );
        assert_eq!(
            tail,
            &[
                CheckedPlaceStep::Field(1),
                CheckedPlaceStep::BoxReferent(*head_box),
                CheckedPlaceStep::Field(1),
            ]
        );
        assert_ne!(selected_box, head_box);
    });
}

#[test]
fn nested_owned_box_take_rejects_a_linear_residual() {
    assert_rule_kind(
        br#"nocopy struct Payload {
  value: u8;
}

nodrop struct Token {
  value: u8;
}

nocopy struct Inner {
  selected: Box<Payload>;
  tail: Token;
}

fn take(token: Token) -> result: u8 pure {
  let payload = Payload(value: 1_u8);
  let selected = box_new::<Payload>(value: move payload);
  let inner = Inner(selected: move selected, tail: move token);
  let owner = box_new::<Inner>(value: move inner);
  let taken = move owner.inner.selected.inner;
  return taken.value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        // PROV-6 precedes WIN-3 under DIAG-1 at this same consumed place:
        // a residual linear value has no derived release, even under Box.
        SemanticRule::Prov6,
        |kind| matches!(kind, SemanticIssueKind::LinearValuePartiallyConsumed { .. }),
    );
}

#[test]
fn indexed_box_content_move_remains_a_win3_source_rejection() {
    assert_rule_kind(
        br#"nocopy struct Payload {
  value: u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let slots = slots_new::<Box<Payload>, 1>();
  let payload = Payload(value: 1_u8);
  let cell = box_new::<Payload>(value: move payload);
  place_back(window: &slots, value: move cell);
  let taken = move slots[0_u64].inner;
  return std::process::exit_status(code: taken.value);
}
"#,
        SemanticRule::Win3,
        |kind| matches!(kind, SemanticIssueKind::MoveOutOfSlot { .. }),
    );
}

/// The ordinary own-rooted judgments a cell-content target reaches: [WIN-3]
/// for a linear old value, which has no release, and [OWN-1] for a dead root,
/// which [SET-1] never revives.
///
/// v0.59's first half asserted [STOR-1]'s `AffineSetTarget`, which refused an
/// affine final selected type outright and pointed at `replace`. [WIN-3]
/// supersedes it: assigning over an owned place releases the old value when
/// it is affine and is refused only when it is linear.
#[test]
fn cell_content_set_targets_keep_their_source_rejections() {
    assert_rule(
        br#"nodrop struct Token {
  value: u64;
}

fn hold(first: Token, second: Token) -> result: unit pure {
  let cell = box_new::<Token>(value: move first);
  set cell.inner = move second;
  let Token(value: seen) = move cell.inner;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Win3,
        SemanticIssueKind::LinearAssignmentTarget {
            target_type: "Token".to_owned(),
            mechanical_fix: "take the linear value out and consume it before writing this place",
        },
    );
    assert_rule(
        br#"fn eat(b: Box<i32>) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let b = box_new::<i32>(value: 4_i32);
  eat(b: move b);
  set b.inner = 7_i32;
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Own1,
        SemanticIssueKind::UseAfterMove {
            mechanical_fix: "introduce a new `let` binding before reuse",
        },
    );
}

// Retired with its subject: `region_bearing_cell_content_rejects_under_stor5_at_both_stores`
// asserted [STOR-5]'s refusal of region-bearing cell content at the general
// store and at the bump extent. Regions, arenas, providers and views are
// retired with no successor, and [STOR-5]'s surviving half — storage is
// reference-free — is closed by [TYPE-8] in the grammar, which admits no
// reference kind in a `Box` content or type-argument position at all, so no
// source program reaches the judgment.

/// [TYPE-9] the cell nominal is derived from the written referent, so a
/// purely local cell names `Box<T>` nowhere for the written-type interning
/// pass to find.
///
/// The control pair differs only in whether some *other* declaration spells
/// `Box<u64>`. Before the checker could intern a derived referent, the first
/// program failed with a compiler failure while the second compiled — an
/// implementation limitation deciding what source was acceptable.
///
/// v0.59 expected two nominals from the second program, because a cell's
/// store region was a component of its type and the helper's written
/// `Box<'s, u64>` was its own region's instance. There is one heap [STOR-8]
/// and a cell carries no brand, so both programs now intern exactly one
/// `Box<u64>` and the two spellings name the same type.
#[test]
fn a_derived_cell_nominal_is_interned_whether_or_not_the_type_is_spelled_elsewhere() {
    let named_nowhere = br#"fn main() -> status: std::process::ExitStatus pure {
  let owner = box_new::<u64>(value: 41_u64);
  let loaded = owner.inner;
  return std::process::exit_status(code: 0_u8);
}
"#;
    let named_in_a_signature = br#"fn take(b: Box<u64>) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let owner = box_new::<u64>(value: 41_u64);
  take(b: move owner);
  return std::process::exit_status(code: 0_u8);
}
"#;
    for source in [named_nowhere.as_slice(), named_in_a_signature.as_slice()] {
        with_semantics(source, |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("a derived cell nominal must check: {outcome:?}");
            };
            // The derived nominal sits inside the executable prefix, because
            // executable code allocates and frees it.
            let cells = checked
                .data
                .nominals
                .iter()
                .take(checked.data.executable_nominal_count)
                .filter(|nominal| {
                    matches!(
                        nominal.kind,
                        CheckedNominalKind::Box {
                            referent: CheckedType::Integer(_),
                            ..
                        }
                    )
                })
                .count();
            assert_eq!(
                cells, 1,
                "one heap, one `Box<u64>`, however many declarations spell it"
            );
        });
    }
}

/// [TYPE-7] `deref(place)` where `place` is not a reference, a `Box`
/// included, is a hard error at the complete `place`, and its restructuring
/// names the field step that reaches a cell's content.
///
/// This case is new in the v0.60 port: v0.59 reached a cell's referent
/// through `deref` and so had no such refusal to state.
#[test]
fn deref_of_a_cell_is_a_type7_rejection_naming_the_field_inner() {
    assert_rule_kind(
        br#"fn main() -> status: std::process::ExitStatus pure {
  let owner = box_new::<u64>(value: 41_u64);
  let loaded = deref(owner);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Type7,
        |kind| {
            matches!(
                kind,
                SemanticIssueKind::MissingDereference { mechanical_fix }
                    if mechanical_fix.contains("a Box's content is its field inner")
            )
        },
    );
}
