//! Research option isolation and shape checks; execution belongs to the
//! maintained program tests, and baseline-revision byte comparison to CI.
use super::{emit_lowered, parallel::function_body};
use crate::{CompilerLimits, OverlapLowering, RecursionBudget, SourceInput};
const DEMAND: OverlapLowering = OverlapLowering::Demand {
    budget: RecursionBudget::RuntimeDerived,
    sequential_refusal: false,
};
const SMALL: &str = r#"fn small(seed: u64) -> result: u64 pure {
  let total = seed;
  for (i in 0_u64..3_u64) {
    set total = total +wrap i;
  }
  return total;
}

fn dynamic(seed: u64, n: u64) -> result: u64 pure {
  let total = seed;
  for (i in 0_u64..n) {
    set total = total +wrap i;
  }
  return total;
}
"#;
const RANGE: &str = r#"fn range(seed: u64, lo: u64, hi: u64) -> result: u64 pure {
  let total = seed;
  for (i in lo..hi) {
    set total = total +wrap i;
  }
  return total;
}
"#;
const NESTED: &str = r#"fn nested(seed: u64, n: u64) -> result: u64 pure {
  let total = seed;
  for (i in 0_u64..n) {
    let inner = i;
    for (j in 0_u64..n) {
      set inner = inner +wrap j;
    }
    set total = total +wrap inner;
  }
  return total;
}
"#;
/// The poll the demand mode emits: one monotonic load of this thread's word.
const POLL: &str = "load atomic i64, ptr @wf__par_demand_word monotonic, align 8";

#[test]
fn demand_slices_runtime_extents_and_prunes_constant_small_extents() {
    let (module, ledger) = crate::compile_with_permission_ledger(
        &[SourceInput::new("test.wf", SMALL.as_bytes())],
        CompilerLimits::default(),
        DEMAND,
    )
    .unwrap();
    let small = function_body(&module, "@wf_small");
    assert!(small.contains("call i64 @wf__par_chunk_"), "{small}");
    assert!(!small.contains("@wf__par_slice_") && !small.contains("@wf__par_demand_word"));
    let dynamic = function_body(&module, "@wf_dynamic");
    assert!(dynamic.contains("call i64 @wf__par_slice_"), "{dynamic}");
    assert!(!dynamic.contains("@wf__par_split_budget"), "{dynamic}");
    assert!(module.contains(POLL), "{module}");
    assert!(
        ledger
            .iter()
            .any(|line| line.contains("small") && line.ends_with("pruned")),
        "{ledger:?}"
    );
    assert!(
        ledger
            .iter()
            .any(|line| line.contains("dynamic") && line.ends_with("slice driver")),
        "{ledger:?}"
    );
}

/// The no-request path is one slice loop in the caller: every slice runs the
/// chunk from one call site over `[cursor, cursor + min(remaining, step))`,
/// so the inlined loop keeps the unknown trip count the sequential loop has.
/// A range below the step takes one slice without touching the word; only an
/// observed request enters the out-of-line recursive driver. The previous
/// candidate advanced by a literal step from one call site and ran the
/// remainder from a second (`par.tail.`): its constant-trip slice loop was
/// left rolled by LLVM, which this shape check rejects.
#[test]
fn demand_runs_one_variable_length_slice_loop_and_enters_the_driver_only_on_request() {
    let module = emit_lowered(SMALL.as_bytes(), DEMAND);
    let caller = function_body(&module, "@wf_dynamic");
    let blocks = llvm_blocks(caller);
    let block = |prefix: &str| {
        *blocks
            .iter()
            .find(|block| block.starts_with(prefix))
            .unwrap_or_else(|| panic!("missing {prefix}: {caller}"))
    };
    let head = block("par.head.");
    let ask = block("par.ask.");
    let slice = block("par.slice.");
    let handoff = block("par.handoff.");
    assert!(
        !blocks.iter().any(|block| block.starts_with("par.tail.")),
        "{caller}"
    );
    assert!(
        head.contains("label %par.ask.") && head.contains("label %par.slice."),
        "{caller}"
    );
    assert!(ask.contains(POLL) && !ask.contains("call "), "{caller}");
    assert!(
        ask.contains("label %par.handoff.") && ask.contains("label %par.slice."),
        "{caller}"
    );
    assert!(
        slice.contains("@llvm.umin.i64(")
            && slice.contains("@wf__par_chunk_")
            && slice.contains("label %par.head.")
            && slice.contains("label %par.sliced."),
        "{caller}"
    );
    assert_eq!(
        caller.matches("call i64 @wf__par_chunk_").count(),
        1,
        "{caller}"
    );
    assert_eq!(
        caller.matches("call i64 @wf__par_slice_").count(),
        1,
        "{caller}"
    );
    assert!(
        handoff.contains("@wf__par_slice_") && handoff.contains("noinline"),
        "{caller}"
    );
    for fast in [head, slice] {
        assert!(
            !fast.contains("@wf__par_demand_word") && !fast.contains("@wf__par_slice_"),
            "{caller}"
        );
    }
    assert!(!caller.contains("@wf__par_demand_requested"), "{caller}");
    // The slice end is cursor plus the minimum of the remaining range and the
    // step: never cursor plus a literal, which would give the inlined loop a
    // constant trip count. The step is the worthwhile-span threshold too.
    let add = slice
        .lines()
        .find(|line| line.contains("add nuw i64"))
        .unwrap_or_else(|| panic!("missing slice end: {caller}"));
    let addend = add.rsplit(", ").next().unwrap().trim();
    assert!(addend.starts_with('%'), "{caller}");
    let literal = |text: &str, operation: &str| {
        text.lines()
            .find(|line| line.contains(operation))
            .and_then(|line| line.split_whitespace().last())
            .and_then(|word| word.trim_end_matches(')').parse::<u64>().ok())
            .unwrap_or_else(|| panic!("missing constant {operation}: {text}"))
    };
    let step = literal(slice, "@llvm.umin.i64(");
    assert_eq!(step, literal(head, "icmp uge i64"));
    assert!(step >= 2);
    assert!(
        !caller.contains("udiv") && !caller.contains("select i1 %"),
        "{caller}"
    );
}

/// Empty/inverted ranges return the original seed without subtracting; a
/// nonempty range can end at MAX, and a partial fold must pass its live seed
/// and cursor to the next slice or a hand-out. These are the facts licensing
/// nuw in the generated scheduling arithmetic, not source proofs.
#[test]
fn demand_range_edges_and_reduction_continuations_keep_their_values() {
    let module = emit_lowered(RANGE.as_bytes(), DEMAND);
    let caller = function_body(&module, "@wf_range");
    let blocks = llvm_blocks(caller);
    let block = |prefix: &str| *blocks.iter().find(|b| b.starts_with(prefix)).unwrap();
    assert!(caller.contains("icmp ult i64 %v1, %v2"), "{caller}");
    assert!(
        caller.contains("label %par.start.") && caller.contains("label %par.empty."),
        "{caller}"
    );
    let empty = block("par.empty.");
    assert!(
        !empty.contains("call ") && !empty.contains("sub "),
        "{caller}"
    );
    let head = block("par.head.");
    let phis: Vec<_> = head
        .lines()
        .filter(|line| line.contains(" = phi "))
        .collect();
    assert_eq!(phis.len(), 2, "{caller}");
    let cursor = phis[0].trim().split(" = ").next().unwrap();
    let seed = phis[1].trim().split(" = ").next().unwrap();
    assert!(
        phis[0].contains("[ %v1, %par.start.") && phis[0].contains("%par.slice."),
        "{caller}"
    );
    assert!(
        phis[1].contains("[ %v0, %par.start.") && phis[1].contains("%par.slice."),
        "{caller}"
    );
    assert!(
        head.contains(&format!("sub nuw i64 %v2, {cursor}")),
        "{caller}"
    );
    let slice = block("par.slice.");
    let end = slice
        .lines()
        .find(|line| line.contains("add nuw i64"))
        .and_then(|line| line.trim().split(" = ").next())
        .unwrap_or_else(|| panic!("missing slice end: {caller}"));
    assert!(
        slice.contains(&format!("(i64 {seed}, i64 {cursor}, i64 {end}")),
        "{caller}"
    );
    assert!(
        block("par.handoff.").contains(&format!("(i64 {seed}, i64 {cursor}, i64 %v2")),
        "{caller}"
    );
    assert!(
        block("par.sliced.").contains("[ %v0, %par.empty."),
        "{caller}"
    );
}

#[test]
fn demand_chunks_expand_before_loop_optimization_but_ordinary_chunks_do_not() {
    // Nested loops exercise both captures and nested synthesized chunks. The
    // assertions inspect every generated chunk, not a workload's symbol.
    for (mode, expected) in [(DEMAND, true), (OverlapLowering::OnWithCallGrain, false)] {
        let module = emit_lowered(NESTED.as_bytes(), mode);
        let chunks: Vec<_> = module
            .lines()
            .filter(|line| line.starts_with("define ") && line.contains("par_chunk_"))
            .collect();
        assert!(chunks.len() >= 2, "{module}");
        for header in chunks {
            assert_eq!(header.contains("alwaysinline"), expected, "{header}");
        }
        for header in module.lines().filter(|line| {
            line.starts_with("define ")
                && (line.contains("par_slice_") || line.contains("@wf_nested("))
        }) {
            assert!(!header.contains("alwaysinline"), "{header}");
        }
    }
}

/// A poll is a load of the module's thread-local request word: no call, so
/// no saved registers around it and nothing for the no-request path to pay
/// beyond a load, a compare and a branch. The word is a weak zero definition
/// the runtime's strong one replaces, and nothing calls the C accessor.
#[test]
fn demand_polls_a_thread_local_word_and_never_calls_the_runtime_accessor() {
    for source in [
        SMALL.as_bytes(),
        include_bytes!("../../../../tests/programs/parallel/tree.wf").as_slice(),
    ] {
        let module = emit_lowered(source, DEMAND);
        assert!(
            module.contains("@wf__par_demand_word = weak thread_local global i64 0, align 8"),
            "{module}"
        );
        assert!(module.contains(POLL), "{module}");
        assert!(!module.contains("@wf__par_demand_requested"), "{module}");
    }
}

fn llvm_blocks(function: &str) -> Vec<&str> {
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in function.split_inclusive('\n') {
        if !line.starts_with(' ') && line.trim_end().ends_with(':') {
            starts.push(offset);
        }
        offset += line.len();
    }
    starts.push(function.len());
    starts
        .windows(2)
        .map(|pair| &function[pair[0]..pair[1]])
        .collect()
}

#[test]
fn a_slice_driver_reads_the_request_word_only_for_a_range_worth_handing_out() {
    let module = emit_lowered(SMALL.as_bytes(), DEMAND);
    let driver = module
        .split("\ndefine ")
        .find(|function| {
            function
                .lines()
                .next()
                .is_some_and(|header| header.contains("@wf__par_slice_dynamic"))
        })
        .unwrap_or_else(|| panic!("the module defines dynamic's slice driver: {module}"));
    // A block runs from its label to the next one. The block that reads the
    // request word must be one the minimum-span comparison already chose, so
    // a range too small to hand out never touches the word idle workers write.
    let mut blocks = vec![String::new()];
    for line in driver.lines() {
        if !line.starts_with(' ') && line.ends_with(':') {
            blocks.push(String::new());
        }
        blocks.last_mut().expect("a block").push_str(line);
        blocks.last_mut().expect("a block").push('\n');
    }
    let asks: Vec<&String> = blocks
        .iter()
        .filter(|block| block.contains("@wf__par_demand_word"))
        .collect();
    assert_eq!(asks.len(), 1, "{driver}");
    assert!(asks[0].contains(POLL), "{driver}");
    assert!(!asks[0].contains("icmp uge"), "{driver}");
    assert!(
        blocks
            .iter()
            .any(|block| block.contains("icmp uge") && !block.contains("@wf__par_demand_word")),
        "{driver}"
    );
}

#[test]
fn demand_checks_groups_before_acquisition_and_keeps_the_budget_cut() {
    let source = include_bytes!("../../../../tests/programs/parallel/tree.wf");
    let demand = emit_lowered(source, DEMAND);
    assert!(demand.contains("par.demand.v"));
    assert!(demand.contains("phi ptr [ null, %par.demand.v"));
    assert!(demand.contains(POLL));
    assert!(demand.contains("@wf__par_budget_"));
    assert!(demand.contains("@wf__par_seq_"));
}

#[test]
fn demand_keeps_indexed_reductions_on_the_legacy_splitter() {
    let source = include_bytes!("../../../../tests/programs/parallel/indexed_reductions.wf");
    let (module, ledger) = crate::compile_with_permission_ledger(
        &[SourceInput::new("test.wf", source)],
        CompilerLimits::default(),
        DEMAND,
    )
    .unwrap();
    assert!(module.contains("@wf__par_split_budget"));
    assert!(
        ledger
            .iter()
            .any(|line| line.ends_with("legacy splitter for indexed")),
        "{ledger:?}"
    );
}

#[test]
fn demand_compilation_leaves_ordinary_parallel_emission_byte_identical() {
    for source in [
        SMALL.as_bytes(),
        include_bytes!("../../../../tests/programs/parallel/tree.wf").as_slice(),
        include_bytes!("../../../../tests/programs/parallel/range_fold.wf").as_slice(),
    ] {
        let before = emit_lowered(source, OverlapLowering::OnWithCallGrain);
        let _candidate = emit_lowered(source, DEMAND);
        let after = emit_lowered(source, OverlapLowering::OnWithCallGrain);
        assert_eq!(before.as_bytes(), after.as_bytes());
        assert!(!after.contains("wf__par_demand") && !after.contains("wf__par_slice_"));
    }
}

#[test]
fn a_pruned_only_entry_does_not_name_an_unemitted_world() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let total = 0_u64;
  for (i in 0_u64..3_u64) {
    set total = total +wrap i;
  }
  if total == 3_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    let module = emit_lowered(source, DEMAND);
    assert!(module.contains("@wf__par_chunk_"));
    assert!(!module.contains("@wf__par_seq_main"), "{module}");
    assert!(!module.contains("@wf__par_pool_active"), "{module}");
    assert!(!module.contains("@wf__par_slice_"), "{module}");
    let directory = super::test_directory();
    let image = super::build_executable(&module, &directory);
    for workers in ["1", "4"] {
        let output = super::BoundedOutput::bounded_output(
            std::process::Command::new(&image).env("WF_WORKERS", workers),
        );
        assert!(output.expect("run pruned-only image").status.success());
    }
    std::fs::remove_dir_all(directory).unwrap();
}
