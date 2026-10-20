use crate::target::{
    TargetAggregateLayout, TargetFramePlan, TargetFrameSlot, TargetLayout, TargetLayoutFailure,
    TargetObject, TargetStorageType, plan_target_frame, validate_static_storage,
};

use super::{emit, emitted_function, system::with_ir};

const FRAME_CONTEXT: &[u8] = br#"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

fn plan(
    slots: &[TargetFrameSlot],
    address_index_max: Option<u64>,
) -> Result<TargetFramePlan, TargetLayoutFailure> {
    with_ir(FRAME_CONTEXT, |program| {
        plan_target_frame(target(address_index_max), program, slots)
    })
}

fn target(address_index_max: Option<u64>) -> TargetLayout {
    let host = TargetLayout::host().expect("the frame test runs on a supported host layout");
    address_index_max
        .map(|maximum| host.with_address_index_max_for_test(maximum))
        .unwrap_or(host)
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
fn mixed_roots_keep_exact_struct_offsets_and_bound_independent_allocations() {
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
    assert_eq!(frame.struct_layout().size(), 16);
    assert_eq!(frame.struct_layout().align(), 8);
    assert_eq!(byte.alignment(), 1);
    assert_eq!(word.alignment(), 8);
    let independent = frame
        .independent_extent(target(Some(23)))
        .expect("the bound fits exactly")
        .expect("positive naturally aligned roots split");
    assert_eq!(independent, (1 + 8 + 7) + 7);
    assert_eq!(
        frame.independent_extent(target(Some(22))),
        Err(TargetLayoutFailure::Unrepresentable(
            TargetObject::StackFrame
        ))
    );
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
    assert_eq!(frame.struct_layout().size(), 8);
    assert_eq!(frame.struct_layout().align(), 8);
    assert_eq!(frame.independent_extent(target(Some(8))), Ok(None));
}

#[test]
fn uniform_positive_roots_split_only_after_the_complete_extent_fits() {
    let slots = [
        TargetFrameSlot::natural(TargetStorageType::integer(64)),
        TargetFrameSlot::natural(TargetStorageType::integer(64)),
    ];
    let frame = plan(&slots, Some(16)).expect("the struct pair fits exactly");
    assert_eq!(frame.struct_layout().size(), 16);
    let independent = frame
        .independent_extent(target(Some(37)))
        .expect("the independent bound fits exactly")
        .expect("uniform positive roots split");
    assert_eq!(independent, 2 * (8 + 7) + 7);
    assert_eq!(
        frame.independent_extent(target(Some(36))),
        Err(TargetLayoutFailure::Unrepresentable(
            TargetObject::StackFrame
        ))
    );
}

#[test]
fn independent_bound_covers_different_struct_orderings() {
    let word = TargetFrameSlot::natural(TargetStorageType::integer(64));
    let byte = TargetFrameSlot::natural(TargetStorageType::integer(8));
    let packed = plan(&[word.clone(), byte.clone(), byte.clone()], None)
        .expect("the complete packed ordering fits");
    let reordered =
        plan(&[byte.clone(), word, byte], None).expect("the complete alternate ordering fits");
    assert_eq!(packed.struct_layout().size(), 16);
    assert_eq!(reordered.struct_layout().size(), 24);
    for frame in [packed, reordered] {
        let independent = frame
            .independent_extent(target(None))
            .expect("the bound fits")
            .expect("mixed positive roots split");
        assert_eq!(independent, (8 + 7) + 1 + 1 + 7);
    }
}

#[test]
fn alternating_byte_and_word_roots_need_padding_for_each_root() {
    let byte = TargetFrameSlot::natural(TargetStorageType::integer(8));
    let word = TargetFrameSlot::natural(TargetStorageType::integer(64));
    let frame = plan(&[byte.clone(), word.clone(), byte, word], Some(32))
        .expect("the struct needs 32 bytes, exceeding the old proposed bound of 26");
    assert_eq!(frame.struct_layout().size(), 32);
    let independent = frame
        .independent_extent(target(Some(39)))
        .expect("the corrected bound fits exactly")
        .expect("mixed positive roots split");
    assert_eq!(independent, 2 * (1 + 8 + 7) + 7);
    assert_eq!(
        frame.independent_extent(target(Some(38))),
        Err(TargetLayoutFailure::Unrepresentable(
            TargetObject::StackFrame
        ))
    );
}

#[test]
fn independent_extent_checks_sum_and_final_padding_overflow() {
    let words = |length| {
        TargetFrameSlot::natural(TargetStorageType::array(
            TargetStorageType::integer(64),
            length,
        ))
    };
    let largest = u64::MAX / 8;
    // Both exact structs fit. One root overflows only at the final +7;
    // three roots overflow while summing their individually padded extents.
    for slots in [
        vec![words(largest)],
        vec![words(largest - 2), words(1), words(1)],
    ] {
        let frame = plan(&slots, Some(u64::MAX)).expect("the exact struct fits");
        assert_eq!(frame.struct_layout().size(), u64::MAX - 7);
        assert_eq!(
            frame.independent_extent(target(Some(u64::MAX))),
            Err(TargetLayoutFailure::Unrepresentable(
                TargetObject::StackFrame
            ))
        );
    }
    let below = plan(&[words(largest - 1)], Some(u64::MAX))
        .expect("the adjacent nonoverflowing struct fits");
    assert_eq!(
        below
            .independent_extent(target(Some(u64::MAX)))
            .expect("the adjacent bound does not overflow")
            .expect("positive natural root"),
        u64::MAX - 1
    );
}

#[test]
fn zero_sized_roots_and_invalid_requested_alignments_never_select_split() {
    let zero = plan(
        &[TargetFrameSlot::natural(TargetStorageType::bytes(0))],
        None,
    )
    .expect("zero extent itself is representable");
    assert_eq!(zero.struct_layout().size(), 0);
    assert_eq!(zero.independent_extent(target(None)), Ok(None));
    let empty = plan(&[], Some(0)).expect("an empty frame fits");
    assert_eq!(empty.independent_extent(target(Some(0))), Ok(None));
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
fn mixed_alignment_ordinary_frame_emits_separate_naturally_aligned_allocas() {
    let module = emit(
        br#"fn read(byte: &u8, word: &u64) -> result: u64 reads(byte), reads(word) {
  let small = cvt::<u8, u64>(byte^);
  return small +wrap word^;
}

fn mixed() -> result: u64 pure {
  let byte = 1_u8;
  let word = 2_u64;
  return read(byte: &byte, word: &word);
}

fn main() -> status: std::process::ExitStatus pure {
  let result = mixed();
  if result == 3_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#,
    );
    let mixed = emitted_function(&module, "mixed");
    assert!(mixed.contains(" = alloca i8, align 1"), "{mixed}");
    assert!(mixed.contains(" = alloca i64, align 8"), "{mixed}");
    assert!(!mixed.contains("%wf.frame"), "{mixed}");
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
