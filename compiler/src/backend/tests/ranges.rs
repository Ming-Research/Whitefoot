//! Range references over the three storage origins, and the compute kernels
//! that take a range of work.
//!
//! This module was `slices`, and four of its tests retired with the rules
//! their subject was:
//!
//! - `affine_view_replacement_returns_the_displaced_owner_and_preserves_neighbors`
//!   retired with [SET-2] and [LIV-2]: `let x = replace values[i] = e;` has no
//!   v0.60 production, and [WIN-3] makes a move out of a window slot or an
//!   array element a hard error in its own right. The successors are [OP-11]
//!   `swap(first: &r[i], second: &r[j])`, [OP-10] `remove_at` and `take_back`,
//!   and the ordinary `set r[k] = x;` whose old value takes [WIN-3]'s
//!   disposition.
//! - `borrowed_array_views_preserve_delegation_and_original_storage_across_calls`
//!   and `returned_slice_descriptors_execute_without_transferring_storage`
//!   retired with [VIEW-4] and [VIEW-6]: a view is not a value. `&[T]` is a
//!   reference kind [GRAM-2, TYPE-8] admitted only in parameter position, is
//!   never stored, never a result and never a generic type argument, and
//!   [REF-3] refuses a returned reference outright with the restructuring
//!   `return an index and let the caller form the reference`. The write-back
//!   and const/local/heap-origin coverage those two carried is kept by
//!   `exclusive_range_references_write_original_local_and_field_storage` and
//!   `const_local_and_heap_run_ranges_share_one_read_only_path` below.
//! - `formal_view_child_last_use_restores_parent_writes_across_retained_calls`
//!   retired with [OWN-6] and [OWN-14]: there is no loan, no child view and no
//!   suspension to restore from. [REF-1] makes a reference a name for a path
//!   and re-slicing `&deref(part)[a..b]` [REF-4] another path, so a write
//!   through either reaches the same storage with nothing to hand back.
//!
//! Every compute case below keeps its own subject: the independent oracles and
//! the work-budget prices of the overlap lowering [PAR-1, PAR-2] are unchanged
//! by the reference amendment.

use super::*;

/// Use the formal program's binding path, including pool-off world selection.
fn bind_compute_host_adapter(module: &str, adapter: &str) -> String {
    let directory = test_directory();
    std::fs::create_dir_all(&directory).expect("create adapter directory");
    let input = directory.join("module.ll");
    let host = directory.join("adapter.ll");
    std::fs::write(&input, module).expect("write adapter module");
    std::fs::write(&host, adapter).expect("write host adapter");
    let output = Command::new("awk")
        .arg("-f")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/programs/compute/host-adapter.awk"
        ))
        .arg(input)
        .arg(host)
        .output()
        .expect("bind compute host adapter");
    assert!(output.status.success(), "{output:?}");
    std::fs::remove_dir_all(directory).expect("remove adapter directory");
    String::from_utf8(output.stdout).expect("adapter LLVM text")
}

/// The binder used to miss unassigned indirect-result calls. A tiny LLVM/C
/// consumer checks both result ABIs through the real pool setting, without WF
/// compilation: one native image, run at W1 and W4. Entry counters distinguish
/// the actual world even when both worlds return the same complete result.
#[test]
fn compute_host_adapter_selects_world_for_both_result_abis() {
    let module = r#"declare i32 @wf__par_pool_active()
declare i64 @adapter_value_body(i64, i32)
declare void @adapter_aggregate_body(ptr, i64, i32)
declare i64 @wf_adapter_uncloned(i64)
declare void @wf_adapter_uncloned_output(ptr, i64)
define i64 @wf_adapter_value(i64 %input) {
  %value = call i64 @adapter_value_body(i64 %input, i32 1)
  ret i64 %value
}
define i64 @wf__par_seq_adapter_value(i64 %input) {
  %value = call i64 @adapter_value_body(i64 %input, i32 0)
  ret i64 %value
}
define void @wf_adapter_aggregate(ptr %result, i64 %input) {
  call void @adapter_aggregate_body(ptr %result, i64 %input, i32 1)
  ret void
}
define void @wf__par_seq_adapter_aggregate(ptr %result, i64 %input) {
  call void @adapter_aggregate_body(ptr %result, i64 %input, i32 0)
  ret void
}
"#;
    let adapter = r#"define i64 @adapter_first(ptr %first, ptr %second, i64 %seed) {
  call void @wf_adapter_aggregate(ptr %first, i64 %seed)
  %value = call i64 @wf_adapter_value(i64 %seed)
  call void @wf_adapter_aggregate(ptr %second, i64 %value)
  %plain = call i64 @wf_adapter_uncloned(i64 %value)
  ret i64 %plain
}
define i64 @adapter_second(ptr %first, ptr %second, ptr %plain, i64 %seed) {
  %value = call i64 @wf_adapter_value(i64 %seed)
  call void @wf_adapter_aggregate(ptr %first, i64 %value)
  call void @wf_adapter_uncloned_output(ptr %plain, i64 %value)
  call void @wf_adapter_aggregate(ptr %second, i64 %seed)
  ret i64 %value
}
"#;
    let llvm = bind_compute_host_adapter(module, adapter);
    let host = r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
extern int wf__floor_run(int, char **);
extern int wf__par_pool_active(void);
extern uint64_t adapter_first(uint64_t *, uint64_t *, uint64_t);
extern uint64_t adapter_second(uint64_t *, uint64_t *, uint64_t *, uint64_t);
static unsigned value_calls[2], aggregate_calls[2], plain_value_calls, plain_output_calls;
uint64_t adapter_value_body(uint64_t input, int world) {
    ++value_calls[world];
    return input + 11;
}
void adapter_aggregate_body(uint64_t *result, uint64_t input, int world) {
    ++aggregate_calls[world];
    result[0] = input + 3;
    result[1] = input * 3 + 1;
    result[2] = input + 113;
}
uint64_t wf_adapter_uncloned(uint64_t input) {
    ++plain_value_calls;
    return input + 29;
}
void wf_adapter_uncloned_output(uint64_t *result, uint64_t input) {
    ++plain_output_calls;
    result[0] = input + 37;
}
int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    const char *workers = getenv("WF_WORKERS");
    if (!workers || (strcmp(workers, "1") && strcmp(workers, "4"))) return 1;
    unsigned world = !strcmp(workers, "4");
    if ((wf__par_pool_active() != 0) != world) return 2;
    struct { uint64_t before, fields[3], after; } cells[5];
    for (unsigned i = 0; i < 5; ++i) {
        cells[i].before = UINT64_C(0x1020304050607080) + i;
        cells[i].after = UINT64_C(0x8070605040302010) + i;
        for (unsigned j = 0; j < 3; ++j) cells[i].fields[j] = 777;
    }
    if (adapter_first(cells[0].fields, cells[1].fields, 17) != 57) return 3;
    if (adapter_second(cells[2].fields, cells[3].fields, cells[4].fields, 23) != 34) return 4;
    const uint64_t expected[5][3] = {
        {20, 52, 130}, {31, 85, 141}, {37, 103, 147}, {26, 70, 136}, {71, 777, 777}
    };
    for (unsigned i = 0; i < 5; ++i) {
        if (cells[i].before != UINT64_C(0x1020304050607080) + i ||
            cells[i].after != UINT64_C(0x8070605040302010) + i) return 5;
        for (unsigned j = 0; j < 3; ++j)
            if (cells[i].fields[j] != expected[i][j]) return 6;
    }
    if (value_calls[world] != 2 || value_calls[1 - world] != 0 ||
        aggregate_calls[world] != 4 || aggregate_calls[1 - world] != 0 ||
        plain_value_calls != 1 || plain_output_calls != 1) return 7;
    printf("host adapter worlds PASS: world=%u\n", world);
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
"#;
    let directory = test_directory();
    let executable = build_linked_executable(&llvm, Some(host), &[], &directory);
    for (workers, world) in [("1", 0), ("4", 1)] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
            .expect("run host adapter world probe");
        assert!(output.status.success(), "WF_WORKERS={workers}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains(&format!("host adapter worlds PASS: world={world}"))
        );
    }
    std::fs::remove_dir_all(directory).expect("remove host adapter world probe");
}

fn run_compute_oracle(executable: &Path, name: &str, parallel: bool) {
    // A sequential image has no worker world to select. The parallel image
    // additionally tests the pool-off fallback and two real pool widths.
    let widths: &[u32] = if parallel { &[1, 2, 4] } else { &[1] };
    for workers in widths {
        let output = Command::new(executable)
            .env("WF_WORKERS", workers.to_string())
            .env_remove("WF_SPLIT_WORK")
            .output()
            .expect("run independent compute oracle");
        assert!(
            output.status.success(),
            "{name} workers={workers}: {output:?}"
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains(&format!("{name} oracle PASS:")));
    }
}

fn build_compute_oracle(
    module: &str,
    oracle: &str,
    defines: &[String],
    directory: &Path,
) -> std::path::PathBuf {
    let parallel = defines
        .iter()
        .any(|define| define == "WFB_ORACLE_PARALLEL=1");
    if parallel {
        build_linked_executable(
            &super::parallel::observe_worker_schedule(module),
            Some(&format!("{}\n{oracle}", super::parallel::WORKER_SCHEDULE)),
            defines,
            directory,
        )
    } else {
        build_linked_executable(module, Some(oracle), defines, directory)
    }
}

#[test]
fn runtime_work_estimates_are_total_for_empty_and_inverted_ranges() {
    let source = br#"fn count_work(lower: u64, upper: u64) -> result: u64 pure {
  let total = 0_u64;
  for (i in lower..upper) {
    set total = total +wrap i;
  }
  return total;
}

fn write_work(count: u64, lower: u64, upper: u64) -> result: u64 pure contract {
  requires count <= 2_u64;
} {
  let output = array_filled::<u64, 2>(value: 0_u64);
  for (i in 0_u64..count) {
    set output[i] = count_work(lower: lower, upper: upper);
  }
  let first = output[0_u64];
  let second = output[1_u64];
  return first +wrap second;
}

fn main() -> status: std::process::ExitStatus pure {
  let total = write_work(count: 2_u64, lower: 0_u64, upper: 17_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    // The written run is a frame-resident `Array<u64, 2>` [TYPE-9] and the
    // worker returns the two slots it filled as one scalar, so the probe reads
    // what the loop produced without a hand-written descriptor ABI. The old
    // `{ ptr, i64 }` probe destructured a `buffer<u64>` return by value, and
    // that spelling, its `buffer_new` head and its `release_work` consumer all
    // left with the storage class [TYPE-9, OP-13].
    let mut llvm = emit_with_overlap(source)
        .replace("@main(", "@wf_total_work_main(")
        .replace("@wf__main_body(", "@wf_total_work_body(")
        .replace(
            "call i64 @wf__par_split_budget(",
            "call i64 @wf_work_budget(",
        );
    llvm.push_str("\ndeclare i64 @wf_work_budget(i64, i64)\n");
    let host = r#"#include <stdint.h>
#include <stdio.h>
extern int wf__floor_run(int, char **);
extern uint64_t wf_write_work(uint64_t, uint64_t, uint64_t);
static uint64_t price;
uint64_t wf_work_budget(uint64_t span, uint64_t weight) {
    (void)span; price = weight; return 0;
}
int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    if (wf_write_work(2, 0, 17) != 272) return 1;
    if (wf_write_work(2, 17, 1) != 0) return 2;
    if (wf_write_work(0, 17, 1) != 0) return 3;
    uint64_t inverted = price;
    if (wf_write_work(0, 0, 0) != 0) return 3;
    if (inverted != price) return 3;
    /* No inner source iteration runs. Pricing must nevertheless remain total
       when its independent work-count arithmetic exceeds the machine word. */
    if (wf_write_work(0, 0, UINT64_MAX) != 0) return 4;
    if (price <= UINT64_MAX / 1024) return 4;
    printf("saturated work price: %llu\n", (unsigned long long)price);
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
"#;
    let directory = test_directory();
    let executable = build_linked_executable(&llvm, Some(host), &[], &directory);
    let output = Command::new(executable)
        .output()
        .expect("run total scheduling estimate probe");
    assert!(output.status.success(), "{output:?}");
    std::fs::remove_dir_all(directory).expect("remove total scheduling estimate probe");
}

#[test]
fn runtime_array_helper_prices_use_only_original_readonly_reference_captures() {
    let mut source = String::from(
        r#"fn sum_owner(input: &Box<Array<u64>>) -> result: u64 reads(input) {
  let count = deref(input).inner.len;
  let total = 0_u64;
  for (i in 0_u64..count) {
    set total = total +wrap deref(input).inner[i];
  }
  return total;
}

fn sum_range(input: &[u64]) -> result: u64 reads(input) {
  let count = deref(input).len;
  let total = 0_u64;
  for (i in 0_u64..count) {
    set total = total +wrap deref(input)[i];
  }
  return total;
}

fn sum_other(input: &Box<u64>) -> result: u64 reads(input) {
  let count = deref(input).inner;
  let total = 0_u64;
  for (i in 0_u64..count) {
    set total = total +wrap 7_u64;
  }
  return total;
}

fn sum_shared(first: &Box<Array<u64>>, second: &Box<Array<u64>>) -> result: u64 reads(first), reads(second) {
  let first_sum = sum_owner(input: first);
  let second_sum = sum_owner(input: second);
  return first_sum +wrap second_sum;
}
"#,
    );
    for (name, parameter, effect, prefix, call, construct, actual) in [
        (
            "owner",
            "&Box<Array<u64>>",
            "reads(input)",
            "",
            "sum_owner(input: input)",
            "box_array_filled::<u64>(count: count, value: 7_u64)",
            "&input",
        ),
        (
            "range",
            "&[u64]",
            "reads(input)",
            "",
            "sum_range(input: input)",
            "box_array_filled::<u64>(count: count, value: 7_u64)",
            "&input.inner[0_u64..count]",
        ),
        (
            "mutable",
            "&Box<Array<u64>>",
            "writes(input)",
            "  if deref(input).inner.len != 0_u64 {\n    set deref(input).inner[0_u64] = 7_u64;\n  }\n",
            "sum_owner(input: input)",
            "box_array_filled::<u64>(count: count, value: 7_u64)",
            "&input",
        ),
        (
            "rebound",
            "&Box<Array<u64>>",
            "reads(input)",
            "  let original_count = deref(input).inner.len;\n  let replacement = box_array_filled::<u64>(count: count, value: 7_u64);\n  let selected = input;\n  set selected = &replacement;\n",
            "sum_owner(input: selected)",
            "box_array_filled::<u64>(count: 1_u64, value: 7_u64)",
            "&input",
        ),
        (
            "local",
            "&Box<Array<u64>>",
            "reads(input)",
            "  let original_count = deref(input).inner.len;\n  let replacement = box_array_filled::<u64>(count: count, value: 7_u64);\n",
            "sum_owner(input: &replacement)",
            "box_array_filled::<u64>(count: 1_u64, value: 7_u64)",
            "&input",
        ),
        (
            "other",
            "&Box<u64>",
            "reads(input)",
            "",
            "sum_other(input: input)",
            "box_new::<u64>(value: count)",
            "&input",
        ),
        (
            "shared",
            "&Box<Array<u64>>",
            "reads(input)",
            "",
            "sum_shared(first: input, second: input)",
            "box_array_filled::<u64>(count: count, value: 7_u64)",
            "&input",
        ),
    ] {
        source.push_str(&format!(
            r#"
fn write_{name}(input: {parameter}, count: u64, iterations: u64) -> result: u64 {effect} contract {{
  requires count <= 65536_u64;
  requires iterations <= 2_u64;
}} {{
{prefix}  let output = array_filled::<u64, 2>(value: 0_u64);
  for (i in 0_u64..iterations) {{
    set output[i] = {call};
  }}
  return output[0_u64] +wrap output[1_u64];
}}

fn probe_{name}(count: u64, iterations: u64) -> result: u64 pure contract {{
  requires count <= 65536_u64;
  requires iterations <= 2_u64;
}} {{
  let input = {construct};
  return write_{name}(input: {actual}, count: count, iterations: iterations);
}}
"#,
        ));
    }
    source.push_str(
        "\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
    );
    let mut llvm = emit_with_overlap(source.as_bytes())
        .replace("@main(", "@wf_reference_price_main(")
        .replace("@wf__main_body(", "@wf_reference_price_body(")
        .replace(
            "call i64 @wf__par_split_budget(",
            "call i64 @wf_work_budget(",
        );
    llvm.push_str("\ndeclare i64 @wf_work_budget(i64, i64)\n");
    let host = r#"#include <stdint.h>
#include <stdio.h>
extern int wf__floor_run(int, char **);
extern uint64_t wf_probe_owner(uint64_t, uint64_t), wf_probe_range(uint64_t, uint64_t);
extern uint64_t wf_probe_mutable(uint64_t, uint64_t), wf_probe_rebound(uint64_t, uint64_t);
extern uint64_t wf_probe_local(uint64_t, uint64_t), wf_probe_other(uint64_t, uint64_t);
extern uint64_t wf_probe_shared(uint64_t, uint64_t);
typedef uint64_t (*probe)(uint64_t, uint64_t);
static uint64_t first_price;
static unsigned observed;
uint64_t wf_work_budget(uint64_t span, uint64_t price) {
    (void)span;
    if (observed++ == 0) first_price = price;
    return 0;
}
int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    const uint64_t extents[] = {0, 1, 17, 4096};
    const probe probes[] = {wf_probe_owner, wf_probe_range, wf_probe_mutable,
        wf_probe_rebound, wf_probe_local, wf_probe_other, wf_probe_shared};
    const char *names[] = {"owner", "range", "mutable", "rebound", "local", "other", "shared"};
    const unsigned dynamic[] = {1, 1, 0, 0, 0, 0, 1};
    for (unsigned p = 0; p < sizeof(probes) / sizeof(*probes); ++p) {
        uint64_t previous = 0;
        for (unsigned e = 0; e < sizeof(extents) / sizeof(*extents); ++e) {
            uint64_t count = extents[e];
            observed = 0;
            uint64_t actual = probes[p](count, 2);
            uint64_t expected = count * (p == 6 ? 28 : 14);
            if (!observed || actual != expected) return 1;
            uint64_t price = first_price;
            printf("%s extent=%llu price=%llu\n", names[p],
                   (unsigned long long)count, (unsigned long long)price);
            if (e && (dynamic[p] ? price <= previous : price != previous)) return 2;
            previous = price;
            observed = 0;
            if (probes[p](count, 0) != 0 || !observed || first_price != price) return 3;
        }
    }
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
"#;
    let directory = test_directory();
    let executable = build_linked_executable(&llvm, Some(host), &[], &directory);
    let output = Command::new(executable)
        .env("WF_WORKERS", "1")
        .output()
        .expect("run checked-reference scheduling price probe");
    assert!(output.status.success(), "{output:?}");
    std::fs::remove_dir_all(directory).expect("remove checked-reference scheduling price probe");
}

#[test]
fn runtime_work_prices_post_loop_arithmetic_once_per_enclosing_iteration() {
    let mut source = String::new();
    for (extent, bound) in [("known", "upper"), ("fallback", "i")] {
        for position in ["before", "after"] {
            let bias = "    let bias = upper *wrap 3_u64;\n";
            let (before, after) = if position == "before" {
                (bias, "")
            } else {
                ("", bias)
            };
            source.push_str(&format!(
                r#"fn count_{extent}_{position}(upper: u64, repeats: u64) -> result: u64 pure {{
  let total = 0_u64;
  for (i in 0_u64..repeats) {{
{before}    for (j in 0_u64..{bound}) {{
      let multiplied = total *wrap 3_u64;
      set total = multiplied +wrap j;
    }}
{after}    set total = total +wrap bias;
  }}
  return total;
}}

fn write_{extent}_{position}(upper: u64, repeats: u64) -> result: u64 pure {{
  let output = array_filled::<u64, 2>(value: 0_u64);
  for (i in 0_u64..2_u64) {{
    set output[i] = count_{extent}_{position}(upper: upper, repeats: repeats);
  }}
  let first = output[0_u64];
  let second = output[1_u64];
  let same = first == second;
  let answer = if same {{
    give first;
  }} else {{
    give 18446744073709551615_u64;
  }}
  return answer;
}}

"#
            ));
        }
    }
    source.push_str(
        r#"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    let mut llvm = emit_with_overlap(source.as_bytes())
        .replace("@main(", "@wf_continuation_main(")
        .replace("@wf__main_body(", "@wf_continuation_body(")
        .replace(
            "call i64 @wf__par_split_budget(",
            "call i64 @wf_work_budget(",
        );
    // The agreement of the two written slots, which the retired `{ ptr, i64 }`
    // probe computed by loading them out of a `buffer<u64>` descriptor, is now
    // computed where the run lives: the worker reads its own frame-resident
    // `Array<u64, 2>` [TYPE-9] and returns one scalar, so the case needs no
    // hand-written descriptor ABI and no `release_work` consumer.
    llvm.push_str("\ndeclare i64 @wf_work_budget(i64, i64)\n");
    let host = r#"#include <stdint.h>
#include <stdio.h>
extern int wf__floor_run(int, char **);
extern uint64_t wf_write_known_before(uint64_t, uint64_t), wf_write_known_after(uint64_t, uint64_t);
extern uint64_t wf_write_fallback_before(uint64_t, uint64_t), wf_write_fallback_after(uint64_t, uint64_t);
static uint64_t price;
uint64_t wf_work_budget(uint64_t span, uint64_t weight) {
    (void)span; price = weight; return 0;
}
int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    const uint64_t extents[] = {0, 1, 16, 17}, repetitions[] = {1, 2, 4};
    uint64_t outer_prices[3];
    for (unsigned r = 0; r < 3; ++r) {
      uint64_t repeats = repetitions[r];
      (void)wf_write_known_before(16, repeats);
      uint64_t fallback_price = price;
      for (unsigned i = 0; i < 4; ++i) {
        uint64_t upper = extents[i];
        uint64_t expected = 0, fallback_expected = 0;
        for (uint64_t pass = 0; pass < repeats; ++pass) {
            for (uint64_t j = 0; j < upper; ++j) expected = expected * 3 + j;
            expected += 3 * upper;
            for (uint64_t j = 0; j < pass; ++j) fallback_expected = fallback_expected * 3 + j;
            fallback_expected += 3 * upper;
        }
        if (wf_write_known_before(upper, repeats) != expected) return 1;
        uint64_t before = price;
        if (!i) outer_prices[r] = before;
        if (wf_write_known_after(upper, repeats) != expected) return 2;
        printf("known %llu: before=%llu after=%llu\n",
               (unsigned long long)upper, (unsigned long long)before, (unsigned long long)price);
        if (!before || before != price) return 3;
        if (wf_write_fallback_before(upper, repeats) != fallback_expected) return 4;
        before = price;
        if (wf_write_fallback_after(upper, repeats) != fallback_expected) return 5;
        printf("fallback %llu: before=%llu after=%llu\n",
               (unsigned long long)upper, (unsigned long long)before, (unsigned long long)price);
        if (!before || before != price || price != fallback_price) return 6;
      }
    }
    /* Moving the arithmetic must not erase the enclosing trip multiplier.
       The unknown inner extent must retain the calibrated static factor 16. */
    if (outer_prices[1] <= outer_prices[0] ||
        outer_prices[2] - outer_prices[1] != 2 * (outer_prices[1] - outer_prices[0])) return 7;
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
"#;
    let directory = test_directory();
    let executable = build_linked_executable(&llvm, Some(host), &[], &directory);
    let output = Command::new(executable)
        .env("WF_WORKERS", "2")
        .output()
        .expect("run continuation work price probe");
    assert!(output.status.success(), "{output:?}");
    std::fs::remove_dir_all(directory).expect("remove continuation work price probe");
}

#[test]
fn runtime_block_sizes_change_budget_prices_without_changing_scan_results() {
    let source = include_bytes!("../../../../tests/programs/compute/prefix.wf");
    let module = emit_with_overlap(source);
    let adapter = include_str!("../../../../tests/programs/compute/prefix_host.ll");
    let mut llvm = bind_compute_host_adapter(&module, adapter)
        .replace("@main(", "@wf_extent_test_main(")
        .replace("@wf__main_body(", "@wf_extent_test_body(")
        .replace(
            "call i64 @wf__par_split_budget(",
            "call i64 @wf_work_budget(",
        );
    llvm.push_str("\ndeclare i64 @wf_work_budget(i64, i64)\n");
    let host = r#"#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
extern int wf__floor_run(int, char **);
extern int wf__par_pool_active(void);
extern uint64_t wf__par_split_budget(uint64_t, uint64_t);
/* The owned result is a Box<Array<u64>> cell; the adapter hands that cell back
 * as the retained handle and the release row consumes exactly it. */
extern void wf_bench_prefix(const uint64_t *, uint64_t, uint64_t, uint64_t, uint64_t **, uint64_t *, void **);
extern void wf_bench_prefix_release(void *);
static _Atomic uint64_t observed;
uint64_t wf_work_budget(uint64_t span, uint64_t weight) {
    if (span == 128) {
        uint64_t previous = atomic_load(&observed);
        while (previous < weight && !atomic_compare_exchange_weak(&observed, &previous, weight)) {}
    }
    return wf__par_split_budget(span, weight);
}
int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    const uint64_t widths[] = {17, 1024, 4096};
    uint64_t previous_price = 0;
    for (unsigned shape = 0; shape < 3; ++shape) {
        uint64_t width = widths[shape], count = 128 * width;
        uint64_t *input = calloc((size_t)count, sizeof(*input));
        if (!input) return 1;
        for (uint64_t i = 0; i < count; ++i) input[i] = i + 1;
        uint64_t *output = NULL, length = 0;
        void *held = NULL;
        atomic_store(&observed, 0);
        wf_bench_prefix(input, count, width, 1, &output, &length, &held);
        if (!output || length != count) return 2;
        uint64_t expected = 0;
        for (uint64_t i = 0; i < count; ++i) {
            if (output[i] != expected || input[i] != i + 1) return 3;
            expected += i + 1;
        }
        uint64_t price = atomic_load(&observed);
        printf("%llu %llu\n", (unsigned long long)width, (unsigned long long)price);
        if (wf__par_pool_active()) {
            if (!price || (shape == 1 && price <= previous_price * 10) ||
                (shape == 2 && price <= previous_price * 3)) return 4;
        } else if (price) return 5;
        previous_price = price;
        wf_bench_prefix_release(held);
        free(input);
    }
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
"#;
    let directory = test_directory();
    let executable = build_linked_executable(&llvm, Some(host), &[], &directory);
    for workers in [1, 2, 4] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers.to_string())
            .env_remove("WF_SPLIT_WORK")
            .output()
            .expect("run runtime extent price probe");
        assert!(output.status.success(), "workers={workers}: {output:?}");
    }
    std::fs::remove_dir_all(directory).expect("remove runtime extent probe");
}

#[test]
fn compute_oracle_rejects_wrong_values_and_missing_worker_observations() {
    let source = include_str!("../../../../tests/programs/compute/prefix.wf");
    let adapter = include_str!("../../../../tests/programs/compute/prefix_host.ll");
    let oracle = format!(
        "#define WFB_BLOCKED_ORACLE\n#define WFB_PREFIX\n{}",
        include_str!("../../../../tests/programs/compute/blocked_oracle.c")
    );
    for corrupt in [true, false] {
        let program = if corrupt {
            let changed = source.replacen(
                "total +wrap deref(input)[i]",
                "total -wrap deref(input)[i]",
                1,
            );
            assert_ne!(
                changed, source,
                "the block-sum mutant must change the algorithm"
            );
            changed
        } else {
            source.to_owned()
        };
        let module = emit_with_overlap(program.as_bytes());
        let llvm = bind_compute_host_adapter(&module, adapter)
            .replace("@main(", "@wf_oracle_negative_main(")
            .replace("@wf__main_body(", "@wf_oracle_negative_body(");
        let directory = test_directory();
        let mut defines = vec!["WFB_ORACLE_PARALLEL=1".to_owned()];
        if !corrupt {
            // The worker runs the real algorithm, but the intentionally
            // disabled counter must still fail the observation check.
            defines.push("WF_SCHED_STATS=0".to_owned());
        }
        let executable = build_compute_oracle(&llvm, &oracle, &defines, &directory);
        let failure = std::panic::catch_unwind(|| run_compute_oracle(&executable, "prefix", true))
            .expect_err("a wrong result or absent observation must fail the oracle test");
        let message = failure
            .downcast_ref::<String>()
            .expect("oracle assertion message");
        if corrupt {
            assert!(message.contains("workers=1"), "{message}");
            assert!(message.contains("wrong result"), "{message}");
        } else {
            assert!(message.contains("workers=2"), "{message}");
            assert!(message.contains("oracle observed no steals"), "{message}");
        }
        std::fs::remove_dir_all(directory).expect("remove oracle negative-test files");
    }
}

/// Two proved-disjoint range references over one storage write that
/// storage's own slots [REF-4, OWN-7], and the callee's declared row carries
/// the write back to the caller's array and window alike [EFF-5, CALL-3].
#[test]
fn range_references_over_one_storage_write_the_original_array_and_window() {
    let source = r#"struct Packet {
  before: u64;
  values: STORAGE;
  after: u64;
}

fn fill(values: &STORAGE) -> result: unit writes(values) contract {
  requires deref(values).len == 8_u64;
  ensures deref(values).len == deref(entry(values)).len;
} {
  let left = &deref(values)[0_u64..4_u64];
  let right = &deref(values)[4_u64..8_u64];
  set deref(left)[0_u64] = 11_u64;
  set deref(left)[3_u64] = 13_u64;
  set deref(right)[0_u64] = 17_u64;
  set deref(right)[3_u64] = 19_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let initial = array_filled::<u64, 8>(value: 7_u64);
  let values = INITIAL;
  let packet = Packet(before: 53_u64, values: TRANSFER, after: 59_u64);
  let done = fill(values: &packet.values);
  if packet.before != 53_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if packet.after != 59_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if packet.values[0_u64] != 11_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if packet.values[3_u64] != 13_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if packet.values[4_u64] != 17_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  if packet.values[7_u64] != 19_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  if packet.values[1_u64] != 7_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  if packet.values[6_u64] != 7_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for (storage, initial, transfer) in [
        ("Array<u64, 8>", "initial", "values"),
        (
            "Slots<u64, 8>",
            "slots_from_array::<u64, 8>(values: initial)",
            "move values",
        ),
    ] {
        let source = source
            .replace("STORAGE", storage)
            .replace("INITIAL", initial)
            .replace("TRANSFER", transfer);
        for overlap in [OverlapLowering::Off, OverlapLowering::On] {
            let module = emit_lowered(source.as_bytes(), overlap);
            for module in [&module, &super::owned_places::retain_calls(&module)] {
                let output = compile_and_run(module);
                assert_eq!(
                    output.status.code(),
                    Some(0),
                    "{storage}: {overlap:?}: {output:?}"
                );
                assert!(output.stdout.is_empty(), "{output:?}");
                assert!(output.stderr.is_empty(), "{output:?}");
            }
        }
    }
}

#[test]
fn blocked_compute_matches_independent_oracles_at_runtime_dimensions() {
    let kernels: [(&str, &[u8], &str, &str); 2] = [
        (
            "prefix",
            include_bytes!("../../../../tests/programs/compute/prefix.wf"),
            include_str!("../../../../tests/programs/compute/prefix_host.ll"),
            "WFB_PREFIX",
        ),
        (
            "histogram",
            include_bytes!("../../../../tests/programs/compute/histogram.wf"),
            include_str!("../../../../tests/programs/compute/histogram_host.ll"),
            "WFB_HISTOGRAM",
        ),
    ];
    for (name, source, adapter, selection) in kernels {
        let oracle = format!(
            "#define WFB_BLOCKED_ORACLE\n#define {selection}\n{}",
            include_str!("../../../../tests/programs/compute/blocked_oracle.c")
        );
        for emitted in [compile(source), emit_with_overlap(source)] {
            let llvm = bind_compute_host_adapter(&emitted, adapter)
                .replace("@main(", "@wf_blocked_smoke_main(")
                .replace("@wf__main_body(", "@wf_blocked_smoke_body(");
            let directory = test_directory();
            let defines = if emitted.contains("call void @wf__par_publish(") {
                vec!["WFB_ORACLE_PARALLEL=1".to_owned()]
            } else {
                Vec::new()
            };
            let executable = build_compute_oracle(&llvm, &oracle, &defines, &directory);
            run_compute_oracle(&executable, name, !defines.is_empty());
            std::fs::remove_dir_all(directory).expect("remove blocked-compute test files");
        }
    }
}

#[test]
fn stable_scatter_matches_an_independent_oracle_and_hands_out_output_work() {
    let source = include_bytes!("../../../../tests/programs/compute/radix_scatter.wf");
    let adapter = include_str!("../../../../tests/programs/compute/radix_scatter_host.ll");
    let oracle = format!(
        "#define WFB_SCATTER_ORACLE\n{}",
        include_str!("../../../../tests/programs/compute/radix_scatter_oracle.c")
    );
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let emitted = emit_lowered(source, overlap);
        let mut llvm = bind_compute_host_adapter(&emitted, adapter)
            .replace("@main(", "@wf_scatter_smoke_main(")
            .replace("@wf__main_body(", "@wf_scatter_smoke_body(");
        let parallel = overlap == OverlapLowering::On;
        let defines = if parallel {
            let partition_entry = llvm
                .lines()
                .find(|line| line.starts_with("define ") && line.contains(" @wf_write_chunk("))
                .expect("ordinary input partition entry")
                .to_owned();
            llvm = llvm.replace(
                &partition_entry,
                &partition_entry.replace("@wf_write_chunk(", "@wf_scatter_original_partition("),
            );
            // Each range reference crosses these calls as its element pointer
            // and count, the ordinary callable ABI (compiler/backend-facts).
            llvm.push_str(
                "\ndeclare void @wf_scatter_partition_done(i64)\n\
define i64 @wf_write_chunk(ptr %input, i64 %input.len, i32 %bit, ptr %output, i64 %output.len) {\n\
  %n = call i64 @wf_scatter_original_partition(ptr %input, i64 %input.len, i32 %bit, ptr %output, i64 %output.len)\n\
  call void @wf_scatter_partition_done(i64 %n)\n\
  ret i64 %n\n}\n",
            );
            // The count/partition map has already joined before this entry.
            // Wrap only the ordinary parallel entry, leaving its recursive
            // budget family and the pool-off sequential world untouched.
            let entry = llvm
                .lines()
                .find(|line| line.starts_with("define ") && line.contains(" @wf_pack_chunks("))
                .expect("ordinary packing entry")
                .to_owned();
            llvm = llvm.replace(
                &entry,
                &entry.replace("@wf_pack_chunks(", "@wf_scatter_original_pack("),
            );
            llvm.push_str(
                "\ndeclare void @wf_scatter_pack_begin()\n\
declare void @wf_scatter_pack_end()\n\
define i64 @wf_pack_chunks(ptr %chunks, i64 %first, ptr %low, i64 %low.len, ptr %high, i64 %high.len) {\n\
  call void @wf_scatter_pack_begin()\n\
  %r = call i64 @wf_scatter_original_pack(ptr %chunks, i64 %first, ptr %low, i64 %low.len, ptr %high, i64 %high.len)\n\
  call void @wf_scatter_pack_end()\n\
  ret i64 %r\n}\n",
            );
            let copy_entry = llvm
                .lines()
                .find(|line| line.starts_with("define ") && line.contains(" @wf_copy_run("))
                .expect("ordinary nonempty output copy entry")
                .to_owned();
            llvm = llvm.replace(
                &copy_entry,
                &copy_entry.replace("@wf_copy_run(", "@wf_scatter_original_copy("),
            );
            // Count completed element copies on a thread other than the
            // packing caller; a stolen empty task is insufficient evidence.
            llvm.push_str(
                "\ndeclare void @wf_scatter_copy_done(i64)\n\
define i64 @wf_copy_run(ptr %values, ptr %output, i64 %output.len) {\n\
  %n = call i64 @wf_scatter_original_copy(ptr %values, ptr %output, i64 %output.len)\n\
  call void @wf_scatter_copy_done(i64 %n)\n\
  ret i64 %n\n}\n",
            );
            vec![
                "WFB_ORACLE_PARALLEL=1".to_owned(),
                "WF_TEST_SCHEDULE_MANUAL=1".to_owned(),
            ]
        } else {
            Vec::new()
        };
        let directory = test_directory();
        let executable = build_compute_oracle(&llvm, &oracle, &defines, &directory);
        run_compute_oracle(&executable, "radix_scatter", parallel);
        std::fs::remove_dir_all(directory).expect("remove scatter oracle test files");
    }
}

#[test]
fn stencil_matches_an_independent_dimension_and_step_matrix() {
    let source = include_bytes!("../../../../tests/programs/compute/stencil.wf");
    let adapter = include_str!("../../../../tests/programs/compute/stencil_host.ll");
    let oracle = format!(
        "#define WFB_STENCIL_ORACLE\n{}",
        include_str!("../../../../tests/programs/compute/stencil_oracle.c")
    );
    for emitted in [compile(source), emit_with_overlap(source)] {
        let llvm = bind_compute_host_adapter(&emitted, adapter)
            .replace("@main(", "@wf_stencil_smoke_main(")
            .replace("@wf__main_body(", "@wf_stencil_smoke_body(");
        let directory = test_directory();
        let defines = if emitted.contains("call void @wf__par_publish(") {
            vec!["WFB_ORACLE_PARALLEL=1".to_owned()]
        } else {
            Vec::new()
        };
        let executable = build_compute_oracle(&llvm, &oracle, &defines, &directory);
        run_compute_oracle(&executable, "stencil", !defines.is_empty());
        std::fs::remove_dir_all(directory).expect("remove native stencil test files");
    }
}

// These receivers check a private condition as well as whole program results:
// a real non-offering worker must enter a published task before its join. The
// independent C oracle and WF inputs have formal shared homes, also consumed
// by the separate performance runner without this scheduling observation.
fn check_formal_compute_matrix(name: &str, source: &[u8], adapter: &str, oracle: &str) {
    let oracle = oracle.replace(
        "#include \"oracle.h\"",
        include_str!("../../../../tests/programs/compute/oracle.h"),
    );
    for overlap in [
        OverlapLowering::Off,
        OverlapLowering::OnWithoutSmallScalarLeaves {
            maximum_operations: 16,
        },
    ] {
        let emitted = emit_lowered(source, overlap);
        let llvm = bind_compute_host_adapter(&emitted, adapter)
            .replace("@main(", "@wf_compute_smoke_main(")
            .replace("@wf__main_body(", "@wf_compute_smoke_body(");
        let parallel = !matches!(overlap, OverlapLowering::Off);
        let defines = if parallel {
            vec!["WFB_ORACLE_PARALLEL=1".to_owned()]
        } else {
            Vec::new()
        };
        let directory = test_directory();
        let executable = build_compute_oracle(&llvm, &oracle, &defines, &directory);
        run_compute_oracle(&executable, name, parallel);
        std::fs::remove_dir_all(directory).expect("remove compute oracle image");
    }
}

#[test]
fn formal_compute_mandelbrot_shapes_preserve_all_escape_counts_on_a_worker() {
    check_formal_compute_matrix(
        "mandelbrot",
        include_bytes!("../../../../tests/programs/compute/mandelbrot.wf"),
        include_str!("../../../../tests/programs/compute/mandelbrot_host.ll"),
        include_str!("../../../../tests/programs/compute/mandelbrot_oracle.c"),
    );
}

#[test]
fn formal_compute_records_preserve_utf8_and_range_results_on_a_worker() {
    check_formal_compute_matrix(
        "records",
        include_bytes!("../../../../tests/programs/compute/records.wf"),
        include_str!("../../../../tests/programs/compute/records_host.ll"),
        include_str!("../../../../tests/programs/compute/records_oracle.c"),
    );
}

#[test]
fn formal_compute_fir_preserves_ordered_rounding_and_inputs_on_a_worker() {
    check_formal_compute_matrix(
        "fir",
        include_bytes!("../../../../tests/programs/compute/fir.wf"),
        include_str!("../../../../tests/programs/compute/fir_host.ll"),
        include_str!("../../../../tests/programs/compute/fir_oracle.c"),
    );
}

#[test]
fn formal_compute_quadrature_matches_postorder_and_analytic_oracles_on_a_worker() {
    check_formal_compute_matrix(
        "quadrature",
        include_bytes!("../../../../tests/programs/compute/quadrature.wf"),
        include_str!("../../../../tests/programs/compute/quadrature_host.ll"),
        include_str!("../../../../tests/programs/compute/quadrature_oracle.c"),
    );
}

/// `range_split.wf` binds its two child ranges before passing them. The same
/// program with the child ranges formed at the two calls names the same
/// storage [REF-4], so [PAR-1] permits the same adjacent recursive calls and
/// each spelling hands the recursive call itself out. Its loop splits publish
/// in either spelling, so the recursive call's own thunk is what separates a
/// handed-out recursion from a denied one. Both spellings then restore the
/// parent access with no pool and with four workers.
#[test]
fn recursive_child_ranges_restore_parent_access() {
    let bound = include_str!("../../../../tests/programs/compute/range_split.wf");
    let inline = bound.replace(
        "  let left = &deref(output)[0_u64..middle];\n  \
         let right = &deref(output)[middle..count];\n  \
         let a = fill_recursive(output: left, depth: remaining);\n  \
         let b = fill_recursive(output: right, depth: remaining);\n",
        "  let a = fill_recursive(output: &deref(output)[0_u64..middle], depth: remaining);\n  \
         let b = fill_recursive(output: &deref(output)[middle..count], depth: remaining);\n",
    );
    assert_ne!(inline, bound, "range_split.wf binds its child ranges first");
    let output = compile_and_run(&compile(bound.as_bytes()));
    assert!(output.status.success(), "{output:?}");
    for (spelling, source) in [("bound", bound), ("inline", inline.as_str())] {
        let parallel = emit_with_overlap(source.as_bytes());
        let recursion = super::parallel::function_body(&parallel, "@wf__par_budget_fill_recursive");
        assert!(
            recursion.contains(", ptr @wf__par_thunk_fill_recursive."),
            "{spelling}: the recursive call must be handed out:\n{recursion}"
        );
        let directory = test_directory();
        let executable = build_executable(&parallel, &directory);
        for workers in ["1", "4"] {
            let output = Command::new(&executable)
                .env("WF_WORKERS", workers)
                .output()
                .expect("run the recursive range split");
            assert!(
                output.status.success(),
                "{spelling} WF_WORKERS={workers}: {output:?}"
            );
        }
        std::fs::remove_dir_all(directory).expect("remove the recursive range split image");
    }
}

/// A write through a range reference reaches the original local storage and
/// the original field storage, whether the caller writes the element itself
/// or a callee writes it through its declared row [REF-4, CALL-3, EFF-5].
#[test]
fn exclusive_range_references_write_original_local_and_field_storage() {
    let source = br#"struct Packet {
  before: u64;
  bytes: Array<u64, 4>;
  after: u64;
}

fn overwrite(view: &[u64], index: u64, value: u64) -> result: unit writes(view) contract {
  requires index < deref(view).len;
} {
  set deref(view)[index] = value;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 7_u64);
  let before0 = values[0_u64];
  let before2 = values[2_u64];
  let view = &values[0_u64..4_u64];
  set deref(view)[0_u64] = 19_u64;
  let done = overwrite(view: view, index: 2_u64, value: 31_u64);
  if values[0_u64] != 19_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if values[1_u64] != 7_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if values[2_u64] != 31_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if values[3_u64] != 7_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if before0 != 7_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  if before2 != 7_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  let bytes = array_filled::<u64, 4>(value: 13_u64);
  let packet = Packet(before: 53_u64, bytes: bytes, after: 59_u64);
  let field = &packet.bytes[0_u64..4_u64];
  let written = overwrite(view: field, index: 1_u64, value: 41_u64);
  if packet.before != 53_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  if packet.after != 59_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  if packet.bytes[0_u64] != 13_u64 {
    return std::process::exit_status(code: 9_u8);
  }
  if packet.bytes[1_u64] != 41_u64 {
    return std::process::exit_status(code: 10_u8);
  }
  if packet.bytes[2_u64] != 13_u64 {
    return std::process::exit_status(code: 11_u8);
  }
  if packet.bytes[3_u64] != 13_u64 {
    return std::process::exit_status(code: 12_u8);
  }
  let shared = &packet.bytes[0_u64..4_u64];
  let seen = deref(shared)[1_u64];
  if seen != 41_u64 {
    return std::process::exit_status(code: 13_u8);
  }
  let empty = array_filled::<u64, 0>(value: 0_u64);
  let nothing = &empty[0_u64..0_u64];
  let length = deref(nothing).len;
  if length != 0_u64 {
    return std::process::exit_status(code: 14_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let module = emit_lowered(source, overlap);
        for module in [&module, &super::owned_places::retain_calls(&module)] {
            let output = compile_and_run(module);
            assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}

const RANGE_ALIAS_FACTS: &str = r#"fn add_into(dst: &[u32], src: &[u32]) -> result: unit reads(src), writes(dst) contract {
  requires deref(dst).len <= deref(src).len;
} {
  let n = deref(dst).len;
  for (i in 0_u64..n) {
    let a = deref(dst)[i];
    let b = deref(src)[i];
    set deref(dst)[i] = a +wrap b;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let x = array_filled::<u32, 16>(value: 1_u32);
  let y = array_filled::<u32, 16>(value: 2_u32);
  add_into(dst: &x[0_u64..16_u64], src: &y[0_u64..16_u64]);
  set x[9_u64] = 5_u32;
  add_into(dst: &x[0_u64..8_u64], src: &x[8_u64..16_u64]);
  if x[0_u64] != 6_u32 {
    return std::process::exit_status(code: 1_u8);
  }
  if x[1_u64] != 8_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  if x[9_u64] != 5_u32 {
    return std::process::exit_status(code: 3_u8);
  }
  if y[15_u64] != 2_u32 {
    return std::process::exit_status(code: 4_u8);
  }
  let first = box_new::<u64>(value: 1_u64);
  let second = box_new::<u64>(value: 2_u64);
  swap(first: &first, second: &second);
  if first.inner != 2_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// A range-reference parameter carries the reference facts on the element
/// pointer it crosses the call as (compiler/backend-facts). [EFF-5] rejects
/// every call whose written range may overlap another argument's path, the
/// fact LLVM's `noalias` needs, so the host vectorizer needs no runtime
/// overlap check; two disjoint ranges of one array remain an admitted call,
/// and `swap` [OP-11] keeps its exception.
#[test]
fn range_reference_parameters_state_the_call_site_disjointness_fact() {
    let overlapping = RANGE_ALIAS_FACTS.replace("src: &x[8_u64..16_u64]", "src: &x[4_u64..12_u64]");
    assert_eq!(
        compile_rejection(overlapping.as_bytes()).rule_id(),
        Some("EFF-5"),
        "the fact rests on the call-site disjointness check"
    );

    let llvm = compile(RANGE_ALIAS_FACTS.as_bytes());
    let add_into = emitted_function(&llvm, "add_into");
    let header = add_into.lines().next().expect("add_into signature");
    let no_capture = env!("WHITEFOOT_NO_CAPTURE_ATTRIBUTE");
    // The written and the read-only range alike: `noalias` constrains only
    // memory the call modifies. A range's extent is its runtime `len`, which
    // may be zero, so no `dereferenceable` extent is stated.
    assert!(
        header.starts_with(&format!(
            "define i8 @wf_add_into(ptr noalias nonnull {no_capture} %wf.arg.v0.data, \
             i64 %wf.arg.v0.len, ptr noalias nonnull {no_capture} %wf.arg.v1.data, \
             i64 %wf.arg.v1.len)"
        )),
        "{header}"
    );
    assert!(!header.contains("dereferenceable"), "{header}");
    assert!(
        add_into.contains("%v0 = insertvalue { ptr, i64 } %v0.data, i64 %wf.arg.v0.len, 1"),
        "the body reassembles its ordinary range pair: {add_into}"
    );
    let main = emitted_function(&llvm, "main");
    assert_eq!(main.matches("call i8 @wf_add_into(ptr %").count(), 2);
    let swap = emitted_prelude_row(&llvm, "swap");
    let swap_header = swap.lines().next().expect("swap signature");
    assert!(!swap_header.contains("noalias"), "{swap_header}");
    assert_eq!(swap_header.matches(" nonnull ").count(), 2, "{swap_header}");

    let optimized = host_optimized_module(&llvm);
    let start = optimized
        .lines()
        .position(|line| line.starts_with("define ") && line.contains(" @wf_add_into("))
        .expect("optimized add_into definition");
    let body = optimized
        .lines()
        .skip(start)
        .take_while(|line| *line != "}")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.contains("vector.body") && !body.contains("vector.memcheck"),
        "the host vectorizer runs without a runtime overlap check: {body}"
    );

    for module in [&llvm, &super::owned_places::retain_calls(&llvm)] {
        let output = compile_and_run(module);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// [REF-4, TYPE-9] carries the complete stored element type through a range:
/// the descriptor ABI stays `{ptr, len}` while element addressing uses the
/// nominal's full layout, including an owned Box nested inside it.
#[test]
fn composite_range_elements_keep_nested_box_storage_and_descriptor_abi() {
    let source = br#"struct Record {
  cell: Box<u64>;
  marker: u64;
}

fn rewrite(records: &[Record]) -> previous: u64 writes(records) contract {
  requires 1_u64 <= deref(records).len;
} {
  let old = deref(records)[0_u64].cell.inner;
  set deref(records)[0_u64].cell.inner = 99_u64;
  set deref(records)[0_u64].marker = 23_u64;
  return old;
}

fn main() -> status: std::process::ExitStatus pure {
  let records = slots_new::<Record, 1>();
  let cell = box_new::<u64>(value: 41_u64);
  let record = Record(cell: move cell, marker: 17_u64);
  place_back(window: &records, value: move record);
  let part = &records[0_u64..1_u64];
  let previous = rewrite(records: part);
  if previous != 41_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if records[0_u64].cell.inner != 99_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if records[0_u64].marker != 23_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let rewrite = emitted_function(&llvm, "rewrite");
    assert!(
        rewrite.contains("extractvalue { ptr, i64 }") && rewrite.contains("getelementptr inbounds"),
        "the range keeps its two-word ABI and addresses the full Record layout: {rewrite}"
    );
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// [REF-4, TYPE-5, OP-4] keeps the complete element type after selecting an
/// element of a range. A second subscript therefore addresses the selected
/// inner Array for reads, writes and references; it does not flatten the two
/// logical indices or address the corresponding column of another row.
#[test]
fn nested_range_elements_read_write_and_borrow_the_selected_inner_array() {
    let source = br#"fn touch(rows: &[Array<u64, 2>], outer: u64, inner: u64, value: u64) -> result: u64 writes(rows) contract {
  requires outer < deref(rows).len;
  requires inner < 2_u64;
} {
  let before = deref(rows)[outer][inner];
  set deref(rows)[outer][inner] = value;
  let cell = &deref(rows)[outer][inner];
  let after = deref(cell);
  let scaled = before *wrap 100_u64;
  return scaled +wrap after;
}

fn nested_range_checksum() -> result: u64 pure {
  let seed = array_filled::<u64, 2>(value: 0_u64);
  let rows = array_filled::<Array<u64, 2>, 2>(value: seed);
  set rows[0_u64][0_u64] = 11_u64;
  set rows[0_u64][1_u64] = 13_u64;
  set rows[1_u64][0_u64] = 17_u64;
  set rows[1_u64][1_u64] = 19_u64;
  let part = &rows[0_u64..2_u64];
  let first = touch(rows: part, outer: 1_u64, inner: 0_u64, value: 71_u64);
  let second = touch(rows: part, outer: 0_u64, inner: 1_u64, value: 83_u64);
  let first_row_first = deref(part)[0_u64][0_u64] *wrap 100000000_u64;
  let first_row_second = deref(part)[0_u64][1_u64] *wrap 10000000000_u64;
  let second_row_first = deref(part)[1_u64][0_u64] *wrap 1000000000000_u64;
  let second_row_second = deref(part)[1_u64][1_u64] *wrap 100000000000000_u64;
  let second_observation = second *wrap 10000_u64;
  let checksum0 = first +wrap second_observation;
  let checksum1 = checksum0 +wrap first_row_first;
  let checksum2 = checksum1 +wrap first_row_second;
  let checksum3 = checksum2 +wrap second_row_first;
  let checksum4 = checksum3 +wrap second_row_second;
  return checksum4;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source)
        .replace("@main(", "@wf_nested_range_main(")
        .replace("@wf__main_body(", "@wf_nested_range_body(");
    let oracle = r#"#include <stdint.h>
#include <stdio.h>
extern int wf__floor_run(int, char **);
extern uint64_t wf_nested_range_checksum(void);
int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    uint64_t rows[2][2] = {{11, 13}, {17, 19}};
    uint64_t first = rows[1][0] * 100 + 71;
    rows[1][0] = 71;
    uint64_t second = rows[0][1] * 100 + 83;
    rows[0][1] = 83;
    uint64_t expected = first + second * UINT64_C(10000)
        + rows[0][0] * UINT64_C(100000000)
        + rows[0][1] * UINT64_C(10000000000)
        + rows[1][0] * UINT64_C(1000000000000)
        + rows[1][1] * UINT64_C(100000000000000);
    uint64_t actual = wf_nested_range_checksum();
    if (actual != expected) {
        (void)fprintf(stderr, "nested range: expected=%llu actual=%llu\n",
                      (unsigned long long)expected, (unsigned long long)actual);
        return 1;
    }
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
"#;
    let directory = test_directory();
    let executable = build_linked_executable(&llvm, Some(oracle), &[], &directory);
    let output = Command::new(executable)
        .output()
        .expect("run nested range element oracle");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    std::fs::remove_dir_all(directory).expect("remove nested range element oracle files");
}

/// [REF-1, REF-2] a loop header carries the runtime reference value selected
/// by either its zero-trip entry or its last executed backedge. The scalar
/// case observes the thin address inside and after the loop. The range case
/// also observes the two-word pointer/count descriptor after each rebinding.
/// A C model computes both answers from the source arrays rather than copying
/// a Whitefoot checksum literal.
#[test]
fn loop_carried_references_execute_zero_trip_and_backedge_values() {
    let source = br#"fn carried_cell(count: u64) -> result: u64 pure contract {
  requires count <= 2_u64;
} {
  let values = array_filled::<u64, 3>(value: 0_u64);
  set values[0_u64] = 11_u64;
  set values[1_u64] = 22_u64;
  set values[2_u64] = 33_u64;
  let selected = &values[0_u64];
  let observed = 0_u64;
  for (i in 0_u64..count) {
    let current = deref(selected);
    set observed = observed +wrap current;
    let next = i + 1_u64;
    set selected = &values[next];
  }
  let final_value = deref(selected);
  let scaled = observed *wrap 100_u64;
  return scaled +wrap final_value;
}

fn carried_range(count: u64) -> result: u64 pure contract {
  requires count <= 2_u64;
} {
  let values = array_filled::<u64, 4>(value: 0_u64);
  set values[0_u64] = 11_u64;
  set values[1_u64] = 23_u64;
  set values[2_u64] = 37_u64;
  set values[3_u64] = 53_u64;
  let selected = &values[0_u64..2_u64];
  let observed = 0_u64;
  for (i in 0_u64..count) {
    let available = 0_u64 < deref(selected).len;
    if available {
      let current = deref(selected)[0_u64];
      set observed = observed +wrap current;
    } else {
      return 1_u64;
    }
    let next = i + 1_u64;
    let end = next + 2_u64;
    set selected = &values[next..end];
  }
  let width = deref(selected).len;
  let has_first = 0_u64 < width;
  if has_first {
    let first = deref(selected)[0_u64];
    let has_second = 1_u64 < width;
    if has_second {
      let second = deref(selected)[1_u64];
      let observed_part = observed *wrap 1000000000_u64;
      let first_part = first *wrap 1000000_u64;
      let second_part = second *wrap 1000_u64;
      let first_sum = observed_part +wrap first_part;
      let second_sum = first_sum +wrap second_part;
      return second_sum +wrap width;
    }
  }
  return 2_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source)
        .replace("@main(", "@wf_loop_carried_main(")
        .replace("@wf__main_body(", "@wf_loop_carried_body(");
    let oracle = r#"#include <stdint.h>
#include <stdio.h>
extern int wf__floor_run(int, char **);
extern uint64_t wf_carried_cell(uint64_t);
extern uint64_t wf_carried_range(uint64_t);

static uint64_t expected_cell(uint64_t count) {
    const uint64_t values[3] = {11, 22, 33};
    uint64_t selected = 0;
    uint64_t observed = 0;
    for (uint64_t i = 0; i < count; ++i) {
        observed += values[selected];
        selected = i + 1;
    }
    return observed * 100 + values[selected];
}

static uint64_t expected_range(uint64_t count) {
    const uint64_t values[4] = {11, 23, 37, 53};
    uint64_t start = 0;
    uint64_t observed = 0;
    for (uint64_t i = 0; i < count; ++i) {
        observed += values[start];
        start = i + 1;
    }
    return observed * UINT64_C(1000000000)
        + values[start] * UINT64_C(1000000)
        + values[start + 1] * UINT64_C(1000)
        + UINT64_C(2);
}

int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    const uint64_t counts[2] = {0, 2};
    for (uint64_t i = 0; i < 2; ++i) {
        uint64_t count = counts[i];
        uint64_t expected = expected_cell(count);
        uint64_t actual = wf_carried_cell(count);
        if (actual != expected) {
            (void)fprintf(stderr,
                          "carried cell count=%llu expected=%llu actual=%llu\n",
                          (unsigned long long)count,
                          (unsigned long long)expected,
                          (unsigned long long)actual);
            return 1;
        }
        expected = expected_range(count);
        actual = wf_carried_range(count);
        if (actual != expected) {
            (void)fprintf(stderr,
                          "carried range count=%llu expected=%llu actual=%llu\n",
                          (unsigned long long)count,
                          (unsigned long long)expected,
                          (unsigned long long)actual);
            return 2;
        }
    }
    return 0;
}

int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
"#;
    let directory = test_directory();
    let executable = build_linked_executable(&llvm, Some(oracle), &[], &directory);
    let output = Command::new(executable)
        .output()
        .expect("run loop-carried reference oracle");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    std::fs::remove_dir_all(directory).expect("remove loop-carried reference oracle files");
}

/// [ENT-2, OP-4, OP-15] lowers the measure of a window selected directly
/// through a range reference. The inner window contains one value, so the
/// process observes the descriptor read rather than merely compiling an
/// unused measure expression.
#[test]
fn a_measured_range_element_has_its_observable_inner_length() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let inner = slots_new::<u64, 2>();
  place_back(window: &inner, value: 41_u64);
  let outer = slots_new::<Slots<u64, 2>, 1>();
  place_back(window: &outer, value: move inner);
  let items = &outer[0_u64..1_u64];
  let observed = deref(items)[0_u64].len;
  if observed != 1_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// [REF-1, REF-4, ENT-2, OP-4, OP-15] a value-if may join range references
/// to different storage roots. The joined value keeps its pointer-and-count
/// representation, and a nested measured element is addressed through the
/// target selected at runtime. Calling both ways makes the driver observe
/// both possible targets rather than accepting an unused semantic join.
#[test]
fn joined_range_element_measures_select_each_runtime_target() {
    let source = br#"fn observe(flag: Bool, expected: u64) -> result: u8 pure {
  let left_row = slots_new::<u64, 3>();
  place_back(window: &left_row, value: 11_u64);
  let right_row = slots_new::<u64, 3>();
  place_back(window: &right_row, value: 21_u64);
  place_back(window: &right_row, value: 22_u64);
  let left = slots_new::<Slots<u64, 3>, 1>();
  place_back(window: &left, value: move left_row);
  let right = slots_new::<Slots<u64, 3>, 1>();
  place_back(window: &right, value: move right_row);
  let items = if flag {
    give &left[0_u64..1_u64];
  } else {
    give &right[0_u64..1_u64];
  }
  if 0_u64 < deref(items).len {
    let observed = deref(items)[0_u64].len;
    if observed == expected {
      return 0_u8;
    }
  }
  return 1_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let selected_left = 0_u64 == 0_u64;
  let left_status = observe(flag: selected_left, expected: 1_u64);
  if left_status != 0_u8 {
    return std::process::exit_status(code: 1_u8);
  }
  let selected_right = 0_u64 != 0_u64;
  let right_status = observe(flag: selected_right, expected: 2_u64);
  if right_status != 0_u8 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// One `&[u8]` consumer reads the three storage origins [STOR-1]: a `const`
/// item's read-only static storage, a frame-resident constant-capacity window,
/// and a runtime-capacity window inside its own cell. The range reference is
/// the same kind over all three [REF-4], and only the cell is freed.
#[test]
fn const_local_and_heap_run_ranges_share_one_read_only_path() {
    let source = br#"const bytes: Array<u8, 4> =[1_u8, 2_u8, 3_u8, 4_u8];

fn sum(values: &[u8]) -> result: u64 reads(values) {
  let total = 0_u64;
  let length = deref(values).len;
  for (offset in 0_u64..length) {
    let byte = deref(values)[offset];
    let word = cvt::<u8, u64>(byte);
    set total = total +wrap word;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  let code = 0_u8;
  let constant = &bytes[0_u64..4_u64];
  let constant_total = sum(values: constant);
  if constant_total != 10_u64 {
    set code = 1_u8;
  }
  let local = slots_new::<u8, 4>();
  for @fill_local (
    at in 0_u64..4_u64,
    invariant grown: local.len >= at,
    invariant spare: local.cap + at >= local.len + 4_u64
  ) {
    place_back(window: &local, value: 3_u8);
  }
  let local_window = &local[0_u64..4_u64];
  let local_total = sum(values: local_window);
  if local_total != 12_u64 {
    set code = 2_u8;
  }
  let cell = box_slots_new::<u8>(capacity: 4_u64);
  for @fill_runtime (
    at in 0_u64..4_u64,
    invariant grown: cell.inner.len >= at,
    invariant spare: cell.inner.cap + at >= cell.inner.len + 4_u64
  ) {
    place_back(window: &cell.inner, value: 2_u8);
  }
  let runtime_window = &cell.inner[0_u64..4_u64];
  let runtime_total = sum(values: runtime_window);
  if runtime_total != 8_u64 {
    set code = 3_u8;
  }
  return std::process::exit_status(code: code);
}
"#;
    let llvm = compile(source);
    let sum = emitted_function(&llvm, "sum");
    let main = emitted_function(&llvm, "main");
    // The counted range discharges the element read before lowering, so the
    // element address forms directly without a runtime bounds branch.
    assert!(sum.contains("getelementptr inbounds i8"));
    assert!(!sum.contains("call void @free"));
    // One release: exactly one of the three origins is a `Box` cell, and
    // [STOR-3] frees that one heap object at the end of `main` [STOR-8].
    assert_eq!(main.matches("call void @free").count(), 1);

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn an_out_of_bounds_range_reference_read_is_an_op4_compile_rejection() {
    // A range reference's one measure is `hi - lo` [REF-4], so the constant
    // offset is refutable at compile time and the program rejects with the
    // residual [OP-4, ENT-6] - the same residual the window origin gives.
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let bytes = slots_new::<u8, 2>();
  place_back(window: &bytes, value: 0_u8);
  place_back(window: &bytes, value: 0_u8);
  let window = &bytes[0_u64..2_u64];
  let value = deref(window)[2_u64];
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile_rejection(source);
    assert_eq!(failure.rule_id(), Some("OP-4"));
    assert!(failure.detail().contains("2_u64 < deref(window).len"));
}

/// A range reference over a frame-resident window reaches that window's own
/// slots, and it stays valid across the linked call that reads through it
/// [REF-4, REF-2].
///
/// The frame-slot planner and the emission that consumes the slot each decide
/// whether a window keeps its slots inline; a range reference the planner did
/// not see would take the address of a slot no entry reserved. The pin is the
/// whole path: the emitted reference is a `getelementptr` into the window's
/// own frame slot rather than a copy of a descriptor's pointer word, the
/// window is the `&[u8]` source operand of an ordinary linked `write_once`,
/// and the process publishes exactly the bytes the fill loop wrote.
#[test]
fn a_range_reference_over_a_frame_resident_window_reaches_its_own_slots() {
    let source = br#"fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  doc "Publishes a frame-resident window through a range reference held until the linked write returns.";
  let std::process::Inputs(args: unused_args, cwd: unused_cwd, stdout: out, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin) = move inputs;
  std::fs::close_directory(factory: &entry_factory, directory: move unused_cwd);
  let page = slots_new::<u8, 4>();
  for @fill (
    at in 0_u64..4_u64,
    invariant grown: page.len >= at,
    invariant spare: page.cap + at >= page.len + 4_u64
  ) {
    place_back(window: &page, value: 65_u8);
  }
  let window = &page[0_u64..4_u64];
  match std::io::write_once(factory: &entry_factory, output: &out, source: window, start: 0_u64, end: 4_u64) {
    Ok(value: written) => {
      if written != 4_u64 {
        return std::process::exit_status(code: 1_u8);
      }
    }
    Err(error: problem) => {
      return std::process::exit_status(code: 2_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let main = emitted_function(&llvm, "main");
    // A frame-resident `Slots<u8, 4>` is the header-first
    // `{ i64 len, [4 x i8] slots }`. Forming the range reads that same
    // aggregate's length field and takes the first inline slot address; the
    // two GEPs must therefore share one frame-slot base.
    let length_gep = main
        .lines()
        .find(|line| {
            line.contains("getelementptr inbounds { i64, [4 x i8] }, ptr %")
                && line.ends_with("i32 0, i32 0")
        })
        .expect("range formation must address the window length in its frame slot");
    let slots_gep = main
        .lines()
        .find(|line| {
            line.contains("getelementptr inbounds { i64, [4 x i8] }, ptr %")
                && line.ends_with("i64 0, i32 1, i64 0")
        })
        .expect("range formation must address the window's first inline slot");
    fn gep_base(line: &str) -> &str {
        line.split_once("ptr ")
            .and_then(|(_, suffix)| suffix.split_once(',').map(|(base, _)| base))
            .expect("window GEP must carry one base pointer")
    }
    assert_eq!(gep_base(length_gep), gep_base(slots_gep));
    fn result_name(line: &str) -> &str {
        line.trim_start()
            .split_once(" =")
            .map(|(name, _)| name)
            .expect("window GEP must define one SSA value")
    }
    assert!(main.contains(&format!("load i64, ptr {}", result_name(length_gep))));
    assert!(main.contains(&format!(
        "insertvalue {{ ptr, i64 }} zeroinitializer, ptr {}, 0",
        result_name(slots_gep)
    )));
    // Nothing is allocated or freed: a frame-resident window owns no heap
    // storage and a reference owns none at all [STOR-1].
    assert!(!main.contains("call void @free"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"AAAA");
    assert!(output.stderr.is_empty());
}

#[test]
fn a_returning_loop_with_no_break_has_a_valid_unreachable_continuation() {
    let source = br#"fn count_down(count: u64) -> result: u64 pure {
  let remaining = count;
  loop {
    if remaining == 0_u64 {
      return 7_u64;
    }
    set remaining = remaining - 1_u64;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = count_down(count: 17_u64);
  if value != 7_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for module in [compile(source), emit_with_overlap(source)] {
        let output = compile_and_run(&module);
        assert!(output.status.success(), "{output:?}");
    }
}

#[test]
fn irregular_compute_matches_independent_sort_and_graph_oracles() {
    let kernels: [(&str, &[u8], &str, &str, &str); 2] = [
        (
            "merge_sort",
            include_bytes!("../../../../tests/programs/compute/merge_sort.wf"),
            include_str!("../../../../tests/programs/compute/merge_sort_host.ll"),
            "WFB_SORT_ORACLE",
            include_str!("../../../../tests/programs/compute/merge_sort_oracle.c"),
        ),
        (
            "bfs",
            include_bytes!("../../../../tests/programs/compute/bfs.wf"),
            include_str!("../../../../tests/programs/compute/bfs_host.ll"),
            "WFB_BFS_ORACLE",
            include_str!("../../../../tests/programs/compute/bfs_oracle.c"),
        ),
    ];
    for (name, source, adapter, selection, harness) in kernels {
        let oracle = format!("#define {selection}\n{harness}");
        for emitted in [compile(source), emit_with_overlap(source)] {
            if name == "merge_sort" && emitted.contains("call void @wf__par_publish(") {
                for stage in ["_par_budget_sort_values", "_par_budget_merge_values"] {
                    assert!(
                        emitted_function(&emitted, stage).contains("call void @wf__par_publish("),
                        "{stage} must offer recursive work independently of initialization"
                    );
                }
            }
            let llvm = bind_compute_host_adapter(&emitted, adapter)
                .replace("@main(", "@wf_irregular_smoke_main(")
                .replace("@wf__main_body(", "@wf_irregular_smoke_body(");
            let directory = test_directory();
            let defines = if emitted.contains("call void @wf__par_publish(") {
                vec!["WFB_ORACLE_PARALLEL=1".to_owned()]
            } else {
                Vec::new()
            };
            let executable = build_compute_oracle(&llvm, &oracle, &defines, &directory);
            run_compute_oracle(&executable, name, !defines.is_empty());
            std::fs::remove_dir_all(directory).expect("remove irregular-compute test files");
        }
    }
}
