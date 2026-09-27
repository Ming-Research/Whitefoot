//! Implementation obligations beyond the source-level FN-10 corpus verdicts.

use super::system::with_mutated_ir_lowering;
use super::{
    build_executable, compile, compile_and_run, compile_rejection, emitted_body, emitted_function,
    test_directory,
};
use crate::{IrNominalKind, IrTerminator, IrType, OverlapLowering, emit_llvm};

#[test]
fn musttail_is_a_backedge_before_host_optimization_in_both_worlds() {
    let source = include_bytes!("../../../../tests/conformance/cases/fn10-pos-self-transfer.wf");
    assert_self_tail_lowering(source);
}

#[test]
fn unmarked_self_calls_share_the_guaranteed_transfer_lowering() {
    let source = include_str!("../../../../tests/conformance/cases/fn10-pos-self-transfer.wf")
        .replace("musttail ", "");
    assert_self_tail_lowering(source.as_bytes());
}

fn assert_self_tail_lowering(source: &[u8]) {
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let module = with_mutated_ir_lowering(source, overlap, |program| {
            let exchange = program
                .functions()
                .iter()
                .find(|function| function.name() == "exchange")
                .expect("exchange function");
            let IrTerminator::Jump { target: entry, .. } = exchange.blocks()[0].terminator() else {
                panic!("the prologue must enter the parameterized body");
            };
            let backedges = exchange
                .blocks()
                .iter()
                .skip(1)
                .filter_map(|block| match block.terminator() {
                    IrTerminator::Jump { target, drops, .. } if target == entry => Some(drops),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(backedges.len(), 1);
            assert_eq!(backedges[0].iter().filter(|drop| matches!(drop.ty(), IrType::Nominal(id) if matches!(program.nominal(id).expect("drop type").kind(), IrNominalKind::Box { .. }))).count(), 1, "the spare cell must be released exactly once on the tail edge");
            let mut module = emit_llvm(program)
                .expect("tail transfer emits")
                .into_string();
            module.push_str(
                &crate::driver::launcher::render(program, "main").expect("ordinary test launcher"),
            );
            module
        });
        let mut names = vec!["sum", "exchange", "swap_pairs", "accumulate"];
        if overlap == OverlapLowering::On {
            // Only functions that can reach an offer need a sequential clone.
            // Accumulate offers ordinary calls before its self-tail edge.
            names.push("_par_seq_accumulate");
        }
        for name in names {
            // A register-returned result's transfer lives in the internal
            // destination-form body behind its public entry
            // (compiler/src/backend/abi.rs). Either definition names itself
            // once, in its header, and calls neither symbol.
            let body = emitted_body(&module, name);
            assert_eq!(
                body.matches(&format!("@wf_{name}(")).count()
                    + body.matches(&format!("@wf_{name}.body(")).count(),
                1,
                "self calls must already be jumps: {body}"
            );
            assert!(
                body.contains("phi "),
                "entry parameters must carry the new activation: {body}"
            );
            let (_, repeated) = body.split_once("bb1:").expect("parameterized body entry");
            assert!(
                !repeated.contains("alloca "),
                "the repeated body must reuse its physical frame: {body}"
            );
        }
        // The two-word Pair returns in registers (compiler/src/backend/abi.rs):
        // the public entry returns the value its internal body constructed
        // through the entry's slot. The transfer, checked above, rebinds the
        // body's parameters and keeps that one destination.
        let swap_pairs = emitted_function(&module, "swap_pairs");
        assert!(
            swap_pairs.starts_with("define %wf.t"),
            "the Pair result returns in registers: {swap_pairs}"
        );
        let header = swap_pairs.lines().next().expect("definition header");
        assert!(!header.contains("%wf.result"), "{header}");
        assert!(
            swap_pairs.contains("  call void @wf_swap_pairs.body(ptr %wf.result, "),
            "{swap_pairs}"
        );
        assert_eq!(swap_pairs.matches("\n  ret ").count(), 1, "{swap_pairs}");
        let swap_pairs_body = emitted_body(&module, "swap_pairs");
        assert!(
            swap_pairs_body
                .starts_with("define internal void @wf_swap_pairs.body(ptr %wf.result, "),
            "{swap_pairs_body}"
        );
        assert!(
            swap_pairs_body
                .lines()
                .filter(|line| line.starts_with("  ret "))
                .all(|line| line == "  ret void"),
            "{swap_pairs_body}"
        );
        // The conformance adapter executes the sequential module; this native
        // run additionally protects actualized parallel lowering.
        if overlap == OverlapLowering::On {
            assert!(
                module.contains("call void @wf__par_publish(ptr "),
                "the parallel control must actually offer work"
            );
            assert!(
                emitted_function(&module, "accumulate").contains("call void @wf__par_publish(ptr ")
            );
            let directory = test_directory();
            let executable = build_executable(&module, &directory);
            for workers in ["0", "2"] {
                let output = std::process::Command::new(&executable)
                    .env("WF_WORKERS", workers)
                    .output()
                    .expect("run the self-tail transfer in the selected world");
                assert!(output.status.success(), "WF_WORKERS={workers}: {output:?}");
                assert!(output.stdout.is_empty(), "{output:?}");
                assert!(output.stderr.is_empty(), "{output:?}");
            }
            std::fs::remove_dir_all(directory).expect("remove self-tail native artifacts");
        }
    }
}

#[test]
fn ineligible_unmarked_calls_keep_their_ordinary_lowering() {
    // These source pairs differ only in whether the writer requires a tail
    // transfer. Their unmarked forms must stay accepted without losing the
    // caller storage, cleanup continuation, or distinct callee they need.
    for (source, caller, callee) in [
        (
            include_str!("../../../../tests/conformance/cases/fn10-neg-local-reference.wf"),
            "walk",
            "walk",
        ),
        (
            include_str!(
                "../../../../tests/conformance/cases/fn10-neg-owned-parameter-reference.wf"
            ),
            "walk",
            "walk",
        ),
        (
            include_str!("../../../../tests/conformance/cases/fn10-neg-joined-reference.wf"),
            "walk",
            "walk",
        ),
        (
            include_str!("../../../../tests/conformance/cases/fn10-neg-pending-release.wf"),
            "walk",
            "walk",
        ),
        (
            include_str!("../../../../tests/conformance/cases/fn10-neg-not-return.wf"),
            "walk",
            "walk",
        ),
        (
            include_str!("../../../../tests/conformance/cases/fn10-neg-other-callee.wf"),
            "left",
            "right",
        ),
    ] {
        let source = source.replace("musttail ", "");
        for overlap in [OverlapLowering::Off, OverlapLowering::On] {
            with_mutated_ir_lowering(source.as_bytes(), overlap, |program| {
                let module = emit_llvm(program)
                    .expect("ordinary call emits")
                    .into_string();
                let body = emitted_function(&module, caller);
                assert!(
                    body.lines()
                        .any(|line| line.contains("call ")
                            && line.contains(&format!("@wf_{callee}("))),
                    "the ineligible transfer must remain a call: {body}"
                );
            });
        }
    }
}

#[test]
fn automatic_tail_selection_cannot_discharge_an_ordinary_requirement() {
    let source = include_str!("../../../../tests/conformance/cases/fn10-neg-requires.wf")
        .replace("musttail ", "");
    assert_eq!(compile_rejection(source.as_bytes()).rule_id(), Some("FN-8"));
}

#[test]
fn tail_selection_uses_the_settled_loop_reference_summary() {
    let marked = String::from_utf8(super::system::corpus_source(
        "fn10-pos-loop-summary-release",
    ))
    .expect("the conformance source is UTF-8");
    let unmarked = marked.replace("musttail ", "");
    for source in [marked, unmarked] {
        for overlap in [OverlapLowering::Off, OverlapLowering::On] {
            with_mutated_ir_lowering(source.as_bytes(), overlap, |program| {
                let module = emit_llvm(program)
                    .expect("settled tail transfer emits")
                    .into_string();
                let body = emitted_function(&module, "walk");
                assert_eq!(body.matches("@wf_walk(").count(), 1, "{body}");
                assert!(body.contains("phi "), "{body}");
            });
        }
    }
}

#[test]
fn one_function_can_mix_tail_transfers_with_calls_retaining_local_storage() {
    let module = compile(
        br#"fn walk(n: u64, value: &u64) -> result: u64 reads(value) {
  if n == 0_u64 {
    return deref(value);
  }
  let next = n - 1_u64;
  if n == 2_u64 {
    let local = deref(value) +wrap 1_u64;
    return walk(n: next, value: &local);
  }
  return walk(n: next, value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let value = 10_u64;
  let seen = walk(n: 100002_u64, value: &value);
  if seen != 11_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    let body = emitted_function(&module, "walk");
    assert_eq!(body.matches("@wf_walk(").count(), 2, "{body}");
    assert!(
        body.contains("phi "),
        "the safe edge must become a backedge"
    );
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}
