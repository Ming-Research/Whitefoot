//! [WIN-1] through [WIN-3], [TYPE-9], [TYPE-10] and the window, construction
//! and release operations [OP-10, OP-13, OP-14, OP-15].
//!
//! This module replaces `tests/buffers.rs` (22 tests), whose subject was
//! v0.59's `buffer` storage class and the store apparatus around it. Each
//! retirement has a named successor:
//!
//! - The storage names `buffer`, `FixedVector`, `Vector<'s, T>`, `arena<'r, T>`
//!   and `box<T>` retire with [TYPE-2]'s and [GRAM-3]'s alternatives. The
//!   successor is [TYPE-9]: `Array`, `Slots` and `Ring` in a constant-capacity
//!   and a runtime-capacity form each, and the cell `Box<T>` whose content is
//!   its field `inner`.
//! - [BLK-1]'s two-run inventory, [BLK-2]'s formation rows, [BLK-3]'s refusal
//!   of swap, growth and middle removal, [BLK-4]'s confinement and [PROV-1]'s
//!   store/provider/capability table retire. The successor is the ordinary
//!   [PRE-1] records: the nine window operations [OP-10], the nine
//!   construction functions [OP-13], `swap` [OP-11] and `free_empty` [OP-14],
//!   over one heap [STOR-8].
//! - `buffer_new`, `buffer_vacant`, `array_new` and `box_new`'s old spelling
//!   leave [OP-1]'s table; `buffer_fits` and its static allocation-fit
//!   predicate leave with them. The successor of the fit predicate is [OP-9]'s
//!   allocation-size obligation, still an `AllocationFit` obligation family in
//!   the checker but now stated over `stride_ceiling(T) * n` staying inside
//!   u64, carried by each runtime-capacity construction and by `grow`.
//! - `dispose` retires with the capability leaves. The successor is [OP-14]
//!   `free_empty(window: move r)` on a proved-empty window, and for an affine
//!   value, moving it into a function that consumes it [PROV-6].
//! - The `len_of` / `cap_of` / `vacant_of` / `extent_of` former family retires.
//!   The successor is [OP-15]: `r.len`, `r.cap` and `r.head` are place forms,
//!   not calls.
//!
//! The sources are the already-ported v0.60 conformance cases; these tests add
//! the rule and issue kind the corpus manifest does not pin.
//!
//! Two of the judgments below are stated by the specification and are NOT
//! implemented by the checker as this package lands. Each assertion here is
//! the normative verdict, not the current one, because an unimplemented
//! feature is not a source-language rejection and must not rewrite a
//! normative expectation. The two gaps, with the code that owns each:
//!
//! 1. [OP-10]'s compiler-owned window type parameter is not inferred from the
//!    operand. `check/generics.rs` (`generic_substitution`) refuses any call
//!    to a callee with type parameters and no written argument list, citing
//!    FN-2, while [OP-10] states that "a window operation, `swap` [OP-11],
//!    and `free_empty` [OP-14] therefore write no type arguments at a call:
//!    every type parameter of those rows is supplied by an operand".
//! 2. A runtime-capacity `Slots<T>` or `Ring<T>`, and a constant-capacity
//!    `Ring<T, N>`, stop as an unimplemented compiler capability at
//!    `check/types.rs`.
//!
//! The judgments this package's port closed, and which are now ordinary
//! assertions above: a `Box`'s content reached by the bare field step
//! `b.inner` and consumed by `let n = move b.inner;` [TYPE-9, WIN-3]; a move
//! out of a window slot citing WIN-3 with `MoveOutOfSlot`; [OP-14]'s own site
//! for an undischarged `free_empty` requirement; [OP-9]'s allocation-size
//! obligation at each runtime-capacity construction and at `grow`, rendered
//! as its defining comparison rather than as the retired `buffer_fits` row;
//! and [OP-15]'s measure member read in expression position, including one
//! over a subscripted place.

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::{assert_accepts, assert_rule_kind, check_case_directory, check_module_sources};

fn assert_op9_allocation_fit(source: &[u8], context: &str) {
    super::with_semantics(source, |outcome| match outcome {
        SemanticOutcome::SourceIssue { issue } => {
            assert_eq!(issue.rule(), SemanticRule::Op9, "{context}");
            assert!(
                matches!(
                    issue.kind(),
                    SemanticIssueKind::UndischargedAllocationFitObligation { .. }
                ),
                "{context} must retain OP-9's allocation-fit issue kind"
            );
        }
        SemanticOutcome::Complete(_) => panic!("{context} was accepted"),
        _ => panic!("{context} did not reach OP-9"),
    });
}

/// [WIN-1] a `Slots` is a run of `cap` slots whose initialized storage is the
/// `len` slots beginning at `head`, and an `Array` has no window at all.
#[test]
fn a_window_is_its_len_slots_and_an_array_has_none() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/win1-pos-window-is-len-slots.wf"
    ));
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/win1-pos-array-len-equals-cap.wf"
    ));
}

/// [WIN-1] a subscript `r[i]` carries [OP-4]'s obligation `i < r.len`, stated
/// against `len` and never against `cap` or `head`.
#[test]
fn a_subscript_above_the_length_is_undischarged() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/win1-neg-subscript-above-len.wf");
    assert_rule_kind(source, SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

/// [WIN-2] the four window parts are vocabulary for effect rows and the
/// overlap judgment only: a declared row may name one, and no program writes
/// one.
#[test]
fn a_window_part_is_row_vocabulary_and_not_a_place() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/win2-pos-part-effect-row.wf"
    ));
    let written =
        include_bytes!("../../../../tests/conformance/cases/win2-neg-window-part-write-target.wf");
    assert_rule_kind(written, SemanticRule::Type10, |kind| {
        matches!(kind, SemanticIssueKind::ReservedPseudoField { .. })
    });
}

/// [TYPE-2] a measure is a readonly field of the shape's [PRE-1]
/// declaration: it is read like any field and is never a write target, and
/// only [OP-10] and [OP-13] change one.
#[test]
fn a_measure_is_read_only() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/type10-pos-measures-are-read-only.wf"
    ));
    let written = include_bytes!(
        "../../../../tests/conformance/cases/type2-neg-readonly-measure-write-target.wf"
    );
    assert_rule_kind(written, SemanticRule::Type2, |kind| {
        matches!(kind, SemanticIssueKind::ReadonlyWriteTarget { .. })
    });
}

/// x1 [TYPE-10]: the measure and window-part spellings reserve nothing, so a
/// source struct of another type declares fields named `len` and `next` and
/// bindings carry the part spellings.
#[test]
fn member_spellings_reserve_nothing() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/type10-pos-member-spellings-reserve-nothing.wf"
    ));
}

/// [TYPE-2] a `set` whose target ends at or passes through a readonly field
/// is refused in a source struct exactly as in a prelude one, and an argument
/// naming such a path at a written reference parameter is refused with it.
#[test]
fn a_readonly_field_is_never_a_write_target() {
    // [TYPE-2, MOD-6] a source readonly field is public and withholds writes
    // only from the modules that do not declare it, so these cases are
    // module programs.
    for case in [
        "type2-neg-readonly-field-set-target",
        "type2-neg-readonly-path-passes-through",
        "type2-neg-readonly-field-written-argument",
    ] {
        let failure = check_case_directory(case).expect_err("an external readonly write rejects");
        assert_eq!(failure.rule_id(), Some("TYPE-2"), "{case}: {failure}");
        assert!(
            failure.to_string().contains(": ReadonlyWriteTarget\n"),
            "{case}: {failure}"
        );
    }
    check_case_directory("type2-pos-readonly-field-read-and-whole-replace")
        .expect("reads, whole replacement and module-internal writes are accepted");
}

/// [TYPE-2] readonly provenance follows references and reborrows. A written
/// actual cannot hide the readonly field by naming an alias or by stepping
/// through a reference to the enclosing value.
#[test]
fn readonly_provenance_survives_reference_aliases_and_reborrows() {
    let graph = b"pkg::records: [];\npkg: [pkg::records, std::process];\n";
    let interface = b"public struct Record {\n  public readonly value: u8;\n}\n\npublic fn make() -> record: Record pure doc \"Makes a record.\";\n";
    let definition = b"fn make() -> record: Record pure {\n  return Record(value: 1_u8);\n}\n";
    for writer in [
        b"  let p = &record.value;\n  put(cell: p);\n".as_slice(),
        b"  let p = &record;\n  put(cell: &deref(p).value);\n".as_slice(),
        b"  let p = &record.value;\n  put(cell: &deref(p));\n".as_slice(),
    ] {
        let mut main = b"alias records = pkg::records;\n\nfn put(cell: &u8) -> result: unit writes(cell) {\n  set deref(cell) = 9_u8;\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let record = records::make();\n".to_vec();
        main.extend_from_slice(writer);
        main.extend_from_slice(b"  return std::process::exit_status(code: 0_u8);\n}\n");
        let failure = check_module_sources(
            graph,
            &[
                ("records/module.wfm", interface.as_slice()),
                ("records/record.wf", definition.as_slice()),
                ("module.wfm", b"\n".as_slice()),
                ("main.wf", main.as_slice()),
            ],
        )
        .expect_err("an external readonly write through a reference rejects");
        assert_eq!(failure.rule_id(), Some("TYPE-2"), "{failure}");
        assert!(
            failure.to_string().contains(": ReadonlyWriteTarget\n"),
            "{failure}"
        );
    }
}

/// [OP-15] a measure read is a place form whose exact type is `own u64`, so
/// the ordinary [TYPE-5] check applies at each use.
#[test]
fn a_measure_read_is_a_place_of_type_u64() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op15-pos-measure-read-is-a-place.wf"
    ));
    let mistyped =
        include_bytes!("../../../../tests/conformance/cases/op15-neg-measure-type-is-u64.wf");
    assert_rule_kind(mistyped, SemanticRule::Type5, |kind| {
        matches!(kind, SemanticIssueKind::TypeMismatch { .. })
    });
}

/// [WIN-3] a move out of a window slot is a hard error; the restructuring is
/// `take_back`, `remove_at`, or `swap`.
#[test]
fn a_move_out_of_a_window_slot_is_refused() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/win3-neg-move-out-of-window-slot.wf");
    assert_rule_kind(source, SemanticRule::Win3, |kind| {
        matches!(kind, SemanticIssueKind::MoveOutOfSlot { .. })
    });
}

/// [WIN-3] assigning over an owned place releases the old value when it is
/// affine and is a hard error at the target `place` when it is linear.
#[test]
fn assignment_releases_an_affine_old_value_and_refuses_a_linear_one() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/win3-pos-overwrite-releases-affine.wf"
    ));
    let linear =
        include_bytes!("../../../../tests/conformance/cases/win3-neg-overwrite-linear-place.wf");
    assert_rule_kind(linear, SemanticRule::Win3, |kind| {
        matches!(kind, SemanticIssueKind::LinearAssignmentTarget { .. })
    });
}

/// [WIN-3] a destructuring consume binds the fields it names and covers the
/// rest with `..`, which is the route a remaining linear part must take.
#[test]
fn a_destructuring_consume_takes_the_whole_owner() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/win3-pos-destructuring-consume.wf"
    ));
}

/// [OP-10] `place_back` fills the append slot and moves `r.len` up by one;
/// `take_back` empties the last filled slot and moves it back down.
#[test]
fn the_back_operations_move_the_boundary() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op10-pos-place-back-take-back.wf"
    ));
}

/// [OP-10] `insert_at`, `remove_at`, `append` and `split_off` each shift or
/// move a run of elements and move the boundary.
#[test]
fn the_shifting_operations_move_runs_and_boundaries() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op10-pos-insert-remove-append-split.wf"
    ));
}

/// [OP-10] `place_front` and `take_front` admit `Ring` alone and shift every
/// logical index.
#[test]
fn the_front_operations_admit_a_ring_alone() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op10-pos-front-operations-on-ring.wf"
    ));
}

/// [OP-10] `grow` is defined on `Box<Slots<T>>` alone and remakes the cell's
/// content whole, carrying [OP-9]'s obligation.
#[test]
fn grow_remakes_a_boxed_window() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op10-pos-boxed-window-grow.wf"
    ));
}

/// [OP-10] `place_back`'s `requires deref(window).len < deref(window).cap` is an ordinary
/// [FN-8] requirement, so a full window is refused at the call.
#[test]
fn place_back_on_a_full_window_is_an_undischarged_requirement() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/op10-neg-place-back-full-window.wf");
    assert_rule_kind(source, SemanticRule::Fn8, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallRequirement { .. })
    });
}

/// [OP-13] the nine construction functions are the only way to build a shape;
/// there is no `Type::name` spelling and no element-list literal in expression
/// position.
#[test]
fn the_construction_functions_build_every_shape() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op13-pos-construction-functions.wf"
    ));
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op13-pos-slots-from-and-into-array.wf"
    ));
}

/// [OP-9] each runtime-capacity construction carries the static
/// allocation-size obligation over its own count.
#[test]
fn a_runtime_capacity_construction_owes_the_size_obligation() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/op13-neg-runtime-capacity-size-obligation.wf"
    );
    assert_rule_kind(source, SemanticRule::Op9, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedAllocationFitObligation { .. }
        )
    });
}

/// [ENT-1, OP-9] a source-canonical generic schema checks an allocation whose
/// stored layout is expressible even when some unrelated part of the function
/// remains symbolic. Scalars, pointers, and aggregates built entirely from
/// fixed-layout components therefore cannot hide an unbounded count behind a
/// const or type parameter.
#[test]
fn known_stored_layouts_keep_op9_in_symbolic_schemas() {
    assert_op9_allocation_fit(
        include_bytes!(
            "../../../../tests/conformance/cases/op9-neg-known-overflow-in-unused-schema.wf"
        ),
        "known AboveU64 layout in an unused generic schema",
    );
    for (case, source) in [
        br#"fn unchecked<const unused: u64>(count: u64) -> result: unit pure {
  let cells = box_slots_new::<u16>(capacity: count);
  free_empty(window: move cells);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"fn unchecked<T>(count: u64) -> result: unit pure {
  let cells = box_slots_new::<Box<T>>(capacity: count);
  free_empty(window: move cells);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"struct Envelope<T> {
  payload: Box<T>;
  tag: u8;
}

fn unchecked<T>(count: u64) -> result: unit pure {
  let cells = box_slots_new::<Envelope<T>>(capacity: count);
  free_empty(window: move cells);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"fn unchecked<T>(count: u64) -> result: unit pure {
  let cells = box_slots_new::<Slots<Box<T>, 2>>(capacity: count);
  free_empty(window: move cells);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
    ]
    .into_iter()
    .enumerate()
    {
        assert_op9_allocation_fit(source, &format!("known-layout symbolic schema case {case}"));
    }
}

/// [ENT-1, FN-2] an opaque stored `T` has no expressible stride in the
/// source-canonical schema, so that schema may defer [OP-9]. Each discovered
/// concrete instance is nevertheless rechecked with its actual scalar,
/// aggregate, pointer, or descriptor layout, including through a second
/// generic caller.
#[test]
fn unresolved_stored_layouts_defer_to_every_concrete_replay() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op9-pos-unresolved-aggregate-layout.wf"
    ));
    let unresolved_schema = br#"fn allocate<T>(count: u64) -> result: unit pure {
  let cells = box_slots_new::<T>(capacity: count);
  free_empty(window: move cells);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(unresolved_schema);

    let concrete_replays = br#"struct Packet {
  word: u64;
  tag: u16;
}

fn allocate<T>(count: u64) -> result: unit pure contract {
  requires count <= 1_u64;
} {
  let cells = box_slots_new::<T>(capacity: count);
  free_empty(window: move cells);
  return unit;
}

fn forward<T>(count: u64) -> result: unit pure contract {
  requires count <= 1_u64;
} {
  allocate::<T>(count: count);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  forward::<u16>(count: 1_u64);
  forward::<Packet>(count: 1_u64);
  forward::<Box<u64>>(count: 1_u64);
  forward::<Slots<u64, 2>>(count: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(concrete_replays);
}

/// A factory can receive a nominal that still contains its caller's opaque
/// parameter. Its nominal identity does not make the allocation layout known.
#[test]
fn transitive_nominal_allocation_layouts_remain_symbolic_until_replay() {
    let source = r#"struct Slab<T> {
  cells: Box<Slots<T>>;
}

struct Envelope<T> {
  payload: T;
}

fn slab_new<T>() -> result: Slab<T> pure {
  let cells = box_slots_new::<T>(capacity: 0_u64);
  return Slab<T>(cells: move cells);
}

fn slab_free<T, fn consume(value: T) -> result: unit pure>(slab: Slab<T>) -> result: unit pure {
  let Slab(cells: cells) = move slab;
  loop @cleanup {
    if cells.inner.len == 0_u64 {
      break @cleanup;
    }
    let value = take_back(window: &cells.inner);
    consume(value: move value);
  }
  free_empty(window: move cells);
  return unit;
}

fn release_envelope<T, fn consume(value: T) -> result: unit pure>(value: Envelope<T>) -> result: unit pure {
  let Envelope(payload: owned_payload) = move value;
  consume(value: move owned_payload);
  return unit;
}

fn empty_store<T, fn consume(value: T) -> result: unit pure>() -> result: unit pure {
  let store = slab_new::<Envelope<T>>();
  slab_free::<Envelope<T>, fn release_envelope::<T, fn consume>>(slab: move store);
  return unit;
}

fn consume_word(value: u64) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source.as_bytes());
    assert_accepts(
        source
            .replace(
                "  return std::process::exit_status(code: 0_u8);",
                "  empty_store::<u64, fn consume_word>();\n  return std::process::exit_status(code: 0_u8);",
            )
            .as_bytes(),
    );

    // The same transitive nominal path still owes its actual size bound
    // once U is concrete. Envelope<u64>'s ceiling has a 16-byte stride.
    let source = |upper| {
        format!(
            r#"struct Envelope<T> {{
  payload: T;
  tag: u8;
}}

fn allocate<T>(count: u64) -> result: unit pure contract {{
  requires count <= {upper}_u64;
}} {{
  let cells = box_slots_new::<T>(capacity: count);
  free_empty(window: move cells);
  return unit;
}}

fn relay<U>(count: u64) -> result: unit pure contract {{
  requires count <= {upper}_u64;
}} {{
  allocate::<Envelope<U>>(count: count);
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  relay::<u64>(count: {upper}_u64);
  return std::process::exit_status(code: 0_u8);
}}
"#
        )
    };
    let limit = u64::MAX / 16;
    assert_accepts(source(limit).as_bytes());
    assert_op9_allocation_fit(
        source(limit + 1).as_bytes(),
        "concrete replay of a transitive nominal allocation",
    );
}

/// [ENT-1, FN-2, OP-9] only a layout depending on an unresolved parameter may
/// take the source-schema deferral. When a generic relay instantiates that same
/// allocation template with a numeric type or a fixed-layout wrapper, the
/// alpha-renamed instance has a known ceiling and must prove its allocation
/// bound. The scratch instance's OP-9 issue is not published as a canonical
/// source issue; instead, a failed proof withholds its verified FN-9 summary,
/// so the canonical relay cannot prove the relation it forwards. Returning
/// the constructed window's capacity makes that relation depend on the
/// constructor's verified `cap == count` publication.
#[test]
fn transitive_known_layouts_do_not_take_the_direct_opaque_deferral() {
    const U64_MAX: u64 = u64::MAX;
    for (declaration, parameter, stored, limit) in [
        ("", "U: Int", "U", U64_MAX / 8),
        ("", "U: Float", "U", U64_MAX / 8),
        ("", "U", "Box<U>", U64_MAX / 8),
        (
            "struct Envelope<T> {\n  payload: Box<T>;\n  tag: u8;\n}",
            "U",
            "Envelope<U>",
            U64_MAX / 16,
        ),
    ] {
        let declaration_prefix = if declaration.is_empty() {
            String::new()
        } else {
            format!("{declaration}\n\n")
        };
        let source = |upper| {
            format!(
                r#"{declaration_prefix}fn allocate<T>(count: u64) -> result: u64 pure contract {{
  requires count <= {upper}_u64;
  ensures result == count;
}} {{
  let cells = box_slots_new::<T>(capacity: count);
  let capacity = cells.inner.cap;
  free_empty(window: move cells);
  return capacity;
}}

fn relay<{parameter}>(count: u64) -> result: u64 pure contract {{
  requires count <= {upper}_u64;
  ensures result == count;
}} {{
  let produced = allocate::<{stored}>(count: count);
  return produced;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#
            )
        };

        let fitting = source(limit);
        assert_accepts(fitting.as_bytes());

        let excessive = source(limit + 1);
        assert_rule_kind(excessive.as_bytes(), SemanticRule::Fn9, |kind| {
            matches!(
                kind,
                SemanticIssueKind::UndischargedPostcondition(detail)
                    if detail.disposition
                        == crate::PostconditionProofDisposition::Unproved
            )
        });
    }
}

/// An `Int` or `Float` bound fixes the largest scalar layout at eight bytes,
/// so its schema has an expressible exact OP-9 limit. The last admitted count
/// is accepted and the following count remains a schema rejection.
#[test]
fn bounded_numeric_layouts_keep_their_exact_symbolic_op9_limit() {
    const LIMIT: u64 = u64::MAX / 8;
    for bound in ["Int", "Float"] {
        let source = format!(
            "fn allocate<T: {bound}>(count: u64) -> result: unit pure contract {{\n  requires count <= {LIMIT}_u64;\n}} {{\n  let cells = box_slots_new::<T>(capacity: count);\n  free_empty(window: move cells);\n  return unit;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        assert_accepts(source.as_bytes());

        let too_large = LIMIT + 1;
        let source = format!(
            "fn allocate<T: {bound}>(count: u64) -> result: unit pure contract {{\n  requires count <= {too_large}_u64;\n}} {{\n  let cells = box_slots_new::<T>(capacity: count);\n  free_empty(window: move cells);\n  return unit;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        assert_op9_allocation_fit(
            source.as_bytes(),
            &format!("{bound}-bounded schema above its eight-byte limit"),
        );
    }
}

/// A by-value array with a symbolic length has no schema layout ceiling, but
/// every concrete replay recomputes it. A small array admits one stored value;
/// an array whose concrete stride is AboveU64 rejects that same count.
#[test]
fn symbolic_const_array_layout_defers_only_until_concrete_replay() {
    let schema = br#"fn allocate<const n: u64>(count: u64) -> result: unit pure contract {
  requires count <= 1_u64;
} {
  let cells = box_slots_new::<Array<u64, n>>(capacity: count);
  free_empty(window: move cells);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(schema);

    let small = br#"fn allocate<const n: u64>(count: u64) -> result: unit pure contract {
  requires count <= 1_u64;
} {
  let cells = box_slots_new::<Array<u64, n>>(capacity: count);
  free_empty(window: move cells);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  allocate::<4>(count: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(small);

    let above_u64 = br#"fn allocate<const n: u64>(count: u64) -> result: unit pure contract {
  requires count <= 1_u64;
} {
  let cells = box_slots_new::<Array<u64, n>>(capacity: count);
  free_empty(window: move cells);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  allocate::<2305843009213693952>(count: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_op9_allocation_fit(above_u64, "AboveU64 concrete const-array replay");
}

/// [OP-9] `AboveU64` is a concrete layout ceiling whose allocation limit is
/// zero, not an unresolved layout. A generic schema may defer opaque `T`, but
/// replaying it with this aggregate must still refuse every positive count.
#[test]
fn a_concrete_above_u64_stride_does_not_defer_op9() {
    let source = br#"struct Giant {
  words: Array<u64, 2305843009213693952>;
}

fn allocate<T>(count: u64) -> result: unit pure contract {
  requires count <= 1_u64;
} {
  let cells = box_slots_new::<T>(capacity: count);
  free_empty(window: move cells);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  allocate::<Giant>(count: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_op9_allocation_fit(source, "AboveU64 concrete aggregate replay");
}

/// [OP-14] `free_empty` consumes any window proved empty, an affine element
/// type and a linear one alike, and its boxed argument frees the cell with it.
#[test]
fn free_empty_consumes_a_proved_empty_window() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op14-pos-free-empty-empty-window.wf"
    ));
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op14-pos-free-empty-boxed-window.wf"
    ));
}

/// [OP-14] an undischarged `requires window.len == 0_u64` is a hard error at
/// the complete `call`, rendering the residual.
#[test]
fn free_empty_of_a_nonempty_window_is_refused() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/op14-neg-free-empty-nonempty-window.wf"
    );
    assert_rule_kind(source, SemanticRule::Op14, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedEmptyRunRelease { .. })
    });
}

/// [OP-14] applies to both window shapes, static and runtime capacities, and
/// copy and linear elements. Direct static shapes and boxed runtime shapes are
/// admitted, and every admitted operand must have proved current length zero.
#[test]
fn free_empty_uses_the_current_length_for_every_window_shape() {
    for source in [
        br#"fn main() -> status: std::process::ExitStatus pure {
  let window = box_slots_new::<u8>(capacity: 2_u64);
  free_empty(window: move window);
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"fn main() -> status: std::process::ExitStatus pure {
  let window = box_ring_new::<u8>(capacity: 2_u64);
  free_empty(window: move window);
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"fn main() -> status: std::process::ExitStatus pure {
  let slots = slots_new::<u8, 4>();
  free_empty(window: move slots);
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"fn main() -> status: std::process::ExitStatus pure {
  let ring = ring_new::<u8, 4>();
  free_empty(window: move ring);
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"nodrop enum Ticket {
  Mark();
}

fn main() -> status: std::process::ExitStatus pure {
  let window = box_slots_new::<Ticket>(capacity: 2_u64);
  let ticket = Ticket::Mark();
  place_back(window: &window.inner, value: move ticket);
  let taken = take_back(window: &window.inner);
  match move taken {
    Mark() => {
      free_empty(window: move window);
      return std::process::exit_status(code: 0_u8);
    }
  }
}
"#
        .as_slice(),
    ] {
        assert_accepts(source);
    }

    for source in [
        br#"fn main() -> status: std::process::ExitStatus pure {
  let window = box_slots_new::<u8>(capacity: 4_u64);
  place_back(window: &window.inner, value: 7_u8);
  free_empty(window: move window);
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"nodrop struct Token {
  value: u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = box_slots_new::<Token>(capacity: 1_u64);
  let token = Token(value: 7_u8);
  place_back(window: &window.inner, value: move token);
  free_empty(window: move window);
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"fn main() -> status: std::process::ExitStatus pure {
  let window = box_ring_new::<Box<u8>>(capacity: 4_u64);
  let value = box_new::<u8>(value: 7_u8);
  place_back(window: &window.inner, value: move value);
  free_empty(window: move window);
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"fn release(window: Box<Slots<u8>>) -> result: unit pure {
  free_empty(window: move window);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
    ] {
        assert_rule_kind(source, SemanticRule::Op14, |kind| {
            matches!(kind, SemanticIssueKind::UndischargedEmptyRunRelease { .. })
        });
    }

    for source in [
        br#"fn main() -> status: std::process::ExitStatus pure {
  let slots = slots_new::<u8, 4>();
  let window = box_new::<Slots<u8, 4>>(value: move slots);
  free_empty(window: move window);
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
        br#"fn main() -> status: std::process::ExitStatus pure {
  let ring = ring_new::<u8, 4>();
  let window = box_new::<Ring<u8, 4>>(value: move ring);
  free_empty(window: move window);
  return std::process::exit_status(code: 0_u8);
}
"#
        .as_slice(),
    ] {
        // The shape refusal carries its repair [OP-14, DIAG-1].
        assert_rule_kind(source, SemanticRule::Op14, |kind| {
            matches!(kind, SemanticIssueKind::UnadmittedOperandShape { .. })
        });
    }
}

/// [TYPE-9] the three shapes have two placements each: constant capacity
/// inline in the owner, runtime capacity only as `Box` content.
#[test]
fn the_two_placements_are_admitted_where_the_rule_admits_them() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/type9-pos-constant-and-runtime-capacity-placement.wf"
    ));
}

/// [TYPE-9] a runtime-capacity form outside a `Box` is a hard error at the
/// complete `type`.
#[test]
fn a_runtime_capacity_shape_outside_a_box_is_refused() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/type9-neg-runtime-capacity-outside-box.wf"
    );
    assert_rule_kind(source, SemanticRule::Type9, |kind| {
        matches!(kind, SemanticIssueKind::TypeMismatch { .. })
    });
}

/// [TYPE-2] each shape is one of the prelude's opaque structs now, and an
/// opaque struct's constructor entry exists to be refused: a shape is built by
/// a construction function [OP-13], never by a constructor `call`.
#[test]
fn a_compiler_owned_constructor_call_is_refused() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/type2-neg-storage-shape-constructor-call.wf"
    );
    assert_rule_kind(source, SemanticRule::Type2, |kind| {
        matches!(kind, SemanticIssueKind::ContainerConstruction { .. })
    });
}

/// [TYPE-2] a `Box` constructor `call` is refused in the same words, its
/// content being supplied by `box_new` and its friends.
#[test]
fn a_box_constructor_call_is_refused() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/type2-neg-box-constructor-call.wf");
    assert_rule_kind(source, SemanticRule::Type2, |kind| {
        matches!(kind, SemanticIssueKind::ContainerConstruction { .. })
    });
}

/// [TYPE-9] the content is reached by the ordinary field step `b.inner`, and
/// `let n = move b.inner;` consumes the `Box`, yields its content and frees
/// the cell.
#[test]
fn box_content_is_read_written_and_unboxed_through_its_field() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/type9-pos-box-content-read-write-unbox.wf"
    ));
}

/// [TYPE-9] a `move` of a runtime-capacity content is a hard error: the `Box`
/// must release it at scope exit, or the window must be emptied and the cell
/// consumed by `free_empty`.
#[test]
fn a_move_of_runtime_capacity_content_is_refused() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/type9-neg-move-runtime-capacity-content.wf"
    );
    assert_rule_kind(source, SemanticRule::Type9, |kind| {
        matches!(kind, SemanticIssueKind::InlineRuntimeCapacityShape { .. })
    });
}

/// [STOR-8] a compilation unit carrying the no-heap declaration cannot name
/// `Box` or a runtime-capacity shape, and cannot call an allocating row.
#[test]
fn a_no_heap_unit_names_no_box_and_calls_no_allocating_row() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/stor8-pos-no-heap-program.wf"
    ));
    let named =
        include_bytes!("../../../../tests/conformance/cases/stor8-neg-no-heap-names-box.wf");
    assert_rule_kind(named, SemanticRule::Stor8, |kind| {
        matches!(kind, SemanticIssueKind::HeapTypeUnderNoHeap { .. })
    });
    let called = include_bytes!(
        "../../../../tests/conformance/cases/stor8-neg-no-heap-calls-allocating-row.wf"
    );
    assert_rule_kind(called, SemanticRule::Stor8, |kind| {
        matches!(kind, SemanticIssueKind::HeapTypeUnderNoHeap { .. })
    });
}

/// [STOR-8] allocation is total in the source: it never returns a failure and
/// no construction carries a `Result`.
#[test]
fn allocation_is_total() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/stor8-pos-allocation-is-total.wf"
    ));
}

/// [OP-11] `swap` exchanges the values at two owned places of one type,
/// consumes neither root, and admits its two arguments naming one place.
#[test]
fn swap_exchanges_two_owned_places() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op11-pos-swap-exchanges-values.wf"
    ));
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/op11-pos-swap-same-place-admission.wf"
    ));
}

/// [OP-11] a `swap` over a copy place is a hard error at the first
/// `borrow_expr`; the restructuring is to read the two values and assign them
/// back.
#[test]
fn swap_over_a_copy_place_is_refused() {
    let source = include_bytes!("../../../../tests/conformance/cases/op11-neg-swap-copy-place.wf");
    assert_rule_kind(source, SemanticRule::Op11, |kind| {
        matches!(kind, SemanticIssueKind::SwapOverCopyPlace { .. })
    });
}
