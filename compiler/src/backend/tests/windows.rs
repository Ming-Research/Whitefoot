//! [TYPE-9]'s storage shapes and the cell, their construction [OP-13], their
//! target qualification [STOR-6, OP-9] and their compiler-derived release
//! [STOR-3, WIN-3], as the backend emits them.
//!
//! This module was `buffers`. The `buffer<T>` storage class, its
//! `buffer_new` / `buffer_vacant` heads and the fallible store take retired
//! together, and three of its tests retired with them:
//!
//! - `a_store_take_of_an_unbounded_runtime_count_emits_rather_than_stopping_at_the_target`
//!   retired with [BLK-2]: its whole subject was that a take the store cannot
//!   satisfy hands back `None`, so an unproved runtime count is an ordinary
//!   program with a refusal arm the writer wrote. [STOR-8] makes allocation
//!   total in the source - it never returns a failure, no allocating
//!   operation carries a `Result`, and exhaustion terminates from the trusted
//!   base outside the language - so there is no arm left and no second
//!   surface to contrast. What refuses an unproved count now is [OP-9] at the
//!   source, which `op9_overflow_is_rejected_before_lowering` and
//!   `a_runtime_capacity_window_op9_overflow_is_rejected_before_lowering`
//!   below keep.
//! - `affine_element_buffers_construct_replace_vacate_and_drop_per_element`
//!   retired with [BLK-2] and [SET-2]: `buffer_vacant` built an all-`None`
//!   run and `let x = replace slots[i] = e;` exchanged one slot with it.
//!   [WIN-1] gives a window no vacancy state at all - no slot carries a tag,
//!   no occupancy bitmap, and the window is the complete typestate - so a
//!   window built by [OP-13] starts empty and grows by [OP-10]. The per-element
//!   release of an affine-element window is kept by
//!   `heap_programs::affine_slot_windows_fill_overwrite_empty_and_release_per_element`
//!   and by `resource_enums`; `trivially_droppable_affine_elements_keep_the_single_free`
//!   below keeps the empty-action contrast.
//!
//! The remaining cases keep their subject and were retargeted onto the [OP-13]
//! construction functions over the one heap [STOR-8].

use crate::backend::emitter::{
    BackendFailure, WindowAddressFacts, emit_llvm_with_window_address_facts,
};
use crate::target::{
    TargetLayout, TargetLayoutFailure, TargetObject, TargetStorageType, validate_program,
    validate_static_storage,
};

use super::system::with_ir;
use super::*;

/// The baseline clears the complete empty-window representation, including
/// every descriptor word. This checks the shared row, not its callers.
fn assert_empty_window_zeroed(module: &str, row: &str, header_fields: usize) {
    let body = emitted_prelude_row(module, row);
    assert!(!body.contains("poison"), "{body}");
    assert!(!body.contains("undef"), "{body}");
    let store = body
        .lines()
        .map(str::trim)
        .find(|line| line.ends_with(" zeroinitializer, ptr %wf.result"))
        .expect("the complete empty window is initialized in its result destination");
    assert!(
        store.starts_with(&format!("store {{ {}[", "i64, ".repeat(header_fields))),
        "the zero aggregate includes every descriptor word before its slots: {body}"
    );
}

const AFFINE_INVARIANT_BOUNDED_ALLOCATION: &[u8] =
    br#"fn allocate(n: u64, half: u64) -> result: unit pure contract {
  requires half <= 500_u64;
} {
  let doubled = half * 2_u64;
  let within = n <= doubled;
  if within {
    invariant tight: n <= 1000_u64;
    let values = box_array_filled::<u16>(count: n, value: 0_u16);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

const U64_RUNTIME_WINDOW: &[u8] = br#"fn main() -> status: std::process::ExitStatus pure {
  doc "One eight-byte slot in a runtime-capacity window, whose actual alignment the selected allocator has to promise.";
  let values = box_slots_new::<u64>(capacity: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;

const U64_CELL: &[u8] = br#"fn main() -> status: std::process::ExitStatus pure {
  doc "One eight-byte cell on the same heap, the other half of the same obligation.";
  let cell = box_new::<u64>(value: 7_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;

/// OP-4 and OP-10 already prove a Slots offset is inside the physical
/// capacity. A Ring still needs its head-relative wrap in both placements.
#[test]
fn slots_addresses_use_proved_offsets_and_ring_addresses_still_wrap() {
    for shape in ["Slots", "Ring"] {
        for capacity in ["", ", 4"] {
            let (ty, window) = if capacity.is_empty() {
                (format!("Box<{shape}<u64>>"), "deref(values).inner")
            } else {
                (format!("{shape}<u64{capacity}>"), "deref(values)")
            };
            let source = format!(
                "fn read(values: &{ty}, index: u64) -> result: u64 reads(values) contract {{\n  requires index < {window}.len;\n}} {{\n  return {window}[index];\n}}\n\nfn roundtrip(values: &{ty}, value: u64) -> result: u64 writes(values) contract {{\n  requires {window}.len < {window}.cap;\n}} {{\n  place_back(window: &{window}, value: value);\n  let result = take_back(window: &{window});\n  return result;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
            );
            let (llvm, withheld) = with_ir(source.as_bytes(), |program| {
                let target = TargetLayout::host().expect("supported target");
                let emit = |facts| {
                    emit_llvm_with_window_address_facts(program, target, facts)
                        .expect("both fact choices use the same qualified program")
                        .into_string()
                };
                (
                    emit(WindowAddressFacts::Emit),
                    emit(WindowAddressFacts::Withhold),
                )
            });
            assert!(!withheld.contains("@llvm.assume"));
            let ordinary_lines = llvm
                .lines()
                .filter(|line| {
                    !line.contains("@llvm.assume") && !line.contains(".nonnegative = icmp sge i64 ")
                })
                .collect::<Vec<_>>();
            assert_eq!(ordinary_lines, withheld.lines().collect::<Vec<_>>());
            let functions = llvm
                .split("\ndefine ")
                .filter(|body| {
                    body.starts_with("i64 @wf_read(")
                        || body.lines().next().is_some_and(|line| {
                            line.contains("@wf_place_back$") || line.contains("@wf_take_back$")
                        })
                })
                .map(|body| body.split("\n}").next().expect("function body"))
                .collect::<Vec<_>>();
            assert_eq!(functions.len(), 3, "{shape}{capacity}");
            for body in functions {
                assert!(body.contains("getelementptr inbounds"), "{body}");
                assert_eq!(body.contains("icmp uge i64"), shape == "Ring", "{body}");
                assert_eq!(body.matches("call void @llvm.assume(").count(), 1, "{body}");
                let lines = body.lines().collect::<Vec<_>>();
                let assume = lines
                    .iter()
                    .position(|line| line.contains("call void @llvm.assume("))
                    .expect("one payload fact");
                let address = lines[assume + 1].trim();
                let (pointer, _) = address.split_once(" = ").expect("payload pointer");
                let (_, operand) = address.rsplit_once(", i64 ").expect("actual GEP index");
                assert_eq!(
                    lines[assume - 1].trim(),
                    format!("{pointer}.nonnegative = icmp sge i64 {operand}, 0")
                );
                assert_eq!(
                    lines[assume].trim(),
                    format!("call void @llvm.assume(i1 {pointer}.nonnegative)")
                );
                assert!(address.contains(" = getelementptr inbounds "), "{address}");
                for line in lines {
                    if line.contains(" = add ") || line.contains(" = sub ") {
                        assert!(
                            !line.contains(" nuw ") && !line.contains(" nsw "),
                            "window coordinate arithmetic keeps its ordinary form: {line}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn empty_fixed_windows_initialize_descriptors_before_return() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let slots = slots_new::<Array<u64, 32>, 4>();
  let ring = ring_new::<Array<u64, 32>, 4>();
  if slots.len != 0_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if ring.len != 0_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if ring.head != 0_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = compile(source);
    assert_empty_window_zeroed(&module, "slots_new", 1);
    assert_empty_window_zeroed(&module, "ring_new", 2);
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// A take changes the descriptor even when no element bytes exist. Ring's
/// captured physical position must still use its old head through wrapping.
/// Huge logical capacities allocate only the header here; two front placements
/// must preserve mathematical coordinates without overflowing an intermediate.
#[test]
fn zero_sized_takes_update_slots_and_wrapped_ring_boundaries_once() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let value = array_filled::<u64, 0>(value: 0_u64);
  let slots = slots_new::<Array<u64, 0>, 2>();
  place_back(window: &slots, value: value);
  place_back(window: &slots, value: value);
  let last = take_back(window: &slots);
  if slots.len != 1_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let first = take_back(window: &slots);
  if slots.len != 0_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  let ring = ring_new::<Array<u64, 0>, 2>();
  place_back(window: &ring, value: value);
  place_back(window: &ring, value: value);
  let front = take_front(window: &ring);
  if ring.head != 1_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  place_back(window: &ring, value: front);
  let back = take_back(window: &ring);
  if ring.len != 1_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if ring.head != 1_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  let remainder = take_front(window: &ring);
  if ring.head != 0_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  if ring.len != 0_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  let large = box_ring_new::<Array<u64, 0>>(capacity: 9223372036854775809_u64);
  place_front(window: &large.inner, value: value);
  if large.inner.head != 9223372036854775808_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  place_front(window: &large.inner, value: value);
  if large.inner.head != 9223372036854775807_u64 {
    return std::process::exit_status(code: 9_u8);
  }
  if large.inner.len != 2_u64 {
    return std::process::exit_status(code: 10_u8);
  }
  let large_first = take_front(window: &large.inner);
  if large.inner.head != 9223372036854775808_u64 {
    return std::process::exit_status(code: 11_u8);
  }
  let large_last = take_front(window: &large.inner);
  if large.inner.head != 0_u64 {
    return std::process::exit_status(code: 12_u8);
  }
  if large.inner.len != 0_u64 {
    return std::process::exit_status(code: 13_u8);
  }
  let single = ring_new::<Array<u64, 0>, 1>();
  place_front(window: &single, value: value);
  if single.head != 0_u64 {
    return std::process::exit_status(code: 14_u8);
  }
  let only = take_front(window: &single);
  if single.head != 0_u64 {
    return std::process::exit_status(code: 15_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for (overlap, facts) in [
        (OverlapLowering::Off, WindowAddressFacts::Emit),
        (OverlapLowering::Off, WindowAddressFacts::Withhold),
        (OverlapLowering::On, WindowAddressFacts::Emit),
    ] {
        let module = super::system::with_mutated_ir_lowering(source, overlap, |program| {
            let target = TargetLayout::host().expect("supported target");
            let mut module = emit_llvm_with_window_address_facts(program, target, facts)
                .expect("the zero-stride program qualifies in both fact choices")
                .into_string();
            module.push_str(
                &crate::driver::launcher::render(program, "main").expect("ordinary test launcher"),
            );
            module
        });
        let assumptions = module
            .lines()
            .filter(|line| line.contains(".nonnegative = icmp sge i64 "))
            .collect::<Vec<_>>();
        if facts == WindowAddressFacts::Emit {
            assert!(!assumptions.is_empty(), "observe zero-stride payload facts");
            assert!(assumptions.iter().all(|line| line.ends_with("i64 0, 0")));
        } else {
            assert!(assumptions.is_empty());
            assert!(!module.contains("@llvm.assume"));
        }
        let retained = super::owned_places::retain_calls(&module);
        let output = super::compile_and_run(&retained);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// Zero capacity removes the element's representation, including alignment.
/// Otherwise an empty high-alignment window silently enlarges its parent and
/// the emitted runtime allocation beyond the qualified byte ceiling. The
/// native observer checks the emitted size independently of the Rust layout.
#[test]
fn zero_capacity_windows_keep_header_layout_inside_nonempty_storage() {
    let source = br#"struct EmptyWindows {
  before: u8;
  slots: Slots<std::io::OutputStream, 0>;
  ring: Ring<std::io::OutputStream, 0>;
  after: u64;
}

fn empty_length(values: &[std::io::OutputStream]) -> result: u64 reads(values) {
  return deref(values).len;
}

fn main() -> status: std::process::ExitStatus pure {
  let initial_slots = slots_new::<std::io::OutputStream, 0>();
  let initial_ring = ring_new::<std::io::OutputStream, 0>();
  let value = EmptyWindows(before: 17_u8, slots: move initial_slots, ring: move initial_ring, after: 29_u64);
  let storage = box_ring_new::<EmptyWindows>(capacity: 1_u64);
  place_back(window: &storage.inner, value: move value);
  let recovered = take_front(window: &storage.inner);
  let EmptyWindows(before: before, slots: slots, ring: ring, after: after) = move recovered;
  if before != 17_u8 {
    return std::process::exit_status(code: 1_u8);
  }
  if after != 29_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if ring.head != 0_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if ring.len != 0_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  let length = empty_length(values: &slots[0_u64..0_u64]);
  if length != 0_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  let array = slots_into_array::<std::io::OutputStream, 0>(values: move slots);
  let restored = slots_from_array::<std::io::OutputStream, 0>(values: move array);
  if restored.len != 0_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = with_ir(source, |program| {
        let host = TargetLayout::host().expect("supported target");
        let owner = program
            .nominals()
            .iter()
            .find(|nominal| nominal.name() == "EmptyWindows")
            .expect("the nested source owner");
        let owner_layout = validate_static_storage(
            host,
            program,
            &TargetStorageType::source(crate::IrType::Nominal(owner.id())),
        )
        .expect("the complete parent fits");
        assert_eq!((owner_layout.size(), owner_layout.align()), (40, 8));
        let crate::IrNominalKind::Struct { fields } = owner.kind() else {
            panic!("the owner is a source struct");
        };
        for (field, size) in [(1, 8), (2, 16)] {
            let layout = validate_static_storage(
                host,
                program,
                &TargetStorageType::source(fields[field].ty()),
            )
            .expect("the zero-capacity child fits");
            assert_eq!((layout.size(), layout.align()), (size, 8));
        }
        // One Ring slot is the complete 40-byte parent after a 24-byte
        // header, with no alignment inherited from the absent handles.
        let exact = host.with_runtime_allocation_limits_for_test(64, 8);
        assert_eq!(validate_program(exact, program), Ok(()));
        let short = host.with_runtime_allocation_limits_for_test(63, 8);
        assert_eq!(
            validate_program(short, program),
            Err(TargetLayoutFailure::Unrepresentable(
                TargetObject::RuntimeSizedAllocation
            ))
        );
        for facts in [WindowAddressFacts::Emit, WindowAddressFacts::Withhold] {
            assert_eq!(
                emit_llvm_with_window_address_facts(program, short, facts),
                Err(BackendFailure::TargetLayout(
                    TargetLayoutFailure::Unrepresentable(TargetObject::RuntimeSizedAllocation)
                ))
            );
        }
        let mut module = crate::backend::emitter::emit_llvm_with_layout(program, exact)
            .expect("the exact allocation boundary emits")
            .into_string();
        module.push_str(
            &crate::driver::launcher::render(program, "main").expect("ordinary test launcher"),
        );
        module
    });
    assert_empty_window_zeroed(&module, "slots_new", 1);
    assert_empty_window_zeroed(&module, "ring_new", 2);
    let observed = super::owned_places::retain_calls(&module)
        .replace("@malloc(", "@wf_observe_window_allocate(");
    let observer = r#"
#include <stdint.h>
#include <stdlib.h>

void *wf_observe_window_allocate(uint64_t size) {
    if (size != 64) exit(6);
    return malloc((size_t)size);
}
"#;
    let output = super::compile_link_and_run(&observed, Some(observer), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// OP-9 admits zero even when the mathematical language ceiling exceeds
/// u64. Lowering must preserve that result so STOR-6, rather than an internal
/// compiler failure, reports the unrepresentable concrete element layout.
#[test]
fn an_above_u64_zero_count_reaches_target_qualification() {
    for generic in ["", "<T>"] {
        let source = format!(
            "struct Giant {{\n  words: Array<u64, 2305843009213693952>;\n}}\n\nfn allocate{generic}(count: u64) -> result: unit pure contract {{\n  requires count <= 0_u64;\n}} {{\n  let cells = box_slots_new::<Giant>(capacity: count);\n  free_empty(window: move cells);\n  return unit;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_ir(source.as_bytes(), |program| {
            let host = TargetLayout::host().expect("the test host is supported");
            assert_eq!(
                validate_program(host, program),
                Err(TargetLayoutFailure::Unrepresentable(
                    TargetObject::Representation
                ))
            );
        });
    }
}

/// [STOR-6] multiplies the retained source bound for a runtime-capacity
/// construction by the actual target stride, adds its emitted header, and
/// requires the result to fit the
/// allocator-parameter domain. The affine invariant supplies that bound, and
/// the boundary is pinned from both sides at the exact byte.
///
/// There is one allocation surface in v0.60 and it is total [STOR-8], so this
/// byte ceiling is the only place a proved count can sit just inside and just
/// outside a target limit; the retired store take's `None` arm [BLK-2] is not
/// a second surface to contrast it against.
#[test]
fn affine_invariant_ceiling_controls_the_exact_selected_target_boundary() {
    with_ir(AFFINE_INVARIANT_BOUNDED_ALLOCATION, |program| {
        let host = TargetLayout::host().expect("the backend test runs on a supported host layout");

        // Array<u16> has one u64 header followed by 1000 two-byte elements.
        let exact = host.with_runtime_allocation_limits_for_test(2008, 8);
        assert_eq!(validate_program(exact, program), Ok(()));

        let one_byte_short = host.with_runtime_allocation_limits_for_test(2007, 8);
        assert_eq!(
            validate_program(one_byte_short, program),
            Err(TargetLayoutFailure::Unrepresentable(
                TargetObject::RuntimeSizedAllocation
            ))
        );
    });
}

/// The heap's own alignment boundary, for the window and for the cell.
///
/// The one heap [STOR-8] hands out storage its host allocator supplies, so the
/// element's *actual* target alignment must be one that allocator promises -
/// the obligation `box_slots_new` and `box_new` each carry [OP-13, STOR-6].
/// Both directions are pinned at the exact boundary: eight-byte alignment
/// admits an eight-byte slot and a four-byte guarantee refuses it, and refuses
/// it as a runtime-sized allocation rather than as a representation failure,
/// because what is short is the allocator's promise and not the language
/// ceiling.
#[test]
fn a_runtime_window_and_a_cell_must_fit_the_selected_allocator_alignment() {
    for fixture in [U64_RUNTIME_WINDOW, U64_CELL] {
        with_ir(fixture, |program| {
            let host =
                TargetLayout::host().expect("the backend test runs on a supported host layout");

            // The byte domain stays the host's own: cutting the
            // address-index domain to the allocation's own size would refuse
            // the window's own block layout before the alignment is reached.
            // Only the alignment guarantee moves here.
            let byte_domain = i64::MAX as u64;
            let exact = host.with_runtime_allocation_limits_for_test(byte_domain, 8);
            assert_eq!(validate_program(exact, program), Ok(()));

            let one_alignment_step_short =
                host.with_runtime_allocation_limits_for_test(byte_domain, 4);
            assert_eq!(
                validate_program(one_alignment_step_short, program),
                Err(TargetLayoutFailure::Unrepresentable(
                    TargetObject::RuntimeSizedAllocation
                ))
            );
        });
    }
}

#[test]
fn weigh_invariant_proves_domains_then_erases_before_llvm() {
    let source = br#"fn weigh(weights: &[u8], count: u64) -> total: u32 reads(weights) contract {
  define capacity = deref(weights).len;
  requires count <= capacity;
  requires count <= 1000_u64;
  ensures total <= 255000_u32;
} {
  let sum = 0_u32;
  for (
    i in 0_u64..count,
    invariant per_byte: sum <= 255_u32 * i
  ) {
    let w = deref(weights)[i];
    let wide = cvt::<u8, u32>(w);
    set sum = sum + wide;
  }
  return sum;
}

fn tally(left: u32, right: u32) -> total: u32 pure {
  return left +wrap right;
}

fn main() -> status: std::process::ExitStatus pure {
  let weights = slots_new::<u8, 4>();
  for @fill (
    at in 0_u64..4_u64,
    invariant grown: weights.len >= at,
    invariant spare: weights.cap + at >= weights.len + 4_u64
  ) {
    place_back(window: &weights, value: 7_u8);
  }
  let code = 0_u8;
  let window = &weights[0_u64..4_u64];
  let total = weigh(weights: window, count: 4_u64);
  if total != 28_u32 {
    set code = 1_u8;
  }
  let around = tally(left: total, right: 0_u32);
  if around != 28_u32 {
    set code = 2_u8;
  }
  return std::process::exit_status(code: code);
}
"#;
    let llvm = compile(source);
    let weigh = emitted_function(&llvm, "weigh");

    // INV-1 and OP-2 discharge before lowering. The loop therefore contains
    // one plain integer addition and no runtime representation of `per_byte`.
    //
    // The addition is spelled `add nuw i32`. [DIAG-2]: "every fact the checker
    // has proved may be supplied to the backend, as target attributes,
    // instruction flags, metadata, or assumptions: ... and a discharged
    // integer-domain obligation [OP-2], the last being what licenses a
    // no-wrap flag on an exact operation", bounded by the same sentence's
    // "only a fact the checker has actually discharged may be supplied, never
    // one a writer states". Both directions are read: `+` here carries the
    // discharged obligation and the flag, and the `+wrap` in `tally` carries
    // no obligation and therefore no flag.
    assert!(weigh.contains("add nuw i32"));
    assert_eq!(weigh.matches("add nuw i32").count(), 1);
    assert!(!weigh.contains(".with.overflow."));
    assert!(!weigh.contains("call void @wf_trap"));
    assert!(!llvm.contains("per_byte"));

    let tally = emitted_function(&llvm, "tally");
    assert!(
        tally.contains("= add i32"),
        "the wrap-mode addition is a plain add: {tally}"
    );
    assert!(
        !tally.contains(" nuw ") && !tally.contains(" nsw "),
        "and carries no no-wrap flag: {tally}"
    );

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// One runtime-capacity window crosses two functions, takes one element
/// assignment [SET-1] and is freed exactly once [STOR-3].
///
/// The exhaustion edge is the other half of the subject: [STOR-8] makes the
/// allocation total in the source and terminates the program from the trusted
/// base when the heap cannot satisfy it, so the emitted allocation still
/// carries a null-result edge into the trusted base and never an arm the
/// writer could have written.
#[test]
fn a_runtime_capacity_window_crosses_functions_updates_and_frees_once() {
    // STOR-1 stores the length and elements in one allocation. The largest
    // u16 count is (i64::MAX - 8) / 2, including the Array header.
    let source = br#"fn bounded_count(n: u64) -> result: u64 pure contract {
  ensures result <= 4611686018427387899_u64;
} {
  if n <= 4611686018427387899_u64 {
    return n;
  } else {
    return 4611686018427387899_u64;
  }
}

fn make(n: u64) -> result: Box<Array<u16>> pure {
  let bounded = bounded_count(n: n);
  return box_array_filled::<u16>(count: bounded, value: 3_u16);
}

fn replacement() -> result: u16 pure {
  return 9_u16;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = make(n: 4_u64);
  let length = values.inner.len;
  let stored = 0_u16;
  let code = 0_u8;
  if 2_u64 < length {
    set values.inner[2_u64] = replacement();
    set stored = values.inner[2_u64];
  } else {
    set code = 3_u8;
  }
  if code == 0_u8 {
    if length != 4_u64 {
      set code = 1_u8;
    }
    if stored != 9_u16 {
      set code = 2_u8;
    }
  }
  return std::process::exit_status(code: code);
}
"#;
    let llvm = compile(source);
    let main = emitted_function(&llvm, "main");
    let make = emitted_function(&llvm, "make");
    // The verified scalar normalizer summary discharges allocation, while a
    // local length branch discharges both indexed sites. The RHS is evaluated
    // once before the target commits one store, with no runtime proof fallback.
    let rhs = main
        .find("call i16 @wf_replacement")
        .expect("SET-1 must evaluate its RHS once");
    let store = main
        .find("store i16 %v")
        .expect("SET-1 must commit one element store");
    assert!(rhs < store);
    assert!(!main.contains("call void @wf_trap"));
    assert_eq!(main.matches("call void @free").count(), 1);
    assert!(!make.contains("call void @free"));

    // The proved count ceiling times the u16 stride fits the selected target's
    // byte domain. Target layout therefore admits the dynamic allocation and
    // the emitter needs only the allocator's null-result edge.
    let filled = emitted_prelude_row(&llvm, "box_array_filled");
    assert!(filled.contains("call ptr @malloc"));
    assert!(filled.contains("icmp ne ptr"));
    // The two labels below are the v0.59 emitted names for the exhaustion
    // edge. [STOR-8] keeps the edge and moves its meaning - it is the trusted
    // base terminating, never a source arm - but does not fix a spelling, so
    // these stay as written for the lowering port to rename.
    assert!(filled.contains("buffer.fill.oom."));
    assert!(filled.contains("call void @wf_resource_abort()"));
    for absent in [
        "buffer.fill.target.",
        "@wf_target_domain_abort",
        "@.wf_resource.target_domain",
    ] {
        assert!(
            !llvm.contains(absent),
            "an allocation checked against the target layout must not emit {absent}:\n{llvm}"
        );
    }

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// A count read back off an existing window's own `len` [OP-15] qualifies the
/// next allocation of the same element type, so no target guard is emitted.
#[test]
fn a_window_length_qualifies_same_element_reallocation_without_a_target_guard() {
    let source = br#"fn refill(source: Box<Array<u8>>) -> result: Box<Array<u8>> pure {
  let length = source.inner.len;
  return box_array_filled::<u8>(count: length, value: 0_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  let initial = box_array_filled::<u8>(count: 4_u64, value: 7_u8);
  let copied = refill(source: move initial);
  let length = copied.inner.len;
  if length != 4_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    // The construction row is one out-of-line body per instance
    // (compiler/prelude-records), so the allocation is in the row and the
    // caller names it.
    let refill = emitted_function(&llvm, "refill");
    assert!(refill.contains("call ptr @wf_box_array_filled$instance$"));
    let filled = emitted_prelude_row(&llvm, "box_array_filled");
    assert!(filled.contains("call ptr @malloc"));
    for absent in [
        "buffer.fill.target.",
        "@wf_target_domain_abort",
        "@.wf_resource.target_domain",
    ] {
        assert!(
            !llvm.contains(absent),
            "a window-length target invariant must not emit {absent}:\n{llvm}"
        );
    }

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn op9_overflow_is_rejected_before_lowering() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let values = box_array_filled::<u64>(count: 18446744073709551615_u64, value: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile_rejection(source);
    assert_eq!(failure.rule_id(), Some("OP-9"));
    assert!(
        failure
            .to_string()
            .contains("]: UndischargedAllocationFitObligation\n")
    );
}

#[test]
fn an_out_of_bounds_run_set_is_an_op4_compile_rejection() {
    // `box_array_filled`'s published count fixes the run's length [OP-13], so
    // 2 < 2 is underivable and the program rejects at compile time with the
    // residual over the [OP-15] measure read [OP-4, ENT-6].
    let source = br#"fn replacement() -> result: u8 pure {
  return 9_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = box_array_filled::<u8>(count: 2_u64, value: 0_u8);
  set values.inner[2_u64] = replacement();
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile_rejection(source);
    assert_eq!(failure.rule_id(), Some("OP-4"));
    assert!(failure.detail().contains("2_u64 < values.inner.len"));
}

#[test]
fn run_cleanup_is_explicit_on_return_and_break_edges() {
    let source = br#"fn cleanup(flag: Bool) -> result: unit pure {
  doc "Every edge that leaves this scope holding a window carries that window's release: the early return, the loop break, and the final return.";
  let values = box_slots_new::<u8>(capacity: 2_u64);
  if flag {
    return unit;
  }
  loop @done {
    let scratch = box_slots_new::<u16>(capacity: 1_u64);
    break @done;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let true_value = True();
  let false_value = False();
  cleanup(flag: true_value);
  cleanup(flag: false_value);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let cleanup = emitted_function(&llvm, "cleanup");
    // Three release sites: the early return and the final return each carry
    // the cell the scope holds, and the loop break carries the body-scope
    // cell it leaves [STOR-3, LIV-1].
    assert_eq!(cleanup.matches("call void @free").count(), 3);
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn range_references_cross_helpers_without_transferring_ownership() {
    let llvm = compile(include_bytes!(
        "../../../../tests/conformance/cases/x-buffer-borrowed-columns-run.wf"
    ));
    let fill = emitted_function(&llvm, "fill");
    let fold = emitted_function(&llvm, "fold");
    let main = emitted_function(&llvm, "main");
    assert!(fill.contains("store i64"));
    assert!(fold.contains("load i64"));
    assert!(!fill.contains("call void @free"));
    assert!(!fold.contains("call void @free"));
    // Both declared length requirements and the counted-range binder facts are
    // checked before lowering. Neither helper retains a runtime proof check.
    assert_eq!(fill.matches("call void @wf_trap").count(), 0);
    assert_eq!(fold.matches("call void @wf_trap").count(), 0);
    // Each counted loop retains exactly its own continuation comparison. No
    // second comparison remains for either proved window bound.
    assert_eq!(fill.matches("icmp ult i64").count(), 1);
    assert_eq!(fold.matches("icmp ult i64").count(), 1);
    // The case has six status exits: the four length checks before the two
    // calls, the checksum branch and the success exit.
    assert!(!main.contains("call void @wf_trap"));
    assert_eq!(
        main.matches("call void @wf_std.process.exit_status")
            .count(),
        6
    );
    // Every exit leaves the scope that owns exactly the two `Box` fields.
    // Check each return edge independently so one edge cannot leak while
    // another happens to contribute the missing releases [STOR-3, PROV-6].
    let exits = main
        .split("ret void")
        .filter(|block| block.contains("call void @wf_std.process.exit_status"))
        .collect::<Vec<_>>();
    assert_eq!(exits.len(), 6);
    for exit in exits {
        assert_eq!(exit.matches("call void @free").count(), 2, "{exit}");
    }
    assert_eq!(main.matches("call void @free").count(), 12);
    assert!(main.contains("call i8 @wf_fill"));
    assert!(main.contains("call i64 @wf_fold"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// One caller-storage update reaches the caller through a single struct
/// pointer: the callee's declared field paths substitute at the call [EFF-5]
/// and the write lands in the caller's own window and scalar field.
#[test]
fn a_reference_parameter_updates_caller_storage_through_one_address_path() {
    let source = br#"struct Pool {
  left: Box<Array<u64>>;
  right: Box<Array<u64>>;
  count: u64;
}

fn update(pool: &Pool) -> result: unit writes(pool.left), writes(pool.count) {
  let spare = deref(pool).left.inner.len;
  let ok = 1_u64 < spare;
  if ok {
    set deref(pool).left.inner[1_u64] = 13_u64;
    set deref(pool).count = 1_u64;
  }
  return unit;
}

fn observe(pool: &Pool) -> result: u64 reads(pool.left), reads(pool.count) {
  let spare = deref(pool).left.inner.len;
  let ok = 1_u64 < spare;
  let count = deref(pool).count;
  if ok {
    let value = deref(pool).left.inner[1_u64];
    return value +wrap count;
  } else {
    return count;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let left = box_array_filled::<u64>(count: 2_u64, value: 0_u64);
  let right = box_array_filled::<u64>(count: 2_u64, value: 0_u64);
  let pool = Pool(left: move left, right: move right, count: 0_u64);
  let code = 0_u8;
  let apply = True();
  if apply {
    update(pool: &pool);
  }
  let observed = observe(pool: &pool);
  if observed != 14_u64 {
    set code = 1_u8;
  }
  return std::process::exit_status(code: code);
}
"#;
    let llvm = compile(source);
    let update = emitted_function(&llvm, "update");
    let observe = emitted_function(&llvm, "observe");
    let main = emitted_function(&llvm, "main");
    assert!(update.starts_with("define i8 @wf_update(ptr "));
    assert!(observe.starts_with("define i64 @wf_observe(ptr "));
    assert!(main.contains("call i8 @wf_update(ptr "));
    assert!(main.contains("call i64 @wf_observe(ptr "));
    assert!(!update.contains("call void @free"));
    assert!(!observe.contains("call void @free"));
    assert_eq!(main.matches("call void @free").count(), 2);

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn a_referenced_pool_tree_preserves_range_reference_and_result_abi() {
    let llvm = compile(include_bytes!(
        "../../../../tests/conformance/cases/x-borrowed-pool-tree-run.wf"
    ));
    let build = emitted_function(&llvm, "build");
    let checksum = emitted_function(&llvm, "checksum");
    let build_body = emitted_body(&llvm, "build");
    let checksum_body = emitted_body(&llvm, "checksum");
    let main = emitted_function(&llvm, "main");
    // The case lends the pool as two range references and one ordinary
    // reference to a scalar-bearing struct. A range reference crosses this
    // boundary as its address-and-length pair [REF-4], passed as the element
    // pointer, which carries the reference facts, and the count
    // (compiler/backend-facts). The three-leaf Result returns in registers
    // from the public entry, so the first range is its first argument. The
    // internal body still constructs the result through its destination
    // (compiler/src/backend/abi.rs), and carries the same facts.
    // `build` also takes its cursor by reference; `checksum` only reads.
    for (function, body, name, references) in [
        (build, build_body, "build", 3),
        (checksum, checksum_body, "checksum", 2),
    ] {
        register_result_type(&llvm, function, &["i32", "i64", "i32"]);
        let header = function.lines().next().expect("helper signature");
        assert!(
            header.contains(&format!(" @wf_{name}(ptr noalias nonnull ")),
            "{header}"
        );
        assert!(!header.contains("%wf.result"), "{header}");
        let body_header = body.lines().next().expect("helper body signature");
        assert!(
            body_header.starts_with(&format!(
                "define internal void @wf_{name}.body(ptr %wf.result, ptr noalias nonnull "
            )),
            "{body_header}"
        );
        for header in [header, body_header] {
            for ordinal in 0..2 {
                assert!(
                    header.contains(&format!(
                        " %wf.arg.v{ordinal}.data, i64 %wf.arg.v{ordinal}.len, "
                    )),
                    "{header}"
                );
            }
            assert_eq!(
                header.matches("ptr noalias nonnull ").count(),
                references,
                "{header}"
            );
        }
        // The body's destination still receives the tag, u64 success
        // payload and three-variant PoolError, each written on its route.
        assert_scalar_result_fields(&llvm, body, &["i32", "i64", "i32"]);
    }
    for definition in [build, build_body] {
        assert!(
            definition
                .lines()
                .next()
                .expect("build signature")
                .contains(", i32 %v3)")
        );
    }
    for definition in [checksum, checksum_body] {
        assert!(
            definition
                .lines()
                .next()
                .expect("checksum signature")
                .contains(", i64 %v2)")
        );
    }
    assert!(!build_body.contains("call void @free"));
    assert!(!checksum_body.contains("call void @free"));
    // Bounds and arithmetic failures are typed results rather than written
    // proofs, so build and checksum contain no trap edge. Main owns two
    // `Box<Slots<u64>>` cells and releases both on each of its five exits.
    assert!(!build_body.contains("call void @wf_trap"));
    assert!(!checksum_body.contains("call void @wf_trap"));
    assert!(!main.contains("call void @wf_trap"));
    assert_eq!(
        main.matches("call void @wf_std.process.exit_status")
            .count(),
        5
    );
    let exits = main
        .split("ret void")
        .filter(|block| block.contains("call void @wf_std.process.exit_status"))
        .collect::<Vec<_>>();
    assert_eq!(exits.len(), 5);
    for exit in exits {
        assert_eq!(exit.matches("call void @free").count(), 2, "{exit}");
    }
    assert_eq!(main.matches("call void @free").count(), 10);
}

/// The case counts lines, words and bytes over two chunks and combines the
/// two summaries. `summarize` is a const generic over its chunk length
/// [TYPE-9, FN-2], so the two chunk lengths reach the backend as two
/// monomorphized instances, each taking its own frame-resident
/// `Slots<u8, n>` by value [STOR-1], and the whole program allocates nothing.
/// The assertions are those facts.
///
/// Re-derived from the ported lowering: a `Slots<T, N>` stores its `len` with
/// the block and a `head` belongs to `Ring` alone [STOR-1, WIN-1], so the
/// constant-capacity block is the header-first `{ i64 len, [N x T] slots }`
/// rather than v0.59's three-word `FixedVector`. The reference parameter is
/// passed as one pointer with the callee-visible `dereferenceable` the block's
/// own size gives it.
#[test]
fn chunk_summary_instances_preserve_window_abi_and_avoid_allocation() {
    let llvm = compile(include_bytes!(
        "../../../../tests/conformance/cases/x-wc-chunk-summary-run.wf"
    ));
    let summaries = llvm
        .lines()
        .filter(|line| line.starts_with("define ") && line.contains(" @wf_summarize$instance$"))
        .map(|header| {
            assert!(header.starts_with("define i8 @wf_summarize$instance$"));
            // Both parameters are `ptr`-typed: the reference `out` by
            // [REF-1], and the owned `Slots<u8, n>` because the aggregate ABI
            // hands a frame-resident block by address. The reference's
            // attribute set (compiler/backend-facts) sits between its type
            // and its name, so the type is read at the head of the parameter
            // rather than immediately before `%v0`.
            let parameters = header
                .split_once('(')
                .expect("parameter list")
                .1
                .rsplit_once(')')
                .expect("parameter list closes")
                .0
                .split(", ")
                .collect::<Vec<_>>();
            let [out, input] = parameters.as_slice() else {
                panic!("summarize takes exactly two parameters: {header}");
            };
            assert!(out.starts_with("ptr ") && out.ends_with(" %v0"), "{header}");
            assert_eq!(*input, "ptr %wf.arg.v1", "{header}");
            let name = header
                .split_once("@wf_")
                .expect("WF symbol")
                .1
                .split_once('(')
                .expect("signature")
                .0;
            emitted_function(&llvm, name)
        })
        .collect::<Vec<_>>();
    assert_eq!(summaries.len(), 2, "two source const instantiations");
    assert_eq!(
        summaries
            .iter()
            .filter(|body| body.contains("getelementptr inbounds { i64, [4 x i8] }"))
            .count(),
        1
    );
    assert_eq!(
        summaries
            .iter()
            .filter(|body| body.contains("getelementptr inbounds { i64, [1 x i8] }"))
            .count(),
        1
    );
    let combine = emitted_function(&llvm, "combine");
    assert!(combine.starts_with("define i8 @wf_combine(ptr "));
    // Each of `combine`'s three reference parameters is one pointer and
    // nothing else: [REF-1] makes a reference "a local name for a path" and
    // [REF-3] keeps it from escaping, so there is nothing beside the address
    // to carry. The emitted head also carries the attribute set the pending
    // amendment compiler/backend-facts fixes -- "`noalias`, `captures(none)`,
    // `nonnull` and `dereferenceable` on every reference parameter" -- so the
    // parameter's *type* is read past its attributes rather than by matching
    // a bare `ptr %v`.
    let signature = combine
        .lines()
        .next()
        .expect("combine signature")
        .split_once('(')
        .expect("parameter list")
        .1
        .rsplit_once(')')
        .expect("parameter list closes")
        .0;
    let parameters: Vec<&str> = signature.split(", ").collect();
    assert_eq!(parameters.len(), 3, "three reference parameters: {combine}");
    for parameter in parameters {
        assert!(
            parameter.starts_with("ptr "),
            "a reference parameter is one pointer: {parameter}"
        );
        assert!(
            parameter.rsplit(' ').next().is_some_and(|name| name
                .strip_prefix("%v")
                .is_some_and(|ordinal| ordinal.bytes().all(|byte| byte.is_ascii_digit()))),
            "and is named by its ordinal: {parameter}"
        );
    }
    assert!(!llvm.contains("call ptr @malloc"));
    assert!(!llvm.contains("call void @free"));
}

#[test]
fn a_projected_window_target_is_formed_once_before_rhs() {
    let source = br#"struct Columns {
  left: Box<Array<u16>>;
  right: Box<Array<u16>>;
}

fn replacement() -> result: u16 pure {
  return 9_u16;
}

fn update(columns: Columns) -> result: Columns pure {
  let spare = columns.left.inner.len;
  let ok = 1_u64 < spare;
  if ok {
    set columns.left.inner[1_u64] = replacement();
  }
  return move columns;
}

fn main() -> status: std::process::ExitStatus pure {
  let left = box_array_filled::<u16>(count: 2_u64, value: 0_u16);
  let right = box_array_filled::<u16>(count: 2_u64, value: 0_u16);
  let columns = Columns(left: move left, right: move right);
  let updated = update(columns: move columns);
  let updated_room = updated.left.inner.len;
  let updated_ok = 1_u64 < updated_room;
  if updated_ok {
    let value = updated.left.inner[1_u64];
    if value != 9_u16 {
      return std::process::exit_status(code: 1_u8);
    }
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    // `Columns`, two Box pointers, returns in registers, so the projections
    // are in `update`'s destination-form body (compiler/src/backend/abi.rs).
    let update = emitted_body(&llvm, "update");
    // The length read projects the field once for the explicit control. The
    // target captures the complete element address once before the RHS, and
    // the store uses that address without rereading its parent [SET-1].
    //
    // STOR-1 places the descriptor in the one heap object. Capture therefore
    // loads the Box pointer from its field slot, then forms an address into
    // that object; it no longer loads a by-value {pointer, length} descriptor.
    let columns = format!("getelementptr inbounds {},", super::nominal_type("Columns"));
    assert_eq!(update.matches(&columns).count(), 2);
    let guard = update
        .find("icmp ult i64")
        .expect("the explicit control must test the projected window length");
    let rhs = update
        .find("call i16 @wf_replacement")
        .expect("the RHS must execute once");
    let store = update
        .find("store i16")
        .expect("the target must receive one store");
    assert_eq!(update.matches("call i16 @wf_replacement").count(), 1);
    let captured = update
        .rfind(" = load ptr, ptr ")
        .expect("the projected Box pointer must be captured");
    let pointer = update[..captured]
        .lines()
        .next_back()
        .expect("captured pointer definition")
        .trim();
    assert!(guard < captured && captured < rhs && rhs < store);
    assert_eq!(update[guard..rhs].matches(" = load ptr, ptr ").count(), 1);
    let address = update[captured..rhs]
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_suffix(&format!(" = getelementptr i8, ptr {pointer}, i64 0"))
        })
        .expect("the captured pointer forms the array address before the RHS");
    let element_projection = format!(
        " = getelementptr inbounds {{ i64, [0 x i16] }}, ptr {address}, i64 0, i32 1, i64 "
    );
    assert_eq!(update.matches(&element_projection).count(), 1);
    let element = update[captured..rhs]
        .lines()
        .find(|line| line.contains(&element_projection))
        .and_then(|line| line.trim().split_once(" = ").map(|(result, _)| result))
        .expect("the element address uses the captured array address before the RHS");
    let target = update[captured..rhs]
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_suffix(&format!(" = getelementptr i8, ptr {element}, i64 0"))
        })
        .expect("the complete typed target is captured before the RHS");
    assert_eq!(update.matches("store i16").count(), 1);
    assert!(
        update[store..]
            .lines()
            .next()
            .unwrap()
            .ends_with(&format!("ptr {target}"))
    );
    assert!(!update[rhs..store].contains("load ptr, ptr "));
    assert!(!update[rhs..store].contains(&columns));
    assert!(!update.contains("call void @wf_trap"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn nested_struct_cleanup_releases_every_run_field() {
    let source = br#"struct Pair {
  first: Box<Slots<u8>>;
  second: Box<Slots<u16>>;
}

struct Owner {
  prefix: Box<Slots<u32>>;
  pair: Pair;
  suffix: Box<Slots<u64>>;
}

fn release(owner: Owner) -> result: unit pure {
  doc "Holds the whole nested owner and nothing else, so its one return edge carries exactly four cell releases.";
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = box_slots_new::<u8>(capacity: 1_u64);
  let second = box_slots_new::<u16>(capacity: 1_u64);
  let pair = Pair(first: move first, second: move second);
  let prefix = box_slots_new::<u32>(capacity: 1_u64);
  let suffix = box_slots_new::<u64>(capacity: 1_u64);
  let owner = Owner(prefix: move prefix, pair: move pair, suffix: move suffix);
  release(owner: move owner);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    // `release` holds the whole nested owner and nothing else, so its one
    // return edge carries exactly four run releases. Allocation identities
    // and their order are checked by the owned-place execution controls.
    let release = emitted_function(&llvm, "release");
    assert_eq!(release.matches("call void @free").count(), 4);
    // Allocation is total [STOR-8], so `main` has no refusal arm to hold a
    // partly built owner on: its one edge hands the whole owner to `release`
    // and carries no release of its own.
    let main = emitted_function(&llvm, "main");
    assert_eq!(main.matches("call void @free").count(), 0);
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn a_projected_run_move_releases_only_residual_siblings() {
    let source = br#"struct Pair {
  first: Box<Slots<u8>>;
  second: Box<Slots<u8>>;
}

struct Owner {
  prefix: Box<Slots<u8>>;
  pair: Pair;
  suffix: Box<Slots<u8>>;
}

fn take(owner: Owner) -> result: Box<Slots<u8>> pure {
  doc "Takes one field out; [WIN-3] consumes the whole owner, so the three residual siblings take their compiler-derived release here.";
  return move owner.pair.first;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = box_slots_new::<u8>(capacity: 1_u64);
  let second = box_slots_new::<u8>(capacity: 1_u64);
  let pair = Pair(first: move first, second: move second);
  let prefix = box_slots_new::<u8>(capacity: 1_u64);
  let suffix = box_slots_new::<u8>(capacity: 1_u64);
  let owner = Owner(prefix: move prefix, pair: move pair, suffix: move suffix);
  let retained = take(owner: move owner);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let take = emitted_function(&llvm, "take");
    // Three residual siblings released where the projected field left
    // [WIN-3, PROV-6].
    assert_eq!(take.matches("call void @free").count(), 3);
    // One retained cell released in `main`. Allocation is total [STOR-8], so
    // there are no refusal arms holding partly built owners.
    assert_eq!(
        emitted_function(&llvm, "main")
            .matches("call void @free")
            .count(),
        1
    );
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn trivially_droppable_affine_elements_keep_the_single_free() {
    let source = br#"nocopy enum Maybe {
  Missing();
  Present(value: u32);
}

fn main() -> status: std::process::ExitStatus pure {
  let slots = box_slots_new::<Maybe>(capacity: 4_u64);
  for @fill (
    at in 0_u64..4_u64,
    invariant grown: slots.inner.len >= at,
    invariant spare: slots.inner.cap + at >= slots.inner.len + 4_u64
  ) {
    let empty = Maybe::Missing();
    place_back(window: &slots.inner, value: move empty);
  }
  let occupied = Maybe::Present(value: 7_u32);
  set slots.inner[2_u64] = move occupied;
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    // An element type whose own release derives no action keeps the composite
    // action exactly the one cell free [STOR-3, PROV-6]: the release graph has
    // no edge to the elements, so no per-element loop is generated.
    assert!(!llvm.contains("@wf.drop.buffer"));
    assert!(!llvm.contains("@wf.drop.run"));
    let main = emitted_function(&llvm, "main");
    assert_eq!(main.matches("call void @free").count(), 1);
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn a_runtime_capacity_window_op9_overflow_is_rejected_before_lowering() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let slots = box_slots_new::<Option<u32>>(capacity: 18446744073709551615_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile_rejection(source);
    assert_eq!(failure.rule_id(), Some("OP-9"));
    assert!(
        failure
            .to_string()
            .contains("]: UndischargedAllocationFitObligation\n")
    );
}

/// [FN-2, OP-9, STOR-3] a concrete instance discovered through a second
/// generic caller retains the source-proved allocation bound, stores and
/// retrieves the element, and releases the now-empty allocation.
#[test]
fn a_transitive_generic_allocation_executes_and_releases_its_concrete_value() {
    let source = br#"fn store<T>(value: T) -> result: T pure {
  let cells = box_slots_new::<T>(capacity: 1_u64);
  place_back(window: &cells.inner, value: move value);
  let output = take_back(window: &cells.inner);
  free_empty(window: move cells);
  return move output;
}

fn forward<T>(value: T) -> result: T pure {
  return store::<T>(value: move value);
}

fn main() -> status: std::process::ExitStatus pure {
  let output = forward::<u64>(value: 37_u64);
  if output != 37_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    assert_eq!(llvm.matches("call ptr @malloc").count(), 1);
    assert_eq!(llvm.matches("call void @free").count(), 1);

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
