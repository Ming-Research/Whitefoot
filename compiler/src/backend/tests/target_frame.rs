use crate::target::{
    TargetAggregateLayout, TargetFramePlan, TargetFrameSlot, TargetLayout, TargetLayoutFailure,
    TargetObject, TargetStorageType, plan_target_frame, validate_static_storage,
};

use super::system::with_ir;

const FRAME_CONTEXT: &[u8] = br#"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

fn plan(
    slots: &[TargetFrameSlot],
    address_index_max: Option<u64>,
) -> Result<TargetFramePlan, TargetLayoutFailure> {
    with_ir(FRAME_CONTEXT, |program| {
        let host = TargetLayout::host().expect("the frame test runs on a supported host layout");
        let target = address_index_max
            .map(|maximum| host.with_address_index_max_for_test(maximum))
            .unwrap_or(host);
        plan_target_frame(target, program, slots)
    })
}

fn validate_static(
    ty: &TargetStorageType,
    address_index_max: u64,
) -> Result<TargetAggregateLayout, TargetLayoutFailure> {
    with_ir(FRAME_CONTEXT, |program| {
        let host =
            TargetLayout::host().expect("the static-storage test runs on a supported host layout");
        let target = host.with_address_index_max_for_test(address_index_max);
        validate_static_storage(target, program, ty)
    })
}

#[test]
fn i64_slot_after_i8_has_explicit_seven_byte_padding() {
    let slots = [
        TargetFrameSlot::natural(TargetStorageType::integer(8)),
        TargetFrameSlot::natural(TargetStorageType::integer(64)),
    ];
    let frame = plan(&slots, None).expect("the frame must be representable");

    let byte = frame.logical_field(0).expect("the byte slot must exist");
    assert_eq!(byte.physical_index(), 0);
    assert_eq!(byte.offset(), 0);
    let word = frame.logical_field(1).expect("the word slot must exist");
    assert_eq!(word.physical_index(), 2);
    assert_eq!(word.offset(), 8);
    assert_eq!(
        frame.physical_fields(),
        &[
            TargetStorageType::integer(8),
            TargetStorageType::bytes(7),
            TargetStorageType::integer(64),
        ]
    );
    assert_eq!(frame.layout().size(), 16);
    assert_eq!(frame.layout().align(), 8);
    assert_eq!(frame.independent_slot_alignment(), None);
}

#[test]
fn requested_alignment_adds_tail_padding_to_byte_array_slot() {
    let slots = [TargetFrameSlot::aligned(TargetStorageType::bytes(3), 8)];
    let frame = plan(&slots, None).expect("the frame must be representable");

    let bytes = frame
        .logical_field(0)
        .expect("the byte-array slot must exist");
    assert_eq!(bytes.physical_index(), 0);
    assert_eq!(bytes.offset(), 0);
    assert_eq!(
        frame.physical_fields(),
        &[TargetStorageType::bytes(3), TargetStorageType::bytes(5),]
    );
    assert_eq!(frame.layout().size(), 8);
    assert_eq!(frame.layout().align(), 8);
    assert_eq!(frame.independent_slot_alignment(), None);
}

#[test]
fn uniform_positive_roots_split_only_after_the_complete_extent_fits() {
    let slots = [
        TargetFrameSlot::natural(TargetStorageType::integer(64)),
        TargetFrameSlot::natural(TargetStorageType::integer(64)),
    ];
    let frame = plan(&slots, Some(16)).expect("the complete pair fits exactly");
    assert_eq!(frame.independent_slot_alignment(), Some(8));
    assert_eq!(frame.layout().size(), 16);
    assert_eq!(
        plan(&slots, Some(15)),
        Err(TargetLayoutFailure::Unrepresentable(
            TargetObject::StackFrame
        ))
    );
}

#[test]
fn mixed_alignment_keeps_the_struct_when_reordering_would_grow_the_frame() {
    let word = TargetFrameSlot::natural(TargetStorageType::integer(64));
    let byte = TargetFrameSlot::natural(TargetStorageType::integer(8));
    let packed = plan(&[word.clone(), byte.clone(), byte.clone()], None)
        .expect("the complete packed ordering fits");
    let reordered =
        plan(&[byte.clone(), word, byte], None).expect("the complete alternate ordering fits");
    assert_eq!(packed.layout().size(), 16);
    assert_eq!(reordered.layout().size(), 24);
    assert_eq!(packed.independent_slot_alignment(), None);
    assert_eq!(reordered.independent_slot_alignment(), None);
}

#[test]
fn zero_sized_roots_and_invalid_requested_alignments_never_select_split() {
    let zero = plan(
        &[TargetFrameSlot::natural(TargetStorageType::bytes(0))],
        None,
    )
    .expect("zero extent itself is representable");
    assert_eq!(zero.layout().size(), 0);
    assert_eq!(zero.independent_slot_alignment(), None);
    for slot in [
        TargetFrameSlot::aligned(TargetStorageType::integer(64), 4),
        TargetFrameSlot::aligned(TargetStorageType::bytes(3), 3),
    ] {
        assert_eq!(plan(&[slot], None), Err(TargetLayoutFailure::InvalidIr));
    }
}

#[test]
fn complete_frame_must_fit_the_selected_target_address_domain() {
    let slots = [
        TargetFrameSlot::natural(TargetStorageType::integer(8)),
        TargetFrameSlot::natural(TargetStorageType::integer(64)),
    ];

    assert_eq!(
        plan(&slots, Some(15)),
        Err(TargetLayoutFailure::Unrepresentable(
            TargetObject::StackFrame
        ))
    );
}

#[test]
fn scalar_static_storage_must_fit_the_selected_target_address_domain() {
    let scalar = TargetStorageType::integer(64);

    assert_eq!(
        validate_static(&scalar, 7),
        Err(TargetLayoutFailure::Unrepresentable(TargetObject::Static))
    );
    let boundary = validate_static(&scalar, 8).expect("the complete scalar fits at the boundary");
    assert_eq!(boundary.size(), 8);
    assert_eq!(boundary.align(), 8);
}

#[test]
fn pointer_static_storage_must_fit_the_selected_target_address_domain() {
    let pointer = TargetStorageType::source(crate::IrType::Address(crate::IrAddressed::Integer {
        width: 8,
        signed: false,
    }));

    assert_eq!(
        validate_static(&pointer, 7),
        Err(TargetLayoutFailure::Unrepresentable(TargetObject::Static))
    );
    let boundary = validate_static(&pointer, 8).expect("the complete pointer fits at the boundary");
    assert_eq!(boundary.size(), 8);
    assert_eq!(boundary.align(), 8);
}
