//! Paged directory growth, run ABI, page geometry and allocation ownership.

use super::*;

const GROWTH: &[u8] = br#"fn main() -> status: std::process::ExitStatus pure {
  doc "Observe directory growth and stable element contents.";
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
/// cell implementation. Lazy allocation needs only three u64 pages for len 1025,
/// despite capacity 8193; cells reserve 4 and 32 directory words after a
/// 24-byte header. Growth copies all four initialized entries, including null.
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
        .replace("@malloc(", "@wf_paged_allocate(")
        .replace("@free(", "@wf_paged_release(")
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
static void *owners[5];
static size_t allocations, releases, copies;
static void require(int ok) { if (!ok) { fputs("paged allocation mismatch\n", stderr); exit(99); } }
void *wf_paged_allocate(uint64_t bytes) {
  static const uint64_t first[] = {56, 4096, 4096, 4096, 280};
  require(allocations < 5);
  require(bytes == first[allocations]);
  void *p = malloc((size_t)bytes); require(p != NULL);
  memset(p, 0xa5, (size_t)bytes);
  owners[allocations++] = p;
  return p;
}
void wf_paged_move(void *destination, const void *source, uint64_t bytes, _Bool is_volatile) {
  require(copies == 0 && allocations == 5 && !is_volatile);
  require(destination == (char *)owners[4] + 24);
  require(source == (char *)owners[0] + 24);
  require(bytes == 32);
  ++copies;
  memmove(destination, source, (size_t)bytes);
}
void wf_paged_release(void *p) {
  static const size_t order[] = {0, 1, 2, 3, 4};
  require(releases < 5 && p == owners[order[releases]]);
  ++releases;
  /* Quarantine allocations so reuse cannot conceal an identity change. */
}
int main(int argc, char **argv) {
  int result = wf_fixture_main(argc, argv);
  require(result == 0 && allocations == 5 && releases == 5 && copies == 1);
  for (size_t i = 0; i < allocations; ++i) free(owners[i]);
  return 0;
}
"#;
    let output = compile_link_and_run(&observed, Some(host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// Observe allocation timing at page boundaries, null spare entries, and reuse
/// after draining and relocating an empty owner with retained pages.
#[test]
fn paged_empty_growth_defers_pages_until_first_placement_and_reuses_them() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  doc "Grow empty storage, allocate on page boundaries and reuse retained pages.";
  let p = box_paged_new::<u64>(capacity: 0_u64);
  grow_paged(cell: &p, capacity: 1025_u64);
  grow_paged(cell: &p, capacity: 8193_u64);
  for (
    i in 0_u64..1025_u64,
    invariant length: p.inner.len == i
  ) {
    place_back(window: &p.inner, value: i);
  }
  let tail = take_back(window: &p.inner);
  place_back(window: &p.inner, value: tail);
  for (
    i in 0_u64..1025_u64,
    invariant length: p.inner.len == 1025_u64 - i
  ) {
    let value = take_back(window: &p.inner);
    let expected = 1024_u64 - i;
    if value != expected {
      return std::process::exit_status(code: 1_u8);
    }
  }
  grow_paged(cell: &p, capacity: 16385_u64);
  place_back(window: &p.inner, value: 73_u64);
  if p.inner.cap != 16385_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if p.inner.pages.len != 1_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  let value = take_back(window: &p.inner);
  if value != 73_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  free_empty(window: move p);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let observed = compile(source)
        .replace("@malloc(", "@wf_paged_allocate(")
        .replace("@free(", "@wf_paged_release(")
        .replace("@main(", "@wf_fixture_main(");
    let host = r#"#include <stdint.h>
#include <stdlib.h>
#include <string.h>
extern int wf_fixture_main(int, char **);
static void *owners[6];
static size_t allocations, releases;
static void require(int ok) { if (!ok) exit(99); }
static void inspect_cell(size_t owner, uint64_t len, uint64_t cap, size_t dircap, size_t pages) {
  uint64_t *header = owners[owner];
  require(header[0] == len && header[1] == cap && header[2] == dircap);
  void **directory = (void **)((char *)owners[owner] + 24);
  for (size_t i = 0; i < dircap; ++i)
    require(directory[i] == (i < pages ? owners[2 + i] : NULL));
}
void *wf_paged_allocate(uint64_t bytes) {
  static const uint64_t sizes[] = {56, 280, 4096, 4096, 4096, 536};
  require(allocations < 6 && bytes == sizes[allocations]);
  if (allocations == 1) inspect_cell(0, 0, 1025, 4, 0);
  if (allocations >= 2 && allocations <= 4)
    inspect_cell(1, 512 * (allocations - 2), 8193, 32, allocations - 2);
  if (allocations == 5) inspect_cell(1, 0, 8193, 32, 3);
  void *p = malloc((size_t)bytes); require(p != NULL);
  memset(p, 0xa5, (size_t)bytes);
  owners[allocations++] = p;
  return p;
}
void wf_paged_release(void *p) {
  require(releases < 6 && p == owners[releases]);
  if (releases >= 2) inspect_cell(5, 0, 16385, 64, 3);
  ++releases;
  /* Quarantine keeps retained page identities inspectable through cleanup. */
}
int main(int argc, char **argv) {
  int result = wf_fixture_main(argc, argv);
  require(result == 0 && allocations == 6 && releases == 6);
  for (size_t i = 0; i < allocations; ++i) free(owners[i]);
  return 0;
}
"#;
    let output = compile_link_and_run(&observed, Some(host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn paged_first_placement_allocation_failure_uses_resource_abort() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  doc "Report heap exhaustion when first placement cannot allocate its page.";
  let p = box_paged_new::<u64>(capacity: 1_u64);
  place_back(window: &p.inner, value: 73_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let observed = compile(source)
        .replace("@malloc(", "@wf_test_allocate(")
        .replace("@free(", "@wf_test_release(");
    let observer = format!(
        "{}\n__attribute__((constructor)) static void unbuffer(void) {{ setvbuf(stdout, NULL, _IONBF, 0); }}\n",
        super::owned_places::allocation_observer(2, 2)
    );
    let output = compile_link_and_run(&observed, Some(&observer), &[]);
    assert!(!output.status.success(), "{output:?}");
    super::exhaustion::assert_resource_record(&output.stderr, "heap");
    assert_eq!(output.stdout, b"A1;X2;", "{output:?}");
}

const RUNS: &[u8] = br#"fn fill(part: &Run<u64>) -> result: unit writes(part) {
  doc "Write every initialized element in a noncontiguous run.";
  let count = part^.len;
  for (i in 0_u64..count) {
    set part^[i] = 7_u64;
  }
  return unit;
}

fn reslice(part: &Run<u64>) -> result: unit writes(part) contract {
  requires part^.len >= 2_u64;
} {
  doc "Fill an interior subrun across its page boundary.";
  let count = part^.len;
  let end = count - 1_u64;
  fill(part: &part^[1_u64..end]);
  return unit;
}

fn slice_total(page: &[u64]) -> result: u64 reads(page) {
  doc "Sum a contiguous page reference.";
  let count = page^.len;
  let total = 0_u64;
  for (i in 0_u64..count) {
    set total = total +wrap page^[i];
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Check empty runs, cross-page subruns and initialized page extents.";
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
        2,
        "lazy placement adds a boundary-only null lookup before element addressing: {append}"
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
  doc "Keep page geometry for zero-stride and oversized elements.";
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
  let wide = box_paged_new::<Array<u8, 4097>>(capacity: 2_u64);
  for (
    i in 0_u64..2_u64,
    invariant length: wide.inner.len == i
  ) {
    let payload = array_filled::<u8, 4097>(value: 7_u8);
    place_back(window: &wide.inner, value: payload);
  }
  if wide.inner[1_u64][4096_u64] != 7_u8 {
    return std::process::exit_status(code: 7_u8);
  }
  if wide.inner.pages.len != 2_u64 {
    return std::process::exit_status(code: 8_u8);
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
    // Lazy pages interleave backing allocations with the element Box allocations.
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  doc "Release initialized owners in order and retain the taken tail.";
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
        .replace("@malloc(", "@wf_paged_allocate(")
        .replace("@free(", "@wf_paged_release(")
        .replace("@main(", "@wf_fixture_main(");
    let host = r#"#include <stdint.h>
#include <stdlib.h>
extern int wf_fixture_main(int, char **);
static void *owners[516];
static size_t allocations, releases;
static void require(int ok) { if (!ok) exit(99); }
void *wf_paged_allocate(uint64_t bytes) {
  require(allocations < 516);
  require(bytes == (allocations == 0 ? 56 : (allocations == 2 || allocations == 515 ? 4096 : 8)));
  void *p = malloc((size_t)bytes); require(p != NULL);
  owners[allocations++] = p;
  return p;
}
void wf_paged_release(void *p) {
  size_t expected;
  if (releases == 0) expected = 514; /* Taken tail's local owner. */
  else if (releases < 513) expected = releases == 1 ? 1 : releases + 1; /* Slots 0..511. */
  else {
    static const size_t backing[] = {2, 515, 0};
    require(releases < 516);
    expected = backing[releases - 513];
  }
  require(p == owners[expected]);
  if (releases < 513) require(*(uint64_t *)p == (expected == 1 ? 0 : expected - 2));
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
type LimitCase = (
    u64,
    &'static str,
    Option<u64>,
    u64,
    bool,
    bool,
    &'static [u8],
);

#[test]
fn paged_page_and_cell_size_failures_precede_their_allocator() {
    // Lazy allocation moves page-size exhaustion to placement; directory limits
    // still fail during construction/grow, before their own allocator call.
    let cases: &[LimitCase] = &[
        (2048, "u64", None, 0, false, true, b"A1;F1;"),
        (2048, "u64", None, 1, false, true, b"A1;F1;"),
        (2048, "u64", None, 1, true, false, b"A1;"),
        (2048, "u64", Some(0), 1, true, false, b"A1;"),
        (2048, "u64", None, u64::MAX, false, false, b""),
        (56, "Array<u8, 0>", None, 4097, false, true, b"A1;F1;"),
        (55, "Array<u8, 0>", None, 4097, false, false, b""),
        (
            88,
            "Array<u8, 0>",
            Some(4096),
            16385,
            false,
            true,
            b"A1;A2;F1;F2;",
        ),
        (87, "Array<u8, 0>", Some(4096), 16385, false, false, b"A1;"),
    ];
    for &(maximum, element, initial, capacity, place, served, expected) in cases {
        let target = crate::target::TargetLayout::host()
            .expect("supported target")
            .with_runtime_allocation_limits_for_test(maximum, 8);
        let growth = initial.map_or_else(String::new, |_| {
            format!("  grow_paged(cell: &p, capacity: {capacity}_u64);\n")
        });
        let placement = if place {
            "  place_back(window: &p.inner, value: 0_u64);\n"
        } else {
            ""
        };
        let initial = initial.unwrap_or(capacity);
        let source = format!(
            "fn main() -> status: std::process::ExitStatus pure {{\n  doc \"Observe allocation limits before invoking the allocator.\";\n  let p = box_paged_new::<{element}>(capacity: {initial}_u64);\n{growth}{placement}  return std::process::exit_status(code: 0_u8);\n}}\n"
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
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let observer = format!(
            "{}\n__attribute__((constructor)) static void unbuffer(void) {{ setvbuf(stdout, NULL, _IONBF, 0); }}\n",
            super::owned_places::allocation_observer(8, 0)
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
