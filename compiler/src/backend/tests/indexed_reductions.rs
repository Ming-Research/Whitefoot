//! Indexed split implementation obligations; complete value oracles live in
//! the program fixture and are also run through its ordinary sequential build.

use super::{BoundedOutput, build_linked_executable, emit, emit_with_overlap, test_directory};
use std::path::{Path, PathBuf};
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
    observed_with(module, &["histogram", "extrema", "products"])
}

/// [`observed`] with the leaf counters tagged by the position of the first
/// listed source function name found in a chunk's symbol; any other chunk
/// carries the tag after the last name.
fn observed_with(module: &str, names: &[&str]) -> String {
    let mut output = String::new();
    let mut chunk_entry = None;
    for line in module.lines() {
        if line.starts_with("define ") && line.contains("@wf__par_chunk_") {
            chunk_entry = Some(
                names
                    .iter()
                    .position(|name| line.contains(name))
                    .unwrap_or(names.len()),
            );
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
static _Atomic unsigned allocations, releases, live, leaves, loops[8];
uint64_t wf_test_indexed_budget(uint64_t span, uint64_t weight) {
    (void)span; (void)weight;
    return getenv("WF_TEST_ZERO") ? 0 : 2;
}
void wf_test_indexed_leaf(unsigned tag) {
    if (tag >= 8) _Exit(113);
    atomic_fetch_add(&leaves, 1);
    atomic_fetch_add(&loops[tag], 1);
}
void *wf_test_private_allocate(uint64_t bytes) {
    unsigned n = atomic_fetch_add(&allocations, 1) + 1;
    const char *fail = getenv("WF_TEST_FAIL");
    if (fail && n == (unsigned)atoi(fail)) return NULL;
    void *p = malloc((size_t)bytes);
    if (p) atomic_fetch_add(&live, 1);
    for (uint64_t i = 0; p && i < bytes; ++i)
        ((unsigned char *)p)[i] = (unsigned char)((i * 0x9E3779B1u) >> 11);
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
    fprintf(stderr, "indexed loops=%u,%u,%u,%u,%u,%u,%u,%u\n",
            atomic_load(&loops[0]), atomic_load(&loops[1]), atomic_load(&loops[2]),
            atomic_load(&loops[3]), atomic_load(&loops[4]), atomic_load(&loops[5]),
            atomic_load(&loops[6]), atomic_load(&loops[7]));
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

const ITERATIONS: u64 = 262_144;
const QUARTER: u64 = ITERATIONS / 4;
const CELLS: usize = 8;
const TOUCHED: u64 = 5;
const OPERATION_FUNCTIONS: [&str; 6] = [
    "int_and", "int_or", "int_xor", "bool_and", "bool_or", "bool_xor",
];

const INT_AND: &str = r#"fn int_and() -> made: Box<Array<u64>> pure {
  doc "Each quarter of the iterations clears its own lane; all of them clear a shared bit and a cell mark; untouched cells keep the all-ones incoming value.";
  let cells = box_array_filled::<u64>(count: 8_u64, value: 18446744073709551615_u64);
  for (i in 0_u64..262144_u64) {
    let quarter = i / 65536_u64;
    let bucket = i % 5_u64;
    let lane = 1_u64;
    if quarter == 1_u64 {
      set lane = 2_u64;
    } else if quarter == 2_u64 {
      set lane = 4_u64;
    } else if quarter == 3_u64 {
      set lane = 8_u64;
    }
    let step = bucket +wrap 1_u64;
    let mark = step *wrap 65536_u64;
    let shared = ior(lane, 256_u64);
    let clear = ior(shared, mark);
    let keep = inot(clear);
    set cells.inner[bucket] = iand(cells.inner[bucket], keep);
  }
  return move cells;
}
"#;

const INT_OR: &str = r#"fn int_or() -> made: Box<Array<u64>> pure {
  doc "Each quarter sets its own lane; all of them set a shared bit and a cell mark; untouched cells keep a nonzero incoming value.";
  let cells = box_array_filled::<u64>(count: 8_u64, value: 1099511627776_u64);
  for (i in 0_u64..262144_u64) {
    let quarter = i / 65536_u64;
    let bucket = i % 5_u64;
    let lane = 1_u64;
    if quarter == 1_u64 {
      set lane = 2_u64;
    } else if quarter == 2_u64 {
      set lane = 4_u64;
    } else if quarter == 3_u64 {
      set lane = 8_u64;
    }
    let step = bucket +wrap 1_u64;
    let mark = step *wrap 65536_u64;
    let shared = ior(lane, 256_u64);
    let bits = ior(shared, mark);
    set cells.inner[bucket] = ior(cells.inner[bucket], bits);
  }
  return move cells;
}
"#;

const INT_XOR: &str = r#"fn int_xor() -> made: Box<Array<u64>> pure {
  doc "Pseudorandom contributions give every leaf and cell an odd or even number of distinct hits; untouched cells keep a nonzero incoming value.";
  let cells = box_array_filled::<u64>(count: 8_u64, value: 1311768467463790320_u64);
  for (i in 0_u64..262144_u64) {
    let bucket = i % 5_u64;
    let scaled = i *wrap 2654435761_u64;
    let value = scaled +wrap 40503_u64;
    set cells.inner[bucket] = ixor(cells.inner[bucket], value);
  }
  return move cells;
}
"#;

const BOOL_AND: &str = r#"fn bool_and() -> made: Box<Array<Bool>> pure {
  doc "Cells zero to three see one quarter that contributes False, cell four sees two, and the remaining cells keep True.";
  let seed = True();
  let cells = box_array_filled::<Bool>(count: 8_u64, value: seed);
  for (i in 0_u64..262144_u64) {
    let quarter = i / 65536_u64;
    let bucket = i % 5_u64;
    let same = quarter == bucket;
    let corner = bucket == 4_u64;
    let early = quarter < 2_u64;
    let both = band(corner, early);
    let clearing = bor(same, both);
    let flag = bnot(clearing);
    set cells.inner[bucket] = band(cells.inner[bucket], flag);
  }
  return move cells;
}
"#;

const BOOL_OR: &str = r#"fn bool_or() -> made: Box<Array<Bool>> pure {
  doc "Cells zero to three see one quarter that contributes True, cell four sees two, and the remaining cells keep False.";
  let seed = False();
  let cells = box_array_filled::<Bool>(count: 8_u64, value: seed);
  for (i in 0_u64..262144_u64) {
    let quarter = i / 65536_u64;
    let bucket = i % 5_u64;
    let same = quarter == bucket;
    let corner = bucket == 4_u64;
    let late = quarter >= 2_u64;
    let both = band(corner, late);
    let flag = bor(same, both);
    set cells.inner[bucket] = bor(cells.inner[bucket], flag);
  }
  return move cells;
}
"#;

const BOOL_XOR: &str = r#"fn bool_xor() -> made: Box<Array<Bool>> pure {
  doc "Every third iteration contributes True, so the hits of a cell are odd in some leaves and even in others; untouched cells keep True.";
  let seed = True();
  let cells = box_array_filled::<Bool>(count: 8_u64, value: seed);
  for (i in 0_u64..262144_u64) {
    let bucket = i % 5_u64;
    let phase = i % 3_u64;
    let flag = phase == 0_u64;
    set cells.inner[bucket] = bxor(cells.inner[bucket], flag);
  }
  return move cells;
}
"#;

/// The sequential meaning of one integer loop, from the definition of
/// `set cells[i % 5] = cells[i % 5] op x(i)` alone.
fn fold_words(
    init: u64,
    contribution: impl Fn(u64) -> u64,
    combine: fn(u64, u64) -> u64,
) -> [u64; CELLS] {
    let mut cells = [init; CELLS];
    for i in 0..ITERATIONS {
        let bucket = (i % TOUCHED) as usize;
        cells[bucket] = combine(cells[bucket], contribution(i));
    }
    cells
}

/// The same fold for Boolean cells.
fn fold_flags(
    init: bool,
    contribution: impl Fn(u64) -> bool,
    combine: fn(bool, bool) -> bool,
) -> [bool; CELLS] {
    let mut cells = [init; CELLS];
    for i in 0..ITERATIONS {
        let bucket = (i % TOUCHED) as usize;
        cells[bucket] = combine(cells[bucket], contribution(i));
    }
    cells
}

/// Quarter q of the iterations is leaf q once a budget of two halves the range
/// twice; its lane is the bit q.
fn lane_bits(i: u64) -> u64 {
    (1_u64 << (i / QUARTER)) | 256 | ((i % TOUCHED + 1) * 65_536)
}

enum Cells {
    Words([u64; CELLS]),
    Flags([bool; CELLS]),
}

fn operation_expectations() -> Vec<(&'static str, Cells)> {
    let and = fold_words(u64::MAX, |i| !lane_bits(i), |cell, bits| cell & bits);
    let or = fold_words(1 << 40, lane_bits, |cell, bits| cell | bits);
    let xor = fold_words(
        0x1234_5678_9ABC_DEF0,
        |i| i.wrapping_mul(2_654_435_761).wrapping_add(40_503),
        |cell, bits| cell ^ bits,
    );
    let band = fold_flags(
        true,
        |i| {
            let (quarter, bucket) = (i / QUARTER, i % TOUCHED);
            !(quarter == bucket || (bucket == 4 && quarter < 2))
        },
        |cell, flag| cell & flag,
    );
    let bor = fold_flags(
        false,
        |i| {
            let (quarter, bucket) = (i / QUARTER, i % TOUCHED);
            quarter == bucket || (bucket == 4 && quarter >= 2)
        },
        |cell, flag| cell | flag,
    );
    let bxor = fold_flags(true, |i| i % 3 == 0, |cell, flag| cell ^ flag);
    // Fixed anchors computed once outside this module keep the oracle itself
    // from drifting with the fixture arithmetic.
    assert_eq!(and[0], 0xFFFF_FFFF_FFFE_FEF0);
    assert_eq!(and[CELLS - 1], u64::MAX);
    assert_eq!(or[0], 0x0100_0001_010F);
    assert_eq!(or[CELLS - 1], 1 << 40);
    assert_eq!(xor[4], 0x1237_D540_037F_C1AC);
    assert_eq!(xor[CELLS - 1], 0x1234_5678_9ABC_DEF0);
    assert_eq!(bxor, [false, true, true, false, true, true, true, true]);
    assert_eq!(band, [false, false, false, false, false, true, true, true]);
    assert_eq!(bor, [true, true, true, true, true, false, false, false]);
    vec![
        ("int_and", Cells::Words(and)),
        ("int_or", Cells::Words(or)),
        ("int_xor", Cells::Words(xor)),
        ("bool_and", Cells::Flags(band)),
        ("bool_or", Cells::Flags(bor)),
        ("bool_xor", Cells::Flags(bxor)),
    ]
}

/// The six loops followed by a driver whose every comparison is a literal
/// computed by the Rust oracle above, never by another compilation.
fn operations_source() -> String {
    let mut source = String::new();
    for function in [INT_AND, INT_OR, INT_XOR, BOOL_AND, BOOL_OR, BOOL_XOR] {
        source.push_str(function);
        source.push('\n');
    }
    source.push_str("fn main() -> status: std::process::ExitStatus pure {\n");
    for (ordinal, (name, cells)) in operation_expectations().iter().enumerate() {
        source.push_str(&format!(
            "  let res_{name} = {name}();\n  let len_{name} = res_{name}.inner.len;\n  if len_{name} != 8_u64 {{\n    return std::process::exit_status(code: {}_u8);\n  }}\n",
            ordinal + 1
        ));
        for cell in 0..CELLS {
            let code = 10 * (ordinal + 1) + cell;
            let failure = format!("    return std::process::exit_status(code: {code}_u8);\n");
            match cells {
                Cells::Words(values) => source.push_str(&format!(
                    "  if res_{name}.inner[{cell}_u64] != {}_u64 {{\n{failure}  }}\n",
                    values[cell]
                )),
                Cells::Flags(values) => {
                    source.push_str(&format!(
                        "  let seen_{name}_{cell} = res_{name}.inner[{cell}_u64];\n"
                    ));
                    if values[cell] {
                        source.push_str(&format!(
                            "  let flipped_{name}_{cell} = bnot(seen_{name}_{cell});\n  if flipped_{name}_{cell} {{\n{failure}  }}\n"
                        ));
                    } else {
                        source.push_str(&format!("  if seen_{name}_{cell} {{\n{failure}  }}\n"));
                    }
                }
            }
        }
    }
    source.push_str("  return std::process::exit_status(code: 0_u8);\n}\n");
    source
}

/// Instrument `source` with the forced-allowance observer and link it.
fn observed_executable(source: &str, names: &[&str], directory: &Path) -> PathBuf {
    let module = observed_with(&emit_with_overlap(source.as_bytes()), names);
    build_linked_executable(&module, Some(OBSERVER), &[], directory)
}

/// Run an observed program with four workers and return the observer's
/// report. A nonzero exit is the program's own failing comparison or the
/// observer's accounting failure (leaked or unmatched private storage).
fn run_observed(executable: &Path, zero: bool) -> String {
    let mut command = Command::new(executable);
    command.env("WF_WORKERS", "4");
    if zero {
        command.env("WF_TEST_ZERO", "1");
    }
    let output = command.bounded_output().expect("run observed program");
    assert_eq!(
        output.status.code(),
        Some(0),
        "zero={zero}: exit code 10*(operation+1)+cell names the first wrong cell: {output:?}"
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn indexed_bitwise_and_boolean_combines_match_an_independent_oracle_across_forced_leaves() {
    let source = operations_source();
    let module = emit_with_overlap(source.as_bytes());
    for name in OPERATION_FUNCTIONS {
        let body = super::emitted_body(&module, name);
        assert!(
            body.contains(".allocation = call ptr @malloc"),
            "{name} must be permitted to split:\n{body}"
        );
    }
    let directory = test_directory();
    let executable = observed_executable(&source, &OPERATION_FUNCTIONS, &directory);
    // Budget two halves the range twice: every loop runs four leaves over
    // four private ranges, each combined into the root after the join. The
    // observer fails the run unless every private range was freed.
    let report = run_observed(&executable, false);
    assert!(
        report.contains("indexed allocations=6 leaves=24\n"),
        "{report}"
    );
    assert!(
        report.contains("indexed loops=4,4,4,4,4,4,0,0\n"),
        "{report}"
    );
    // The same source with no allowance allocates nothing and still matches
    // the oracle, so the oracle and the fixture agree on the sequential meaning.
    let report = run_observed(&executable, true);
    assert!(
        report.contains("indexed allocations=0 leaves=6\n"),
        "{report}"
    );
    std::fs::remove_dir_all(directory).expect("remove operation artifacts");
}

const OUTER_SHARED: u64 = 32;
const OUTER_OWNED: u64 = 64;
const INNER: u64 = 4096;

/// The outer loop and the inner loop both update `cells`, so the inner split
/// runs inside an outer leaf and must combine into that leaf's private range.
const SHARED_CELLS: &str = r#"fn shared_cells() -> made: Box<Array<u64>> pure {
  doc "Both counted loops update the same cells; the inner updates reach the enclosing leaf's private range.";
  let cells = box_array_filled::<u64>(count: 8_u64, value: 5_u64);
  for (batch in 0_u64..32_u64) {
    for (offset in 0_u64..4096_u64) {
      let base = batch *wrap 4096_u64;
      let item = base +wrap offset;
      let bucket = item % 5_u64;
      let scaled = item *wrap 11400714819323198485_u64;
      let amount = scaled +wrap 1_u64;
      set cells.inner[bucket] = cells.inner[bucket] +wrap amount;
    }
  }
  return move cells;
}
"#;

/// Each outer iteration owns a row; only the inner loop updates it by index.
const OWNED_ROWS: &str = r#"fn owned_rows() -> result: u64 pure {
  doc "Every outer iteration owns a four-cell row that its inner loop updates by index; the outer loop folds the rows into a scalar.";
  let total = 0_u64;
  for (batch in 0_u64..64_u64) {
    let row = array_filled::<u64, 4>(value: 0_u64);
    for (offset in 0_u64..4096_u64) {
      let bucket = offset % 4_u64;
      let amount = batch +wrap offset;
      set row[bucket] = row[bucket] +wrap amount;
    }
    let first = row[0_u64];
    let second = row[1_u64] *wrap 3_u64;
    let third = row[2_u64] *wrap 5_u64;
    let fourth = row[3_u64] *wrap 7_u64;
    let low = first +wrap second;
    let high = third +wrap fourth;
    let combined = low +wrap high;
    set total = total +wrap combined;
  }
  return total;
}
"#;

fn shared_cells_expectation() -> [u64; CELLS] {
    let mut cells = [5_u64; CELLS];
    for batch in 0..OUTER_SHARED {
        for offset in 0..INNER {
            let item = batch * INNER + offset;
            let amount = item
                .wrapping_mul(11_400_714_819_323_198_485)
                .wrapping_add(1);
            let bucket = (item % TOUCHED) as usize;
            cells[bucket] = cells[bucket].wrapping_add(amount);
        }
    }
    cells
}

fn owned_rows_expectation() -> u64 {
    let mut total = 0_u64;
    for batch in 0..OUTER_OWNED {
        let mut row = [0_u64; 4];
        for offset in 0..INNER {
            row[(offset % 4) as usize] += batch + offset;
        }
        for (weight, cell) in [1_u64, 3, 5, 7].into_iter().zip(row) {
            total = total.wrapping_add(cell.wrapping_mul(weight));
        }
    }
    total
}

fn nested_source() -> String {
    let mut source = format!("{SHARED_CELLS}\n{OWNED_ROWS}\n");
    source.push_str("fn main() -> status: std::process::ExitStatus pure {\n  let res_shared = shared_cells();\n  let len_shared = res_shared.inner.len;\n  if len_shared != 8_u64 {\n    return std::process::exit_status(code: 1_u8);\n  }\n");
    for (cell, value) in shared_cells_expectation().into_iter().enumerate() {
        source.push_str(&format!(
            "  if res_shared.inner[{cell}_u64] != {value}_u64 {{\n    return std::process::exit_status(code: {}_u8);\n  }}\n",
            10 + cell
        ));
    }
    source.push_str(&format!(
        "  let rows = owned_rows();\n  if rows != {}_u64 {{\n    return std::process::exit_status(code: 2_u8);\n  }}\n  return std::process::exit_status(code: 0_u8);\n}}\n",
        owned_rows_expectation()
    ));
    source
}

/// The chunk definitions the splitter of `function` calls, numbered by their
/// preorder among the function's helpers.
fn chunk_bodies<'module>(module: &'module str, function: &str) -> Vec<&'module str> {
    let prefix = format!("@wf__par_chunk_{function}.");
    let mut bodies = Vec::new();
    let mut offset = 0;
    for line in module.split_inclusive('\n') {
        if line.starts_with("define ")
            && let Some((_, tail)) = line.split_once(&prefix)
            && let Some((number, _)) = tail.split_once('(')
            && number.parse::<u32>().is_ok()
        {
            let end = module[offset..]
                .find("\n}\n")
                .map_or(module.len(), |length| offset + length + 2);
            bodies.push(&module[offset..end]);
        }
        offset += line.len();
    }
    bodies
}

fn enters_a_splitter(body: &str) -> bool {
    body.matches("@wf__par_split_").count() > body.matches("@wf__par_split_budget").count()
}

#[test]
fn indexed_reductions_split_inside_a_split_and_release_every_nested_private_range() {
    let source = nested_source();
    let module = emit_with_overlap(source.as_bytes());
    for (function, outer_allocates_in_caller) in [("shared_cells", true), ("owned_rows", false)] {
        let chunks = chunk_bodies(&module, function);
        assert_eq!(
            chunks.len(),
            2,
            "{function}: outer and inner chunk\n{module}"
        );
        let outer: Vec<_> = chunks
            .iter()
            .filter(|chunk| enters_a_splitter(chunk))
            .collect();
        let [outer] = outer.as_slice() else {
            panic!("{function}: exactly the outer chunk enters the inner splitter\n{module}");
        };
        assert!(
            outer.contains(".allocation = call ptr @malloc")
                && outer.contains(".combine.head:")
                && outer.contains("call void @free(ptr %indexed."),
            "{function}: the inner split must own its private ranges inside the outer leaf:\n{outer}"
        );
        let inner: Vec<_> = chunks
            .iter()
            .filter(|chunk| !enters_a_splitter(chunk))
            .collect();
        assert!(
            inner
                .iter()
                .all(|chunk| !chunk.contains(".allocation = call ptr @malloc")),
            "{function}: a leaf never allocates for itself"
        );
        assert_eq!(
            super::emitted_body(&module, function).contains(".allocation = call ptr @malloc"),
            outer_allocates_in_caller,
            "{function}"
        );
    }
    let directory = test_directory();
    let executable = observed_executable(&source, &["shared_cells", "owned_rows"], &directory);
    // Each program has four outer leaves under a budget of two. Every outer
    // iteration runs its own inner split of four leaves with one private
    // allocation; the shared-cells program adds the outer split's allocation.
    // The observer fails the run unless frees equal allocations.
    let report = run_observed(&executable, false);
    let allocations = (1 + OUTER_SHARED) + OUTER_OWNED;
    let leaves = (4 + 4 * OUTER_SHARED) + (4 + 4 * OUTER_OWNED);
    let expected = format!("indexed allocations={allocations} leaves={leaves}\n");
    assert!(report.contains(&expected), "{expected}{report}");
    let expected = format!(
        "indexed loops={},{},0,0,0,0,0,0\n",
        4 + 4 * OUTER_SHARED,
        4 + 4 * OUTER_OWNED
    );
    assert!(report.contains(&expected), "{expected}{report}");
    // No allowance: no private range, one chunk call per splitter call, and
    // the same exact answers.
    let report = run_observed(&executable, true);
    let expected = format!(
        "indexed allocations=0 leaves={}\n",
        (1 + OUTER_SHARED) + (1 + OUTER_OWNED)
    );
    assert!(report.contains(&expected), "{expected}{report}");
    std::fs::remove_dir_all(directory).expect("remove nested artifacts");
}
