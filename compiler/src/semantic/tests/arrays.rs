//! The two inline storage shapes a source program writes without a cell
//! [TYPE-9]: the const-eligible `Array<T, N>`, whose `len` and `cap` are both
//! the type constant, and the window `Slots<T, N>`, whose `len` is a runtime
//! number its block stores [WIN-1, MSR-1].
//!
//! v0.60 renamed both: `array<T, n>` is `Array<T, N>` and `FixedVector<T, n>`
//! is `Slots<T, N>`, the constructions are the [OP-13] records
//! `array_filled`, `slots_new` and `slots_from_array`, a measure is the place
//! form `p.len` [OP-15] rather than the retired `len_of(p)` former, and a
//! window's boundary moves only through the [OP-10] operations.

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::model::{
    CheckedConst, CheckedContainerRoot, CheckedExpression, CheckedPlaceStep, CheckedSetTarget,
    CheckedStatement, CheckedTargetDomainObligation, CheckedType, CheckedValue, IntegerType,
    WindowShape,
};
use super::{assert_rule, assert_rule_kind, with_semantics};

/// [CONST-2] a const is pure static rodata: `enum`, `Box`, `Slots` and `Ring`
/// are not const-eligible, and neither is a runtime-capacity `Array<T>`,
/// whose capacity is fixed at a construction a const never performs.
///
/// v0.59 answered a nested `FixedVector` const with a capability stop, because
/// the run form was eligible and its element was not implemented. The rule now
/// decides it: a window is refused at the written type, whatever its element.
#[test]
fn nested_window_constants_are_a_const2_eligibility_rejection() {
    assert_rule(
        br#"const rows: Array<Slots<u64, 2>, 1> =[[7_u64, 9_u64]];

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Const2,
        SemanticIssueKind::InvalidConstValue,
    );
}

/// A named const is immutable and is read by subscript, measure member, field
/// suffix, or `&` reference [CONST-2].
///
/// Two v0.59 cases retire with their subjects. `let old = replace rows[..] =
/// 5_u64;` was [SET-2]'s statement, which is gone: [SET-1] writes the place
/// and [WIN-3] owns the old value's disposition. `&uniq rows` and
/// `&uniq rows[0_u64]` were the permission marker, which is gone: there is no
/// marker on a reference [REF-1], and [CONST-2] admits reading a const
/// through `&` so that a const table may be passed to a consumer.
/// `set deref(target)[..] = 5_u64;` through a reference to a const cited
/// [OWN-5], whose successors are [REF-1], [REF-2] and [EFF-5]; none of the
/// three states a const-target rule of its own, so the case retires here and
/// the writability of a const root stays [CONST-2]'s, asserted directly
/// above.
#[test]
fn constant_typed_places_remain_immutable_and_proof_checked() {
    let prefix = "const rows: Array<Array<u64, 2>, 1> =[[7_u64, 9_u64]];\n\n";
    let source = format!(
        "{prefix}fn main() -> status: std::process::ExitStatus pure {{\n  set rows[0_u64][1_u64] = 5_u64;\n  return std::process::exit_status(code: 0_u8);\n}}\n"
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Const2, |kind| {
        matches!(kind, SemanticIssueKind::ImmutableSetTarget)
    });
    for (action, rule) in [
        ("let taken = move rows;", SemanticRule::Own1),
        // Structural copy makes this array element a copy place, so [OWN-1]
        // refuses the unnecessary `move` before [WIN-3] is relevant.
        ("let taken = move rows[0_u64];", SemanticRule::Own1),
        ("let taken = move rows[0_u64][0_u64];", SemanticRule::Own1),
        ("let value = rows[1_u64][0_u64];", SemanticRule::Op4),
    ] {
        let source = format!(
            "{prefix}fn main() -> status: std::process::ExitStatus pure {{\n  {action}\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("{action}: {outcome:?}");
            };
            assert_eq!(issue.rule(), rule, "{action}: {issue:?}");
        });
    }
}

/// [CONST-2] a const is read through a `&` reference, so a const table may be
/// passed to a consumer, and the reference's referent is the const's own
/// storage [REF-1].
///
/// This replaces `normalized_constant_run_storage_is_not_an_array_borrow`,
/// whose subject was the v0.59 `FixedVector` const form's dense array backing
/// and whose assertion was the retired
/// `UnsupportedSemanticFeature::RegionsAndBorrows`. A window is not
/// const-eligible in v0.60 [CONST-2], so there is no normalization left to
/// distinguish and the surviving question is that the const is readable
/// through the reference at all.
#[test]
fn a_const_array_is_read_through_a_reference() {
    with_semantics(
        br#"const table: Array<u64, 2> =[7_u64, 9_u64];

fn read(values: &Array<u64, 2>) -> result: u64 reads(values) {
  return deref(values)[1_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  let value = read(values: &table);
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "a const table is passed to a consumer by reference: {outcome:?}"
            );
        },
    );
}

#[test]
fn constant_array_entries_follow_the_type_directed_value_shapes() {
    // An `Array<T, N>`-typed const takes `[cvalue, ..., cvalue]` with exactly
    // N entries, each of type T [CONST-2]; an IDENT naming an earlier
    // composite const is not one of those entry shapes.
    assert_rule_kind(
        br#"const inner: Array<u64, 2> =[7_u64, 9_u64];

const rows: Array<Array<u64, 2>, 1> =[inner];

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Const2,
        |kind| matches!(kind, SemanticIssueKind::InvalidConstValue),
    );
    with_semantics(
        br#"const scalar: u64 = 7_u64;

const rows: Array<u64, 1> =[scalar];

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "primitive entry references remain valid: {outcome:?}"
            )
        },
    );
}

#[test]
fn generic_struct_constants_prepare_their_own_instances() {
    for preparation in ["", "struct Holder {\n  pair: Pair<u64>;\n}\n\n"] {
        let source = format!(
            "struct Pair<T: copy> {{\n  left: T;\n  right: T;\n}}\n\n{preparation}const pair: Pair<u64> = Pair<u64>(left: 3_u64, right: 2_u64);\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("a constant needs no unrelated instance seed: {outcome:?}");
            };
            let constant = &checked.data.constants[0];
            let CheckedValue::Struct { ty, fields } = &constant.value else {
                panic!("the constant must retain its ordinary struct representation");
            };
            assert_eq!(*ty, constant.declared_type);
            assert!(matches!(
                fields.as_slice(),
                [
                    CheckedValue::Integer {
                        ty: IntegerType::U64,
                        bits: 3
                    },
                    CheckedValue::Integer {
                        ty: IntegerType::U64,
                        bits: 2
                    },
                ]
            ));
        });
    }
}

#[test]
fn generic_struct_constants_match_phantom_arguments_and_require_explicit_lists() {
    let declarations =
        "struct Marker<T, const n: u64> {\n}\n\nstruct AlternativeMarker<T, const n: u64> {\n}\n\n";
    for (initializer, rule) in [
        ("Marker<u64, 3>()", None),
        ("Marker<u32, 3>()", Some(SemanticRule::Const2)),
        ("Marker<u64, 4>()", Some(SemanticRule::Const2)),
        ("AlternativeMarker<u64, 3>()", Some(SemanticRule::Const2)),
        ("Marker()", Some(SemanticRule::Type5)),
        ("Marker<u64>()", Some(SemanticRule::Type5)),
        ("Marker<u64, 3, 4>()", Some(SemanticRule::Type5)),
    ] {
        let source = format!(
            "{declarations}const marker: Marker<u64, 3> = {initializer};\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        if let Some(rule) = rule {
            super::assert_rule_at(source.as_bytes(), rule, initializer);
        } else {
            super::assert_accepts(source.as_bytes());
        }
    }
    let source = format!(
        "{declarations}const marker: Marker<u64, 3> = Marker<3, u64>();\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
    );
    super::assert_rule_at(source.as_bytes(), SemanticRule::Type5, "3");
}

#[test]
fn generic_struct_constants_check_instantiated_fields_and_bounds() {
    let declarations = "struct Pair<T: copy> {\n  left: T;\n  right: T;\n}\n\n";
    for (initializer, rule) in [
        ("Pair<u64>(left: 3_u64, right: 2_u32)", SemanticRule::Const2),
        ("Pair<u64>(right: 2_u64, left: 3_u64)", SemanticRule::Gram8),
        ("Pair<u64>(left: 3_u64)", SemanticRule::Gram8),
        (
            "Pair<u64>(left: 3_u64, right: 2_u64, extra: 1_u64)",
            SemanticRule::Gram8,
        ),
        (
            "Pair<Box<u64>>(left: 3_u64, right: 2_u64)",
            SemanticRule::Prov6,
        ),
    ] {
        let source = format!(
            "{declarations}const pair: Pair<u64> = {initializer};\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("{initializer} must be a source rejection: {outcome:?}");
            };
            assert_eq!(issue.rule(), rule, "{initializer}: {issue:?}");
        });
    }
}

#[test]
fn generic_struct_constant_eligibility_follows_fields_not_phantom_arguments() {
    let declarations = "enum Flag {\n  On();\n  Off();\n}\n\nopaque struct Token {\n}\n\nstruct Cell<T> {\n  value: T;\n}\n\nstruct Phantom<T> {\n}\n\n";
    for argument in [
        "Box<u64>",
        "Slots<u64, 2>",
        "Ring<u64, 2>",
        "Flag",
        "Token",
        "Array<Box<u64>, 0>",
    ] {
        let ty = format!("Cell<{argument}>");
        let source = format!(
            "{declarations}const cell: {ty} = {ty}(value: unit);\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        super::assert_rule_at(source.as_bytes(), SemanticRule::Const2, &ty);
    }
    let source = format!(
        "{declarations}const marker: Phantom<Box<u64>> = Phantom<Box<u64>>();\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
    );
    super::assert_accepts(source.as_bytes());
}

#[test]
fn generic_struct_constants_preserve_function_arguments() {
    let source = br#"interface Read<T: copy> {
  fn read(value: &T) -> result: T reads(value);
}

fn read_value(value: &u64) -> result: u64 reads(value) {
  return deref(value);
}

binding ReadWord : Read<u64> {
  read = read_value;
}

struct Cell<interface Read<T>> {
  value: T;
}

const cell: Cell<ReadWord> = Cell<ReadWord>(value: 7_u64);

fn main() -> status: std::process::ExitStatus pure {
  let value = cell.value;
  return std::process::exit_status(code: 0_u8);
}
"#;
    super::assert_accepts(source);
}

#[test]
fn constant_array_eligibility_closes_recursive_types_before_checking_the_value() {
    let source = br#"struct Recursive {
  children: Array<Recursive, 0>;
}

const invalid: Recursive = Recursive(children:[unit]);

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("finite malformed constant must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Const2);
        assert_eq!(issue.kind(), &SemanticIssueKind::InvalidConstValue);
        let crate::SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        assert_eq!(
            &source[start..end],
            b"[unit]",
            "the initializer has one entry for a zero-extent field"
        );
    });
    let forbidden = br#"const forbidden: Array<Array<Box<u64>, 0>, 1> =[unit];

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(forbidden, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("zero extent does not erase forbidden elements: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Const2);
        assert_eq!(issue.kind(), &SemanticIssueKind::InvalidConstValue);
        let crate::SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        assert_eq!(&forbidden[start..end], b"Box<u64>");
    });
}

/// [OP-13] `slots_into_array` consumes a window whose `len` equals its `cap`,
/// and that `requires` is an ordinary [FN-8] call requirement rather than the
/// retired [BLK-0] kernel-row requirement. The linear obligation travels with
/// the value across the conversion in both directions [PROV-6].
#[test]
fn full_array_conversion_requires_fullness_and_preserves_linear_obligations() {
    assert_rule_kind(
        br#"fn main() -> status: std::process::ExitStatus pure {
  let partial = slots_new::<u64, 2>();
  place_back(window: &partial, value: 7_u64);
  let invalid = slots_into_array::<u64, 2>(values: move partial);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Fn8,
        |kind| matches!(kind, SemanticIssueKind::UndischargedCallRequirement(_)),
    );
    assert_rule_kind(
        br#"nodrop struct Token {
  value: u64;
}

fn abandon(values: Array<Token, 0>) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Prov6,
        |kind| matches!(kind, SemanticIssueKind::LinearValueNotConsumed { .. }),
    );
    with_semantics(
        br#"nodrop struct Token {
  value: u64;
}

fn convert(values: Array<Token, 0>) -> result: Slots<Token, 0> pure {
  return slots_from_array::<Token, 0>(values: move values);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "whole conversion preserves the zero-extent obligation: {outcome:?}"
            )
        },
    );
}

// Retired with its subject: `general_array_views_remain_an_explicit_capability_gap`
// formed a view with `slice_of(&values)` over an array of arrays and asserted
// the capability stop that answered it. Views are retired: [VIEW-1], [VIEW-2],
// [VIEW-4] and [VIEW-6] become [REF-4]'s range references and [CALL-3], a
// range is formed by `&x[lo..hi]`, and its own formation obligations and
// placement restrictions belong to the range-reference suite rather than to
// the array shapes this module owns.

/// [EFF-1] a by-value parameter has no effect entry at all: the call site
/// records the consumption of a `move` argument or the read of a copy one
/// [EFF-5], so an element read through an own parameter exhibits nothing and
/// the declaration writes `pure`. A reference parameter's read is attributed
/// to the path the reference names [EFF-2].
///
/// v0.59 required `reads(values)` on the own parameter and asserted the
/// [EFF-2] mismatch that omitting it produced. The same spelling is now an
/// [EFF-1] row rejection, so both directions moved.
#[test]
fn incoming_array_element_reads_exhibit_the_resolved_formal_effect() {
    with_semantics(
        br#"fn read(values: Array<u64, 2>) -> result: u64 pure {
  return values[0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "an own parameter's element read exhibits nothing: {outcome:?}"
            )
        },
    );
    assert_rule_kind(
        br#"fn read(values: Array<u64, 2>) -> result: u64 reads(values) {
  return values[0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Eff1,
        |kind| matches!(kind, SemanticIssueKind::InvalidEffectRow { .. }),
    );
    with_semantics(
        br#"fn read(values: &Array<u64, 2>) -> result: u64 reads(values) {
  return deref(values)[0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "exact incoming read row through a reference: {outcome:?}"
            )
        },
    );
}

#[test]
fn full_array_elements_preserve_cells_through_generic_replay_and_reference_reads() {
    let source = br#"struct Record {
  value: u64;
  owner: Box<u64>;
}

fn pass<T>(value: T) -> result: T pure {
  return move value;
}

fn relay(values: Array<Box<u64>, 2>) -> result: Array<Box<u64>, 2> pure {
  return pass::<Array<Box<u64>, 2>>(value: move values);
}

fn read(values: &Array<Record, 2>) -> result: u64 reads(values) {
  return deref(values)[0_u64].value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("full owning array type graph: {outcome:?}");
        };
        let relay = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "relay")
            .expect("relay");
        assert_eq!(relay.parameters[0].ty, relay.result);
        let CheckedType::Array {
            element,
            length: CheckedConst::Value(2),
        } = relay.result
        else {
            panic!("relay must retain its full array result");
        };
        let Some(CheckedType::Nominal(owner)) = checked.element_type(element) else {
            panic!("array must retain its owner element");
        };
        assert!(matches!(
            checked.data.nominals[owner.0 as usize].kind,
            super::super::model::CheckedNominalKind::Box { .. }
        ));
        let pass = checked
            .data
            .functions
            .iter()
            .find(|function| function.name.starts_with("pass"))
            .expect("instantiated pass");
        assert_eq!(pass.parameters[0].ty, relay.result);
        assert_eq!(pass.result, relay.result);
        crate::lower_checked(*checked, crate::lowering::OverlapLowering::Off)
            .expect("full array reference read and relay lower");
    });
}

/// An array's slots are inline in its owner [STOR-1], so an element type of
/// unknown extent has no layout and a zero-extent one has no layout edge.
///
/// v0.59 also swept `Slice<'r, u8>`, `Heap<'r>` and `Arena<'r, 64, 8>` as
/// array content and asserted [STOR-5]'s `RegionBearingStorage`. Views,
/// providers and arenas are retired with regions, and [STOR-5]'s surviving
/// half — storage is reference-free — is closed by [TYPE-8] in the grammar,
/// which admits no reference kind in an element or type-argument position at
/// all, so no source program reaches that judgment.
#[test]
fn full_arrays_keep_their_inline_layout_boundaries() {
    super::assert_unsupported(
        b"struct Recursive {\n  values: Array<Recursive, 1>;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        crate::UnsupportedSemanticFeature::RecursiveNominalLayout,
    );
    with_semantics(
        b"struct Empty {\n  values: Array<Empty, 0>;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else { panic!("zero extent has no recursive layout edge: {outcome:?}"); };
            crate::lower_checked(*checked, crate::lowering::OverlapLowering::Off).expect("zero extent recursive type lowers");
        },
    );
}

/// [TYPE-9, MSR-1] a cell's content is the field `inner`, and a measure over
/// it is `b.inner.len`. The checked path retains the `Box` step so lowering
/// follows the allocation rather than measuring the pointer slot or a copied
/// window value.
///
/// v0.59 wrote the same place as `len_of(deref(block))`. Both the former and
/// the `deref` route to a cell are retired [OP-15, TYPE-7].
#[test]
fn a_cell_content_is_an_admitted_measured_place() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let one = slots_new::<u8, 4>();
  let block = box_new::<Slots<u8, 4>>(value: move one);
  let capacity = block.inner.cap;
  let length = block.inner.len;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a cell's content is a measure place: {outcome:?}");
        };
        let body = checked.data.functions[0].body.as_deref().expect("WF body");
        let CheckedStatement::Let {
            value: CheckedExpression::ContainerMeasure { root, .. },
            ..
        } = &body[2]
        else {
            panic!("capacity must lower as a container measure");
        };
        assert!(matches!(
            root.path.as_slice(),
            [CheckedPlaceStep::BoxReferent(_)]
        ));
    });
}

// Retired with its subject: `a_bare_owned_box_still_requires_explicit_dereference_to_measure_its_referent`
// asserted [TYPE-7]'s implicit read through a cell holder, `cap_of(block)`
// against `cap_of(deref(block))`. v0.60 retires the `deref` route to a cell
// outright — a cell's content is its field `inner` [TYPE-9] and `deref` of a
// `Box` is itself a TYPE-7 rejection — so the successor case is
// `cells::deref_of_a_cell_is_a_type7_rejection_naming_the_field_inner`, and
// the positive `block.inner.cap` is asserted directly above.

/// A fact about the old content must die when the whole cell owner is
/// assigned over. Otherwise its old nonempty length could authorize an
/// element read from the new empty window.
///
/// v0.59 wrote the whole-owner write as `let previous = replace block = move
/// fresh;`. [SET-2] is retired: [SET-1] writes the place and [WIN-3] releases
/// the displaced affine value.
#[test]
fn assigning_an_owned_cell_kills_its_contents_old_measure() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let old_run = slots_new::<u8, 1>();
  place_back(window: &old_run, value: 7_u8);
  let block = box_new::<Slots<u8, 1>>(value: move old_run);
  let fresh_run = slots_new::<u8, 1>();
  let fresh = box_new::<Slots<u8, 1>>(value: move fresh_run);
  let old_length = block.inner.len;
  let in_old = 0_u64 < old_length;
  if in_old {
    set block = move fresh;
    let invalid = block.inner[0_u64];
    return std::process::exit_status(code: invalid);
  } else {
    return std::process::exit_status(code: 3_u8);
  }
}
"#;
    assert_rule_kind(source, SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

/// A const keeps the array place as its storage type — N exact constants over
/// N slots, materialized from the type [CONST-2, MSR-1] — so a const's checked
/// type is `CheckedType::Array` and its subscript uses `ReadStorage` rooted at
/// the constant. A constructed `Slots<T, N>` is a window whose capacity is
/// standing and whose `len`, `room` and `head` are runtime numbers its block
/// stores, so a built window's measure is `ContainerMeasure`, its subscript is
/// `ReadStorage`, and its indexed commit is `CheckedSetTarget::Storage`, each
/// retaining the complete typed projection.
#[test]
fn constants_fill_length_and_index_share_exact_run_types() {
    let source = br#"const count: u64 = 4_u64;

const table: Array<u8, count> =[10_u8, 20_u8, 30_u8, 40_u8];

fn main() -> status: std::process::ExitStatus pure {
  let base = array_filled::<i32, count>(value: 7_i32);
  let values = slots_from_array::<i32, count>(values: base);
  let length = values.len;
  let local = values[1_u64];
  let stored = table[2_u64];
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("window family must check: {outcome:?}");
        };
        assert_eq!(checked.data.constants.len(), 2);
        assert!(matches!(
            checked.data.constants[1].ty,
            CheckedType::Array {
                element,
                length: CheckedConst::Value(4),
            } if checked.element_type(element) == Some(CheckedType::Integer(IntegerType::U8))
        ));
        let CheckedValue::Array { elements, .. } = &checked.data.constants[1].value else {
            panic!("table must retain its complete checked initializer");
        };
        assert_eq!(elements.len(), 4);

        let body = &checked.data.functions[0].body.as_deref().expect("WF body");
        assert!(matches!(
            &body[2],
            CheckedStatement::Let {
                value: CheckedExpression::ContainerMeasure {
                    root: CheckedContainerRoot {
                        ty: CheckedType::Window {
                            shape: WindowShape::Slots,
                            element,
                            capacity: Some(CheckedConst::Value(4)),
                        },
                        ..
                    },
                    ..
                },
                ..
            } if checked.element_type(*element) == Some(CheckedType::Integer(IntegerType::I32))
        ));
        assert!(matches!(
            &body[3],
            CheckedStatement::Let {
                value: CheckedExpression::ReadStorage {
                    root: CheckedContainerRoot {
                        ty: CheckedType::Integer(IntegerType::I32),
                        path,
                        ..
                    },
                    ..
                },
                ..
            } if matches!(path.as_slice(), [CheckedPlaceStep::Subscript(index)]
                if matches!(index.base_type, CheckedType::Window { capacity: Some(CheckedConst::Value(4)), .. })
                && index.target_domain == CheckedTargetDomainObligation::ElementAddress
                && !index.obligation.components().is_empty())
        ));
        assert!(matches!(
            &body[4],
            CheckedStatement::Let {
                value: CheckedExpression::ReadStorage {
                    root: CheckedContainerRoot {
                        root: super::super::places::PlaceRoot::Constant(_),
                        ty: CheckedType::Integer(IntegerType::U8),
                        path,
                    },
                    ..
                },
                ..
            } if matches!(path.as_slice(), [CheckedPlaceStep::Subscript(index)]
                if matches!(index.base_type, CheckedType::Array { length: CheckedConst::Value(4), .. })
                && index.target_domain == CheckedTargetDomainObligation::ElementAddress
                && !index.obligation.components().is_empty())
        ));
    });
}

#[test]
fn const_expression_and_const_value_failures_keep_their_rule_owners() {
    assert_rule(
        include_bytes!("../../../../tests/conformance/cases/const1-neg-noninteger.wf"),
        SemanticRule::Const1,
        SemanticIssueKind::InvalidConstValue,
    );
    assert_rule(
        b"const table: Array<u8, 2> =[1_u8];\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Const2,
        SemanticIssueKind::InvalidConstValue,
    );
    assert_rule(
        include_bytes!("../../../../tests/conformance/cases/const2-neg-noneligible.wf"),
        SemanticRule::Const2,
        SemanticIssueKind::InvalidConstValue,
    );
    assert_rule(
        b"struct Cell {\n  value: i32;\n}\n\nconst bad: Cell = unit;\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Const2,
        SemanticIssueKind::InvalidConstValue,
    );
    assert_rule(
        include_bytes!("../../../../tests/conformance/cases/const2-neg-set.wf"),
        SemanticRule::Const2,
        SemanticIssueKind::ImmutableSetTarget,
    );
    assert_rule_kind(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let items = slots_new::<u8, 2>();\n  let value = items[0_u32];\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

#[test]
fn named_lengths_and_tag_only_enum_elements_work_in_nominal_layouts() {
    let source = br#"const count: u64 = 2_u64;

enum Flag {
  Off();
  On();
}

struct Holder {
  flags: Slots<Flag, count>;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!(
                "nominal window fields must use earlier lengths and completed enum layouts: {outcome:?}"
            );
        };
        let super::super::model::CheckedNominalKind::Struct { fields } =
            &checked.data.nominals[1].kind
        else {
            panic!("Holder must remain a struct");
        };
        let CheckedType::Window {
            shape: WindowShape::Slots,
            element,
            capacity,
        } = fields[0].ty
        else {
            panic!("field must be a constant-capacity window");
        };
        assert_eq!(capacity, Some(CheckedConst::Value(2)));
        assert_eq!(
            checked.element_type(element),
            Some(CheckedType::Nominal(checked.data.nominals[0].id))
        );
    });

    // TYPE-2 admits complete owning elements in full arrays too; the former
    // flat-only rejection is superseded by that explicit amendment.
    with_semantics(
        b"enum Payload {\n  Item(value: i32);\n}\n\nstruct Holder {\n  values: Array<Payload, 2>;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| assert!(matches!(outcome, SemanticOutcome::Complete(_)), "owning array member: {outcome:?}"),
    );
}

#[test]
fn indexed_set_retains_its_pre_rhs_guard_and_copy_target() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let base = array_filled::<u8, 2>(value: 0_u8);
  let values = slots_from_array::<u8, 2>(values: base);
  set values[1_u64] = 9_u8;
  let stored = values[1_u64];
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("indexed window set must check: {outcome:?}");
        };
        let CheckedStatement::Set { target, .. } =
            &checked.data.functions[0].body.as_deref().expect("WF body")[2]
        else {
            panic!("third statement must be the indexed set");
        };
        let CheckedSetTarget::Storage(target) = target else {
            panic!("indexed set must retain a window-index target");
        };
        let [CheckedPlaceStep::Subscript(index)] = target.path.as_slice() else {
            panic!("the complete target must retain its subscript");
        };
        let CheckedType::Window {
            shape: WindowShape::Slots,
            element,
            capacity,
        } = index.base_type
        else {
            panic!("index base must be a constant-capacity window");
        };
        assert_eq!(capacity, Some(CheckedConst::Value(2)));
        assert_eq!(
            checked.element_type(element),
            Some(CheckedType::Integer(IntegerType::U8))
        );
        assert_eq!(target.ty, CheckedType::Integer(IntegerType::U8));
        assert_eq!(index.offset.ty(), CheckedType::Integer(IntegerType::U64));
        assert!(!index.obligation.components().is_empty());
        assert_eq!(
            index.target_domain,
            CheckedTargetDomainObligation::ElementAddress
        );
    });
}

#[test]
fn indexed_set_rechecks_type_effect_and_root_liveness() {
    // A discharged subscript adds no runtime effect: the indexed set with a
    // constant in-range offset is accepted in a `pure` function.
    with_semantics(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let base = array_filled::<u8, 2>(value: 0_u8);\n  let values = slots_from_array::<u8, 2>(values: base);\n  set values[0_u64] = 1_u8;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "a discharged indexed set must remain pure: {outcome:?}"
            );
        },
    );
    assert_rule_kind(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let base = array_filled::<u8, 2>(value: 0_u8);\n  let values = slots_from_array::<u8, 2>(values: base);\n  set values[0_u64] = 1_u16;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
    assert_rule(
        b"fn consume(values: Slots<u8, 2>) -> result: u8 pure {\n  return 1_u8;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let base = array_filled::<u8, 2>(value: 0_u8);\n  let values = slots_from_array::<u8, 2>(values: base);\n  set values[0_u64] = consume(values: move values);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Own1,
        SemanticIssueKind::UseAfterMove {
            mechanical_fix: "introduce a new `let` binding before reuse",
        },
    );
}

/// A window reached through struct fields keeps its complete path in the
/// checked target and the checked read.
///
/// The third v0.59 case held a shared borrow of the whole owner across an
/// indexed write and asserted [OWN-5]'s `BorrowConflict`. That rule is
/// retired: [REF-1] gives a reference a path and nothing else, and the only
/// reference-use rejection left is the use of an invalid one [REF-2]. A write
/// below the referenced whole owner is a content write and does not invalidate
/// that reference, so the case retains a use after the indexed write.
#[test]
fn nested_struct_run_places_retain_their_complete_paths() {
    let source = br#"struct Inner {
  values: Slots<u8, 2>;
}

struct Outer {
  inner: Inner;
}

fn main() -> status: std::process::ExitStatus pure {
  let base = array_filled::<u8, 2>(value: 0_u8);
  let values = slots_from_array::<u8, 2>(values: base);
  let inner = Inner(values: move values);
  let outer = Outer(inner: move inner);
  let length = outer.inner.values.len;
  set outer.inner.values[1_u64] = 9_u8;
  let stored = outer.inner.values[1_u64];
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("nested struct window places must check: {outcome:?}");
        };
        let body = &checked.data.functions[0].body.as_deref().expect("WF body");
        let CheckedStatement::Set { target, .. } = &body[5] else {
            panic!("sixth statement must be the projected indexed set");
        };
        let CheckedSetTarget::Storage(target) = target else {
            panic!("set must retain one checked window-index target");
        };
        assert!(matches!(
            target.path.as_slice(),
            [
                CheckedPlaceStep::Field(0),
                CheckedPlaceStep::Field(0),
                CheckedPlaceStep::Subscript(_)
            ]
        ));
        assert!(matches!(
            &body[6],
            CheckedStatement::Let {
                value: CheckedExpression::ReadStorage {
                    root: CheckedContainerRoot { path, .. },
                    ..
                },
                ..
            } if matches!(path.as_slice(), [CheckedPlaceStep::Field(0), CheckedPlaceStep::Field(0), CheckedPlaceStep::Subscript(_)])
        ));
    });

    assert_rule(
        br#"struct Inner {
  values: Slots<u8, 2>;
}

struct Outer {
  inner: Inner;
}

fn replacement(value: Outer) -> result: u8 pure {
  return 9_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let base = array_filled::<u8, 2>(value: 0_u8);
  let values = slots_from_array::<u8, 2>(values: base);
  let inner = Inner(values: move values);
  let outer = Outer(inner: move inner);
  set outer.inner.values[1_u64] = replacement(value: move outer);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Own1,
        SemanticIssueKind::UseAfterMove {
            mechanical_fix: "introduce a new `let` binding before reuse",
        },
    );

    with_semantics(
        br#"struct Inner {
  values: Slots<u8, 2>;
}

struct Outer {
  inner: Inner;
  marker: u8;
}

fn observe(value: &Outer) -> result: u8 reads(value.marker) {
  return deref(value).marker;
}

fn main() -> status: std::process::ExitStatus pure {
  let base = array_filled::<u8, 2>(value: 0_u8);
  let values = slots_from_array::<u8, 2>(values: base);
  let inner = Inner(values: move values);
  let outer = Outer(inner: move inner, marker: 7_u8);
  let held = &outer;
  set outer.inner.values[1_u64] = 9_u8;
  let seen = observe(value: held);
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "a descendant content write must preserve the whole-owner reference: {outcome:?}"
            );
        },
    );
}

// Retired with its subject: `region_bearing_array_content_rejects_under_stor5`
// and `general_elements_reject_deep_stored_views_and_providers` both asserted
// [STOR-5]'s `RegionBearingStorage` over array and window content whose
// element was a view, a provider or an arena. Views, providers, arenas and
// regions are retired; [TYPE-8] closes the one surviving case — a reference
// kind in an element or type-argument position — in the grammar, so no source
// program reaches the semantic judgment.

#[test]
fn general_elements_allow_an_array_value_inside_a_run_slot() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let row = array_filled::<u64, 2>(value: 7_u64);
  let rows = slots_new::<Array<u64, 2>, 2>();
  place_back(window: &rows, value: row);
  set rows[0_u64][1_u64] = 9_u64;
  let value = rows[0_u64][1_u64];
  let width = rows[0_u64].len;
  invariant width_lower: width >= 2_u64;
  invariant width_upper: width <= 2_u64;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("an array is a complete nameable window element: {outcome:?}");
        };
        let main = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main");
        let CheckedStatement::Set {
            target: CheckedSetTarget::Storage(target),
            ..
        } = &main.body.as_deref().expect("WF body")[3]
        else {
            panic!("nested array mutation must use the ordinary typed storage path");
        };
        assert_eq!(target.ty, CheckedType::Integer(IntegerType::U64));
        assert!(
            matches!(target.path.as_slice(), [CheckedPlaceStep::Subscript(_), CheckedPlaceStep::Subscript(index)]
            if matches!(index.base_type, CheckedType::Array { length: CheckedConst::Value(2), .. }))
        );
    });
}
