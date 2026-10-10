//! Experimental transfer widths and overlap semantics. These run in the
//! backend CI group; source editing uses only prebuilt --check admission.

use super::system::with_ir;
use super::{compile_link_and_run, emit, emitted_function};
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

fn main() -> status: std::process::ExitStatus pure {
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
    let frame = emitted_function(&module, "copy_frame");
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
    let value = emitted_function(&module, "copy_value");
    assert_typed_transfer(value, &[(0, "i32"), (4, "i32"), (8, "i64")]);
    assert!(
        !value.contains("@llvm.memmove") && !value.contains("@llvm.memcpy"),
        "{value}"
    );
    // Inclusive limit: a 128-byte/16-leaf scalar record still expands. The
    // 17-byte ManyLeaves fixture independently detects a missing leaf bound;
    // LargeScalars exceeds both limits (current scalar leaves are <=8 bytes).
    let boundary = emitted_function(&module, "copy_boundary");
    let expected: Vec<_> = (0..16).map(|leaf| (leaf * 8, "i64")).collect();
    assert_typed_transfer(boundary, &expected);
    assert!(
        !boundary.contains("@llvm.memmove") && !boundary.contains("@llvm.memcpy"),
        "{boundary}"
    );
    for name in ["copy_large", "copy_array", "copy_many", "copy_scalars"] {
        let body = emitted_function(&module, name);
        assert!(body.contains("call void @llvm.memmove."), "{body}");
        assert!(!body.contains("; layout-bounded transfer "), "{body}");
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
            "x86_64-pc-windows-msvc",
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
    let body = emitted_function(&module, "copy_overlay");
    let (region, source, destination) = transfer_region(body);
    // Resolve all byte addresses back to their root. A restore before a later
    // source capture, a wrong byte width or a wrong offset fails independently
    // of the common-boundary planner being tested above.
    let mut pointers =
        std::collections::HashMap::from([(source, (source, 0)), (destination, (destination, 0))]);
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
        [(4, 1), (8, 4), (12, 4)],
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
