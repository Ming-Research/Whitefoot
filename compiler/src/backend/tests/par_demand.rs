//! Research option isolation and shape checks; execution belongs to the
//! maintained program tests, and baseline-revision byte comparison to CI.
use super::{emit_lowered, parallel::function_body};
use crate::{CallGrain, CompilerLimits, OverlapLowering, RecursionBudget, SourceInput};
/// Every permitted offer stays, so these shape checks see each offer path.
const DEMAND: OverlapLowering = OverlapLowering::Demand {
    ablation: crate::DemandAblation::None,
    budget: RecursionBudget::RuntimeDerived,
    call_grain: CallGrain::Every,
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

const POLL: &str = "load atomic i64, ptr @wf__par_demand_word monotonic, align 8";

fn ablation(arm: crate::DemandAblation) -> OverlapLowering {
    OverlapLowering::Demand {
        ablation: arm,
        budget: RecursionBudget::RuntimeDerived,
        call_grain: CallGrain::Every,
        sequential_refusal: false,
    }
}

#[test]
fn default_demand_is_unchanged_after_each_ablation() {
    let before = emit_lowered(SMALL.as_bytes(), DEMAND);
    for arm in [crate::DemandAblation::Order, crate::DemandAblation::Seed, crate::DemandAblation::Extent] {
        let _ = emit_lowered(SMALL.as_bytes(), ablation(arm));
        assert_eq!(before, emit_lowered(SMALL.as_bytes(), ablation(Default::default())));
    }
}

#[test]
fn order_publishes_near_but_combines_near_then_far() {
    use crate::{IrInstruction, IrOperation, IrIntegerOperation};
    super::system::with_mutated_ir_lowering(SMALL.as_bytes(), ablation(crate::DemandAblation::Order), |program| {
        let driver = program.functions().iter().find(|f| f.name().starts_with("_par_slice_dynamic")).unwrap();
        let members = &driver.overlaps[0].members;
        let operation = |value| driver.blocks().iter().flat_map(|b| b.instructions()).find_map(|i| match i {
            IrInstruction::Define { result, operation, .. } if *result == value => Some(operation),
            _ => None,
        }).unwrap();
        let IrOperation::Call { arguments: near, .. } = operation(members[0]) else { panic!("near call") };
        let IrOperation::Call { arguments: far, .. } = operation(members[1]) else { panic!("far call") };
        assert_eq!(near[2], far[1]);
        assert_eq!(far[2], driver.parameters()[2].0);
        assert_ne!(near[0], far[0]);
        assert!(driver.blocks().iter().flat_map(|b| b.instructions()).any(|i| matches!(i,
            IrInstruction::Define { operation: IrOperation::Integer {
                operation: IrIntegerOperation::AddWrap, arguments, .. }, .. } if arguments == members)));
    });
}

#[test]
fn seed_has_an_entry_frontier_and_retains_the_request_refinement() {
    let module = emit_lowered(SMALL.as_bytes(), ablation(crate::DemandAblation::Seed));
    let caller = function_body(&module, "@wf_dynamic");
    assert!(caller.contains("call i64 @wf__par_demand_frontier()"), "{caller}");
    assert!(caller.contains("@llvm.umin.i64"), "{caller}");
    let driver = module.split("\ndefine ").find(|function| {
        function.lines().next().is_some_and(|header| header.contains("@wf__par_slice_dynamic"))
    }).expect("dynamic slice driver");
    assert!(driver.contains(POLL), "{driver}");
    assert_eq!(driver.matches("call void @wf__par_publish").count(), 2, "{driver}");
    assert!(driver.contains("udiv i64") && driver.contains("sub i64"), "{driver}");
}

#[test]
fn extent_prices_demand_with_the_available_runtime_work() {
    let source = include_bytes!("../../../../tests/programs/compute/stencil.wf");
    let plain = emit_lowered(source, DEMAND);
    let extent = emit_lowered(source, ablation(crate::DemandAblation::Extent));
    assert!(!plain.contains("udiv i64 149999,"));
    assert!(extent.contains("udiv i64 149999, %"), "{extent}");
    assert!(extent.contains("@llvm.umax.i64"));
    assert!(extent.contains(POLL));
}

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
    assert!(module.contains(POLL));
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
fn a_range_below_the_minimum_span_calls_the_chunk_without_entering_the_driver() {
    let module = emit_lowered(SMALL.as_bytes(), DEMAND);
    let caller = function_body(&module, "@wf_dynamic");
    let mut blocks = vec![String::new()];
    for line in caller.lines() {
        if !line.starts_with(' ') && line.ends_with(':') {
            blocks.push(String::new());
        }
        blocks.last_mut().expect("a block").push_str(line);
        blocks.last_mut().expect("a block").push('\n');
    }
    let small = blocks
        .iter()
        .find(|block| block.starts_with("par.small.v"))
        .unwrap_or_else(|| panic!("the caller tests the range first: {caller}"));
    assert!(small.contains("@wf__par_chunk_"), "{caller}");
    assert!(!small.contains("@wf__par_slice_"), "{caller}");
    let slice = blocks
        .iter()
        .find(|block| block.starts_with("par.slice.v"))
        .unwrap_or_else(|| panic!("a large range still enters the driver: {caller}"));
    assert!(slice.contains("@wf__par_slice_"), "{caller}");
    // The comparison's right operand is an integer literal: the site's
    // weight is static, so no division is left for run time.
    assert!(
        caller.lines().any(|line| {
            line.contains(" = icmp ult i64 ")
                && line
                    .rsplit(", ")
                    .next()
                    .is_some_and(|operand| operand.trim().parse::<u64>().is_ok())
        }),
        "the minimum span is a folded constant: {caller}"
    );
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

#[test]
fn demand_polls_a_thread_local_word_and_never_calls_the_runtime_accessor() {
    for source in [
        SMALL.as_bytes(),
        include_bytes!("../../../../tests/programs/parallel/tree.wf").as_slice(),
    ] {
        let module = emit_lowered(source, DEMAND);
        assert!(
            module.contains("@wf__par_demand_word = weak thread_local(initialexec) global i64 0, align 8"),
            "{module}"
        );
        assert!(module.contains(POLL), "{module}");
        assert!(!module.contains("@wf__par_demand_requested"), "{module}");
    }
}

const CHEAP_GROUP: &str = r#"fn work(n: u64, seed: u64) -> result: u64 pure {
  let i = 0_u64;
  let value = seed;
  loop @work {
    if i == n {
      break @work;
    }
    let rotated = irotl(value, 7_u32);
    set value = rotated +wrap i;
    set i = i +wrap 1_u64;
  }
  return value;
}

fn helper(n: u64, seed: u64) -> result: u64 pure {
  let next = seed +wrap 1_u64;
  let a = work(n: n, seed: seed);
  let b = work(n: n, seed: next);
  return ixor(a, b);
}
"#;

/// A group of two calls whose callee is acyclic and far below the work unit,
/// the shape of the par-demand experiment's `hot_helper`: under the `--par` grain,
/// demand's default, the group is pruned and its caller never polls, while
/// keeping every offer polls at the group. Honouring a request at such a
/// group handed out one call of about 15 ns per iteration on a native
/// 14900K, eleven to fifteen times slower than sequential.
#[test]
fn demand_takes_the_par_call_grain_so_a_cheap_group_never_polls() {
    let source = CHEAP_GROUP.as_bytes();
    let grained = emit_lowered(
        source,
        OverlapLowering::Demand {
            ablation: crate::DemandAblation::None,
            budget: RecursionBudget::RuntimeDerived,
            call_grain: CallGrain::WorkUnit,
            sequential_refusal: false,
        },
    );
    let helper = function_body(&grained, "@wf_helper");
    assert!(!helper.contains("@wf__par_demand_word"), "{helper}");
    assert!(!helper.contains("@wf__par_acquire_lane"), "{helper}");
    let every_offer = emit_lowered(source, DEMAND);
    let every = function_body(&every_offer, "@wf_helper");
    assert!(every.contains(POLL), "{every}");
}
