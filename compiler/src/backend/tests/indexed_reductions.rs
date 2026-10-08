//! Indexed split implementation obligations; complete value oracles live in
//! the program fixture and are also run through its ordinary sequential build.

use super::{BoundedOutput, build_linked_executable, emit, emit_with_overlap, test_directory};
use std::process::Command;

const PROGRAM: &[u8] = include_bytes!("../../../../tests/programs/parallel/indexed_reductions.wf");

#[test]
fn indexed_ir_contains_private_fill_ordered_combine_and_release() {
    let module = emit_with_overlap(PROGRAM);
    for expected in [
        ".allocation = call ptr @malloc",
        ".fill.head:",
        "store i64 9223372036854775807, ptr %indexed.",
        "store i64 -9223372036854775808, ptr %indexed.",
        "store i64 1, ptr %indexed.",
        ".combine.head:",
        " = urem i64 %indexed.",
        "call void @free(ptr %indexed.",
        "@llvm.umul.with.overflow.i64",
        "@llvm.uadd.with.overflow.i64",
        ".pays = icmp ugt i64",
    ] {
        assert!(module.contains(expected), "missing {expected}:\n{module}");
    }
    assert!(!emit(PROGRAM).contains("%indexed."));
    // The site folds ascending flat offsets (leaf-major, cell-minor), after
    // the structured splitter call has joined, and releases afterwards.
    for name in ["histogram", "extrema", "products"] {
        let body = super::emitted_body(&module, name);
        let split = body.find("@wf__par_split_").expect("split call");
        let combine = body.find(".combine.head:").expect("combine loop");
        let free = body[combine..]
            .find("call void @free")
            .expect("release after combine");
        assert!(split < combine && free > 0);
        assert!(body.contains(".next = add i64 %indexed."));
    }
}

/// Force an allowance at the ordinary runtime query, then observe actual
/// private allocations and leaf invocations. A correct answer reached only
/// through the sequential clone fails the observer.
fn observed(module: &str) -> String {
    let mut output = String::new();
    let mut chunk_entry = None;
    for line in module.lines() {
        if line.starts_with("define ") && line.contains("@wf__par_chunk_") {
            chunk_entry = Some(if line.contains("histogram") {
                0
            } else if line.contains("extrema") {
                1
            } else if line.contains("products") {
                2
            } else {
                3
            });
        }
        let line = if line.contains(".allocation = call ptr @malloc(") {
            line.replace("@malloc(", "@wf_test_private_allocate(")
        } else if line.contains("call void @free(ptr %indexed.") {
            line.replace("@free(", "@wf_test_private_free(")
        } else {
            line.replace(
                "call i64 @wf__par_split_budget(",
                "call i64 @wf_test_indexed_budget(",
            )
            .replace(
                "call void @wf_resource_abort()",
                "call void @wf_test_indexed_abort()",
            )
        };
        output.push_str(&line);
        output.push('\n');
        if line.ends_with(':')
            && let Some(tag) = chunk_entry.take()
        {
            output.push_str(&format!("  call void @wf_test_indexed_leaf(i32 {tag})\n"));
        }
    }
    output.push_str("\ndeclare ptr @wf_test_private_allocate(i64)\ndeclare void @wf_test_private_free(ptr)\ndeclare i64 @wf_test_indexed_budget(i64, i64)\ndeclare void @wf_test_indexed_abort()\ndeclare void @wf_test_indexed_leaf(i32)\n");
    output
}

const OBSERVER: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdatomic.h>
static _Atomic unsigned allocations, releases, live, leaves, loops[4];
uint64_t wf_test_indexed_budget(uint64_t span, uint64_t weight) {
    (void)span; (void)weight;
    return getenv("WF_TEST_ZERO") ? 0 : 2;
}
void wf_test_indexed_leaf(unsigned tag) {
    if (tag >= 4) _Exit(113);
    atomic_fetch_add(&leaves, 1);
    atomic_fetch_add(&loops[tag], 1);
}
void *wf_test_private_allocate(uint64_t bytes) {
    unsigned n = atomic_fetch_add(&allocations, 1) + 1;
    const char *fail = getenv("WF_TEST_FAIL");
    if (fail && n == (unsigned)atoi(fail)) return NULL;
    void *p = malloc((size_t)bytes);
    if (p) atomic_fetch_add(&live, 1);
    return p;
}
void wf_test_private_free(void *p) {
    if (!p || !atomic_load(&live)) _Exit(110);
    atomic_fetch_sub(&live, 1);
    atomic_fetch_add(&releases, 1);
    free(p);
}
void wf_test_indexed_abort(void) {
    const char *fail = getenv("WF_TEST_FAIL");
    if (!fail || atomic_load(&live) ||
        atomic_load(&allocations) != (unsigned)atoi(fail) ||
        atomic_load(&releases) + 1 != atomic_load(&allocations)) _Exit(111);
    _Exit(90);
}
static void report(void) {
    unsigned a = atomic_load(&allocations), r = atomic_load(&releases);
    unsigned l = atomic_load(&leaves);
    int zero = getenv("WF_TEST_ZERO") || getenv("WF_TEST_TINY");
    if (atomic_load(&live) || a != r || (!zero && (!a || l < 4)) ||
        (zero && a) || getenv("WF_TEST_FAIL")) _Exit(112);
    if (getenv("WF_TEST_PROGRAM")) {
        unsigned expected = zero ? 1 : 4;
        if (a != (zero ? 0 : 4)) _Exit(114);
        for (unsigned i = 0; i < 3; ++i)
            if (atomic_load(&loops[i]) != expected) _Exit(115);
    }
    fprintf(stderr, "indexed allocations=%u leaves=%u\n", a, l);
}
__attribute__((constructor)) static void setup(void) { atexit(report); }
"#;

#[test]
fn indexed_split_executes_private_leaves_and_zero_budget_executes_source() {
    let directory = test_directory();
    let module = observed(&emit_with_overlap(PROGRAM));
    let executable = build_linked_executable(&module, Some(OBSERVER), &[], &directory);
    for zero in [false, true] {
        let mut command = Command::new(&executable);
        command.env("WF_WORKERS", "4").env("WF_TEST_PROGRAM", "1");
        if zero {
            command.env("WF_TEST_ZERO", "1");
        }
        let output = command.bounded_output().expect("run indexed program");
        assert_eq!(output.status.code(), Some(0), "zero={zero}: {output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("indexed allocations="));
    }
    std::fs::remove_dir_all(directory).expect("remove indexed native artifacts");
}

const TWO_ROOTS: &str = r#"fn main() -> status: std::process::ExitStatus pure {
  doc "Two live private roots exercise release when the second allocation fails.";
  let low = array_filled::<i64, 4>(value: 100_i64);
  let high = array_filled::<i64, 4>(value: -100_i64);
  for (i in 0_u64..262144_u64) {
    set low[0_u64] = imin(low[0_u64], 7_i64);
    set high[0_u64] = imax(high[0_u64], -7_i64);
  }
  if low[0_u64] != 7_i64 {
    return std::process::exit_status(code: 1_u8);
  }
  if high[0_u64] != -7_i64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn indexed_allocation_failure_releases_every_previously_acquired_buffer() {
    let directory = test_directory();
    let module = observed(&emit_with_overlap(TWO_ROOTS.as_bytes()));
    let executable = build_linked_executable(&module, Some(OBSERVER), &[], &directory);
    for fail in ["1", "2"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", "4")
            .env("WF_TEST_FAIL", fail)
            .bounded_output()
            .expect("run resource failure");
        assert_eq!(
            output.status.code(),
            Some(90),
            "allocation {fail}: {output:?}"
        );
    }
    std::fs::remove_dir_all(directory).expect("remove resource test artifacts");
}

#[test]
fn indexed_tiny_loop_refuses_buffers_even_when_runtime_offers_a_split() {
    let source = TWO_ROOTS
        .replace("i64, 4>", "i64, 4096>")
        .replace("262144_u64", "4_u64");
    let directory = test_directory();
    let module = observed(&emit_with_overlap(source.as_bytes()));
    let executable = build_linked_executable(&module, Some(OBSERVER), &[], &directory);
    let output = Command::new(&executable)
        .env("WF_WORKERS", "4")
        .env("WF_TEST_TINY", "1")
        .bounded_output()
        .expect("run refused indexed split");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let report = String::from_utf8_lossy(&output.stderr);
    assert!(
        report.contains("indexed allocations=0 leaves=1"),
        "{report}"
    );
    std::fs::remove_dir_all(directory).expect("remove grain test artifacts");
}

#[test]
fn indexed_storage_shapes_and_all_operations_emit_private_ranges() {
    // These formal fixtures already own the type/operation coverage. This
    // checks their lowering payload, without changing conformance verdicts.
    let shapes = include_bytes!("../../../../tests/conformance/cases/par2-pos-indexed-shapes.wf");
    let module = emit_with_overlap(shapes);
    for name in [
        "slots",
        "inline_array",
        "referenced",
        "several_roots",
        "branches",
        "overlapping_maps",
        "measured_index",
        "borrowed_index",
    ] {
        let body = super::emitted_body(&module, name);
        assert!(
            body.contains(".allocation = call ptr @malloc"),
            "{name}: {body}"
        );
    }
    for source in [
        include_bytes!("../../../../tests/conformance/cases/par2-pos-indexed-band.wf").as_slice(),
        include_bytes!("../../../../tests/conformance/cases/par2-pos-indexed-bor.wf").as_slice(),
        include_bytes!("../../../../tests/conformance/cases/par2-pos-indexed-bxor.wf").as_slice(),
        include_bytes!("../../../../tests/conformance/cases/par2-pos-indexed-iand.wf").as_slice(),
        include_bytes!("../../../../tests/conformance/cases/par2-pos-indexed-ior.wf").as_slice(),
        include_bytes!("../../../../tests/conformance/cases/par2-pos-indexed-ixor.wf").as_slice(),
    ] {
        let module = emit_with_overlap(source);
        assert!(module.contains(".fill.head:"));
        assert!(module.contains(".combine.head:"));
    }
}

#[test]
fn indexed_capture_pruning_keeps_counts_and_rebinds_refused_roots() {
    for (length, splits) in [(1, true), (256, false)] {
        // A live by-value array forces the shared target-layout query after
        // pruning. The small one fits; the wide one must reuse the body with
        // indexed ranges rebound to the original cells.
        let source = format!(
            r#"fn folded(values: Array<u8, {length}>) -> result: u64 pure {{
  let cells = array_filled::<u64, 4>(value: 3_u64);
  for (i in 0_u64..262144_u64) {{
    let copied = values;
    let byte = copied[0_u64];
    let word = cvt::<u8, u64>(byte);
    set cells[0_u64] = cells[0_u64] +wrap word;
  }}
  return cells[0_u64];
}}

fn main() -> status: std::process::ExitStatus pure {{
  let values = array_filled::<u8, {length}>(value: 19_u8);
  let total = folded(values: values);
  if total != 4980739_u64 {{
    return std::process::exit_status(code: 1_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
        let module = emit_with_overlap(source.as_bytes());
        let body = super::emitted_body(&module, "folded");
        assert_eq!(
            body.contains(".allocation = call ptr @malloc"),
            splits,
            "length={length}: {body}"
        );
        let directory = test_directory();
        let executable =
            build_linked_executable(&observed(&module), Some(OBSERVER), &[], &directory);
        let mut command = Command::new(&executable);
        command.env("WF_WORKERS", "4");
        if !splits {
            command.env("WF_TEST_ZERO", "1");
        }
        let output = command.bounded_output().expect("run indexed capture frame");
        assert_eq!(output.status.code(), Some(0), "length={length}: {output:?}");
        let report = String::from_utf8_lossy(&output.stderr);
        let expected = if splits {
            "indexed allocations=1 leaves=4"
        } else {
            "indexed allocations=0 leaves=0"
        };
        assert!(report.contains(expected), "length={length}: {report}");
        std::fs::remove_dir_all(directory).expect("remove capture frame artifacts");
    }
}
