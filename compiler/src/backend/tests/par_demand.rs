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
    assert!(!small.contains("@wf__par_slice_") && !small.contains("@wf__par_demand_requested"));
    let dynamic = function_body(&module, "@wf_dynamic");
    assert!(dynamic.contains("call i64 @wf__par_slice_"), "{dynamic}");
    assert!(!dynamic.contains("@wf__par_split_budget"), "{dynamic}");
    assert!(module.contains("call i64 @wf__par_demand_requested()"));
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

/// A small range reaches the unsliced tail without a runtime call. A large
/// one must keep polling after progress, and only an observed request may
/// enter the out-of-line recursive driver. The old caller dispatched straight
/// to that driver on size alone, so it fails this check.
#[test]
fn demand_executes_local_slices_and_only_enters_the_driver_on_request() {
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
    let tail = block("par.tail.");
    let handoff = block("par.handoff.");
    assert!(
        head.contains("label %par.tail.") && head.contains("label %par.ask."),
        "{caller}"
    );
    assert!(ask.contains("@wf__par_demand_requested()"), "{caller}");
    assert!(
        ask.contains("label %par.handoff.") && ask.contains("label %par.slice."),
        "{caller}"
    );
    assert!(
        slice.contains("@wf__par_chunk_") && slice.contains("br label %par.head."),
        "{caller}"
    );
    assert!(
        tail.contains("@wf__par_chunk_") && tail.contains("br label %par.sliced."),
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
    for fast in [head, slice, tail] {
        assert!(
            !fast.contains("@wf__par_demand_requested") && !fast.contains("@wf__par_slice_"),
            "{caller}"
        );
    }
    // The step is a literal and the same as the worthwhile-span threshold:
    // no variable division and no 5,000-unit slices inside a 150,000-unit
    // hand-out grain. A wrong, shorter polling interval fails this equality.
    let literal = |text: &str, operation: &str| {
        text.lines()
            .find(|line| line.contains(operation))
            .and_then(|line| line.rsplit(", ").next())
            .and_then(|word| word.trim().parse::<u64>().ok())
            .unwrap_or_else(|| panic!("missing constant {operation}: {text}"))
    };
    let step = literal(slice, "add nuw i64");
    assert_eq!(step, literal(head, "icmp ult i64"));
    assert!(step >= 2);
    assert!(
        !caller.contains("udiv") && !caller.contains("select i1 %"),
        "{caller}"
    );
}

/// Empty/inverted ranges return the original seed without subtracting; a
/// nonempty range can end at MAX, and a partial fold must pass its live seed
/// and cursor to either the final tail or a hand-out. These are the facts
/// licensing nuw in the generated scheduling arithmetic, not source proofs.
#[test]
fn demand_range_edges_and_reduction_continuations_keep_their_values() {
    let module = emit_lowered(
        br#"fn range(seed: u64, lo: u64, hi: u64) -> result: u64 pure {
  let total = seed;
  for (i in lo..hi) { set total = total +wrap i; }
  return total;
}
"#,
        DEMAND,
    );
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
    for part in ["par.tail.", "par.handoff."] {
        assert!(
            block(part).contains(&format!("(i64 {seed}, i64 {cursor}, i64 %v2")),
            "{caller}"
        );
    }
    assert!(
        block("par.sliced.").contains("[ %v0, %par.empty."),
        "{caller}"
    );
}

#[test]
fn demand_chunks_expand_before_loop_optimization_but_ordinary_chunks_do_not() {
    // Nested loops exercise both captures and nested synthesized chunks. The
    // assertions inspect every generated chunk, not a workload's symbol.
    let source = br#"fn nested(seed: u64, n: u64) -> result: u64 pure {
  let total = seed;
  for (i in 0_u64..n) {
    let inner = i;
    for (j in 0_u64..n) { set inner = inner +wrap j; }
    set total = total +wrap inner;
  }
  return total;
}
"#;
    for (mode, expected) in [(DEMAND, true), (OverlapLowering::OnWithCallGrain, false)] {
        let module = emit_lowered(source, mode);
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
        .filter(|block| block.contains("@wf__par_demand_requested"))
        .collect();
    assert_eq!(asks.len(), 1, "{driver}");
    assert!(!asks[0].contains("icmp uge"), "{driver}");
    assert!(
        blocks
            .iter()
            .any(|block| block.contains("icmp uge") && !block.contains("@wf__par_demand_requested")),
        "{driver}"
    );
}

#[test]
fn demand_checks_groups_before_acquisition_and_keeps_the_budget_cut() {
    let source = include_bytes!("../../../../tests/programs/parallel/tree.wf");
    let demand = emit_lowered(source, DEMAND);
    assert!(demand.contains("par.demand.v"));
    assert!(demand.contains("phi ptr [ null, %par.demand.v"));
    assert!(demand.contains("call i64 @wf__par_demand_requested()"));
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
