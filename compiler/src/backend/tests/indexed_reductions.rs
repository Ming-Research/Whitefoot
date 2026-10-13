//! Indexed range-driver obligations; complete value oracles live in the
//! program fixtures and are also run through their ordinary sequential build.
//!
//! The forced observer changes only scheduling inputs: it prices every driver
//! entered from source or a chunk so its whole range is exactly one hand-out
//! wide, and answers each request poll itself instead of the lane word, so
//! the split tree and the private allocations are deterministic at every
//! worker count. Results are the program's own oracles.

use super::{BoundedOutput, build_linked_executable, emit, emit_with_overlap, test_directory};
use std::path::Path;
use std::process::Command;

const PROGRAM: &[u8] = include_bytes!("../../../../tests/programs/parallel/indexed_reductions.wf");

const CALL_PROGRAM: &[u8] = include_bytes!("../../../../tests/programs/parallel/indexed_calls.wf");

/// Every range-driver definition synthesized for `function`.
fn driver_bodies<'module>(module: &'module str, function: &str) -> Vec<&'module str> {
    let prefix = format!("@wf__par_split_{function}.");
    module
        .lines()
        .filter(|line| line.starts_with("define "))
        .filter_map(|line| {
            let (_, tail) = line.split_once(&prefix)?;
            let (number, _) = tail.split_once('(')?;
            number.parse::<u32>().ok()?;
            Some(super::parallel::function_body(
                module,
                &format!("{prefix}{number}"),
            ))
        })
        .collect()
}

/// A fixture whose named literal loop extents are read from a parameter
/// instead, so a call-free loop that the compile-time small-site rule makes
/// ordinary code keeps its range driver. Each replaced text occurs once.
fn with_runtime_extents(source: &[u8], replacements: &[(&str, &str)]) -> Vec<u8> {
    let mut text = std::str::from_utf8(source)
        .expect("UTF-8 fixture")
        .to_owned();
    for (literal, runtime) in replacements {
        assert_eq!(text.matches(literal).count(), 1, "{literal}");
        text = text.replacen(literal, runtime, 1);
    }
    text.into_bytes()
}

/// `direct`, the alias case's one call-free loop, with its extent a parameter.
const RUNTIME_DIRECT: &[(&str, &str)] = &[
    (
        "fn direct() -> result: u64 pure {",
        "fn direct(limit: u64) -> result: u64 pure {",
    ),
    (
        "  for (i in 0_u64..16_u64) {\n    set first^[0_u64]",
        "  for (i in 0_u64..limit) {\n    set first^[0_u64]",
    ),
    (
        "let updated = direct();",
        "let updated = direct(limit: 16_u64);",
    ),
];

/// The field case's call-free `reduce` loop with its extent a parameter.
const RUNTIME_FIELDS: &[(&str, &str)] = &[
    (
        "fn reduce() -> result: u64 pure {",
        "fn reduce(limit: u64) -> result: u64 pure {",
    ),
    ("for (i in 0_u64..128_u64)", "for (i in 0_u64..limit)"),
    (
        "let result = reduce();",
        "let result = reduce(limit: 128_u64);",
    ),
];

fn position(body: &str, needle: &str) -> usize {
    body.find(needle)
        .unwrap_or_else(|| panic!("missing {needle}:\n{body}"))
}

#[test]
fn indexed_sites_query_no_budget_and_drivers_privatize_only_granted_far_halves() {
    let module = emit_with_overlap(PROGRAM);
    assert!(!module.contains("wf__par_split_budget"), "{module}");
    for (name, identities) in [
        ("histogram", &["store i64 0, ptr %indexed.far."][..]),
        (
            "extrema",
            &[
                "store i64 9223372036854775807, ptr %indexed.far.",
                "store i64 -9223372036854775808, ptr %indexed.far.",
            ][..],
        ),
        ("products", &["store i64 1, ptr %indexed.far."][..]),
    ] {
        let site = super::emitted_body(&module, name);
        assert!(site.contains("@wf__par_split_"), "{name}: {site}");
        assert!(
            !site.contains("@wf__heap_take("),
            "{name}: the site predicts and allocates no private storage:\n{site}"
        );
        let drivers = driver_bodies(&module, name);
        let [driver] = drivers.as_slice() else {
            panic!("{name}: one range driver\n{module}");
        };
        // The request word is read directly, once, at the eligible boundary,
        // after the far half's setup charge has been compared.
        assert_eq!(
            driver
                .matches("load atomic i64, ptr @wf__par_demand_word monotonic")
                .count(),
            1,
            "{name}: {driver}"
        );
        assert!(driver.contains("@llvm.uadd.sat.i64"), "{name}: {driver}");
        for identity in identities {
            assert!(
                driver.contains(identity),
                "{name}: missing {identity}:\n{driver}"
            );
        }
        // Granted edge: lane, then allocation and identity fill, then publish.
        let acquire = position(driver, "call ptr @wf__par_acquire_lane(");
        let take = position(driver, ".allocation = call ptr @wf__heap_take(");
        let fill = position(driver, ".fill.head:");
        let publish = position(driver, "call void @wf__par_publish(");
        assert!(
            acquire < take && take < fill && fill < publish,
            "{name}: {driver}"
        );
        // Wait edge: join, combine into the destination, free, then the lane.
        let join = position(driver, "call void @wf__par_join(");
        let combine = position(driver, ".combine.head:");
        let free = position(driver, "call void @wf__heap_give(ptr %indexed.join.");
        let release = position(driver, "call void @wf__par_release(");
        assert!(
            publish < join && join < combine && combine < free && free < release,
            "{name}: {driver}"
        );
        // The retained and the refused far calls name the current
        // destinations; only frame fields receive private storage.
        let recursive: Vec<_> = driver
            .lines()
            .filter(|line| line.contains(" call ") && line.contains("@wf__par_split_"))
            .collect();
        assert_eq!(recursive.len(), 2, "{name}: near and refused far: {driver}");
        assert!(
            recursive.iter().all(|line| !line.contains("%indexed.")),
            "{name}: {driver}"
        );
    }
    assert!(!emit(PROGRAM).contains("%indexed."));
}

#[test]
fn a_call_updated_root_acquires_one_block_shared_by_its_families() {
    let module = emit_with_overlap(CALL_PROGRAM);
    // Two helper families and a direct third family share one root.
    for name in ["call_histogram", "shared_call_fields"] {
        let drivers = driver_bodies(&module, name);
        let [driver] = drivers.as_slice() else {
            panic!("{name}: one range driver\n{module}");
        };
        assert_eq!(
            driver
                .matches(".allocation = call ptr @wf__heap_take(")
                .count(),
            1,
            "{name}: {driver}"
        );
        assert_eq!(
            driver
                .matches("call void @wf__heap_give(ptr %indexed.join.")
                .count(),
            1,
            "{name}: {driver}"
        );
        assert!(driver.contains(".zero.head:"), "{name}: {driver}");
        assert!(!super::emitted_body(&module, name).contains("@wf__heap_take("));
    }
}

/// See the module comment. Private allocations and frees are counted; a
/// heap-exhaustion termination is checked against the acquired prefix. Every
/// entry choice enters the parallel body, so each forced driver is reached.
fn observed(module: &str) -> String {
    let module = super::without_entry_selection(module);
    let mut output = String::new();
    let mut source_or_chunk = false;
    let mut ordinal = 0;
    for line in module.lines() {
        if line.starts_with("define ") {
            let symbol = line.split_once('@').unwrap().1.split_once('(').unwrap().0;
            source_or_chunk = (!symbol.starts_with("wf__par_")
                || symbol.starts_with("wf__par_chunk_"))
                && !symbol.starts_with("wf__ctx_thunk_");
        }
        if source_or_chunk && line.contains(" call ") && line.contains("@wf__par_split_") {
            let (_, arguments) = line.split_once("@wf__par_split_").unwrap();
            let (_, arguments) = arguments.split_once('(').unwrap();
            let (arguments, _) = arguments.rsplit_once(')').unwrap();
            let values: Vec<_> = arguments.split(", ").collect();
            let lower = values[1].strip_prefix("i64 ").unwrap();
            let upper = values[2].strip_prefix("i64 ").unwrap();
            output.push_str(&format!(
                "  %force.{ordinal}.ascending = icmp ult i64 {lower}, {upper}\n  %force.{ordinal}.difference = sub i64 {upper}, {lower}\n  %force.{ordinal}.span = select i1 %force.{ordinal}.ascending, i64 %force.{ordinal}.difference, i64 1\n  %force.{ordinal}.quotient = udiv i64 149999, %force.{ordinal}.span\n  %force.{ordinal}.price = add i64 %force.{ordinal}.quotient, 1\n"
            ));
            let (prefix, _) = line.rsplit_once(", i64 ").unwrap();
            output.push_str(&format!("{prefix}, i64 %force.{ordinal}.price)\n"));
            ordinal += 1;
            continue;
        }
        let line = if line.contains("load atomic i64, ptr @wf__par_demand_word") {
            let (result, _) = line.split_once(" = ").unwrap();
            format!("{result} = call i64 @wf_test_indexed_request()")
        } else if line.contains(".allocation = call ptr @wf__heap_take(") {
            line.replace("@wf__heap_take(", "@wf_test_private_allocate(")
        } else if line.contains("call void @wf__heap_give(ptr %indexed.") {
            line.replace("@wf__heap_give(", "@wf_test_private_free(")
        } else {
            line.replace(
                "call void @wf_resource_abort()",
                "call void @wf_test_indexed_abort()",
            )
        };
        output.push_str(&line);
        output.push('\n');
    }
    output.push_str("\ndeclare i64 @wf_test_indexed_request()\ndeclare ptr @wf_test_private_allocate(i64)\ndeclare void @wf_test_private_free(ptr, i64)\ndeclare void @wf_test_indexed_abort()\n");
    output
}

const OBSERVER: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdatomic.h>
static _Atomic unsigned allocations, releases, live, requests, polls;
uint64_t wf_test_indexed_request(void) {
    unsigned poll = atomic_fetch_add(&polls, 1);
    const char *limit = getenv("WF_TEST_REQUESTS");
    if (limit && poll >= (unsigned)atoi(limit)) return 0;
    atomic_fetch_add(&requests, 1);
    return 1;
}
void *wf_test_private_allocate(uint64_t bytes) {
    unsigned n = atomic_fetch_add(&allocations, 1) + 1;
    const char *fail = getenv("WF_TEST_FAIL");
    if (fail && n == (unsigned)atoi(fail)) return NULL;
    uint64_t *block = malloc(sizeof(*block) * 2 + (size_t)bytes);
    void *p = block ? block + 2 : NULL;
    if (block) {
        *block = bytes;
        atomic_fetch_add(&live, 1);
    }
    /* Poison: a missed identity fill or a copied seed reads garbage. */
    for (uint64_t i = 0; p && i < bytes; ++i)
        ((unsigned char *)p)[i] = (unsigned char)((i * 0x9E3779B1u) >> 11);
    return p;
}
void wf_test_private_free(void *p, uint64_t bytes) {
    if (!p || !atomic_load(&live)) _Exit(110);
    uint64_t *block = (uint64_t *)p - 2;
    if (*block != bytes) _Exit(116);
    atomic_fetch_sub(&live, 1);
    atomic_fetch_add(&releases, 1);
    free(block);
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
    if (atomic_load(&live) || a != r || getenv("WF_TEST_FAIL")) _Exit(112);
    fprintf(stderr, "indexed allocations=%u requests=%u polls=%u\n", a,
            atomic_load(&requests), atomic_load(&polls));
}
__attribute__((constructor)) static void setup(void) { atexit(report); }
"#;

/// What one observed run allocated and asked.
#[derive(Debug, Eq, PartialEq)]
struct Report {
    allocations: u32,
    requests: u32,
    polls: u32,
}

fn observed_executable(source: &[u8], directory: &Path) -> std::path::PathBuf {
    build_linked_executable(
        &observed(&emit_with_overlap(source)),
        Some(OBSERVER),
        &[],
        directory,
    )
}

/// Run an observed program; `requests` limits how many polls the observer
/// answers with a request (`None` answers every poll). A nonzero exit is the
/// program's own failing comparison or the observer's accounting failure:
/// leaked, unmatched or double-freed private storage.
fn run_observed(executable: &Path, workers: u32, requests: Option<u32>) -> Report {
    let mut command = Command::new(executable);
    command.env("WF_WORKERS", workers.to_string());
    if let Some(requests) = requests {
        command.env("WF_TEST_REQUESTS", requests.to_string());
    }
    let output = command.bounded_output().expect("run observed program");
    assert_eq!(
        output.status.code(),
        Some(0),
        "workers={workers} requests={requests:?}: {output:?}"
    );
    let report = String::from_utf8_lossy(&output.stderr);
    let field = |name: &str| -> u32 {
        report
            .split_once(&format!("{name}="))
            .unwrap_or_else(|| panic!("{name} in {report}"))
            .1
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    Report {
        allocations: field("allocations"),
        requests: field("requests"),
        polls: field("polls"),
    }
}

/// A forced allocation failure must free the bundle's acquired prefix and
/// terminate through heap exhaustion [STOR-8], never complete.
fn assert_failures_terminate(executable: &Path, failures: &[u32]) {
    for fail in failures {
        let output = Command::new(executable)
            .env("WF_WORKERS", "4")
            .env("WF_TEST_FAIL", fail.to_string())
            .bounded_output()
            .expect("run private allocation failure");
        assert_eq!(
            output.status.code(),
            Some(90),
            "allocation {fail}: {output:?}"
        );
    }
}

#[test]
fn indexed_split_results_and_private_storage_match_at_every_width() {
    let directory = test_directory();
    let executable = observed_executable(PROGRAM, &directory);
    for workers in [1, 2, 4, 8] {
        let forced = run_observed(&executable, workers, None);
        // One worker selects the sequential computation: no driver runs.
        // Otherwise each loop splits once and its granted far half owns one
        // contribution per root: histogram 1, extrema 2, products 1.
        let expected = if workers == 1 {
            Report {
                allocations: 0,
                requests: 0,
                polls: 0,
            }
        } else {
            Report {
                allocations: 4,
                requests: 3,
                polls: 3,
            }
        };
        assert_eq!(forced, expected, "workers={workers}");
        // Unrequested slices on the current destinations allocate nothing.
        let unrequested = run_observed(&executable, workers, Some(0));
        assert_eq!(unrequested.allocations, 0, "workers={workers}");
        assert_eq!(unrequested.requests, 0, "workers={workers}");
    }
    std::fs::remove_dir_all(directory).expect("remove indexed native artifacts");
}

#[test]
fn indexed_helper_calls_execute_private_blocks_and_preserve_sequential_values() {
    let directory = test_directory();
    let sequential_directory = test_directory();
    let sequential = build_linked_executable(&emit(CALL_PROGRAM), None, &[], &sequential_directory);
    let reference = Command::new(&sequential)
        .env("WF_WORKERS", "1")
        .bounded_output()
        .expect("run sequential helper-call oracle");
    assert_eq!(reference.status.code(), Some(0), "{reference:?}");
    assert!(reference.stderr.is_empty(), "{reference:?}");
    let executable = observed_executable(CALL_PROGRAM, &directory);
    for workers in [2, 4, 8] {
        for requests in [None, Some(0)] {
            let mut command = Command::new(&executable);
            command.env("WF_WORKERS", workers.to_string());
            if let Some(requests) = requests {
                command.env("WF_TEST_REQUESTS", requests.to_string());
            }
            let output = command.bounded_output().expect("run helper-call observer");
            assert_eq!(
                output.status.code(),
                Some(0),
                "{workers} {requests:?}: {output:?}"
            );
            assert_eq!(output.stdout, reference.stdout);
        }
        // The first seven functions enter 13 drivers holding 15 roots; each
        // alias shape enters 1 outer and 16 inner drivers of one root each.
        assert_eq!(
            run_observed(&executable, workers, None),
            Report {
                allocations: 15 + 2 * 17,
                requests: 13 + 2 * 17,
                polls: 13 + 2 * 17,
            },
            "workers={workers}"
        );
    }
    // The first three allocations belong to unnested loops, so nothing else
    // is live when one fails.
    assert_failures_terminate(&executable, &[1, 2, 3]);
    std::fs::remove_dir_all(directory).expect("remove helper-call observer artifacts");
    std::fs::remove_dir_all(sequential_directory).expect("remove sequential helper-call artifacts");
}

#[test]
fn indexed_aliases_acquire_one_private_storage_per_resolved_root() {
    let source = with_runtime_extents(
        include_bytes!("../../../../tests/conformance/cases/par2-pos-indexed-aliased-updates.wf"),
        RUNTIME_DIRECT,
    );
    let directory = test_directory();
    let executable = observed_executable(&source, &directory);
    // One dense family and four call-updated roots; aliased sibling fields
    // share one block.
    assert_eq!(
        run_observed(&executable, 4, None),
        Report {
            allocations: 5,
            requests: 5,
            polls: 5,
        }
    );
    assert_eq!(run_observed(&executable, 4, Some(0)).allocations, 0);
    std::fs::remove_dir_all(directory).expect("remove alias observer artifacts");
}

/// The extent is a parameter: over a literal range this call-free loop is a
/// compile-time small site and keeps no driver.
const TWO_ROOTS: &str = r#"fn two_roots(limit: u64) -> result: u8 pure {
  doc "Two live private roots exercise release when the second allocation fails.";
  let low = box_array_filled::<i64>(count: 4_u64, value: 100_i64);
  let high = box_array_filled::<i64>(count: 4_u64, value: -100_i64);
  for (i in 0_u64..limit) {
    set low.inner[0_u64] = imin(low.inner[0_u64], 7_i64);
    set high.inner[0_u64] = imax(high.inner[0_u64], -7_i64);
  }
  if low.inner[0_u64] != 7_i64 {
    return 1_u8;
  }
  if high.inner[0_u64] != -7_i64 {
    return 2_u8;
  }
  if low.inner[3_u64] != 100_i64 {
    return 3_u8;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let code = two_roots(limit: 4096_u64);
  return std::process::exit_status(code: code);
}
"#;

#[test]
fn indexed_allocation_failure_releases_the_acquired_prefix_and_terminates() {
    let directory = test_directory();
    let executable = observed_executable(TWO_ROOTS.as_bytes(), &directory);
    assert_eq!(
        run_observed(&executable, 4, None),
        Report {
            allocations: 2,
            requests: 1,
            polls: 1,
        }
    );
    // The second root's failure frees the first root's contribution.
    assert_failures_terminate(&executable, &[1, 2]);
    std::fs::remove_dir_all(directory).expect("remove resource test artifacts");
}

#[test]
fn a_far_half_that_cannot_pay_for_its_cells_reads_no_request() {
    // At the forced grain the far half carries about 75,000 work units; two
    // 16,384-cell roots charge 3 * 32,768 = 98,304 for one contribution.
    let source = TWO_ROOTS.replace("count: 4_u64", "count: 16384_u64");
    let directory = test_directory();
    let executable = observed_executable(source.as_bytes(), &directory);
    assert_eq!(
        run_observed(&executable, 4, None),
        Report {
            allocations: 0,
            requests: 0,
            polls: 0,
        }
    );
    std::fs::remove_dir_all(directory).expect("remove setup charge artifacts");
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
        assert_eq!(
            driver_bodies(&module, "folded").len(),
            usize::from(splits),
            "length={length}"
        );
        let directory = test_directory();
        let executable = observed_executable(source.as_bytes(), &directory);
        let expected = if splits {
            Report {
                allocations: 1,
                requests: 1,
                polls: 1,
            }
        } else {
            Report {
                allocations: 0,
                requests: 0,
                polls: 0,
            }
        };
        assert_eq!(
            run_observed(&executable, 4, None),
            expected,
            "length={length}"
        );
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

/// Quarter q of the iterations contributes the lane bit q, so a lost or
/// duplicated contribution of any part of the range changes a cell.
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

#[test]
fn indexed_bitwise_and_boolean_combines_match_an_independent_oracle_across_forced_splits() {
    let source = operations_source();
    let module = emit_with_overlap(source.as_bytes());
    for name in OPERATION_FUNCTIONS {
        assert_eq!(driver_bodies(&module, name).len(), 1, "{name}: {module}");
    }
    let directory = test_directory();
    let executable = observed_executable(source.as_bytes(), &directory);
    // Every loop splits once and combines one private contribution into the
    // root after the join; exit code 10*(operation+1)+cell names the first
    // wrong cell. The unrequested run checks the oracle's sequential meaning.
    for workers in [2, 4, 8] {
        assert_eq!(
            run_observed(&executable, workers, None),
            Report {
                allocations: 6,
                requests: 6,
                polls: 6,
            },
            "workers={workers}"
        );
    }
    assert_eq!(run_observed(&executable, 4, Some(0)).allocations, 0);
    std::fs::remove_dir_all(directory).expect("remove operation artifacts");
}

const OUTER_SHARED: u64 = 32;
const OUTER_OWNED: u64 = 64;
const INNER: u64 = 4096;

/// The outer loop and the inner loop both update `cells`, so the inner split
/// runs inside an outer leaf and must combine into that leaf's private range.
const SHARED_CELLS: &str = r#"fn shared_cells(inner: u64) -> made: Box<Array<u64>> pure {
  doc "Both counted loops update the same cells; the inner updates reach the enclosing leaf's private range.";
  let cells = box_array_filled::<u64>(count: 8_u64, value: 5_u64);
  for (batch in 0_u64..32_u64) {
    for (offset in 0_u64..inner) {
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
const OWNED_ROWS: &str = r#"fn owned_rows(inner: u64) -> result: u64 pure {
  doc "Every outer iteration owns a four-cell row that its inner loop updates by index; the outer loop folds the rows into a scalar.";
  let total = 0_u64;
  for (batch in 0_u64..64_u64) {
    let row = array_filled::<u64, 4>(value: 0_u64);
    for (offset in 0_u64..inner) {
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
    source.push_str("fn main() -> status: std::process::ExitStatus pure {\n  let res_shared = shared_cells(inner: 4096_u64);\n  let len_shared = res_shared.inner.len;\n  if len_shared != 8_u64 {\n    return std::process::exit_status(code: 1_u8);\n  }\n");
    for (cell, value) in shared_cells_expectation().into_iter().enumerate() {
        source.push_str(&format!(
            "  if res_shared.inner[{cell}_u64] != {value}_u64 {{\n    return std::process::exit_status(code: {}_u8);\n  }}\n",
            10 + cell
        ));
    }
    source.push_str(&format!(
        "  let rows = owned_rows(inner: 4096_u64);\n  if rows != {}_u64 {{\n    return std::process::exit_status(code: 2_u8);\n  }}\n  return std::process::exit_status(code: 0_u8);\n}}\n",
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

fn enters_a_driver(body: &str) -> bool {
    body.contains("@wf__par_split_")
}

#[test]
fn indexed_reductions_split_inside_a_split_and_release_every_nested_private_range() {
    let source = nested_source();
    let module = emit_with_overlap(source.as_bytes());
    for (function, outer_privatizes) in [("shared_cells", true), ("owned_rows", false)] {
        let chunks = chunk_bodies(&module, function);
        assert_eq!(
            chunks.len(),
            2,
            "{function}: outer and inner chunk\n{module}"
        );
        let outer: Vec<_> = chunks
            .iter()
            .filter(|chunk| enters_a_driver(chunk))
            .collect();
        assert_eq!(
            outer.len(),
            1,
            "{function}: the outer chunk enters the inner driver"
        );
        assert!(
            chunks
                .iter()
                .all(|chunk| !chunk.contains("@wf__heap_take(")),
            "{function}: a chunk never allocates private storage"
        );
        let drivers = driver_bodies(&module, function);
        assert_eq!(drivers.len(), 2, "{function}: outer and inner driver");
        let privatizing = drivers
            .iter()
            .filter(|driver| driver.contains(".allocation = call ptr @wf__heap_take("))
            .count();
        assert_eq!(privatizing, 1 + usize::from(outer_privatizes), "{function}");
    }
    let directory = test_directory();
    let executable = observed_executable(source.as_bytes(), &directory);
    // Each outer driver splits once; every source outer iteration enters an
    // inner driver that splits once into the current destination, private
    // for the shared cells' far half. The owned-row outer loop is scalar.
    let entries = 1 + OUTER_SHARED + 1 + OUTER_OWNED;
    for workers in [2, 4, 8] {
        assert_eq!(
            run_observed(&executable, workers, None),
            Report {
                allocations: u32::try_from((1 + OUTER_SHARED) + OUTER_OWNED).unwrap(),
                requests: u32::try_from(entries).unwrap(),
                polls: u32::try_from(entries).unwrap(),
            },
            "workers={workers}"
        );
    }
    assert_eq!(run_observed(&executable, 4, Some(0)).allocations, 0);
    std::fs::remove_dir_all(directory).expect("remove nested artifacts");
}

const EXTENDED_FORMS: &str = r#"fn measured() -> made: Box<Array<u64>> pure {
  doc "Read the unchanged descriptor inside every colliding update.";
  let cells = box_array_filled::<u64>(count: 8_u64, value: 5_u64);
  for (i in 0_u64..262144_u64) {
    let bucket = i % 5_u64;
    if bucket < cells.inner.len {
      set cells.inner[bucket] = cells.inner[bucket] +wrap 1_u64;
    }
  }
  return move cells;
}

fn temporaries() -> made: Box<Array<u64>> pure {
  doc "A commuted one-step update crosses unrelated work before its store.";
  let cells = box_array_filled::<u64>(count: 8_u64, value: 5_u64);
  for (i in 0_u64..262144_u64) {
    let bucket = i % 5_u64;
    let updated = 2_u64 +wrap cells.inner[bucket];
    let unrelated = i +wrap 3_u64;
    set cells.inner[bucket] = updated;
  }
  return move cells;
}

fn indexed_saturation() -> made: Box<Array<u32>> pure {
  doc "Each quarter sums to 1310720000; only combining quarters saturates.";
  let cells = box_array_filled::<u32>(count: 2_u64, value: 7_u32);
  for (i in 0_u64..262144_u64) {
    set cells.inner[0_u64] = cells.inner[0_u64] +sat 20000_u32;
  }
  return move cells;
}

fn scalar_saturation() -> result: u32 pure {
  doc "The same cross-leaf clamp applies to a scalar accumulator.";
  let total = 7_u32;
  for (i in 0_u64..262144_u64) {
    set total = 20000_u32 +sat total;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Check literal counts, untouched cells, and the mathematical clamped sum.";
  let measured_cells = measured();
  let temporary_cells = temporaries();
  let saturated_cells = indexed_saturation();
  let saturated_scalar = scalar_saturation();
  let measured_len = measured_cells.inner.len;
  let temporary_len = temporary_cells.inner.len;
  let saturated_len = saturated_cells.inner.len;
  if measured_len != 8_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  if temporary_len != 8_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  if saturated_len != 2_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  for (cell in 0_u64..8_u64) {
    let expected_measure = 5_u64;
    let expected_temporary = 5_u64;
    if cell < 4_u64 {
      set expected_measure = 52434_u64;
      set expected_temporary = 104863_u64;
    } else if cell == 4_u64 {
      set expected_measure = 52433_u64;
      set expected_temporary = 104861_u64;
    }
    if measured_cells.inner[cell] != expected_measure {
      return std::process::exit_status(code: 1_u8);
    }
    if temporary_cells.inner[cell] != expected_temporary {
      return std::process::exit_status(code: 2_u8);
    }
  }
  if saturated_cells.inner[0_u64] != 4294967295_u32 {
    return std::process::exit_status(code: 3_u8);
  }
  if saturated_cells.inner[1_u64] != 7_u32 {
    return std::process::exit_status(code: 4_u8);
  }
  if saturated_scalar != 4294967295_u32 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn extended_forms_lower_to_private_ranges_and_unsigned_saturating_combines() {
    let module = emit_with_overlap(EXTENDED_FORMS.as_bytes());
    for name in ["measured", "temporaries", "indexed_saturation"] {
        let drivers = driver_bodies(&module, name);
        let [driver] = drivers.as_slice() else {
            panic!("{name}: one range driver\n{module}");
        };
        for expected in [
            ".allocation = call ptr @wf__heap_take(",
            ".fill.head:",
            ".combine.head:",
            "call void @wf__heap_give(ptr %indexed.join.",
        ] {
            assert!(
                driver.contains(expected),
                "{name}: missing {expected}:\n{driver}"
            );
        }
    }
    let indexed = driver_bodies(&module, "indexed_saturation")[0];
    assert!(
        indexed.contains("store i32 0, ptr %indexed.far."),
        "{indexed}"
    );
    assert!(
        indexed.contains("= call i32 @llvm.uadd.sat.i32(i32 %indexed.join."),
        "{indexed}"
    );
    assert!(!emit(EXTENDED_FORMS.as_bytes()).contains("%indexed."));
}

#[test]
fn extended_forms_execute_forced_splits_against_literal_oracles() {
    let directory = test_directory();
    let executable = observed_executable(EXTENDED_FORMS.as_bytes(), &directory);
    // Three indexed loops own one contribution each; the scalar loop splits
    // without private storage and keeps its own saturating-result assertion.
    assert_eq!(
        run_observed(&executable, 4, None),
        Report {
            allocations: 3,
            requests: 4,
            polls: 4,
        }
    );
    assert_eq!(run_observed(&executable, 4, Some(0)).allocations, 0);
    std::fs::remove_dir_all(directory).expect("remove extended-form artifacts");
}

const MARKS_FIELDS: &str =
    include_str!("../../../../tests/programs/parallel/indexed_marks_fields.wf");

#[test]
fn indexed_marks_and_fields_execute_private_dense_slabs_and_nested_joins() {
    let module = emit_with_overlap(MARKS_FIELDS.as_bytes());
    for function in ["marks_one", "marks_zero", "marks_bool"] {
        let drivers = driver_bodies(&module, function);
        let [driver] = drivers.as_slice() else {
            panic!("{function}: one range driver\n{module}");
        };
        // False masks, the granted lane's private mode, and a mask-guarded
        // mark into the destination's own mode.
        assert!(driver.contains("store i1 0, ptr %indexed.far."), "{driver}");
        assert!(driver.contains("store i1 true, ptr %"), "{driver}");
        assert!(driver.contains(".mark:"), "{driver}");
    }
    let fields = driver_bodies(&module, "field_values");
    let [fields] = fields.as_slice() else {
        panic!("field_values: one range driver\n{module}");
    };
    assert_eq!(
        fields
            .matches(".allocation = call ptr @wf__heap_take(")
            .count(),
        2
    );
    assert!(fields.contains(".field = select i1"), "{fields}");
    assert!(fields.contains(".stride = select i1"), "{fields}");
    assert!(!emit(MARKS_FIELDS.as_bytes()).contains("%indexed."));
    let directory = test_directory();
    let executable = observed_executable(MARKS_FIELDS.as_bytes(), &directory);
    // marks_one 2 roots, marks_zero 1, marks_bool 1, field_values 2; the
    // nested outer driver owns 4 dense families and each of its 4 inner
    // drivers owns 3.
    for workers in [2, 4, 8] {
        assert_eq!(
            run_observed(&executable, workers, None),
            Report {
                allocations: 22,
                requests: 9,
                polls: 9,
            },
            "workers={workers}"
        );
    }
    assert_eq!(run_observed(&executable, 4, Some(0)).allocations, 0);
    std::fs::remove_dir_all(directory).expect("remove mark/field native artifacts");
}

#[test]
fn indexed_field_family_acquisition_failure_releases_earlier_sibling_slabs() {
    let source = with_runtime_extents(
        include_bytes!("../../../../tests/conformance/cases/par2-pos-indexed-fields.wf"),
        RUNTIME_FIELDS,
    );
    let directory = test_directory();
    let executable = observed_executable(&source, &directory);
    assert_failures_terminate(&executable, &[1, 2]);
    std::fs::remove_dir_all(directory).expect("remove field acquisition artifacts");
}
