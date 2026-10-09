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
