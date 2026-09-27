//! [TYPE-9]'s `Array` as the backend emits it: constant array globals,
//! filled and converted construction [OP-13], the discharged subscript
//! [OP-4], indexed assignment [SET-1], the measure member reads [OP-15], and
//! the owning element's compiler-derived release [STOR-3, WIN-3].
//!
//! [TYPE-9] keeps `Array`, so this module's subject survives the v0.60
//! storage redesign and almost every case was retargeted rather than retired.
//! What moved in the sources:
//!
//! - `array<T, N>` is `Array<T, n>`; `FixedVector<T, N>` is `Slots<T, n>`;
//!   `box<T>` and `Box<'s, T>` are `Box<T>`, whose content is the ordinary
//!   field `inner` and never `deref` [TYPE-9]. `array_new::<T, n>(v)` is
//!   `array_filled::<T, n>(value: v)`, `fixed_vector::<T, n>()` is
//!   `slots_new::<T, n>()`, and `array_from_fixed` / `fixed_from_array` are
//!   `slots_into_array` / `slots_from_array` [OP-13].
//! - The `len_of` / `cap_of` / `room_of` / `head_of` former family retires.
//!   The successor is [OP-15]: `a.len` is a place form,
//!   not calls. An `Array` has no `head` cell at all [MSR-1], so the two
//!   standing `head_of` checks below went with the measure rather than being
//!   weakened into something the table still answers.
//! - `let old = replace a[i] = e;` retires with [SET-2] and [LIV-2]. Where the
//!   displaced value is observed the successor is [OP-11] `swap`, which
//!   exchanges two owned places without a hole and leaves both roots live;
//!   where it is not, it is the ordinary `set a[i] = e;` taking [WIN-3]'s
//!   disposition of the old value.
//! - `region { .. }`, region parameters, `arena_frame` and `arena_box` retire
//!   with [OWN-3], [OWN-4] and [STOR-4]. Allocation is total over one heap
//!   [STOR-8], so every `heap_box` / `heap_vector` refusal arm and every
//!   refusing allocation ledger below went with the `Result` the source can
//!   no longer write.
//! - `place_front` admits `Ring` alone [OP-10]. A front insertion into a
//!   `Slots` is `insert_at(window: &r, index: 0_u64, value: v)`, which moves
//!   the same boundary and produces the same order.
//!
//! Two subjects retired outright, each recorded beside what replaced it:
//!
//! - `general_run_elements_preserve_box_brands_across_region_polymorphic_calls`
//!   retired with [OWN-3], [OWN-4] and [STOR-4]. Its whole subject was that a
//!   heap `Box<'h, u64>` and an arena `Box<'a, u64>` nested three run layers
//!   deep keep two different release classes across a region-polymorphic
//!   call. [TYPE-9] gives `Box<T>` no brand and [STOR-8] gives the language
//!   one heap, so there is no second class to separate and no region to be
//!   polymorphic over. The three-layer nesting and the generic handoff that
//!   case also covered are kept by
//!   `general_run_elements_preserve_nested_owners_across_generic_calls`.
//! - `constant_typed_places_execute_shared_calls_and_nested_projections` kept
//!   its subject but lost its `retain` helper and that helper's
//!   returned-pointer ABI assertion, which retired with [OWN-6] and [OWN-14]:
//!   `fn retain<'r>(values: &'r T) -> result: &'r T` handed a region-tied
//!   loan back to its caller. The successor is [REF-1], which makes a
//!   reference a local name for a path, and [REF-3], which refuses a returned
//!   reference outright with the restructuring `return an index and let the
//!   caller form the reference`. The shared-reference call boundary that case
//!   observed is kept by the three reading helpers that remain.

use super::owned_places::retain_calls;
use super::system::with_ir;
use super::{compile, compile_and_run, compile_rejection, emitted_function};
use crate::target::{TargetLayout, TargetLayoutFailure, TargetObject, validate_program};

fn invariant_bounded_runtime_allocation(
    construction: &str,
    half_ceiling: u64,
    count_ceiling: u64,
) -> Vec<u8> {
    format!(
        r#"fn allocate(n: u64, half: u64) -> result: unit pure contract {{
  requires half <= {half_ceiling}_u64;
}} {{
  let doubled = half * 2_u64;
  let within = n <= doubled;
  if within {{
    invariant tight: n <= {count_ceiling}_u64;
    let values = {construction};
  }}
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
    .into_bytes()
}

/// [STOR-6] qualifies each accepted runtime allocation at its source call,
/// using the selected target's actual element stride and padded block header.
/// The retained bound comes from an invariant rather than a literal count.
/// Validation alone observes the oversized cases, so no impossible allocation
/// is executed.
#[test]
fn runtime_allocation_bounds_include_each_shape_header_at_target_qualification() {
    let host = TargetLayout::host().expect("the backend test runs on a supported host layout");
    for (shape, construction, header) in [
        (
            "Array",
            "box_array_filled::<u16>(count: n, value: 0_u16)",
            8_u64,
        ),
        ("Slots", "box_slots_new::<u16>(capacity: n)", 16_u64),
        ("Ring", "box_ring_new::<u16>(capacity: n)", 24_u64),
    ] {
        let boundary = invariant_bounded_runtime_allocation(construction, 500, 1_000);
        with_ir(&boundary, |program| {
            let exact_bytes = header + 2_000;
            let exact = host.with_runtime_allocation_limits_for_test(exact_bytes, 8);
            assert_eq!(validate_program(exact, program), Ok(()), "{shape}");

            let one_byte_short = host.with_runtime_allocation_limits_for_test(exact_bytes - 1, 8);
            assert_eq!(
                validate_program(one_byte_short, program),
                Err(TargetLayoutFailure::Unrepresentable(
                    TargetObject::RuntimeSizedAllocation
                )),
                "{shape}"
            );
        });

        let oversized = invariant_bounded_runtime_allocation(
            construction,
            2_500_000_000_000_000_000,
            5_000_000_000_000_000_000,
        );
        with_ir(&oversized, |program| {
            assert_eq!(
                validate_program(host, program),
                Err(TargetLayoutFailure::Unrepresentable(
                    TargetObject::RuntimeSizedAllocation
                )),
                "{shape}"
            );
        });
    }
}

#[test]
fn structural_copy_aggregates_keep_independent_storage_after_generic_substitution() {
    let source = br#"struct Pair {
  values: Array<u8, 2>;
  tag: u8;
}

fn pass<T>(value: T) -> result: T pure {
  return move value;
}

fn change(pair: &Pair) -> result: unit writes(pair.values) {
  set deref(pair).values[0_u64] = 9_u8;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let seed = array_filled::<u8, 2>(value: 3_u8);
  let original = Pair(values: seed, tag: 5_u8);
  let twin = pass::<Pair>(value: original);
  set seed[0_u64] = 8_u8;
  change(pair: &original);
  set twin.values[1_u64] = 7_u8;
  let heap = box_new::<Pair>(value: twin);
  let extracted = heap.inner;
  set extracted.tag = 99_u8;
  if original.values[0_u64] == 9_u8 {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  if original.values[1_u64] == 3_u8 {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  if twin.values[0_u64] == 3_u8 {
  } else {
    return std::process::exit_status(code: 3_u8);
  }
  if twin.values[1_u64] == 7_u8 {
  } else {
    return std::process::exit_status(code: 4_u8);
  }
  if heap.inner.tag == 5_u8 {
  } else {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = compile(source);
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// A complete aggregate written through a reference or into a window remains
/// a storage copy. Loading the value into SSA before either write makes LLVM's
/// first SROA pass split the nested array into thousands of scalar
/// instructions; the native compiler then spends the program-test deadline
/// compiling one ordinary transfer. The same typed `memmove` used by other
/// stored-value snapshots preserves overlap and representation padding without
/// that expansion.
#[test]
fn referenced_aggregate_writes_remain_typed_storage_copies() {
    let source = br#"struct Record {
  bytes: Array<u8, 64>;
}

fn replace(target: &Record, value: Record) -> result: unit writes(target) {
  set deref(target) = value;
  return unit;
}

fn append_record(target: &Slots<Record, 2>, value: Record) -> result: unit writes(target) contract {
  requires deref(target).len < deref(target).cap;
} {
  place_back(window: target, value: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let old_bytes = array_filled::<u8, 64>(value: 1_u8);
  let target = Record(bytes: old_bytes);
  let new_bytes = array_filled::<u8, 64>(value: 2_u8);
  let value = Record(bytes: new_bytes);
  replace(target: &target, value: value);
  let records = slots_new::<Record, 2>();
  append_record(target: &records, value: target);
  if target.bytes[63_u64] == 2_u8 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    let module = compile(source);
    let replace = emitted_function(&module, "replace");
    assert!(
        replace.contains("call void @llvm.memmove.p0.p0.i64"),
        "{replace}"
    );
    assert!(
        !replace.lines().any(|line| {
            let line = line.trim_start();
            line.contains(" = load %wf.t") || line.starts_with("store %wf.t")
        }),
        "stored aggregates must not cross the write as SSA values: {replace}"
    );
    let marker = module
        .find("@wf_place_back$instance$")
        .expect("the aggregate place_back instance must be emitted");
    let start = module[..marker]
        .rfind("define ")
        .expect("the place_back definition must start");
    let end = module[start..]
        .find("\n}\n")
        .map(|offset| start + offset + 2)
        .expect("the place_back definition must close");
    let place_back = &module[start..end];
    assert!(
        place_back.contains("call void @llvm.memmove.p0.p0.i64"),
        "{place_back}"
    );
    assert!(
        !place_back.lines().any(|line| {
            let line = line.trim_start();
            line.contains(" = load %wf.t") || line.starts_with("store %wf.t")
        }),
        "window element transfers must not cross SSA: {place_back}"
    );
}

#[test]
fn ordinary_generic_readers_execute_inline_and_boxed_window_values() {
    let source = br#"enum SmallBytes<const n: u64> {
  Inline(values: Slots<u8, n>);
  Spilled(values: Box<Slots<u8>>);
}

fn checksum<const n: u64>(bytes: &SmallBytes<n>) -> result: u64 reads(bytes) {
  let result = 0_u64;
  match deref(bytes) {
    Inline(values: run) => {
      let length = deref(run).len;
      for (index in 0_u64..length) {
        let byte = deref(run)[index];
        let word = cvt::<u8, u64>(byte);
        let prefix = result *wrap 31_u64;
        set result = prefix +wrap word;
      }
    }
    Spilled(values: run) => {
      let length = deref(run).inner.len;
      for (index in 0_u64..length) {
        let byte = deref(run).inner[index];
        let word = cvt::<u8, u64>(byte);
        let prefix = result *wrap 31_u64;
        set result = prefix +wrap word;
      }
    }
  }
  return result;
}

fn read<const n: u64>(bytes: &SmallBytes<n>) -> result: u64 reads(bytes) {
  return checksum::<n>(bytes: bytes);
}

fn main() -> status: std::process::ExitStatus pure {
  let cell = box_slots_new::<u8>(capacity: 4_u64);
  place_back(window: &cell.inner, value: 7_u8);
  place_back(window: &cell.inner, value: 11_u8);
  let spilled = SmallBytes<4>::Spilled(values: move cell);
  let observed = read::<4>(bytes: &spilled);
  if observed != 228_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let inline_window = slots_new::<u8, 4>();
  place_back(window: &inline_window, value: 19_u8);
  place_back(window: &inline_window, value: 23_u8);
  let rotated = remove_at(window: &inline_window, index: 0_u64);
  place_back(window: &inline_window, value: rotated);
  let small = SmallBytes<4>::Inline(values: move inline_window);
  let local = read::<4>(bytes: &small);
  if local != 732_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        // Ordinary visibility plus noinline preserves the pointer
        // ABI, including the helper-to-helper call, under host optimization.
        let module = retain_calls(&super::emit_lowered(source, overlap))
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        // KEPT AS WRITTEN for the lowering port: the `$` marks the emitted
        // instantiation suffix of a generic source helper. The two helpers
        // lost their region parameter but keep their one const argument, so
        // the mangling to re-derive is the const-only instance name.
        for helper in [" @wf_read$", " @wf_checksum$"] {
            let headers: Vec<_> = module
                .lines()
                .filter(|line| line.starts_with("define ") && line.contains(helper))
                .collect();
            assert!(!headers.is_empty(), "missing instantiated helper {helper}");
            assert!(headers.iter().all(|line| line.contains(" noinline ")));
        }
        let observer = super::owned_places::allocation_observer(1, 0);
        let output = super::compile_link_and_run(&module, Some(&observer), &[]);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        // KEPT AS WRITTEN for the lowering port: the one boxed window is the
        // only heap object the program owns, and it is released exactly once.
        assert_eq!(output.stdout, b"A1;F1;", "{overlap:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
fn constant_typed_places_execute_shared_calls_and_nested_projections() {
    let source = br#"struct Entry {
  tag: u64;
  samples: Array<Array<u64, 2>, 2>;
}

const entries: Array<Entry, 2> =[Entry(tag: 17_u64, samples:[[19_u64, 23_u64],[29_u64, 31_u64]]), Entry(tag: 37_u64, samples:[[41_u64, 43_u64],[47_u64, 53_u64]])];

const stored_entry: Entry = Entry(tag: 59_u64, samples:[[61_u64, 67_u64],[71_u64, 73_u64]]);

fn read(values: &Array<Entry, 2>, outer: u64, row: u64, column: u64) -> result: u64 reads(values) contract {
  requires outer < 2_u64;
  requires row < 2_u64;
  requires column < 2_u64;
} {
  return deref(values)[outer].samples[row][column];
}

fn read_row(values: &Array<u64, 2>, index: u64) -> result: u64 reads(values) contract {
  requires index < 2_u64;
} {
  return deref(values)[index];
}

fn read_entry(value: &Entry) -> result: u64 reads(value.tag) {
  return deref(value).tag;
}

fn main() -> status: std::process::ExitStatus pure {
  if entries[1_u64].samples[1_u64][1_u64] != 53_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if stored_entry.tag != 59_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  let length = stored_entry.samples[1_u64].len;
  if length != 2_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  let first = read(values: &entries, outer: 0_u64, row: 0_u64, column: 1_u64);
  let trailing = read(values: &entries, outer: 1_u64, row: 1_u64, column: 1_u64);
  if first != 23_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if trailing != 53_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  let projected = read_row(values: &stored_entry.samples[1_u64], index: 0_u64);
  if projected != 71_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  let tag = read_entry(value: &stored_entry);
  if tag != 59_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    // Independent calls with storage unavailable to the WF optimizer retain
    // the shared-reference argument ABI. The WF caller above separately
    // supplies actual immutable globals through the same helpers.
    //
    // The fourth helper of the v0.59 fixture, `retain`, returned its own
    // `&'r` argument so the host could compare the returned pointer with the
    // one it passed. That helper and its pointer-identity check retired with
    // [OWN-6] and [OWN-14]: the successor is [REF-1]'s path naming and
    // [REF-3]'s outright refusal of a returned reference, carrying the
    // restructuring `return an index and let the caller form the reference`.
    let observer = r#"#include <stdint.h>
#include <stdlib.h>
struct Entry { uint64_t tag; uint64_t samples[2][2]; };
extern uint64_t wf_read(const struct Entry *, uint64_t, uint64_t, uint64_t);
extern uint64_t wf_read_row(const uint64_t *, uint64_t);
extern uint64_t wf_read_entry(const struct Entry *);
__attribute__((constructor)) static void check_shared_abi(void) {
    const struct Entry values[2] = {
        {101, {{103, 107}, {109, 113}}},
        {127, {{131, 137}, {139, 149}}}
    };
    if (wf_read(values, 1, 1, 0) != 139) exit(82);
    if (wf_read_row(values[0].samples[1], 1) != 113) exit(83);
    if (wf_read_entry(&values[1]) != 127) exit(84);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        // Keep an externally callable pointer ABI as well as call boundaries:
        // noinline alone still lets IPSCCP specialize an internal helper to
        // this caller's one constant global and remove its pointer argument.
        let module = retain_calls(&super::emit_lowered(source, overlap));
        for helper in ["read", "read_row", "read_entry"] {
            assert!(
                emitted_function(&module, helper)
                    .lines()
                    .next()
                    .expect("helper definition")
                    .contains(" noinline ")
            );
        }
        let output = super::compile_link_and_run(&module, Some(observer), &[]);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
fn nested_constant_arrays_emit_complete_recursive_global_layouts() {
    let source = br#"struct Entry {
  tag: u64;
  samples: Array<u64, 2>;
}

const rows: Array<Array<u64, 2>, 2> =[[7_u64, 9_u64],[11_u64, 13_u64]];

const entries: Array<Entry, 2> =[Entry(tag: 17_u64, samples:[19_u64, 23_u64]), Entry(tag: 29_u64, samples:[31_u64, 37_u64])];

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    // This observes recursive global layout independently of the WF access
    // test above, using the host's ordinary C array and struct layout.
    //
    // `@.wf_const.` and the digest of the constant's name, and `private
    // unnamed_addr constant`, are the emitted constant-pool spelling the
    // rename below depends on [MOD-8].
    let module = compile(source)
        .replace(&super::constant_global("rows"), "@wf_test_rows")
        .replace(&super::constant_global("entries"), "@wf_test_entries")
        .replace("= private unnamed_addr constant", "= constant");
    let observer = r#"#include <stdint.h>
#include <stdlib.h>
struct Entry { uint64_t tag; uint64_t samples[2]; };
extern const uint64_t wf_test_rows[2][2];
extern const struct Entry wf_test_entries[2];
__attribute__((constructor)) static void check_global_layout(void) {
    if (wf_test_rows[0][0] != 7 || wf_test_rows[0][1] != 9 ||
        wf_test_rows[1][0] != 11 || wf_test_rows[1][1] != 13) exit(81);
    if (wf_test_entries[0].tag != 17 || wf_test_entries[0].samples[0] != 19 ||
        wf_test_entries[0].samples[1] != 23 || wf_test_entries[1].tag != 29 ||
        wf_test_entries[1].samples[0] != 31 || wf_test_entries[1].samples[1] != 37) exit(82);
}
"#;
    let output = super::compile_link_and_run(&module, Some(observer), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// The v0.59 case placed its third record with `place_front` on a
/// `FixedVector`. [OP-10] admits `place_front` on a `Ring` alone; the same
/// boundary move on a `Slots` is `insert_at(index: 0_u64)`, which produces the
/// same element order, so the observed order is unchanged.
///
/// Its element exchange was `let old = replace values[1_u64] = e;`. [SET-2]
/// retires; [OP-11] `swap` is the successor that still hands the displaced
/// owner back, exchanging two owned places with no hole and no dead root.
#[test]
fn full_owning_arrays_preserve_insert_order_exchange_and_exact_cleanup() {
    let source = br#"struct Record {
  payload: Array<u64, 16>;
  owner: Box<u64>;
}

fn make_record(tag: u64) -> result: Record pure {
  let payload = array_filled::<u64, 16>(value: tag);
  let owner = box_new::<u64>(value: tag);
  return Record(payload: payload, owner: move owner);
}

fn seal(values: Slots<Record, 3>) -> result: Array<Record, 3> pure contract {
  requires values.len == 3_u64;
} {
  return slots_into_array::<Record, 3>(values: move values);
}

fn reopen(values: Array<Record, 3>) -> result: Slots<Record, 3> pure contract {
  ensures result.len == 3_u64;
} {
  let full = slots_from_array::<Record, 3>(values: move values);
  return move full;
}

fn relay<T: drop>(values: T) -> result: T pure {
  return move values;
}

fn read(values: &Array<Record, 3>, index: u64) -> result: u64 reads(values) contract {
  requires index < 3_u64;
} {
  return deref(values)[index].payload[7_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  let first = make_record(tag: 11_u64);
  let second_tag = first.payload[0_u64] +wrap 11_u64;
  let second = make_record(tag: second_tag);
  let third_tag = second.payload[0_u64] +wrap 11_u64;
  let third = make_record(tag: third_tag);
  let built = slots_new::<Record, 3>();
  place_back(window: &built, value: move first);
  place_back(window: &built, value: move second);
  insert_at(window: &built, index: 0_u64, value: move third);
  let values = seal(values: move built);
  let a = read(values: &values, index: 0_u64);
  let b = read(values: &values, index: 1_u64);
  let c = read(values: &values, index: 2_u64);
  if a != 33_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if b != 11_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if c != 22_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let fresh = make_record(tag: 99_u64);
  swap(first: &values[1_u64], second: &fresh);
  let Record(payload: old_payload, owner: old_owner) = move fresh;
  if old_payload[7_u64] != 11_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if old_owner.inner != 11_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  let passed = relay::<Array<Record, 3>>(values: move values);
  let full = reopen(values: move passed);
  let dense = seal(values: move full);
  let a_again = read(values: &dense, index: 0_u64);
  let b_again = read(values: &dense, index: 1_u64);
  let c_again = read(values: &dense, index: 2_u64);
  if a_again != 33_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if b_again != 99_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if c_again != 22_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let host = super::owned_places::allocation_observer(4, 0);
        let output = super::compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        // KEPT AS WRITTEN for the lowering port: the four cells are the three
        // records and the exchanged one. `swap` leaves the array holding
        // A3, A4, A2 and the destructured `fresh` holding A1, and reverse
        // declaration order releases the array's slots before that owner.
        assert_eq!(
            output.stdout, b"A1;A2;A3;A4;F3;F4;F2;F1;",
            "{overlap:?}: {output:?}"
        );
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// The v0.59 case drove the abandonment with a refused `heap_box`: each
/// `Result` arm returned the store's refusal and the partly built window had
/// to release exactly the prefix it held. [STOR-8] makes allocation total in
/// the source: it never returns a failure and no allocating operation
/// carries a `Result`, so the refusal half retired with [BLK-2]'s fallible
/// store take and there is no `X` marker left to observe.
///
/// The surviving half is the one this case was really about: the compiler
/// derived release [STOR-3] of a partly built owning window on an early
/// leaving edge frees each initialized element exactly once and nothing else.
/// An ordinary source-written failure arm reaches the same edges.
#[test]
fn abandoned_full_array_construction_releases_each_initialized_prefix_once() {
    let source = br#"struct Record {
  tag: u64;
  owner: Box<u64>;
}

fn relay<T: drop>(value: T) -> result: T pure {
  return move value;
}

fn build(stop: u64) -> result: Result<Array<Record, 3>, u64> pure {
  let empty = slots_new::<Record, 3>();
  let first_owner = box_new::<u64>(value: 11_u64);
  let first_record = Record(tag: 11_u64, owner: move first_owner);
  place_back(window: &empty, value: move first_record);
  if stop == 1_u64 {
    return Err<Array<Record, 3>, u64>(error: 11_u64);
  }
  let second_owner = box_new::<u64>(value: 22_u64);
  let second_record = Record(tag: 22_u64, owner: move second_owner);
  place_back(window: &empty, value: move second_record);
  if stop == 2_u64 {
    return Err<Array<Record, 3>, u64>(error: 22_u64);
  }
  let third_owner = box_new::<u64>(value: 33_u64);
  let third_record = Record(tag: 33_u64, owner: move third_owner);
  place_back(window: &empty, value: move third_record);
  if stop == 3_u64 {
    return Err<Array<Record, 3>, u64>(error: 33_u64);
  }
  let values = slots_into_array::<Record, 3>(values: move empty);
  let passed = relay::<Array<Record, 3>>(value: move values);
  return Ok<Array<Record, 3>, u64>(value: move passed);
}

fn main() -> status: std::process::ExitStatus pure {
  let one = build(stop: 1_u64);
  match move one {
    Ok(value: unexpected_one) => {
      return std::process::exit_status(code: 71_u8);
    }
    Err(error: refused_one) => {
      if refused_one != 11_u64 {
        return std::process::exit_status(code: 61_u8);
      }
    }
  }
  let two = build(stop: 2_u64);
  match move two {
    Ok(value: unexpected_two) => {
      return std::process::exit_status(code: 72_u8);
    }
    Err(error: refused_two) => {
      if refused_two != 22_u64 {
        return std::process::exit_status(code: 62_u8);
      }
    }
  }
  let three = build(stop: 3_u64);
  match move three {
    Ok(value: unexpected_three) => {
      return std::process::exit_status(code: 73_u8);
    }
    Err(error: refused_three) => {
      if refused_three != 33_u64 {
        return std::process::exit_status(code: 63_u8);
      }
    }
  }
  let complete = build(stop: 0_u64);
  match move complete {
    Ok(value: values) => {
      if values[0_u64].tag != 11_u64 {
        return std::process::exit_status(code: 1_u8);
      }
      if values[1_u64].tag != 22_u64 {
        return std::process::exit_status(code: 2_u8);
      }
      if values[2_u64].tag != 33_u64 {
        return std::process::exit_status(code: 3_u8);
      }
    }
    Err(error: unexpected_complete) => {
      return std::process::exit_status(code: 74_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        // Each abandoned prefix is released before the next build allocates,
        // so allocation IDs identify source slots. The observer aborts on a
        // duplicate or unknown release, so the ledger also pins "once".
        let host = super::owned_places::allocation_observer(9, 0);
        let output = super::compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        // KEPT AS WRITTEN for the lowering port: the release order inside one
        // abandoned window, and inside the completed array, is logical index
        // order. The completed run's `A7;A8;A9;F7;F8;F9;` is the v0.59
        // ledger's own success row unchanged.
        assert_eq!(
            output.stdout,
            b"A1;F1;A2;A3;F2;F3;A4;A5;A6;F4;F5;F6;A7;A8;A9;F7;F8;F9;".as_slice(),
            "{overlap:?}: {output:?}"
        );
        assert!(output.stderr.is_empty(), "{overlap:?}: {output:?}");
    }
}

/// Counts the stack slots one emitted function declares, and how many of those
/// declarations sit outside its entry block.
///
/// A slot declared anywhere else is reached once per execution of its block, so
/// a slot inside a loop grows the frame once per iteration. The emitter must
/// therefore keep every declaration in the entry block, which runs exactly once
/// per call, and leave only the store at the use site.
fn slot_declarations(function: &str) -> (usize, usize) {
    let mut in_entry = false;
    let mut total = 0;
    let mut outside_entry = 0;
    for line in function.lines() {
        if let Some(label) = line.strip_suffix(':')
            && !label.starts_with(' ')
        {
            in_entry = label == "entry";
            continue;
        }
        if line.contains(" = alloca ") {
            total += 1;
            if !in_entry {
                outside_entry += 1;
            }
        }
    }
    (total, outside_entry)
}

/// The v0.59 case carried a heap `Box<'h, u64>` and an arena `Box<'a, u64>`
/// through the same helpers so the two release classes could be separated.
/// Regions and arenas retire with [OWN-3], [OWN-4] and [STOR-4], and [TYPE-9]
/// gives `Box<T>` no brand, so one owner over the one heap [STOR-8] is the
/// whole population now.
#[test]
fn full_arrays_preserve_boxed_element_ownership_through_generic_helpers() {
    let source = br#"struct Record {
  tag: u64;
  owner: Box<u64>;
}

fn pass<T>(value: T) -> result: T pure {
  return move value;
}

fn make(owner: Box<u64>, tag: u64) -> result: Array<Record, 1> pure {
  let record = Record(tag: tag, owner: move owner);
  let empty = slots_new::<Record, 1>();
  place_back(window: &empty, value: move record);
  return slots_into_array::<Record, 1>(values: move empty);
}

fn relay(values: Array<Record, 1>) -> result: Array<Record, 1> pure {
  let passed = pass::<Array<Record, 1>>(value: move values);
  let full = slots_from_array::<Record, 1>(values: move passed);
  return slots_into_array::<Record, 1>(values: move full);
}

fn main() -> status: std::process::ExitStatus pure {
  let owner = box_new::<u64>(value: 17_u64);
  let held = make(owner: move owner, tag: 17_u64);
  let returned = relay(values: move held);
  let tag = returned[0_u64].tag;
  if tag != 17_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let host = super::owned_places::allocation_observer(1, 0);
        let output = super::compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        // KEPT AS WRITTEN for the lowering port: the element owner crosses
        // two generic boundaries and both window conversions, and is released
        // exactly once when the returned array dies.
        assert_eq!(output.stdout, b"A1;F1;", "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// The v0.59 case also read out the displaced element with
/// `let previous = replace empty_returned[1_u64] = Empty();`. [SET-2] retires
/// and nothing here observes the old value, so the successor is the ordinary
/// `set` whose old occupant takes [WIN-3]'s disposition, an empty action for
/// a zero-byte element.
#[test]
fn full_array_zero_extents_and_zero_byte_elements_execute_without_payload_access() {
    let source = br#"struct Empty {
}

struct Mixed {
  tag: u64;
  empty: Empty;
}

struct Recursive {
  children: Array<Recursive, 0>;
}

fn relay<T: drop>(value: T) -> result: T pure {
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  let none = slots_new::<Box<u64>, 0>();
  let zero_owners = slots_into_array::<Box<u64>, 0>(values: move none);
  let zero_returned = relay::<Array<Box<u64>, 0>>(value: move zero_owners);
  let zero_run = slots_from_array::<Box<u64>, 0>(values: move zero_returned);
  let zero_again = slots_into_array::<Box<u64>, 0>(values: move zero_run);
  let first = Empty();
  let second = Empty();
  let empty = slots_new::<Empty, 2>();
  place_back(window: &empty, value: first);
  insert_at(window: &empty, index: 0_u64, value: second);
  let empty_values = slots_into_array::<Empty, 2>(values: move empty);
  let empty_returned = relay::<Array<Empty, 2>>(value: empty_values);
  set empty_returned[1_u64] = Empty();
  let empty_run = slots_from_array::<Empty, 2>(values: empty_returned);
  let empty_again = slots_into_array::<Empty, 2>(values: move empty_run);
  let recursion = slots_new::<Recursive, 0>();
  let children = slots_into_array::<Recursive, 0>(values: move recursion);
  let node = Recursive(children: children);
  let carried = relay::<Recursive>(value: node);
  let mixed_empty_seed = Empty();
  let mixed_seed = Mixed(tag: 7_u64, empty: mixed_empty_seed);
  let mixed = array_filled::<Mixed, 2>(value: mixed_seed);
  set mixed[1_u64].tag = 19_u64;
  set mixed[1_u64].empty = Empty();
  let mixed_empty = mixed[1_u64].empty;
  if mixed[0_u64].tag != 7_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if mixed[1_u64].tag != 19_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  let zero_length = zero_again.len;
  let empty_length = empty_again.len;
  if zero_length != 0_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if empty_length != 2_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = retain_calls(&super::emit_lowered(source, overlap));
        // Empty fields touch no bytes, so the native tag observations alone
        // cannot detect wrongly collapsing their nonzero-stride outer step.
        let mixed_type = module
            .lines()
            .find_map(|line| line.split_once(" = type { i64, %wf.t"))
            .map(|(name, _)| name)
            .expect("Mixed has a tag and an Empty field");
        let field_step = format!("getelementptr inbounds {mixed_type}, ptr ");
        let mut empty_element_steps = 0;
        for line in module.lines().filter(|line| line.ends_with("i32 1")) {
            let Some((_, operands)) = line.split_once(&field_step) else {
                continue;
            };
            let (parent, _) = operands.split_once(',').expect("field pointer operand");
            // ProjectAddress gives the selected element an ordinary i8-GEP
            // identity before the following field projection uses it.
            let alias = format!("{parent} = getelementptr i8, ptr ");
            let parent = module
                .lines()
                .find_map(|line| line.trim_start().strip_prefix(&alias))
                .and_then(|operands| operands.split_once(','))
                .map_or(parent, |(pointer, _)| pointer);
            let element_step = format!("{parent} = getelementptr inbounds [2 x {mixed_type}],");
            if let Some(step) = module
                .lines()
                .find(|line| line.trim_start().starts_with(&element_step))
            {
                let (_, index) = step.rsplit_once("i64 ").expect("element index");
                let one = format!("{index} = select i1 true, i64 1, i64 1");
                assert!(
                    index == "1" || module.lines().any(|line| line.trim() == one),
                    "Mixed[1].empty must retain element one: {step}"
                );
                empty_element_steps += 1;
            }
        }
        assert!(empty_element_steps > 0, "observe an indexed Empty field");
        let output = compile_and_run(&module);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
fn const_runs_are_immutable_globals_and_execute_through_index_and_len() {
    let llvm = compile(include_bytes!(
        "../../../../tests/conformance/cases/const2-pos-array-lookup.wf"
    ));
    // KEPT AS WRITTEN for the lowering port: the constant pool's LLVM type and
    // element spelling for a four-byte `Array<u8, 4>` global.
    assert!(llvm.contains(&format!(
        "{} = private unnamed_addr constant [4 x i8] [i8 10, i8 20, i8 30, i8 40]",
        super::constant_global("table")
    )));
    let main = emitted_function(&llvm, "main");
    // The constant lookup is discharged [OP-4]: no bounds compare remains.
    // The source's two terminal result checks are ordinary control flow, not
    // claims, so all three outcomes return an ExitStatus without a trap edge.
    assert!(!main.contains("icmp ult i64"));
    assert_eq!(main.matches("icmp eq").count(), 2);
    assert_eq!(
        main.matches("call void @wf_std.process.exit_status")
            .count(),
        3
    );
    assert!(!main.contains("call void @wf_trap"));
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// Copy-fill remains a distinct array operation [OP-13]. Its emitted fill
/// region and the proved read across retained source function boundaries are
/// both checked; owning-element construction instead uses the consuming
/// full-window conversion `slots_into_array`.
#[test]
fn filled_arrays_cross_function_boundaries_and_keep_a_checked_read() {
    let source = br#"fn make() -> result: Array<u16, 4> pure {
  return array_filled::<u16, 4>(value: 42_u16);
}

fn clamp_three(value: u64) -> result: u64 pure contract {
  ensures result < 4_u64;
} {
  if value < 4_u64 {
    return value;
  } else {
    return 3_u64;
  }
}

fn read(values: Array<u16, 4>, offset: u64) -> result: u16 pure {
  let bounded = clamp_three(value: offset);
  let value = values[bounded];
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = make();
  let length = values.len;
  if length != 4_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let value = read(values: values, offset: 3_u64);
  if value != 42_u16 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let read = emitted_function(&llvm, "read");
    // The verified callee summary discharges the subscript directly: the
    // caller emits no runtime fallback and no second bounds branch.
    assert!(!read.contains("icmp ult i64"));
    assert!(!read.contains("call void @wf_trap"));
    // KEPT AS WRITTEN for the lowering port: the emitted element address type
    // for an inline `Array<u16, 4>`, and the two labels the copy-fill region
    // emits.
    assert!(read.contains("getelementptr inbounds [4 x i16]"));
    assert!(llvm.contains("array.fill.head"));
    assert!(llvm.contains("array.fill.done"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn an_out_of_bounds_run_read_is_an_op4_compile_rejection() {
    // Under discharge-or-reject [OP-4] no runtime bounds trap exists: the
    // underivable obligation rejects at compile time with the exact
    // [ENT-6] residual, rendered in [OP-15]'s measure member form.
    let source = br#"const values: Array<u8, 2> =[7_u8, 7_u8];

fn main() -> status: std::process::ExitStatus pure {
  let value = values[2_u64];
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile_rejection(source);
    assert_eq!(failure.rule_id(), Some("OP-4"));
    assert!(failure.detail().contains("2_u64 < values.len"));
}

#[test]
fn indexed_set_checks_before_rhs_and_updates_the_run() {
    let source = br#"fn replacement() -> result: u8 pure {
  return 9_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u8, 2>();
  place_back(window: &values, value: 0_u8);
  place_back(window: &values, value: 0_u8);
  set values[1_u64] = replacement();
  let stored = values[1_u64];
  if stored != 9_u8 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let main = emitted_function(&llvm, "main");
    // The discharged target carries no bounds branch [OP-4]; SET-1's order
    // remains: the RHS call is emitted after target formation and the store
    // after the RHS.
    //
    // KEPT AS WRITTEN for the lowering port: the RHS call and the element
    // store are matched by their emitted spellings.
    let rhs = main
        .find("call i8 @wf_replacement")
        .expect("RHS must be emitted after target evaluation");
    assert!(
        !main[..rhs].contains("call void @wf_trap"),
        "no bounds trap edge precedes the RHS"
    );
    let store = main[rhs..]
        .find("store i8")
        .map(|offset| rhs + offset)
        .expect("run element update must store only after the RHS");
    assert!(rhs < store);

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn an_out_of_bounds_indexed_set_is_an_op4_compile_rejection() {
    // A target whose obligation is underivable cannot reach runtime: the
    // program rejects at the subscript with the residual [OP-4, ENT-6].
    let source = br#"fn replacement() -> result: u8 pure {
  return 9_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u8, 2>();
  place_back(window: &values, value: 0_u8);
  place_back(window: &values, value: 0_u8);
  set values[2_u64] = replacement();
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile_rejection(source);
    assert_eq!(failure.rule_id(), Some("OP-4"));
    assert!(failure.detail().contains("2_u64 < values.len"));
}

#[test]
fn a_long_loop_over_a_dynamically_indexed_run_keeps_the_frame_bounded() {
    // Both the read and the indexed set need a stack slot for the run value,
    // and the index is not a compile-time constant, so neither slot can be
    // promoted away. The structural half is what discriminates: no slot may be
    // declared outside the entry block, so a frame that grew once per
    // iteration fails here whatever it would survive. The run is a
    // corroboration rather than the measurement; 200000 iterations of a
    // 64-byte slot is about 12 MB, which used to be past a default 8 MB limit
    // and now fits inside the 1 GiB stack the runtime gives every thread. A
    // window's length is not a fact of its type [WIN-1], so the two loops
    // carry the `len` invariant the array place had standing. The fill loop's
    // third v0.59 invariant, `head_of(built) <= 0_u64`, retired with the
    // measure: [MSR-1] gives `Slots` no `head` cell at all, and the window it
    // would have pinned to zero begins at slot zero by [WIN-1].
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  doc "Nested counted loops read and write one fixed run for two hundred thousand iterations.";
  let built = slots_new::<u64, 8>();
  for @fill (
    at in 0_u64..8_u64,
    invariant grown: built.len >= at,
    invariant spare: built.cap + at >= built.len + 8_u64
  ) {
    place_back(window: &built, value: 1_u64);
  }
  let completed = 0_u64;
  let total = 0_u64;
  for @outer (
    batch in 0_u64..25000_u64,
    invariant held: built.len >= 8_u64
  ) {
    for @inner (
      cursor in 0_u64..8_u64,
      invariant still: built.len >= 8_u64
    ) {
      let previous = built[cursor];
      let mixed = ixor(previous, completed);
      set built[cursor] = mixed *wrap 1099511628211_u64;
      set total = total +wrap previous;
      set completed = completed +wrap 1_u64;
    }
  }
  if completed != 200000_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let main = emitted_function(&llvm, "main");
    let (total, outside_entry) = slot_declarations(main);
    assert!(
        total > 0,
        "the kernel must still need stack slots for this test to mean anything:\n{main}"
    );
    assert_eq!(
        outside_entry, 0,
        "{outside_entry} of {total} stack slots are declared outside the entry block, so the frame grows once per iteration:\n{main}"
    );

    let output = compile_and_run(&llvm);
    assert!(
        output.status.success(),
        "the loop must run to completion instead of exhausting the stack: {:?}",
        output.status
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn nested_struct_run_update_uses_the_element_address_prepared_before_the_rhs() {
    let source = br#"struct Inner {
  values: Slots<u8, 2>;
  sibling: u16;
}

struct Outer {
  prefix: u32;
  inner: Inner;
}

fn replacement() -> result: u8 pure {
  return 9_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let base = array_filled::<u8, 2>(value: 0_u8);
  let values = slots_from_array::<u8, 2>(values: base);
  let inner = Inner(values: move values, sibling: 77_u16);
  let outer = Outer(prefix: 123_u32, inner: move inner);
  set outer.inner.values[1_u64] = replacement();
  let stored = outer.inner.values[1_u64];
  if stored != 9_u8 {
    return std::process::exit_status(code: 1_u8);
  }
  if outer.inner.sibling != 77_u16 {
    return std::process::exit_status(code: 2_u8);
  }
  if outer.prefix != 123_u32 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let main = emitted_function(&llvm, "main");
    // The discharged projected target carries no bounds branch [OP-4].
    //
    // KEPT AS WRITTEN for the lowering port: the RHS call spelling, the
    // `getelementptr` that prepares the complete element address once, the
    // `store i8` of the RHS result, and the `store %wf.t` shape that would
    // mark a rebuilt enclosing struct.
    let rhs = main
        .find("call i8 @wf_replacement")
        .expect("RHS must follow projected target evaluation");
    assert!(
        !main[..rhs].contains("call void @wf_trap"),
        "no bounds trap edge precedes the RHS"
    );
    assert_eq!(main.matches("call i8 @wf_replacement").count(), 1);
    let replacement = main[..rhs]
        .lines()
        .next_back()
        .expect("RHS result definition")
        .trim()
        .strip_suffix(" =")
        .expect("RHS assigns its result");
    let store_text = format!("store i8 {replacement}, ptr ");
    let store = main
        .find(&store_text)
        .expect("one element receives the RHS value");
    assert_eq!(main.matches(&store_text).count(), 1);
    let address = main[store..]
        .lines()
        .next()
        .expect("element store")
        .strip_prefix(&store_text)
        .expect("element address operand");
    let prepared = main
        .find(&format!("{address} = getelementptr "))
        .expect("the complete element address is prepared once");
    assert!(prepared < rhs && rhs < store);
    assert!(
        !main[rhs..store].contains("store %wf.t"),
        "the element write must not rebuild its enclosing structs"
    );

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// The v0.59 case also read `head_of(rows[0_u64])` and required it to be zero,
/// and x1 retires the `cap` and `room` reads beside it: [MSR-1]'s table now
/// gives `Array<T, N>` only a `len` cell, an array being its own extent with
/// no second capacity quantity and no window origin. Their successor is the
/// `len` observation this case keeps, over the same element writes and reads.
#[test]
fn general_run_elements_preserve_array_places_and_standing_extents() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let row = array_filled::<u64, 2>(value: 7_u64);
  let rows = slots_new::<Array<u64, 2>, 2>();
  place_back(window: &rows, value: row);
  let width = rows[0_u64].len;
  set rows[0_u64][1_u64] = 9_u64;
  if rows[0_u64][0_u64] != 7_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if rows[0_u64][1_u64] != 9_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if width != 2_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [crate::OverlapLowering::Off, crate::OverlapLowering::On] {
        let llvm = super::emit_lowered(source, overlap);
        let output = compile_and_run(&llvm);
        assert!(output.status.success(), "{overlap:?}: {output:?}");
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

/// An owning element crosses more than the former one-level run lift. The
/// allocation ledger observes the complete nested window and generic handoff,
/// rather than only whether a deeply nested type can be named.
#[test]
fn general_run_elements_preserve_nested_owners_across_generic_calls() {
    let source = br#"fn pass<T>(value: T) -> result: T pure {
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = box_new::<u64>(value: 17_u64);
  let second = box_new::<u64>(value: 29_u64);
  let leaf = slots_new::<Box<u64>, 2>();
  place_back(window: &leaf, value: move first);
  place_back(window: &leaf, value: move second);
  let middle = slots_new::<Slots<Box<u64>, 2>, 1>();
  place_back(window: &middle, value: move leaf);
  let outer = slots_new::<Slots<Slots<Box<u64>, 2>, 1>, 1>();
  place_back(window: &outer, value: move middle);
  let carried = pass::<Slots<Slots<Slots<Box<u64>, 2>, 1>, 1>>(value: move outer);
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let host = super::owned_places::u64_allocation_observer(2);
        let output = super::compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        // STOR-3 releases elements in logical index order. PAR-1 permits the
        // two independent allocations to reach the host in either order, so
        // their payloads identify the elements even when allocation IDs swap.
        // The observer still rejects unknown and duplicate releases.
        assert_eq!(output.stdout, b"A1;A2;V17;V29;", "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

// Retired with [OWN-3], [OWN-4] and [STOR-4]:
// `general_run_elements_preserve_box_brands_across_region_polymorphic_calls`
// had the region-tied brand as its whole subject: a heap `Box<'h, u64>` and
// an arena `Box<'a, u64>` nested three run layers deep, carried through one
// region-polymorphic helper, keeping two different release classes.
// v0.60 has no regions, no arenas and no brand: [TYPE-9] states that a `Box`
// carries none and [STOR-8] gives the language one heap, so the two classes
// this case separated are now one. The three-layer nesting and the generic
// handoff it shared with the case above are kept by
// `general_run_elements_preserve_nested_owners_across_generic_calls`.

/// A window's descriptor has finite layout even when its elements own another
/// window of the same node type, because a runtime-capacity shape lives only
/// as the content of a `Box` [TYPE-9] and a cell is one pointer. The release
/// graph follows actual initialized windows.
///
/// The v0.59 case drove two refused `heap_vector` takes to observe cleanup of
/// the already constructed child. [STOR-8] makes allocation total and no
/// construction returns a `Result` [OP-13], so the two refusal rows retired
/// with [BLK-2]'s fallible store take.
#[test]
fn general_run_elements_close_recursive_descriptor_layout_and_cleanup() {
    let source = br#"struct Tree {
  children: Box<Slots<Tree>>;
}

fn build() -> result: Tree pure {
  let empty_children = box_slots_new::<Tree>(capacity: 1_u64);
  let child = Tree(children: move empty_children);
  let parent_children = box_slots_new::<Tree>(capacity: 1_u64);
  place_back(window: &parent_children.inner, value: move child);
  let root = Tree(children: move parent_children);
  return move root;
}

fn main() -> status: std::process::ExitStatus pure {
  let tree = build();
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let host = super::owned_places::allocation_observer(2, 0);
        let output = super::compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        // KEPT AS WRITTEN for the lowering port: the child's cell is
        // allocated first and released first, the recursive walk reaching it
        // through the parent's one initialized slot.
        assert_eq!(output.stdout, b"A1;A2;F1;F2;", "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// A complete owning array can reside in a `Box` while its element owners
/// survive helper calls, shared reads and an exchange.
///
/// The v0.59 case placed the array with a fallible `heap_box` and observed the
/// returned array on refusal; [STOR-8] makes allocation total, so that arm
/// retired with the `Result` the source can no longer write. Its exchange was
/// `let previous = replace deref(storage)[0_u64] = e;` delivered through a
/// two-place `set`. [SET-2] and [LIV-2] both retire, because [SET-1] writes exactly
/// one place, and [OP-11] `swap` is the successor that still hands the
/// displaced owner back.
#[test]
fn heap_full_arrays_preserve_elements_across_calls_and_exchange() {
    let source = br#"struct Record {
  payload: Array<u64, 16>;
  owner: Box<u64>;
}

fn make_record(tag: u64) -> result: Record pure {
  let payload = array_filled::<u64, 16>(value: tag);
  let owner = box_new::<u64>(value: tag);
  return Record(payload: payload, owner: move owner);
}

fn relay<T: drop>(value: T) -> result: T pure {
  return move value;
}

fn read(storage: &Box<Array<Record, 2>>, index: u64) -> result: u64 reads(storage) contract {
  requires index < 2_u64;
} {
  return deref(storage).inner[index].payload[7_u64];
}

fn pass_box(value: Box<Array<Record, 2>>) -> result: Box<Array<Record, 2>> pure {
  return relay::<Box<Array<Record, 2>>>(value: move value);
}

fn update(storage: &Box<Array<Record, 2>>, index: u64, replacement: &Record) -> result: unit writes(storage.inner[index]), writes(replacement) contract {
  requires index < 2_u64;
} {
  swap(first: &deref(storage).inner[index], second: replacement);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = make_record(tag: 11_u64);
  let second_tag = first.payload[0_u64] +wrap 11_u64;
  let second = make_record(tag: second_tag);
  let built = slots_new::<Record, 2>();
  place_back(window: &built, value: move first);
  place_back(window: &built, value: move second);
  let values = slots_into_array::<Record, 2>(values: move built);
  let storage = box_new::<Array<Record, 2>>(value: move values);
  let passed = pass_box(value: move storage);
  let before = read(storage: &passed, index: 0_u64);
  if before != 11_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let replacement = make_record(tag: 99_u64);
  update(storage: &passed, index: 0_u64, replacement: &replacement);
  if replacement.payload[7_u64] != 11_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  let first_tag = read(storage: &passed, index: 0_u64);
  let remaining_tag = read(storage: &passed, index: 1_u64);
  if first_tag != 99_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if remaining_tag != 22_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        // The second record depends on the first, and the cell consumes both,
        // so allocation IDs identify these source owners.
        let host = super::owned_places::allocation_observer(4, 0);
        let output = super::compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        // KEPT AS WRITTEN for the lowering port: after the exchange the local
        // `replacement` holds A1 and the cell's array holds A4 then A2, and
        // reverse declaration order releases that local before the cell.
        assert_eq!(
            output.stdout,
            b"A1;A2;A3;A4;F1;F4;F2;F3;".as_slice(),
            "{overlap:?}: {output:?}"
        );
        assert!(output.stderr.is_empty(), "{overlap:?}: {output:?}");
    }
}

/// Runtime-capacity arrays carry complete inline element types through generic
/// construction, typed ranges, whole-element copies, and nested writes.
#[test]
fn runtime_arrays_preserve_nested_fixed_array_storage_and_release() {
    let source = br#"fn make<T: copy>(value: T) -> result: Box<Array<T>> pure contract {
  ensures result.inner.len == 2_u64;
} {
  let rows = box_array_filled::<T>(count: 2_u64, value: value);
  return move rows;
}

fn read(rows: &[Array<u64, 2>], index: u64) -> result: u64 reads(rows) contract {
  requires index < deref(rows).len;
} {
  return deref(rows)[index][1_u64];
}

fn update(row: &Array<u64, 2>) -> result: unit writes(row) {
  set deref(row)[1_u64] = 19_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let seed = array_filled::<u64, 2>(value: 7_u64);
  let rows = make::<Array<u64, 2>>(value: seed);
  set rows.inner[1_u64][0_u64] = 11_u64;
  update(row: &rows.inner[1_u64]);
  if rows.inner[0_u64][0_u64] != 7_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  if rows.inner[0_u64][1_u64] != 7_u64 {
    return std::process::exit_status(code: 9_u8);
  }
  let snapshot = rows.inner[1_u64];
  set rows.inner[0_u64] = snapshot;
  set snapshot[1_u64] = 23_u64;
  if rows.inner[0_u64][0_u64] != 11_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if rows.inner[0_u64][1_u64] != 19_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if rows.inner[1_u64][0_u64] != 11_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  let observed = read(rows: &rows.inner[0_u64..2_u64], index: 1_u64);
  if observed != 19_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if seed[0_u64] != 7_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  let empty = box_array_filled::<Array<u64, 2>>(count: 0_u64, value: seed);
  if empty.inner.len != 0_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  let empty_seed = array_filled::<u64, 0>(value: 0_u64);
  let zero_width = box_array_filled::<Array<u64, 0>>(count: 3_u64, value: empty_seed);
  if zero_width.inner[2_u64].len != 0_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = retain_calls(&compile(source))
        .replace("@malloc(", "@wf_test_allocate(")
        .replace("@free(", "@wf_test_release(");
    let observer = super::owned_places::allocation_observer(3, 0);
    let output = super::compile_link_and_run(&module, Some(&observer), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"A1;A2;A3;F3;F2;F1;", "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// An ordinary callable receives the same header-first runtime owner from the
/// native host. This exercises affine element cleanup independently of the
/// copy-only fill constructor.
#[test]
fn runtime_arrays_release_nested_fixed_array_owners_in_element_order() {
    let source =
        br#"fn release_rows(values: Box<Array<Array<Box<u64>, 2>>>) -> result: u64 pure contract {
  requires values.inner.len == 2_u64;
} {
  return values.inner[1_u64][1_u64].inner;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = retain_calls(&compile(source))
        .replace("@malloc(", "@wf_test_allocate(")
        .replace("@free(", "@wf_test_release(");
    let observer = format!(
        "{}\n{}",
        super::owned_places::allocation_observer(5, 0),
        r#"#include <stdint.h>
struct Rows { uint64_t len; uint64_t *items[2][2]; };
extern uint64_t wf_release_rows(struct Rows *);
__attribute__((constructor)) static void check_nested_cleanup(void) {
    struct Rows *rows = wf_test_allocate(sizeof(*rows));
    rows->len = 2;
    const uint64_t payloads[4] = {17, 19, 23, 29};
    for (unsigned i = 0; i < 4; ++i) {
        uint64_t *cell = wf_test_allocate(sizeof(*cell));
        *cell = payloads[i];
        rows->items[i / 2][i % 2] = cell;
    }
    if (wf_release_rows(rows) != 29) abort();
}
"#
    );
    let output = super::compile_link_and_run(&module, Some(&observer), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        output.stdout, b"A1;A2;A3;A4;A5;F2;F3;F4;F5;F1;",
        "{output:?}"
    );
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// Inspect ordinary, pre-optimization address operands. The enormous logical
/// count is never executed; zero stride must be handled before LLVM can erase
/// the fill loop, and the range retains its independent logical length.
#[test]
fn zero_stride_large_logical_indices_use_representable_address_operands() {
    let source = br#"struct Empty {
}

fn inspect(values: &[Empty]) -> result: Empty reads(values) contract {
  requires deref(values).len == 1_u64;
} {
  return deref(values)[0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  let empty = Empty();
  let values = box_array_filled::<Empty>(count: 9223372036854775809_u64, value: empty);
  let last = values.inner[9223372036854775808_u64];
  set values.inner[9223372036854775808_u64] = Empty();
  let observed = inspect(values: &values.inner[9223372036854775808_u64..9223372036854775809_u64]);
  let fixed = array_filled::<Empty, 9223372036854775809>(value: empty);
  let fixed_last = fixed[9223372036854775808_u64];
  let nested_seed = array_filled::<u64, 0>(value: 0_u64);
  let nested = box_array_filled::<Array<u64, 0>>(count: 9223372036854775809_u64, value: nested_seed);
  let nested_length = nested.inner[9223372036854775808_u64].len;
  let window = slots_from_array::<Empty, 9223372036854775809>(values: fixed);
  let returned = slots_into_array::<Empty, 9223372036854775809>(values: move window);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    assert!(
        llvm.contains("9223372036854775809"),
        "logical count survives"
    );
    // Each family must actually be present, and every element operand must
    // be literal zero. Checking only for a large literal would miss the
    // ordinary SSA register carrying that same logical index.
    for (family, marker) in [
        (
            "runtime buffer and nested zero array",
            "getelementptr inbounds { i64, [0 x ",
        ),
        (
            "fixed fill and indexed place",
            "getelementptr inbounds [9223372036854775809 x ",
        ),
        (
            "range adjustment and range element",
            "getelementptr inbounds %wf.t",
        ),
        ("whole-element transfer size", "getelementptr %wf.t"),
    ] {
        let steps: Vec<_> = llvm
            .lines()
            .filter(|line| line.contains(" = ") && line.contains(marker))
            // Header-only projections end in i32; the element step has an
            // i64 final operand, including its generated loop register.
            .filter(|line| {
                line.rsplit_once(", ")
                    .is_some_and(|(_, last)| last.starts_with("i64 "))
            })
            .collect();
        assert!(!steps.is_empty(), "missing {family} address coverage");
        for step in steps {
            assert!(step.ends_with("i64 0"), "{family} must use zero: {step}");
        }
    }
    let out_of_bounds = std::str::from_utf8(source).expect("source text").replace(
        "let last = values.inner[9223372036854775808_u64];",
        "let last = values.inner[9223372036854775809_u64];",
    );
    assert_eq!(
        compile_rejection(out_of_bounds.as_bytes()).rule_id(),
        Some("OP-4")
    );
}

#[test]
fn zero_stride_allocation_still_qualifies_headers_and_nonzero_controls() {
    let host = TargetLayout::host().expect("supported target");
    for (element, value, header) in [("Empty", "Empty()", 8_u64), ("u8", "0_u8", 8_u64)] {
        let source = format!(
            "struct Empty {{\n}}\n\nfn allocate() -> result: unit pure {{\n  let seed = {value};\n  let values = box_array_filled::<{element}>(count: 9223372036854775809_u64, value: seed);\n  return unit;\n}}\n"
        );
        with_ir(source.as_bytes(), |program| {
            if element == "Empty" {
                assert_eq!(validate_program(host, program), Ok(()));
                let exact = host.with_runtime_allocation_limits_for_test(header, 8);
                assert_eq!(validate_program(exact, program), Ok(()));
                let short = host.with_runtime_allocation_limits_for_test(header - 1, 8);
                assert!(matches!(
                    validate_program(short, program),
                    Err(TargetLayoutFailure::Unrepresentable(_))
                ));
                // Narrow only the address domain, retaining enough space for
                // the ordinary prelude's concrete nominal representations.
                let narrow = host.with_address_index_max_for_test(i32::MAX as u64);
                let llvm = crate::backend::emitter::emit_llvm_with_layout(program, narrow)
                    .expect("logical count does not occupy the address domain")
                    .into_string();
                let element_steps: Vec<_> = llvm
                    .lines()
                    .filter(|line| line.contains("getelementptr") && line.contains("i32 1, i64"))
                    .collect();
                assert!(!element_steps.is_empty(), "observe narrow-target addresses");
                assert!(element_steps.iter().all(|line| line.ends_with("i64 0")));
            } else {
                assert_eq!(
                    validate_program(host, program),
                    Err(TargetLayoutFailure::Unrepresentable(
                        TargetObject::RuntimeSizedAllocation
                    ))
                );
            }
        });
    }
}
