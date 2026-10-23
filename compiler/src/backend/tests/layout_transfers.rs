//! Experimental transfer widths and overlap semantics. These run in the
//! backend CI group; source editing uses only prebuilt --check admission.

use super::system::with_ir;
use super::{compile_link_and_run, emit, emitted_body, emitted_function, nominal_type};
use crate::IrType;
use crate::target::{TargetLayout, TransferAccess, bounded_transfer};

const SOURCE: &[u8] = br#"struct Frame {
  a: u64;
  b: u64;
  c: u32;
  d: i64;
  e: u32;
  f: u8;
  g: u64;
  h: u64;
}

enum Value {
  Nil();
  Num(n: f64);
  H1(h: u32);
  H2(h: u32);
  H3(h: u32);
}

enum Product {
  One(value: u8);
  Two(value: u8);
}

enum ByteChoice {
  First();
  Second();
  Third();
}

enum ByteOverlay {
  Choice(value: ByteChoice);
  Pair(lo: u8, hi: u8);
  Other(value: u8);
}

enum ByteValue {
  FirstByte(value: u8);
  SecondByte(value: u8);
  ThirdByte(value: u8);
}

enum HalfValue {
  FirstHalf(value: u16);
  SecondHalf(value: u16);
  ThirdHalf(value: u16);
}

enum MixedPrefix {
  WidePrefix(value: u64);
  BytePrefix(value: u8);
  WordPrefix(value: u32);
}

enum BitChoice {
  BitOff();
  BitOn();
}

struct Large {
  bytes: Array<u8, 129>;
}

struct SmallArray {
  bytes: Array<u8, 8>;
}

struct Cell {
  inner: u64;
}

nocopy enum Overlay {
  Owner(value: Box<Cell>);
  Bits(lo: u32, hi: u32);
  Flag(value: Bool);
}

struct ManyLeaves {
  a: u8;
  b: u8;
  c: u8;
  d: u8;
  e: u8;
  f: u8;
  g: u8;
  h: u8;
  i: u8;
  j: u8;
  k: u8;
  l: u8;
  m: u8;
  n: u8;
  o: u8;
  p: u8;
  q: u8;
}

struct Boundary {
  a: u64;
  b: u64;
  c: u64;
  d: u64;
  e: u64;
  f: u64;
  g: u64;
  h: u64;
  i: u64;
  j: u64;
  k: u64;
  l: u64;
  m: u64;
  n: u64;
  o: u64;
  p: u64;
}

struct LargeScalars {
  a: u64;
  b: u64;
  c: u64;
  d: u64;
  e: u64;
  f: u64;
  g: u64;
  h: u64;
  i: u64;
  j: u64;
  k: u64;
  l: u64;
  m: u64;
  n: u64;
  o: u64;
  p: u64;
  q: u64;
}

fn copy_boundary(target: &Boundary, value: Boundary) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn copy_many(target: &ManyLeaves, value: ManyLeaves) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn copy_scalars(target: &LargeScalars, value: LargeScalars) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn copy_frame(target: &Frame, value: Frame) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn copy_value(target: &Value, value: Value) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn copy_product(target: &Product, value: Product) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn copy_byte_overlay(target: &ByteOverlay, value: ByteOverlay) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn copy_byte_value(target: &ByteValue, value: ByteValue) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn copy_half_value(target: &HalfValue, value: HalfValue) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn nil_value() -> result: Value pure {
  return Value::Nil();
}

fn handle_value() -> result: Value pure {
  return Value::H2(h: 305419896_u32);
}

fn byte_value() -> result: ByteValue pure {
  return ByteValue::ThirdByte(value: 197_u8);
}

fn half_value() -> result: HalfValue pure {
  return HalfValue::ThirdHalf(value: 50021_u16);
}

fn mixed_prefix() -> result: MixedPrefix pure {
  return MixedPrefix::WidePrefix(value: 305419896_u64);
}

fn bit_choice() -> result: BitChoice pure {
  return BitChoice::BitOn();
}

fn byte_product() -> result: Product pure {
  return Product::Two(value: 193_u8);
}

fn copy_large(target: &Large, value: Large) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn copy_array(target: &SmallArray, value: SmallArray) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn copy_overlay(target: &Overlay, value: Overlay) -> result: unit writes(target) {
  set target^ = move value;
  return unit;
}

fn return_frame(value: &Frame) -> result: Frame reads(value) {
  return value^;
}

fn return_value(value: &Value) -> result: Value reads(value) {
  return value^;
}

struct Wrapped {
  prefix: u64;
  value: Value;
}

fn return_wrapped(value: &Wrapped) -> result: Wrapped reads(value) {
  return value^;
}

struct TenWords {
  a: u32;
  b: u32;
  c: u32;
  d: u32;
  e: u32;
  f: u32;
  g: u32;
  h: u32;
  i: u32;
  j: u32;
}

fn copy_ten(target: &TenWords, value: TenWords) -> result: unit writes(target) {
  set target^ = value;
  return unit;
}

fn return_ten(value: &TenWords) -> result: TenWords reads(value) {
  return value^;
}

fn make_frame_into(target: &Frame) -> result: u64 writes(target) {
  let value = Frame(a: 1_u64, b: 2_u64, c: 3_u32, d: 4_i64, e: 5_u32, f: 6_u8, g: 7_u64, h: 8_u64);
  set target^ = value;
  return value.a;
}

fn make_boundary_into(target: &Boundary) -> result: u64 writes(target) {
  let value = Boundary(a: 1_u64, b: 2_u64, c: 3_u64, d: 4_u64, e: 5_u64, f: 6_u64, g: 7_u64, h: 8_u64, i: 9_u64, j: 10_u64, k: 11_u64, l: 12_u64, m: 13_u64, n: 14_u64, o: 15_u64, p: 16_u64);
  set target^ = value;
  return value.a;
}

fn make_ten_into(target: &TenWords) -> result: u32 writes(target) {
  let value = TenWords(a: 1_u32, b: 2_u32, c: 3_u32, d: 4_u32, e: 5_u32, f: 6_u32, g: 7_u32, h: 8_u32, i: 9_u32, j: 10_u32);
  set target^ = value;
  return value.a;
}

fn make_value_into(target: &Value) -> result: u32 writes(target) {
  let value = Value::H1(h: 7_u32);
  set target^ = value;
  match value {
    H1(h: handle) => {
      return handle;
    }
    Nil() => {
      return 0_u32;
    }
    Num(..) => {
      return 0_u32;
    }
    H2(..) => {
      return 0_u32;
    }
    H3(..) => {
      return 0_u32;
    }
  }
}

fn make_many_into(target: &ManyLeaves) -> result: u8 writes(target) {
  let value = ManyLeaves(a: 1_u8, b: 2_u8, c: 3_u8, d: 4_u8, e: 5_u8, f: 6_u8, g: 7_u8, h: 8_u8, i: 9_u8, j: 10_u8, k: 11_u8, l: 12_u8, m: 13_u8, n: 14_u8, o: 15_u8, p: 16_u8, q: 17_u8);
  set target^ = value;
  return value.a;
}

fn make_scalars_into(target: &LargeScalars) -> result: u64 writes(target) {
  let value = LargeScalars(a: 1_u64, b: 2_u64, c: 3_u64, d: 4_u64, e: 5_u64, f: 6_u64, g: 7_u64, h: 8_u64, i: 9_u64, j: 10_u64, k: 11_u64, l: 12_u64, m: 13_u64, n: 14_u64, o: 15_u64, p: 16_u64, q: 17_u64);
  set target^ = value;
  return value.a;
}

fn make_array_into(target: &SmallArray) -> result: u8 writes(target) {
  let bytes = array_filled::<u8, 8>(value: 7_u8);
  let value = SmallArray(bytes: bytes);
  set target^ = value;
  return value.bytes[0_u64];
}

fn make_large_into(target: &Large) -> result: u8 writes(target) {
  let bytes = array_filled::<u8, 129>(value: 7_u8);
  let value = Large(bytes: bytes);
  set target^ = value;
  return value.bytes[0_u64];
}

fn observe_overlay(value: &Overlay) -> result: u32 reads(value) {
  match value^ {
    Bits(lo: low, hi: high) => {
      return low^;
    }
    Owner(..) => {
      return 0_u32;
    }
    Flag(..) => {
      return 0_u32;
    }
  }
}

fn observe_value(value: &Value) -> result: u32 reads(value) {
  match value^ {
    H1(h: handle) => {
      return handle^;
    }
    Nil() => {
      return 0_u32;
    }
    Num(..) => {
      return 0_u32;
    }
    H2(..) => {
      return 0_u32;
    }
    H3(..) => {
      return 0_u32;
    }
  }
}

fn return_local_value(seed: u32) -> result: Value pure {
  let inner = Value::H1(h: seed);
  let holder = Wrapped(prefix: 19_u64, value: inner);
  let observed = observe_value(value: &holder.value);
  if observed != seed {
    return Value::Nil();
  }
  return holder.value;
}

fn copy_local_ten(source: &TenWords, target: &TenWords) -> result: u32 reads(source), writes(target) {
  let local = source^;
  set target^ = local;
  return local.a;
}

fn update_local_ten(source: &TenWords, target: &TenWords, replacement: u32) -> result: u32 reads(source), writes(target) {
  let local = source^;
  set local.a = replacement;
  set target^ = local;
  return local.a;
}

fn make_overlay_into(target: &Overlay) -> result: u32 writes(target) {
  let value = Overlay::Bits(lo: 11_u32, hi: 22_u32);
  let observed = observe_overlay(value: &value);
  set target^ = move value;
  return observed;
}

fn main() -> status: std::process::ExitStatus pure {
  let target = TenWords(a: 0_u32, b: 0_u32, c: 0_u32, d: 0_u32, e: 0_u32, f: 0_u32, g: 0_u32, h: 0_u32, i: 0_u32, j: 0_u32);
  let observed = make_ten_into(target: &target);
  if observed != 1_u32 {
    return std::process::exit_status(code: 1_u8);
  }
  let returned = return_ten(value: &target);
  let copied = TenWords(a: 0_u32, b: 0_u32, c: 0_u32, d: 0_u32, e: 0_u32, f: 0_u32, g: 0_u32, h: 0_u32, i: 0_u32, j: 0_u32);
  copy_ten(target: &copied, value: returned);
  if copied.a != 1_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  if copied.b != 2_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  if copied.c != 3_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  if copied.d != 4_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  if copied.e != 5_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  if copied.f != 6_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  if copied.g != 7_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  if copied.h != 8_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  if copied.i != 9_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  if copied.j != 10_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  let local_result = return_local_value(seed: 37_u32);
  let observed_result = observe_value(value: &local_result);
  if observed_result != 37_u32 {
    return std::process::exit_status(code: 3_u8);
  }
  let updated = update_local_ten(source: &target, target: &copied, replacement: 41_u32);
  if updated != 41_u32 {
    return std::process::exit_status(code: 4_u8);
  }
  if copied.a != 41_u32 {
    return std::process::exit_status(code: 4_u8);
  }
  if copied.j != 10_u32 {
    return std::process::exit_status(code: 4_u8);
  }
  let kept = copy_local_ten(source: &target, target: &copied);
  if kept != 1_u32 {
    return std::process::exit_status(code: 5_u8);
  }
  if copied.a != 1_u32 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// Verify the actual memory instructions and addresses, not just a planner
/// result or absence of an intrinsic. A whole-size load, wrong width/offset,
/// copied padding, omitted leaf or interleaved stores fails this oracle.
pub(super) fn assert_typed_transfer(body: &str, expected: &[(u64, &str)]) {
    let (region, source, destination) = transfer_region(body);
    let mut pointers =
        std::collections::HashMap::from([(source, (source, 0)), (destination, (destination, 0))]);
    let mut loads = Vec::new();
    let mut stores = Vec::new();
    let mut stored = false;
    let mut values = std::collections::HashMap::new();
    for line in region.lines().map(str::trim) {
        if let Some((name, gep)) = line.split_once(" = getelementptr inbounds i8, ptr ") {
            let (base, offset) = gep.split_once(", i64 ").expect("byte offset");
            let offset = offset.parse::<u64>().expect("constant offset");
            assert!(base == source || base == destination, "{line}");
            pointers.insert(name, (base, offset));
        } else if let Some((name, load)) = line.split_once(" = load ") {
            assert!(!stored, "all source loads precede stores: {body}");
            let (ty, address) = load.split_once(", ptr ").expect("typed load");
            let address = address
                .strip_suffix(", align 1")
                .expect("conservative alignment");
            let &(base, offset) = pointers.get(address).expect("source granule pointer");
            assert_eq!(base, source, "load must read the source: {line}");
            loads.push((offset, ty));
            values.insert(name, (offset, ty));
        } else if let Some(store) = line.strip_prefix("store ") {
            stored = true;
            let (operand, address) = store.split_once(", ptr ").expect("typed store");
            let (ty, value) = operand.split_once(' ').expect("stored SSA value");
            let address = address
                .strip_suffix(", align 1")
                .expect("conservative alignment");
            let &(base, offset) = pointers.get(address).expect("destination granule pointer");
            assert_eq!(
                base, destination,
                "store must write the destination: {line}"
            );
            assert_eq!(values.get(value), Some(&(offset, ty)), "{line}");
            stores.push((offset, ty));
        } else {
            assert!(
                !line.contains("call "),
                "no memory intrinsic in typed transfer: {body}"
            );
        }
    }
    assert_eq!(loads, expected, "{body}");
    assert_eq!(stores, expected, "{body}");
}

pub(super) fn transfer_region(body: &str) -> (&str, &str, &str) {
    let prefix = "  ; layout-bounded transfer from ";
    let start = body.find(prefix).expect("bounded transfer");
    let text = &body[start + prefix.len()..];
    let (header, _) = text.split_once('\n').expect("transfer header");
    let (source, destination) = header.split_once(" to ").expect("transfer places");
    let end = text
        .find("  ; end layout-bounded transfer ")
        .expect("transfer end");
    (&text[header.len() + 1..end], source, destination)
}

#[test]
fn struct_and_union_transfers_respect_leaf_boundaries() {
    let module = emit(SOURCE);
    assert!(module.contains(&format!("{} = type {{ i32, [12 x i8], [0 x ", nominal_type("Value"))),
        "the raw carrier merges the four-byte prefix without changing its sixteen-byte extent: {module}");
    let frame = emitted_function(&module, "make_frame_into");
    assert_typed_transfer(
        frame,
        &[
            (0, "i64"),
            (8, "i64"),
            (16, "i32"),
            (24, "i64"),
            (32, "i32"),
            (36, "i8"),
            (40, "i64"),
            (48, "i64"),
        ],
    );
    assert!(
        !frame.contains("@llvm.memmove") && !frame.contains("@llvm.memcpy"),
        "{frame}"
    );
    let value = emitted_function(&module, "make_value_into");
    // Every view leaves bytes 1..4 vacant. Tag and common padding share
    // one four-byte prefix, without crossing the u32 boundary at 4.
    assert_typed_transfer(
        value,
        &[(0, "i32"), (4, "i32"), (8, "i64")],
    );
    assert!(
        !value.contains("@llvm.memmove") && !value.contains("@llvm.memcpy"),
        "{value}"
    );
    // The product has its byte tag and both declared byte payloads. Unlike
    // the union, it retains inactive payload fields but omits struct padding.
    let product = emitted_function(&module, "copy_product");
    assert_typed_transfer(product, &[(0, "i8"), (1, "i8"), (2, "i8")]);
    // A three-state tag-only payload uses a complete LLVM byte value.
    // It may use integer interval transport; the Bool overlay below may not.
    let byte_overlay = emitted_function(&module, "copy_byte_overlay");
    assert_typed_transfer(byte_overlay, &[(0, "i8"), (1, "i8"), (2, "i8")]);
    let byte_value = emitted_function(&module, "copy_byte_value");
    assert_typed_transfer(byte_value, &[(0, "i8"), (1, "i8")]);
    assert!(module.contains(&format!("{} = type {{ i8, [1 x i8], [0 x ", nominal_type("ByteValue"))),
        "a byte-aligned union remains exactly two bytes: {module}");
    assert_typed_transfer(emitted_function(&module, "copy_half_value"), &[(0, "i16"), (2, "i16")]);
    assert!(module.contains(&format!("{} = type {{ i16, [2 x i8], [0 x ", nominal_type("HalfValue"))),
        "one common padding byte gives a two-byte prefix in a four-byte union: {module}");
    // Inclusive limit: a 128-byte/16-leaf scalar record still expands. The
    // 17-byte ManyLeaves fixture independently detects a missing leaf bound;
    // LargeScalars exceeds both limits (current scalar leaves are <=8 bytes).
    let boundary = emitted_function(&module, "make_boundary_into");
    let expected: Vec<_> = (0..16).map(|leaf| (leaf * 8, "i64")).collect();
    assert_typed_transfer(boundary, &expected);
    assert!(
        !boundary.contains("@llvm.memmove") && !boundary.contains("@llvm.memcpy"),
        "{boundary}"
    );
    for name in [
        "copy_large",
        "copy_array",
        "copy_many",
        "copy_scalars",
        "make_many_into",
        "make_scalars_into",
        "make_array_into",
        "make_large_into",
    ] {
        let body = emitted_function(&module, name);
        assert!(body.contains("call void @llvm.memmove."), "{body}");
        assert!(!body.contains("; layout-bounded transfer "), "{body}");
    }
}

#[test]
fn incoming_and_external_transfers_keep_granules_across_call_boundaries() {
    let module = emit(SOURCE);
    let frame_fields = [
        (0, "i64"),
        (8, "i64"),
        (16, "i32"),
        (24, "i64"),
        (32, "i32"),
        (36, "i8"),
        (40, "i64"),
        (48, "i64"),
    ];
    // Value's count-sized byte tag shares the vacant leading bytes in one
    // four-byte prefix; Wrapped carries that same prefix at offset eight.
    let value_fields = [(0, "i32"), (4, "i32"), (8, "i64")];
    let wrapped_fields = [(0, "i64"), (8, "i32"), (12, "i32"), (16, "i64")];
    let ten_fields: Vec<_> = (0..10).map(|leaf| (leaf * 4, "i32")).collect();
    let boundary_fields: Vec<_> = (0..16).map(|leaf| (leaf * 8, "i64")).collect();
    // A caller can have written every field separately. Formal parameters,
    // reference snapshots and public result destinations retain that boundary.
    for (name, expected) in [
        ("copy_frame", frame_fields.as_slice()),
        ("return_frame", frame_fields.as_slice()),
        ("copy_value", value_fields.as_slice()),
        ("return_value", value_fields.as_slice()),
        ("return_wrapped", wrapped_fields.as_slice()),
        ("copy_boundary", boundary_fields.as_slice()),
        ("return_ten", ten_fields.as_slice()),
        ("copy_ten", ten_fields.as_slice()),
    ] {
        let body = emitted_function(&module, name);
        assert_typed_transfer(body, expected);
        assert_eq!(
            body.matches("  ; layout-bounded transfer from ").count(),
            1,
            "{body}"
        );
        assert!(
            !body.contains("@llvm.memmove") && !body.contains("@llvm.memcpy"),
            "{body}"
        );
        if name.starts_with("return_") {
            assert_eq!(transfer_region(body).2, "%wf.result", "{body}");
            assert!(!body.contains("alloca"), "{body}");
        }
    }
    // A reserved entry slot must not introduce a redundant incoming capture.
    let incoming = emitted_function(&module, "copy_ten");
    let (_, source, destination) = transfer_region(incoming);
    assert_eq!((source, destination), ("%wf.arg.v1", "%v0"), "{incoming}");
    for name in ["make_ten_into", "update_local_ten", "copy_local_ten"] {
        let body = emitted_function(&module, name);
        let regions: Vec<_> = body
            .match_indices("  ; layout-bounded transfer from ")
            .collect();
        assert!(!regions.is_empty(), "{body}");
        for (start, _) in regions {
            assert_typed_transfer(&body[start..], &ten_fields);
        }
        assert!(
            !body.contains("@llvm.memmove") && !body.contains("@llvm.memcpy"),
            "{body}"
        );
    }
    // A local parent retains its union field independently of the public
    // destination. This distinguishes the rule from the former union-result
    // exception, which would suppress these producer-width loads.
    let local_result = emitted_function(&module, "return_local_value");
    let result_write = local_result
        .find(" to %wf.result\n")
        .expect("public result copy");
    let result_start = local_result[..result_write]
        .rfind("  ; layout-bounded transfer from ")
        .expect("producer transfer to the public result");
    let result_copy = &local_result[result_start..];
    assert_typed_transfer(result_copy, &[(0, "i32"), (4, "i32"), (8, "i64")]);
    assert_eq!(
        transfer_region(result_copy).2,
        "%wf.result",
        "{local_result}"
    );
    // Missing/truncated transfers fail the ten independently observed fields.
    for native in [module.clone(), super::owned_places::retain_calls(&module)] {
        let output = super::compile_and_run(&native);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}

#[test]
fn heap_union_snapshot_and_unchanged_call_result_use_the_public_destination() {
    let source = br#"enum Snapshot {
  Number(n: u64);
  Function(id: u64);
  Native(id: u64);
  Empty();
}

fn pick(values: &Box<Array<Snapshot>>, index: u64) -> result: Snapshot reads(values) contract {
  requires index < values^.inner.len;
} {
  return values^.inner[index];
}

fn relay(values: &Box<Array<Snapshot>>, index: u64) -> result: Snapshot reads(values) contract {
  requires index < values^.inner.len;
} {
  return pick(values: values, index: index);
}

fn classify(value: Snapshot) -> result: u64 pure {
  match value {
    Number(n: number) => {
      return number;
    }
    Function(id: function_id) => {
      return function_id +wrap 1_u64;
    }
    Native(id: native_id) => {
      return native_id +wrap 2_u64;
    }
    Empty() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let seed = Snapshot::Number(n: 17_u64);
  let values = box_array_filled::<Snapshot>(count: 2_u64, value: seed);
  let first = relay(values: &values, index: 0_u64);
  set values.inner[0_u64] = Snapshot::Function(id: 29_u64);
  let second = relay(values: &values, index: 0_u64);
  let first_observed = classify(value: first);
  let second_observed = classify(value: second);
  let untouched = relay(values: &values, index: 1_u64);
  let untouched_observed = classify(value: untouched);
  if first_observed != 17_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if second_observed != 30_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if untouched_observed != 17_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = emit(source);
    let pick = emitted_function(&module, "pick");
    assert_typed_transfer(pick, &[(0, "i32"), (4, "i32"), (8, "i64")]);
    assert_eq!(
        pick.matches("  ; layout-bounded transfer from ").count(),
        1,
        "{pick}"
    );
    assert_eq!(
        transfer_region(pick).2,
        "%wf.result",
        "the unavoidable heap snapshot writes directly into the result: {pick}"
    );
    assert!(
        !pick.contains("@llvm.memmove") && !pick.contains("@llvm.memcpy"),
        "{pick}"
    );
    assert!(!pick.contains("alloca"), "{pick}");
    let relay = emitted_function(&module, "relay");
    assert!(relay.contains("@wf_pick(ptr %wf.result,"), "{relay}");
    assert!(!relay.contains("alloca"), "{relay}");
    assert!(!relay.contains("@llvm.memmove"), "{relay}");
    assert!(!relay.contains("; layout-bounded transfer "), "{relay}");
    let retained = super::owned_places::retain_calls(&module);
    let output = super::compile_and_run(&retained);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn union_results_preserve_consumed_inputs_when_the_destination_aliases_them() {
    let source = br#"nocopy enum AliasedSnapshot {
  Number(n: u64);
  Function(id: u64);
  Native(id: u64);
  Empty();
}

fn observe_snapshot(value: AliasedSnapshot) -> result: u64 pure {
  match move value {
    Number(n: number) => {
      return number;
    }
    Function(id: function_id) => {
      return function_id +wrap 1_u64;
    }
    Native(id: native_id) => {
      return native_id +wrap 2_u64;
    }
    Empty() => {
      return 0_u64;
    }
  }
}

fn unchanged_snapshot(value: AliasedSnapshot) -> result: AliasedSnapshot pure {
  return move value;
}

fn replace_snapshot(value: AliasedSnapshot, observed: &u64) -> result: AliasedSnapshot writes(observed) {
  let replacement = AliasedSnapshot::Function(id: 99_u64);
  let original = observe_snapshot(value: move value);
  set observed^ = original;
  return move replacement;
}

fn main() -> status: std::process::ExitStatus pure {
  for (round in 0_u64..32_u64) {
    let seed = round +wrap 7_u64;
    let unchanged_input = AliasedSnapshot::Number(n: seed);
    let kept = unchanged_snapshot(value: move unchanged_input);
    let unchanged_observed = observe_snapshot(value: move kept);
    if unchanged_observed != seed {
      return std::process::exit_status(code: 1_u8);
    }
    let replacement_input = AliasedSnapshot::Number(n: seed);
    let original_observed = 0_u64;
    let replacement = replace_snapshot(value: move replacement_input, observed: &original_observed);
    let replacement_observed = observe_snapshot(value: move replacement);
    if original_observed != seed {
      return std::process::exit_status(code: 2_u8);
    }
    if replacement_observed != 100_u64 {
      return std::process::exit_status(code: 3_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = emit(source);
    let unchanged = emitted_function(&module, "unchanged_snapshot");
    let result_start = unchanged
        .match_indices("  ; layout-bounded transfer from ")
        .find(|&(start, _)| transfer_region(&unchanged[start..]).2 == "%wf.result")
        .map(|(start, _)| start)
        .expect("bounded union return into the public destination");
    let result_body = &unchanged[result_start..];
    assert_typed_transfer(result_body, &[(0, "i32"), (4, "i32"), (8, "i64")]);
    let (result_transfer, _, _) = transfer_region(result_body);
    assert!(
        !result_transfer.contains("!tbaa")
            && !result_transfer.contains("!alias.scope")
            && !result_transfer.contains("!noalias"),
        "the result/input overlap must remain admitted: {result_transfer}"
    );
    let replacement = emitted_function(&module, "replace_snapshot");
    let capture = replacement
        .lines()
        .position(|line| {
            ((line.contains("@llvm.memmove.") || line.contains("@llvm.memcpy."))
                && line.contains(", ptr %wf.arg.v0, i64 "))
                || line.contains("; end layout-bounded transfer from %wf.arg.v0 to ")
        })
        .expect("preserve the incoming snapshot before an aliased result write");
    let write = replacement
        .lines()
        .position(|line| {
            line.trim_start().starts_with("store ") && line.contains(", ptr %wf.result")
        })
        .expect("construct the new variant directly in the public result");
    let read = replacement
        .lines()
        .position(|line| line.contains("@wf_observe_snapshot("))
        .expect("observe the original after constructing the replacement");
    assert!(capture < write && write < read, "{replacement}");
    assert!(
        !replacement.contains("@wf_observe_snapshot(ptr %wf.arg.v0)"),
        "the later observation must use the captured snapshot: {replacement}"
    );
    let main = super::emitted_body(&module, "main");
    for name in ["unchanged_snapshot", "replace_snapshot"] {
        let call = main
            .lines()
            .find(|line| line.contains(&format!("@wf_{name}(")))
            .expect("main calls the union-result callee");
        let arguments = call
            .split_once(&format!("@wf_{name}("))
            .unwrap()
            .1
            .split(')')
            .next()
            .unwrap()
            .split(", ")
            .collect::<Vec<_>>();
        assert_eq!(
            arguments[0], arguments[1],
            "the union destination really aliases the consumed input: {call}"
        );
    }
    // An incorrect overlap promise can corrupt the unchanged Number result
    // (code 1). A missing/late capture reads Function(99) as 100 instead of
    // the original Number(7) in the first iteration (code 2); code 3 checks
    // that the replacement itself survives the original-input observation.
    for native in [module.clone(), super::owned_places::retain_calls(&module)] {
        let output = super::compile_and_run(&native);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}

#[test]
fn union_constructors_store_only_the_common_tag_and_padding_prefix() {
    let module = emit(SOURCE);
    assert!(module.contains(&format!("{} = type {{ i8, [15 x i8], [0 x ", nominal_type("MixedPrefix"))),
        "another variant's live byte limits even the wide variant's prefix: {module}");
    assert!(module.contains(&format!("{} = type {{ i8, i8, i8 }}", nominal_type("Product"))),
        "register-returned products retain the count-sized tag leaf: {module}");
    for (name, ty, width, tag) in [
        ("nil_value", "Value", 32, 0),
        ("handle_value", "Value", 32, 3),
        ("byte_value", "ByteValue", 8, 2),
        ("half_value", "HalfValue", 16, 2),
        ("mixed_prefix", "MixedPrefix", 8, 0),
        ("byte_product", "Product", 8, 1),
    ] {
        let body = emitted_body(&module, name);
        let ty = nominal_type(ty);
        assert!(body.contains(&format!("store {ty} zeroinitializer, ptr %wf.result")),
            "inactive payload remains initialized: {body}");
        let address = body.lines().find_map(|line| {
            let (address, gep) = line.trim().split_once(" = ")?;
            (gep == format!("getelementptr inbounds {ty}, ptr %wf.result, i32 0, i32 0"))
                .then_some(address)
        }).expect("the tag prefix starts at field zero of the destination");
        assert!(body.lines().any(|line| line.trim() == format!("store i{width} {tag}, ptr {address}")),
            "tag and common padding use one store with the independently expected width: {body}");
        for other in [8, 16, 24, 32].into_iter().filter(|other| *other != width) {
            assert!(!body.lines().any(|line| line.trim().starts_with(&format!("store i{other} "))
                && line.ends_with(&format!(", ptr {address}"))),
                "the prefix is not split or widened across an occupied payload: {body}");
        }
    }
    let bits = emitted_function(&module, "bit_choice");
    assert!(bits.contains("or i1 0, 1"), "a two-state tag-only value remains one bit: {bits}");
    for (name, nominal, variant, field, width) in [
        ("handle_value", "Value", Some(3), 1, 32),
        ("byte_value", "ByteValue", Some(2), 1, 8),
        ("half_value", "HalfValue", Some(2), 1, 16),
        ("mixed_prefix", "MixedPrefix", Some(0), 1, 64),
        ("byte_product", "Product", None, 2, 8),
    ] {
        let body = emitted_body(&module, name);
        let view = match variant {
            Some(tag) => format!("{}.v{tag}", nominal_type(nominal)),
            None => nominal_type(nominal),
        };
        let address = body.lines().find_map(|line| {
            let (address, gep) = line.trim().split_once(" = ")?;
            (gep == format!("getelementptr inbounds {view}, ptr %wf.result, i32 0, i32 {field}"))
                .then_some(address)
        }).expect("the selected payload has its declared field address");
        assert!(body.lines().any(|line| line.trim().starts_with(&format!("store i{width} "))
            && line.ends_with(&format!(", ptr {address}"))),
            "prefix transport preserves each selected payload's own typed store: {body}");
    }
    for variant in 0..5 {
        let fields = match variant {
            0 => continue,
            1 => "double",
            _ => "i32",
        };
        assert!(module.contains(&format!("{}.v{variant} = type {{ i8, {fields} }}", nominal_type("Value"))),
            "variant views retain their count-sized tag: {module}");
    }
}

#[test]
fn union_pointer_and_one_bit_intervals_use_byte_capture() {
    with_ir(SOURCE, |program| {
        for triple in [
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
            crate::target::WINDOWS_X86_64_TRIPLE,
        ] {
            let target = TargetLayout::for_triple(triple).expect("supported target");
            let nominal = program
                .nominals()
                .iter()
                .find(|n| n.name() == "Overlay")
                .expect("Overlay");
            let plan = bounded_transfer(target, program, IrType::Nominal(nominal.id()))
                .expect("layout")
                .expect("bounded overlay");
            assert!(
                plan.iter().any(|g| g.access == TransferAccess::Bytes),
                "{plan:?}"
            );
            assert!(
                !plan
                    .iter()
                    .any(|g| g.offset >= 4 && matches!(g.access, TransferAccess::Pointer)),
                "pointer overlaps plain fields or inactive storage: {plan:?}"
            );
        }
    });
    let module = emit(SOURCE);
    for name in ["copy_overlay", "make_overlay_into"] {
        let body = emitted_function(&module, name);
        let (region, source, destination) = transfer_region(body);
        // Resolve all byte addresses back to their root. A restore before a later
        // source capture, a wrong byte width or a wrong offset fails independently
        // of the common-boundary planner being tested above.
        let mut pointers = std::collections::HashMap::from([
            (source, (source, 0)),
            (destination, (destination, 0)),
        ]);
        let mut captures = Vec::new();
        let mut restores = Vec::new();
        let mut writing = false;
        for line in region.lines().map(str::trim) {
            if let Some((name, gep)) = line.split_once(" = getelementptr inbounds i8, ptr ") {
                let (base, offset) = gep.split_once(", i64 ").expect("byte address");
                let (root, prior) = pointers.get(base).copied().unwrap_or((base, 0));
                pointers.insert(name, (root, prior + offset.parse::<u64>().expect("offset")));
            } else if let Some(args) = line.strip_prefix("call void @llvm.memmove.p0.p0.i64(ptr ") {
                let (dst, args) = args.split_once(", ptr ").expect("memmove source");
                let (src, args) = args.split_once(", i64 ").expect("memmove size");
                let size = args
                    .split_once(',')
                    .unwrap()
                    .0
                    .parse::<u64>()
                    .expect("width");
                let (src_root, src_offset) = pointers.get(src).copied().unwrap_or((src, 0));
                let (dst_root, dst_offset) = pointers.get(dst).copied().unwrap_or((dst, 0));
                assert_eq!(src_offset, dst_offset, "matching interval offsets: {line}");
                if src_root == source {
                    assert!(
                        !writing,
                        "capture all source intervals before writing: {body}"
                    );
                    assert_ne!(dst_root, destination, "capture into scratch: {line}");
                    captures.push((src_offset, size, dst_root));
                } else {
                    writing = true;
                    assert_eq!(dst_root, destination, "restore into destination: {line}");
                    restores.push((dst_offset, size, src_root));
                }
            } else if line.starts_with("store ") {
                writing = true;
            } else if line.contains(" = load ") {
                assert!(
                    !writing,
                    "scalar source capture also precedes writes: {body}"
                );
            }
        }
        assert_eq!(
            captures
                .iter()
                .map(|&(offset, size, _)| (offset, size))
                .collect::<Vec<_>>(),
            [(1, 1), (8, 4), (12, 4)],
            "{body}"
        );
        assert_eq!(
            captures, restores,
            "each captured interval restored once: {body}"
        );
        let scratch = captures[0].2;
        assert!(
            body.lines().any(|line| line
                .trim()
                .starts_with(&format!("{scratch} = alloca [16 x i8]"))),
            "one target-planned byte scratch at entry: {body}"
        );
        assert!(
            !region.contains("load ptr"),
            "no pointer load from mixed bytes: {body}"
        );
    }
}

/// Feed actual emitted copy instructions equal pointers and partial overlap
/// in both directions. Checked WF references cannot express this ABI witness;
/// the C observer compares each field byte against an independent snapshot.
/// Interleaved loads/stores corrupt the +8 case; the equal case checks runtime
/// equality with distinct SSA names. Padding is excluded from the oracle.
#[test]
fn bounded_transfer_preserves_equal_and_overlapping_storage_natively() {
    let module = emit(SOURCE);
    let body = emitted_function(&module, "copy_frame");
    let (region, source, destination) = transfer_region(body);
    let region = region
        .replace(source, "%source")
        .replace(destination, "%destination");
    let llvm = format!(
        "define void @wf_transfer(ptr %destination, ptr %source) noinline {{\nentry:\n{region}  ret void\n}}\n"
    );
    let host = r#"#include <stdint.h>
#include <string.h>
extern void wf_transfer(void *, const void *);
extern int wf__floor_run(int, char **);
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
int wf__main_body(int argc, char **argv) {
    (void)argc;
    (void)argv;
    const unsigned offsets[] = {0, 8, 16, 24, 32, 36, 40, 48};
    const unsigned widths[] = {8, 8, 4, 8, 4, 1, 8, 8};
    for (int displacement = -8; displacement <= 8; displacement += 8) {
        unsigned char buffer[96], before[56];
        for (unsigned i = 0; i < sizeof buffer; ++i) buffer[i] = (unsigned char)(i * 3 + 1);
        unsigned char *source = buffer + 16, *destination = source + displacement;
        memcpy(before, source, sizeof before);
        wf_transfer(destination, source);
        for (unsigned field = 0; field < 8; ++field)
            if (memcmp(destination + offsets[field], before + offsets[field], widths[field])) return 1;
    }
    return 0;
}
"#;
    let output = compile_link_and_run(&llvm, Some(host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}
