//! The block-parameter merge on the lowered IR of real programs.
//!
//! The merge removes block parameters whose every incoming edge carries one
//! value and rewrites each use of such a parameter. The hand-built cases of
//! `builder/parameters.rs` and the three-function fixture of
//! `nested_counted_breaks_keep_each_exit_interface_local_to_its_range` cannot
//! show the shapes real programs give the builder: matches nested in loops,
//! `break` and `continue` out of several levels, owned values moved through
//! joins and dropped on exits, and the chunk functions that loop splitting
//! synthesizes. These tests only lower, so they add no compile-and-run case.
//!
//! The reference is the IR's own contract, checked by code that does not use
//! the merge: a lowered function is in SSA form, so each use is dominated by
//! its definition; a `Jump` gives each parameter of its target one argument
//! of the parameter's type; a `Match` or `IntegerMatch` edge carries no
//! arguments, so its target has no parameters; and no parameter keeps being
//! fed one value (`redundant_block_parameters`).

use super::*;

/// Programs of `tests/programs` that, between them, hold the control-flow
/// shapes above: a header `match` interpreter, loops with `break` and
/// `continue`, early returns that drop owners, the container kernels, the
/// text and byte scanners and a program with data-parallel loops.
const PROGRAMS: &[(&str, &[u8])] = &[
    (
        "continue_interpreter",
        include_bytes!("../../../../tests/programs/continue_interpreter.wf"),
    ),
    (
        "adaptive_quadrature",
        include_bytes!("../../../../tests/programs/adaptive_quadrature.wf"),
    ),
    (
        "byte_string",
        include_bytes!("../../../../tests/programs/byte_string.wf"),
    ),
    (
        "utf8parse",
        include_bytes!("../../../../tests/programs/utf8parse.wf"),
    ),
    (
        "growable_vec",
        include_bytes!("../../../../tests/programs/growable_vec.wf"),
    ),
    (
        "heap_run",
        include_bytes!("../../../../tests/programs/heap_run.wf"),
    ),
    (
        "option_slots",
        include_bytes!("../../../../tests/programs/option_slots.wf"),
    ),
    (
        "cancellation",
        include_bytes!("../../../../tests/programs/cancellation.wf"),
    ),
    (
        "dir_walk",
        include_bytes!("../../../../tests/programs/dir_walk.wf"),
    ),
    (
        "wfgrep",
        include_bytes!("../../../../tests/programs/wfgrep.wf"),
    ),
    (
        "raw_deflate_dynamic",
        include_bytes!("../../../../tests/programs/raw_deflate_dynamic.wf"),
    ),
    (
        "hash-map-program",
        include_bytes!("../../../../tests/programs/containers/hash-map-program.wf"),
    ),
    (
        "ordered-map-cursor-program",
        include_bytes!("../../../../tests/programs/containers/ordered-map-cursor-program.wf"),
    ),
    (
        "priority-queue-program",
        include_bytes!("../../../../tests/programs/containers/priority-queue-program.wf"),
    ),
    (
        "bfs",
        include_bytes!("../../../../tests/programs/compute/bfs.wf"),
    ),
    (
        "radix_scatter",
        include_bytes!("../../../../tests/programs/compute/radix_scatter.wf"),
    ),
    (
        "histogram",
        include_bytes!("../../../../tests/programs/compute/histogram.wf"),
    ),
    (
        "indexed_calls",
        include_bytes!("../../../../tests/programs/parallel/indexed_calls.wf"),
    ),
];

/// Every `Jump` hands its target one argument of the parameter's type for
/// each parameter, and a block that no `Jump` reaches, or that a `Match` or
/// `IntegerMatch` also enters, has no parameter that could be left without a
/// value on some edge.
fn assert_parameter_interfaces(function: &IrFunction) {
    let blocks = function.blocks();
    let mut jumped = vec![false; blocks.len()];
    let mut matched = vec![false; blocks.len()];
    for (index, block) in blocks.iter().enumerate() {
        match block.terminator() {
            IrTerminator::Jump {
                target, arguments, ..
            } => {
                let target_block = &blocks[target.index()];
                assert_eq!(
                    arguments.len(),
                    target_block.parameters().len(),
                    "{}: block {index} jumps to block {} with the wrong number of arguments",
                    function.name(),
                    target.index()
                );
                for (argument, (parameter, ty)) in arguments.iter().zip(target_block.parameters())
                {
                    assert_eq!(
                        function.values[argument.index()],
                        *ty,
                        "{}: block {index} gives {parameter:?} of block {} a {argument:?} of another type",
                        function.name(),
                        target.index()
                    );
                }
                jumped[target.index()] = true;
            }
            IrTerminator::Match { targets, .. } => {
                for target in targets {
                    matched[target.block().index()] = true;
                }
            }
            IrTerminator::IntegerMatch {
                targets, default, ..
            } => {
                for block in targets
                    .iter()
                    .map(|target| target.block())
                    .chain(default.iter().copied())
                {
                    matched[block.index()] = true;
                }
            }
            IrTerminator::Return { .. } | IrTerminator::Unreachable => {}
        }
    }
    for (index, block) in blocks.iter().enumerate() {
        for (parameter, ty) in block.parameters() {
            assert_eq!(
                function.values[parameter.index()],
                *ty,
                "{}: block {index} declares {parameter:?} with another type than its value table",
                function.name()
            );
        }
        if matched[index] {
            assert!(
                block.parameters().is_empty(),
                "{}: block {index} is entered by a match edge, which carries no arguments, and has parameters",
                function.name()
            );
        }
        if index != 0 && !block.parameters().is_empty() {
            assert!(
                jumped[index],
                "{}: block {index} has parameters and no jump reaches it",
                function.name()
            );
        }
    }
}

fn assert_lowered_functions_are_well_formed(overlap: OverlapLowering) {
    let reports = crate::in_parallel(PROGRAMS, |(name, source)| {
        with_ir_mode(source, overlap, |program| {
            let mut redundant = Vec::new();
            for function in program.functions() {
                assert_parameter_interfaces(function);
                assert_uses_follow_definitions(function);
                let parameters = redundant_block_parameters(function);
                if !parameters.is_empty() {
                    redundant.push(format!("{} {parameters:?}", function.name()));
                }
            }
            (*name, redundant)
        })
    });
    let kept: Vec<String> = reports
        .into_iter()
        .filter(|(_, redundant)| !redundant.is_empty())
        .map(|(name, redundant)| format!("{name}: {}", redundant.join("; ")))
        .collect();
    assert!(
        kept.is_empty(),
        "no join or loop header may keep a parameter every edge feeds one value: {kept:#?}"
    );
}

#[test]
fn sequential_programs_lower_to_ir_the_parameter_merge_leaves_well_formed() {
    assert_lowered_functions_are_well_formed(OverlapLowering::Off);
}

#[test]
fn overlapped_programs_lower_to_ir_the_parameter_merge_leaves_well_formed() {
    // Loop splitting synthesizes chunk functions and splitters whose captured
    // values are operands the merge rewrites; the sequential world has none.
    assert_lowered_functions_are_well_formed(OverlapLowering::On);
}
