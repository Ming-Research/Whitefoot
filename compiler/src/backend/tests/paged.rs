//! Paged directory growth, run ABI, page geometry and allocation ownership.

use super::*;

const GROWTH: &[u8] = br#"fn main() -> status: std::process::ExitStatus pure {
  let p = box_paged_new::<u64>(capacity: 1_u64);
  place_back(window: &p.inner, value: 73_u64);
  grow_paged(cell: &p, capacity: 513_u64);
  grow_paged(cell: &p, capacity: 1025_u64);
  for (
    i in 1_u64..1025_u64,
    invariant length: p.inner.len == i
  ) {
    place_back(window: &p.inner, value: i);
  }
  grow_paged(cell: &p, capacity: 8193_u64);
  if p.inner.cap != 8193_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if p.inner[0_u64] != 73_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  for (i in 1_u64..1025_u64) {
    if p.inner[i] != i {
      return std::process::exit_status(code: 3_u8);
    }
  }
  let last = take_back(window: &p.inner);
  if last != 1024_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// Allocation identities and release order are observed independently of the
/// cell implementation. U64's 512-element pages require 17 page owners
/// for capacity 8193; cells reserve 1, 2, 4 and 32 directory words after a
/// 24-byte header. Growth copies only the 1, 2 and 3 initialized pointers.
#[test]
fn paged_growth_preserves_page_owners_and_releases_every_allocation() {
    let llvm = compile(GROWTH);
    let growth = emitted_prelude_row(&llvm, "grow_paged");
    assert_eq!(
        growth.matches("call void @llvm.memmove").count(),
        1,
        "{growth}"
    );
    assert!(growth.contains(".copied = mul nuw i64"), "{growth}");
    assert!(!growth.contains("load i64, ptr %element"), "{growth}");
    let mut observed = llvm
        .replace("@wf__heap_take(", "@wf_paged_allocate(")
        .replace("@wf__heap_give(", "@wf_paged_release(")
        .replace(
            "call void @llvm.memmove.p0.p0.i64(",
            "call void @wf_paged_move(",
        )
        .replace("@main(", "@wf_fixture_main(");
    observed.push_str("\ndeclare void @wf_paged_move(ptr, ptr, i64, i1)\n");
    let host = r#"#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
extern int wf_fixture_main(int, char **);
static void *owners[21];
static size_t allocations, releases, copies;
static void require(int ok) { if (!ok) { fputs("paged allocation mismatch\n", stderr); exit(99); } }
void *wf_paged_allocate(uint64_t bytes) {
  static const uint64_t first[] = {32, 4096, 40, 4096, 56, 4096, 280};
  require(allocations < 21);
  require(bytes == (allocations < 7 ? first[allocations] : 4096));
  void *p = malloc((size_t)bytes); require(p != NULL);
  memset(p, 0xa5, (size_t)bytes);
  owners[allocations++] = p;
  return p;
}
void wf_paged_move(void *destination, const void *source, uint64_t bytes, _Bool is_volatile) {
  static const size_t old[] = {0, 2, 4}, fresh[] = {2, 4, 6};
  require(copies < 3 && allocations == fresh[copies] + 1 && !is_volatile);
  require(destination == (char *)owners[fresh[copies]] + 24);
  require(source == (char *)owners[old[copies]] + 24);
  require(bytes == 8 * (copies + 1));
  ++copies;
  memmove(destination, source, (size_t)bytes);
}
void wf_paged_release(void *p, uint64_t bytes) {
  static const size_t order[] = {0, 2, 4, 1, 3, 5, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 6};
  require(releases < 21 && p == owners[order[releases]]);
  static const uint64_t first[] = {32, 4096, 40, 4096, 56, 4096, 280};
  size_t owner = order[releases];
  require(bytes == (owner < 7 ? first[owner] : 4096));
  ++releases;
  /* Quarantine allocations so reuse cannot conceal an identity change. */
}
int main(int argc, char **argv) {
  int result = wf_fixture_main(argc, argv);
  require(result == 0 && allocations == 21 && releases == 21 && copies == 3);
  for (size_t i = 0; i < allocations; ++i) free(owners[i]);
  return 0;
}
"#;
    let output = compile_link_and_run(&observed, Some(host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

const RUNS: &[u8] = br#"fn fill(part: &Run<u64>) -> result: unit writes(part) {
  let count = part^.len;
  for (i in 0_u64..count) {
    set part^[i] = 7_u64;
  }
  return unit;
}

fn reslice(part: &Run<u64>) -> result: unit writes(part) contract {
  requires part^.len >= 2_u64;
} {
  let count = part^.len;
  let end = count - 1_u64;
  fill(part: &part^[1_u64..end]);
  return unit;
}

fn slice_total(page: &[u64]) -> result: u64 reads(page) {
  let count = page^.len;
  let total = 0_u64;
  for (i in 0_u64..count) {
    set total = total +wrap page^[i];
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  let p = box_paged_new::<u64>(capacity: 0_u64);
  fill(part: &p.inner[0_u64..0_u64]);
  grow_paged(cell: &p, capacity: 0_u64);
  grow_paged(cell: &p, capacity: 1025_u64);
  for (
    i in 0_u64..1025_u64,
    invariant length: p.inner.len == i
  ) {
    place_back(window: &p.inner, value: 1_u64);
  }
  reslice(part: &p.inner[510_u64..515_u64]);
  if p.inner[510_u64] != 1_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  for (i in 511_u64..514_u64) {
    if p.inner[i] != 7_u64 {
      return std::process::exit_status(code: 2_u8);
    }
  }
  if p.inner[514_u64] != 1_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  let total = 0_u64;
  let pages = p.inner.pages.len;
  for (k in 0_u64..pages) {
    let page = &p.inner.pages[k];
    let subtotal = slice_total(page: page);
    set total = total +wrap subtotal;
  }
  if total != 1043_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if pages != 3_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn paged_runs_cross_pages_reslice_and_bridge_to_contiguous_pages() {
    let llvm = compile(RUNS);
    let append = emitted_prelude_row(&llvm, "place_back");
    assert_eq!(
        append.matches(" = load ptr, ptr ").count(),
        1,
        "the page pointer is the only dependent load from &Paged: {append}"
    );
    let fill = emitted_function(&llvm, "fill");
    let header = fill.lines().next().expect("fill definition");
    assert!(header.contains("ptr readonly nonnull"), "{header}");
    assert!(!header.contains("noalias"), "shared directory: {header}");
    assert!(!header.contains("dereferenceable"), "{header}");
    assert!(header.contains(".lo, i64"), "{header}");
    assert!(
        fill.contains("lshr i64") && fill.contains("and i64"),
        "{fill}"
    );
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn paged_page_geometry_covers_zero_stride_and_elements_larger_than_a_page() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let bytes = paged_page_len::<u8>();
  let words = paged_page_len::<u64>();
  let empty = paged_page_len::<Array<u8, 0>>();
  let large = paged_page_len::<Array<u8, 4097>>();
  if bytes != 4096_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if words != 512_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if empty != 4096_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if large != 1_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  let p = box_paged_new::<Array<u8, 0>>(capacity: 4097_u64);
  for (
    i in 0_u64..4097_u64,
    invariant length: p.inner.len == i
  ) {
    let value = array_filled::<u8, 0>(value: 0_u8);
    place_back(window: &p.inner, value: value);
  }
  grow_paged(cell: &p, capacity: 8193_u64);
  let value = take_back(window: &p.inner);
  if value.len != 0_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  if p.inner.pages.len != 1_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn paged_run_loops_use_the_same_split_and_thunk_path_as_slices() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/par2-pos-paged-run-partitions.wf");
    let llvm = emit_with_overlap(source);
    assert!(llvm.contains("extractvalue { ptr, i64, i64 }"), "{llvm}");
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn paged_release_drops_only_initialized_owners_in_logical_order() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let p = box_paged_new::<Box<u64>>(capacity: 513_u64);
  for (
    i in 0_u64..513_u64,
    invariant length: p.inner.len == i
  ) {
    let value = box_new::<u64>(value: i);
    place_back(window: &p.inner, value: move value);
  }
  let tail = take_back(window: &p.inner);
  if tail.inner != 512_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let observed = compile(source)
        .replace("@wf__heap_take(", "@wf_paged_allocate(")
        .replace("@wf__heap_give(", "@wf_paged_release(")
        .replace("@main(", "@wf_fixture_main(");
    let host = r#"#include <stdint.h>
#include <stdlib.h>
extern int wf_fixture_main(int, char **);
static void *owners[516];
static size_t allocations, releases;
static void require(int ok) { if (!ok) exit(99); }
void *wf_paged_allocate(uint64_t bytes) {
  static const uint64_t first[] = {40, 4096, 4096};
  require(allocations < 516);
  require(bytes == (allocations < 3 ? first[allocations] : 8));
  void *p = malloc((size_t)bytes); require(p != NULL);
  owners[allocations++] = p;
  return p;
}
void wf_paged_release(void *p, uint64_t bytes) {
  size_t expected;
  if (releases == 0) expected = 515; /* Taken tail's local owner. */
  else if (releases < 513) expected = releases + 2; /* Slots 0..511. */
  else {
    static const size_t backing[] = {1, 2, 0};
    require(releases < 516);
    expected = backing[releases - 513];
  }
  require(p == owners[expected]);
  require(bytes == (expected == 0 ? 40 : expected < 3 ? 4096 : 8));
  if (expected >= 3) require(*(uint64_t *)p == expected - 3);
  ++releases;
}
int main(int argc, char **argv) {
  int result = wf_fixture_main(argc, argv);
  require(result == 0 && allocations == 516 && releases == 516);
  for (size_t i = 0; i < allocations; ++i) free(owners[i]);
  return 0;
}
"#;
    let output = compile_link_and_run(&observed, Some(host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

/// One row of `paged_page_and_cell_size_failures_precede_their_allocator`.
type LimitCase = (u64, &'static str, Option<u64>, u64, bool, &'static [u8]);

#[test]
fn paged_page_and_cell_size_failures_precede_their_allocator() {
    // The page limit, directory doubling limit, and the exact 24 + 8*2
    // cell boundary, both at construction and at replacement. Zero-stride
    // pages isolate the cell limit from the ordinary 4096-byte page limit.
    let cases: &[LimitCase] = &[
        (2048, "u64", None, 0, true, b"A1;F1;"),
        (2048, "u64", None, 1, false, b"A1;"),
        (2048, "u64", None, u64::MAX, false, b""),
        (40, "Array<u8, 0>", None, 4097, true, b"A1;A2;A3;F2;F3;F1;"),
        (39, "Array<u8, 0>", None, 4097, false, b""),
        (
            40,
            "Array<u8, 0>",
            Some(4096),
            4097,
            true,
            b"A1;A2;A3;F1;A4;F2;F4;F3;",
        ),
        (39, "Array<u8, 0>", Some(4096), 4097, false, b"A1;A2;"),
    ];
    for &(maximum, element, initial, capacity, served, expected) in cases {
        let target = crate::target::TargetLayout::host()
            .expect("supported target")
            .with_runtime_allocation_limits_for_test(maximum, 8);
        let growth = initial.map_or_else(String::new, |_| {
            format!("  grow_paged(cell: &p, capacity: {capacity}_u64);\n")
        });
        let initial = initial.unwrap_or(capacity);
        let source = format!(
            "fn main() -> status: std::process::ExitStatus pure {{\n  let p = box_paged_new::<{element}>(capacity: {initial}_u64);\n{growth}  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        let module = super::system::with_ir(source.as_bytes(), |program| {
            let mut llvm = crate::backend::emitter::emit_llvm_with_layout(program, target)
                .expect("fixed cell layout qualifies")
                .into_string();
            llvm.push_str(
                &crate::driver::launcher::render(program, "main")
                    .expect("test launcher")
                    .render(),
            );
            llvm
        });
        let observed = module
            .replace("@wf__heap_take(", "@wf_test_allocate(")
            .replace("@wf__heap_give(", "@wf_test_release(");
        let observer = format!(
            "{}\n__attribute__((constructor)) static void unbuffer(void) {{ setvbuf(stdout, NULL, _IONBF, 0); }}\n",
            super::owned_places::allocation_observer(4, 0)
        );
        let output = compile_link_and_run(&observed, Some(&observer), &[]);
        if served {
            assert_eq!(output.status.code(), Some(0), "{output:?}");
        } else {
            assert!(!output.status.success(), "unservable capacity {capacity}");
            super::exhaustion::assert_resource_record(&output.stderr, "heap");
        }
        assert_eq!(
            output.stdout, expected,
            "{maximum} bytes, capacity {capacity}"
        );
    }
}
