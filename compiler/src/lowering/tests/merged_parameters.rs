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

/// Single-file programs of `tests/programs` that use no host module (the
/// bare lowering bundle has none), and that between them hold the control-flow
/// shapes above: a header `match` interpreter, loops with `break` and
/// `continue`, early returns that drop owners, text and byte scanners, linked
/// owners and data-parallel loops.
const PROGRAMS: &[(&str, &[u8])] = &[
    (
        "continue_interpreter",
        include_bytes!("../../../../tests/programs/continue_interpreter.wf"),
    ),
    (
        "utf8parse",
        include_bytes!("../../../../tests/programs/utf8parse.wf"),
    ),
    (
        "option_slots",
        include_bytes!("../../../../tests/programs/option_slots.wf"),
    ),
    (
        "percent_decode",
        include_bytes!("../../../../tests/programs/percent_decode.wf"),
    ),
    (
        "sha256_abc",
        include_bytes!("../../../../tests/programs/sha256_abc.wf"),
    ),
    (
        "telemetry_packet",
        include_bytes!("../../../../tests/programs/telemetry_packet.wf"),
    ),
    (
        "owned_link_cursors",
        include_bytes!("../../../../tests/programs/owned_link_cursors.wf"),
    ),
    (
        "block_pool",
        include_bytes!("../../../../tests/programs/block_pool.wf"),
    ),
    (
        "prefix_expression",
        include_bytes!("../../../../tests/programs/prefix_expression.wf"),
    ),
    (
        "unit_set",
        include_bytes!("../../../../tests/programs/unit_set.wf"),
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
        "records",
        include_bytes!("../../../../tests/programs/compute/records.wf"),
    ),
    (
        "merge_sort",
        include_bytes!("../../../../tests/programs/compute/merge_sort.wf"),
    ),
    (
        "indexed_calls",
        include_bytes!("../../../../tests/programs/parallel/indexed_calls.wf"),
    ),
    (
        "indexed_marks_fields",
        include_bytes!("../../../../tests/programs/parallel/indexed_marks_fields.wf"),
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

/// Where `parameter` is, what type it has, and each edge that feeds it with
/// the argument's own type and, for a parameter argument, the edges feeding
/// that in turn, so a failure shows why a parameter stayed.
fn describe_parameter(function: &IrFunction, parameter: IrValueId) -> String {
    let blocks = function.blocks();
    let home = blocks
        .iter()
        .position(|block| block.parameters().iter().any(|(value, _)| *value == parameter))
        .expect("a block declares the parameter");
    let mut edges = Vec::new();
    let mut entered_by_match = false;
    for (index, block) in blocks.iter().enumerate() {
        match block.terminator() {
            IrTerminator::Jump {
                target, arguments, ..
            } if target.index() == home => {
                let position = blocks[home]
                    .parameters()
                    .iter()
                    .position(|(value, _)| *value == parameter)
                    .expect("declared");
                let argument = arguments[position];
                edges.push(format!(
                    "from block {index}: {argument:?}: {:?}",
                    function.values[argument.index()]
                ));
            }
            IrTerminator::Match { targets, .. } => {
                entered_by_match |= targets.iter().any(|target| target.block().index() == home);
            }
            IrTerminator::IntegerMatch {
                targets, default, ..
            } => {
                entered_by_match |= targets.iter().any(|target| target.block().index() == home)
                    || default.is_some_and(|block| block.index() == home);
            }
            _ => {}
        }
    }
    format!(
        "{}: {parameter:?}: {:?} in block {home} (match edge: {entered_by_match}) fed by {edges:?}",
        function.name(),
        function.values[parameter.index()]
    )
}

fn assert_lowered_functions_are_well_formed(overlap: OverlapLowering) {
    let reports = crate::in_parallel(PROGRAMS, |(name, source)| {
        // A failing check panics inside the lowering fixture; the program's
        // name is what tells the failures of different programs apart.
        let outcome = std::panic::catch_unwind(|| {
            with_ir_mode(source, overlap, |program| {
                let mut redundant = Vec::new();
                for function in program.functions() {
                    assert_parameter_interfaces(function);
                    assert_uses_follow_definitions(function);
                    let parameters = redundant_block_parameters(function);
                    for parameter in parameters {
                        redundant.push(describe_parameter(function, parameter));
                    }
                }
                redundant
            })
        });
        let problems = outcome.unwrap_or_else(|payload| {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                .unwrap_or_default();
            vec![format!("panicked: {message}")]
        });
        (*name, problems)
    });
    let failed: Vec<String> = reports
        .into_iter()
        .filter(|(_, problems)| !problems.is_empty())
        .map(|(name, problems)| format!("{name}: {}", problems.join("; ")))
        .collect();
    assert!(
        failed.is_empty(),
        "every lowered function keeps its parameters well formed and none keeps a parameter every edge feeds one value: {failed:#?}"
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
