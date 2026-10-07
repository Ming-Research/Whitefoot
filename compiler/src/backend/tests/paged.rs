//! Paged directory growth, run ABI, page geometry and allocation ownership.

use super::*;

const GROWTH: &[u8] = br#"fn main() -> status: std::process::ExitStatus pure {
  let p = box_paged_new::<u64>(capacity: 1_u64);
  place_back(window: &p.inner, value: 73_u64);
  grow_paged(cell: &p, capacity: 513_u64);
  grow_paged(cell: &p, capacity: 1025_u64);
  for (i in 1_u64..1025_u64, invariant length: p.inner.len == i) {
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
/// descriptor implementation. U64's 512-element pages require 17 page owners
/// for capacity 8193; growth replaces only directories of 1, 2 and 4 words.
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
    let observed = llvm
        .replace("@malloc(", "@wf_paged_allocate(")
        .replace("@free(", "@wf_paged_release(")
        .replace("@main(", "@wf_fixture_main(");
    let host = r#"#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
extern int wf_fixture_main(int, char **);
static void *owners[22];
static size_t allocations, releases;
static void require(int ok) { if (!ok) { fputs("paged allocation mismatch\n", stderr); exit(99); } }
void *wf_paged_allocate(uint64_t bytes) {
  static const uint64_t first[] = {32, 8, 4096, 16, 4096, 32, 4096, 256};
  require(allocations < 22);
  require(bytes == (allocations < 8 ? first[allocations] : 4096));
  void *p = malloc((size_t)bytes); require(p != NULL);
  memset(p, 0xa5, (size_t)bytes);
  owners[allocations++] = p;
  return p;
}
void wf_paged_release(void *p) {
  static const size_t order[] = {1, 3, 5, 2, 4, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 7, 0};
  require(releases < 22 && p == owners[order[releases]]);
  ++releases;
  /* Quarantine allocations so reuse cannot conceal an identity change. */
}
int main(int argc, char **argv) {
  int result = wf_fixture_main(argc, argv);
  require(result == 0 && allocations == 22 && releases == 22);
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
  for (i in 0_u64..1025_u64, invariant length: p.inner.len == i) {
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
  for (i in 0_u64..4097_u64, invariant length: p.inner.len == i) {
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
  for (i in 0_u64..513_u64, invariant length: p.inner.len == i) {
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
        .replace("@malloc(", "@wf_paged_allocate(")
        .replace("@free(", "@wf_paged_release(")
        .replace("@main(", "@wf_fixture_main(");
    let host = r#"#include <stdint.h>
#include <stdlib.h>
extern int wf_fixture_main(int, char **);
static void *owners[518];
static size_t allocations, releases;
static void require(int ok) { if (!ok) exit(99); }
void *wf_paged_allocate(uint64_t bytes) {
  static const uint64_t first[] = {32, 8, 16, 4096, 4096};
  require(allocations < 518);
  require(bytes == (allocations < 5 ? first[allocations] : 8));
  void *p = malloc((size_t)bytes); require(p != NULL);
  owners[allocations++] = p;
  return p;
}
void wf_paged_release(void *p) {
  size_t expected;
  if (releases == 0) expected = 1; /* Initial directory replaced. */
  else if (releases == 1) expected = 517; /* Taken tail's local owner. */
  else if (releases < 514) expected = releases + 3; /* Slots 0..511. */
  else {
    static const size_t backing[] = {3, 4, 2, 0};
    require(releases < 518);
    expected = backing[releases - 514];
  }
  require(p == owners[expected]);
  if (expected >= 5) require(*(uint64_t *)p == expected - 5);
  ++releases;
}
int main(int argc, char **argv) {
  int result = wf_fixture_main(argc, argv);
  require(result == 0 && allocations == 518 && releases == 518);
  for (size_t i = 0; i < allocations; ++i) free(owners[i]);
  return 0;
}
"#;
    let output = compile_link_and_run(&observed, Some(host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn paged_page_and_directory_size_failures_precede_their_allocator() {
    let target = crate::target::TargetLayout::host()
        .expect("supported target")
        .with_runtime_allocation_limits_for_test(2048, 8);
    for (capacity, served) in [(0_u64, true), (1, false), (u64::MAX, false)] {
        let source = format!(
            "fn main() -> status: std::process::ExitStatus pure {{\n  let p = box_paged_new::<u64>(capacity: {capacity}_u64);\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        let module = super::system::with_ir(source.as_bytes(), |program| {
            let mut llvm = crate::backend::emitter::emit_llvm_with_layout(program, target)
                .expect("descriptor layout qualifies")
                .into_string();
            llvm.push_str(
                &crate::driver::launcher::render(program, "main")
                    .expect("test launcher")
                    .render(),
            );
            llvm
        });
        let observed = module
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let observer = format!(
            "{}\n__attribute__((constructor)) static void unbuffer(void) {{ setvbuf(stdout, NULL, _IONBF, 0); }}\n",
            super::owned_places::allocation_observer(2, 0)
        );
        let output = compile_link_and_run(&observed, Some(&observer), &[]);
        if served {
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(output.stdout, b"A1;A2;F2;F1;");
        } else {
            assert!(!output.status.success(), "unservable capacity {capacity}");
            assert_eq!(
                output.stdout, b"A1;A2;",
                "no page or replacement directory allocation"
            );
            super::exhaustion::assert_resource_record(&output.stderr, "heap");
        }
    }
}
