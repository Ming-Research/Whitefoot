//! Block-parameter simplification: dead-parameter elimination shared by
//! ordinary lowering and chunk captures, and the merge of parameters whose
//! every incoming edge carries one value.

use crate::{IrFunction, IrInstruction, IrTerminator, IrValueId, LoweringFailure};

/// Keep a parameter exactly when forwarding it can reach a runtime use.
/// Instruction operands (including calls, stores and cleanup places), match
/// scrutinees, returns and drops seed the walk. An edge argument depends on
/// its destination parameter, not the reverse: a forwarding cycle with no
/// observation is dead. Each value is visited once, so cycles terminate.
///
/// Ordinary lowering passes zero for `reconstruction_count`: every instruction
/// remains, including unused results with effects or owned contents. Outlined
/// chunks may additionally remove their generated entry reconstruction prefix,
/// whose operands are needed only if its result is needed. `retained` roots
/// describe captures used by the chunk's caller rather than its body.
///
/// Function parameters and all IDs stay stable. The caller alone may prune a
/// synthesized capture ABI; source-call, overlap and counted-range identities
/// are not renumbered. No cleanup action, ordering or storage lifetime changes.
/// Only a parameter and its incoming arguments are removed: no instruction
/// operand is rewritten (see [`merge_equal_block_parameters`] for that).
pub(super) fn prune_block_parameters(
    function: &mut IrFunction,
    reconstruction_count: usize,
    retained: &[IrValueId],
) -> Result<Vec<bool>, LoweringFailure> {
    let mut dependencies = vec![Vec::new(); function.values.len()];
    let mut pending = retained.to_vec();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if block_index == 0 && instruction_index < reconstruction_count {
                let IrInstruction::Define {
                    result, operation, ..
                } = instruction
                else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                dependencies[result.index()] = operation.operands();
            } else {
                pending.extend(instruction.operands());
            }
        }
        if let IrTerminator::Jump {
            target,
            arguments,
            drops,
        } = &block.terminator
        {
            let target = function
                .blocks
                .get(target.index())
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            if arguments.len() != target.parameters.len() {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            for ((parameter, _), argument) in target.parameters.iter().zip(arguments) {
                dependencies[parameter.index()].push(*argument);
            }
            pending.extend(drops.iter().map(|drop| drop.operand()));
        } else {
            pending.extend(block.terminator.operands());
        }
    }
    let mut needed = vec![false; function.values.len()];
    while let Some(value) = pending.pop() {
        if !needed[value.index()] {
            needed[value.index()] = true;
            pending.extend(dependencies[value.index()].iter().copied());
        }
    }
    let block_parameters = function
        .blocks
        .iter()
        .map(|block| {
            block
                .parameters
                .iter()
                .map(|(value, _)| needed[value.index()])
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for (block_index, block) in function.blocks.iter_mut().enumerate() {
        block.parameters.retain(|(value, _)| needed[value.index()]);
        if let IrTerminator::Jump {
            target, arguments, ..
        } = &mut block.terminator
        {
            let mut keep = block_parameters[target.index()].iter();
            arguments.retain(|_| *keep.next().expect("checked jump arity"));
        }
        if block_index == 0 {
            let mut index = 0;
            block.instructions.retain(|instruction| {
                let reconstruction = index < reconstruction_count;
                index += 1;
                !reconstruction || matches!(instruction, IrInstruction::Define { result, .. } if needed[result.index()])
            });
        }
    }
    Ok(needed)
}

/// Replace every block parameter whose incoming `Jump` edges all carry one
/// value, directly or through other such parameters, by that value, and
/// remove it with its incoming arguments.
///
/// The decision covers the whole function ([`settle_parameters`] removes
/// trivial parameters and cycles of parameters with one entering value until
/// neither is left), so a loop-carried parameter that a branch of the body
/// sets back to its value before the loop, or a counted loop's carried
/// bounds, merge with the value they carry although no parameter's arguments
/// are all equal as written.
///
/// A parameter stays when its incoming values differ, when an incoming
/// argument has another type than the parameter, when no `Jump` reaches its
/// block, when its block is the entry or the target of a `Match` or
/// `IntegerMatch` (which carry no arguments), and when only a cycle of
/// parameters feeds it: no value enters such a cycle, so its copies collapse
/// into one parameter that feeds itself, which has nothing to be replaced by.
///
/// Uses of a removed parameter are rewritten everywhere a runtime operand or
/// an operand-indexed record names it: instructions, jump arguments, cleanup
/// subjects and counted-range bounds. Value ids and types are not renumbered,
/// so a removed parameter's id stays unused in `values`, as in
/// [`prune_block_parameters`]. The surviving value is defined on every path
/// into the join, because it is what each edge already carried. No block,
/// instruction, cleanup or overlap group is added or removed.
pub(super) fn merge_equal_block_parameters(
    function: &mut IrFunction,
) -> Result<(), LoweringFailure> {
    let count = function.values.len();
    let mut is_parameter = vec![false; count];
    let mut incoming = vec![Vec::new(); count];
    let mut fixed = vec![false; count];
    let mut fed_by_jump = vec![false; function.blocks.len()];
    let mut matched = vec![false; function.blocks.len()];
    for block in &function.blocks {
        for (parameter, _) in &block.parameters {
            *is_parameter
                .get_mut(parameter.index())
                .ok_or(LoweringFailure::InvalidCheckedProgram)? = true;
        }
    }
    for block in &function.blocks {
        match &block.terminator {
            IrTerminator::Jump {
                target, arguments, ..
            } => {
                let target_block = function
                    .blocks
                    .get(target.index())
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                if arguments.len() != target_block.parameters.len() {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                fed_by_jump[target.index()] = true;
                for ((parameter, _), argument) in target_block.parameters.iter().zip(arguments) {
                    let argument_type = function
                        .values
                        .get(argument.index())
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                    if *argument_type != function.values[parameter.index()] {
                        fixed[parameter.index()] = true;
                    }
                    incoming[parameter.index()].push(*argument);
                }
            }
            IrTerminator::Match { targets, .. } => {
                for target in targets {
                    *matched
                        .get_mut(target.block.index())
                        .ok_or(LoweringFailure::InvalidCheckedProgram)? = true;
                }
            }
            IrTerminator::IntegerMatch {
                targets, default, ..
            } => {
                for block in targets.iter().map(|target| target.block).chain(*default) {
                    *matched
                        .get_mut(block.index())
                        .ok_or(LoweringFailure::InvalidCheckedProgram)? = true;
                }
            }
            IrTerminator::Return { .. } | IrTerminator::Unreachable => {}
        }
    }
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (parameter, _) in &block.parameters {
            if block_index == 0 || !fed_by_jump[block_index] || matched[block_index] {
                fixed[parameter.index()] = true;
            }
        }
    }
    let replacement = settle_parameters(&incoming, &is_parameter, &fixed)?;
    if replacement.iter().all(Option::is_none) {
        return Ok(());
    }
    // A surviving value is never itself merged: it is defined by an
    // instruction or a function parameter, or it is a varying block parameter.
    debug_assert!(
        replacement
            .iter()
            .flatten()
            .all(|value| replacement[value.index()].is_none())
    );
    let resolve = |value: IrValueId| replacement[value.index()].unwrap_or(value);
    let kept: Vec<Vec<bool>> = function
        .blocks
        .iter()
        .map(|block| {
            block
                .parameters
                .iter()
                .map(|(parameter, _)| replacement[parameter.index()].is_none())
                .collect()
        })
        .collect();
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            instruction.remap_operands(resolve);
        }
        block.terminator.remap_operands(resolve);
        block
            .parameters
            .retain(|(parameter, _)| replacement[parameter.index()].is_none());
        if let IrTerminator::Jump {
            target, arguments, ..
        } = &mut block.terminator
        {
            let mut keep = kept[target.index()].iter();
            arguments.retain(|_| *keep.next().expect("checked jump arity"));
        }
    }
    for range in &mut function.counted_ranges {
        range.lower = resolve(range.lower);
        range.upper = resolve(range.upper);
    }
    Ok(())
}

/// For each parameter, the value it is replaced by, or `None` where it stays.
///
/// A candidate is a block parameter not in `fixed`; a parameter in `fixed`
/// stays whatever feeds it, though it may be the value a candidate is
/// replaced by. A candidate whose incoming arguments, once the replacements
/// found so far are applied and the candidate itself is left out, are one
/// value is replaced by that value; replacing it can make the candidates it
/// feeds trivial in turn. Candidates that feed only each other may still carry
/// one value: a strongly connected component of the forwarding graph with
/// exactly one value entering from outside is replaced by that value. The two
/// steps alternate until neither replaces anything, so a copy of a varying
/// parameter is replaced although the varying parameters it belongs to are
/// not, and no answer rests on a guess about a parameter that later turns out
/// to vary.
fn settle_parameters(
    incoming: &[Vec<IrValueId>],
    is_parameter: &[bool],
    fixed: &[bool],
) -> Result<Vec<Option<IrValueId>>, LoweringFailure> {
    fn find(parent: &mut [usize], value: usize) -> usize {
        let mut root = value;
        while parent[root] != root {
            root = parent[root];
        }
        let mut walk = value;
        while parent[walk] != root {
            let next = parent[walk];
            parent[walk] = root;
            walk = next;
        }
        root
    }
    let count = fixed.len();
    let candidate: Vec<bool> = (0..count)
        .map(|index| is_parameter[index] && !fixed[index])
        .collect();
    let mut users: Vec<Vec<usize>> = vec![Vec::new(); count];
    for parameter in (0..count).filter(|index| candidate[*index]) {
        for argument in &incoming[parameter] {
            if is_parameter[argument.index()] {
                users[argument.index()].push(parameter);
            }
        }
    }
    let mut parent: Vec<usize> = (0..count).collect();
    let mut pending: Vec<usize> = (0..count).filter(|index| candidate[*index]).collect();
    loop {
        while let Some(parameter) = pending.pop() {
            if parent[parameter] != parameter {
                continue;
            }
            let mut only = None;
            let mut several = false;
            for argument in &incoming[parameter] {
                let value = find(&mut parent, argument.index());
                if value == parameter {
                    continue;
                }
                match only {
                    None => only = Some(value),
                    Some(first) if first != value => {
                        several = true;
                        break;
                    }
                    Some(_) => {}
                }
            }
            if let (false, Some(value)) = (several, only) {
                parent[parameter] = value;
                pending.extend(users[parameter].iter().copied());
            }
        }
        // Strongly connected components of the candidates still standing,
        // dependencies first.
        let unvisited = usize::MAX;
        let mut order = vec![unvisited; count];
        let mut low = vec![0usize; count];
        let mut on_stack = vec![false; count];
        let mut stack: Vec<usize> = Vec::new();
        let mut components: Vec<Vec<usize>> = Vec::new();
        let mut counter = 0usize;
        let standing: Vec<usize> = (0..count)
            .filter(|index| candidate[*index] && parent[*index] == *index)
            .collect();
        for root in standing {
            if order[root] != unvisited {
                continue;
            }
            order[root] = counter;
            low[root] = counter;
            counter += 1;
            stack.push(root);
            on_stack[root] = true;
            let mut work: Vec<(usize, usize)> = vec![(root, 0)];
            while let Some(&(node, edge)) = work.last() {
                if let Some(argument) = incoming[node].get(edge) {
                    work.last_mut().expect("a node being visited").1 += 1;
                    let next = find(&mut parent, argument.index());
                    if next == node || !candidate[next] {
                        continue;
                    }
                    if order[next] == unvisited {
                        order[next] = counter;
                        low[next] = counter;
                        counter += 1;
                        stack.push(next);
                        on_stack[next] = true;
                        work.push((next, 0));
                    } else if on_stack[next] {
                        low[node] = low[node].min(order[next]);
                    }
                    continue;
                }
                work.pop();
                if let Some(&(up, _)) = work.last() {
                    low[up] = low[up].min(low[node]);
                }
                if low[node] != order[node] {
                    continue;
                }
                let mut members = Vec::new();
                loop {
                    let member = stack.pop().expect("a component's root is on the stack");
                    on_stack[member] = false;
                    members.push(member);
                    if member == node {
                        break;
                    }
                }
                components.push(members);
            }
        }
        let mut changed = false;
        let mut inside = vec![usize::MAX; count];
        for (number, members) in components.iter().enumerate() {
            for member in members {
                inside[*member] = number;
            }
            let mut outside = None;
            let mut several = false;
            'members: for member in members {
                for argument in &incoming[*member] {
                    let value = find(&mut parent, argument.index());
                    if value == *member || inside[value] == number {
                        continue;
                    }
                    match outside {
                        None => outside = Some(value),
                        Some(first) if first != value => {
                            several = true;
                            break 'members;
                        }
                        Some(_) => {}
                    }
                }
            }
            if let (false, Some(value)) = (several, outside) {
                for member in members {
                    parent[*member] = value;
                    pending.extend(users[*member].iter().copied());
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (0..count)
        .map(|index| {
            let root = find(&mut parent, index);
            if root == index {
                Ok(None)
            } else {
                IrValueId::from_index(root)
                    .map(Some)
                    .ok_or(LoweringFailure::CounterOverflow)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{merge_equal_block_parameters, prune_block_parameters};
    use crate::{
        IrAddressed, IrBlock, IrBlockId, IrCountedRange, IrDrop, IrDropSubject, IrEnumType,
        IrFunction, IrInstruction, IrIntegerMatchTarget, IrIntegerOperation, IrMatchTarget,
        IrOperation, IrOverlap, IrTerminator, IrType, IrValueId,
    };

    #[test]
    fn forwarding_cycles_keep_every_runtime_root_and_leave_schedules_intact() {
        let word = IrType::Integer {
            width: 64,
            signed: false,
        };
        let address = IrType::Address(IrAddressed::Integer {
            width: 64,
            signed: false,
        });
        let value = IrValueId;
        let mut types = vec![word; 25];
        for index in [2, 10, 18] {
            types[index] = address;
        }
        for index in [7, 15, 23] {
            types[index] = IrType::Bool;
        }
        let parameters = |range: std::ops::Range<u32>| {
            range
                .map(|index| (value(index), types[index as usize]))
                .collect()
        };
        let drop = |i| IrDrop {
            subject: IrDropSubject::Value(value(i)),
            ty: word,
        };
        let function = IrFunction {
            name: "forwarding_cycles".into(),
            parameters: parameters(0..8),
            readonly_reference_parameters: vec![value(2)],
            whole_read_reference_parameters: Vec::new(),
            whole_write_reference_parameters: Vec::new(),
            box_keeping_reference_parameters: Vec::new(),
            source_signature: None,
            source_calls: Vec::new(),
            lock_ledger: Vec::new(),
            result: word,
            values: types.clone(),
            counted_ranges: Vec::new(),
            overlaps: Vec::new(),
            synthesis: None,
            waits: false,
            blocks: vec![
                IrBlock {
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminator: IrTerminator::Jump {
                        target: IrBlockId(1),
                        arguments: (0..8).map(value).collect(),
                        drops: Vec::new(),
                    },
                },
                IrBlock {
                    parameters: parameters(8..16),
                    instructions: vec![
                        IrInstruction::Define {
                            result: value(24),
                            ty: word,
                            operation: IrOperation::Call {
                                function: 0,
                                arguments: vec![value(8)],
                            },
                        },
                        IrInstruction::Drops(vec![
                            drop(9),
                            IrDrop {
                                subject: IrDropSubject::Place(value(10)),
                                ty: word,
                            },
                        ]),
                    ],
                    terminator: IrTerminator::Match {
                        scrutinee: value(15),
                        enum_type: IrEnumType::Bool,
                        targets: vec![
                            IrMatchTarget {
                                tag: 1,
                                block: IrBlockId(2),
                            },
                            IrMatchTarget {
                                tag: 0,
                                block: IrBlockId(3),
                            },
                        ],
                    },
                },
                IrBlock {
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminator: IrTerminator::Jump {
                        target: IrBlockId(4),
                        arguments: (8..16).map(value).collect(),
                        drops: vec![drop(11)],
                    },
                },
                IrBlock {
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminator: IrTerminator::Return {
                        value: value(12),
                        drops: vec![drop(13)],
                    },
                },
                IrBlock {
                    parameters: parameters(16..24),
                    instructions: Vec::new(),
                    terminator: IrTerminator::Jump {
                        target: IrBlockId(1),
                        arguments: (16..24).map(value).collect(),
                        drops: Vec::new(),
                    },
                },
            ],
        };
        for scheduled in [false, true] {
            let mut pruned = function.clone();
            pruned.waits = scheduled;
            if scheduled {
                pruned.overlaps.push(IrOverlap {
                    members: vec![value(24)],
                });
            }
            let before = pruned.clone();
            let needed = prune_block_parameters(&mut pruned, 0, &[]).expect("valid graph");
            // 14 <-> 22 is forwarding with no observation. Every other carry
            // reaches a call, either drop form, a branch, a jump drop or return.
            assert_eq!(
                needed,
                (0..25)
                    .map(|index| ![6, 14, 22, 24].contains(&index))
                    .collect::<Vec<_>>()
            );
            for (block, original) in pruned.blocks.iter().zip(&before.blocks) {
                assert_eq!(block.instructions, original.instructions);
                assert_eq!(
                    block.parameters.len(),
                    original.parameters.len().saturating_sub(1)
                );
                if let IrTerminator::Jump {
                    target,
                    arguments,
                    drops,
                } = &block.terminator
                {
                    assert_eq!(
                        arguments.len(),
                        pruned.blocks[target.index()].parameters.len()
                    );
                    let IrTerminator::Jump {
                        drops: previous, ..
                    } = &original.terminator
                    else {
                        panic!("jump stays a jump");
                    };
                    assert_eq!(drops, previous);
                    assert!(!arguments.iter().any(|v| [6, 14, 22].contains(&v.0)));
                } else {
                    assert_eq!(block.terminator, original.terminator);
                }
            }
            assert_eq!(
                pruned.parameters, before.parameters,
                "source ABI stays fixed"
            );
            assert_eq!(pruned.values, before.values, "value identities stay fixed");
            assert_eq!(pruned.overlaps, before.overlaps);
            assert_eq!(pruned.waits, before.waits);
            let once = pruned.clone();
            prune_block_parameters(&mut pruned, 0, &[]).expect("already pruned");
            assert_eq!(pruned, once, "the walk reaches a fixed point in one pass");
        }
    }

    const WORD: IrType = IrType::Integer {
        width: 64,
        signed: false,
    };

    fn value(index: u32) -> IrValueId {
        IrValueId(index)
    }

    fn block_id(index: u32) -> IrBlockId {
        IrBlockId(index)
    }

    /// A function with the given value types, whose first `formals` values are
    /// its parameters, and the given blocks.
    fn function_of(types: &[IrType], formals: u32, blocks: Vec<IrBlock>) -> IrFunction {
        IrFunction {
            name: "merge".into(),
            parameters: (0..formals)
                .map(|index| (value(index), types[index as usize]))
                .collect(),
            readonly_reference_parameters: Vec::new(),
            whole_read_reference_parameters: Vec::new(),
            whole_write_reference_parameters: Vec::new(),
            box_keeping_reference_parameters: Vec::new(),
            source_signature: None,
            source_calls: Vec::new(),
            lock_ledger: Vec::new(),
            result: WORD,
            values: types.to_vec(),
            counted_ranges: Vec::new(),
            overlaps: Vec::new(),
            synthesis: None,
            waits: false,
            blocks,
        }
    }

    fn block(
        types: &[IrType],
        parameters: &[u32],
        instructions: Vec<IrInstruction>,
        terminator: IrTerminator,
    ) -> IrBlock {
        IrBlock {
            parameters: parameters
                .iter()
                .map(|index| (value(*index), types[*index as usize]))
                .collect(),
            instructions,
            terminator,
        }
    }

    fn jump(target: u32, arguments: &[u32]) -> IrTerminator {
        IrTerminator::Jump {
            target: block_id(target),
            arguments: arguments.iter().map(|index| value(*index)).collect(),
            drops: Vec::new(),
        }
    }

    fn ret(operand: u32) -> IrTerminator {
        IrTerminator::Return {
            value: value(operand),
            drops: Vec::new(),
        }
    }

    fn branch(condition: u32, then: u32, otherwise: u32) -> IrTerminator {
        IrTerminator::Match {
            scrutinee: value(condition),
            enum_type: IrEnumType::Bool,
            targets: vec![
                IrMatchTarget {
                    tag: 1,
                    block: block_id(then),
                },
                IrMatchTarget {
                    tag: 0,
                    block: block_id(otherwise),
                },
            ],
        }
    }

    fn double(result: u32, operand: u32) -> IrInstruction {
        IrInstruction::Define {
            result: value(result),
            ty: WORD,
            operation: IrOperation::Integer {
                operation: IrIntegerOperation::AddWrap,
                operand_type: WORD,
                arguments: vec![value(operand), value(operand)],
            },
        }
    }

    /// Merge, and require the second run to change nothing.
    fn merged(mut function: IrFunction) -> IrFunction {
        merge_equal_block_parameters(&mut function).expect("valid graph");
        let once = function.clone();
        merge_equal_block_parameters(&mut function).expect("already merged");
        assert_eq!(function, once, "merging reaches its fixed point in one run");
        function
    }

    const B: IrType = IrType::Bool;

    #[test]
    fn a_join_every_edge_gives_the_same_value_is_replaced_by_that_value() {
        // 0 value, 1 condition, 2 join parameter, 3 its double.
        let types = [WORD, B, WORD, WORD];
        let build = |join: &[u32], arguments: &[u32], operand: u32| {
            let mut function = function_of(
                &types,
                2,
                vec![
                    block(&types, &[], vec![], branch(1, 1, 2)),
                    block(&types, &[], vec![], jump(3, arguments)),
                    block(&types, &[], vec![], jump(3, arguments)),
                    block(&types, join, vec![double(3, operand)], ret(3)),
                ],
            );
            function.overlaps.push(IrOverlap {
                members: vec![value(3)],
            });
            function
        };
        assert_eq!(merged(build(&[2], &[0], 2)), build(&[], &[], 0));
    }

    #[test]
    fn a_loop_carried_value_that_every_trip_passes_on_is_replaced_by_its_entry_value() {
        // 0 value, 1 condition, 2 header parameter, 3 its double.
        let types = [WORD, B, WORD, WORD];
        let build = |header: &[u32], entry: &[u32], back: &[u32], operand: u32| {
            let mut function = function_of(
                &types,
                2,
                vec![
                    block(&types, &[], vec![], jump(1, entry)),
                    block(&types, header, vec![double(3, operand)], branch(1, 2, 3)),
                    block(&types, &[], vec![], jump(1, back)),
                    block(&types, &[], vec![], ret(3)),
                ],
            );
            // A counted loop's recorded bounds name the header's parameter.
            function.counted_ranges.push(IrCountedRange {
                blocks: 1..3,
                continuation: block_id(3),
                lower: value(operand),
                upper: value(operand),
            });
            function.overlaps.push(IrOverlap {
                members: vec![value(3)],
            });
            function
        };
        let merged = merged(build(&[2], &[0], &[2], 2));
        assert_eq!(merged, build(&[], &[], &[], 0));
        assert_eq!(merged.values, vec![WORD, B, WORD, WORD], "ids stay put");
    }

    #[test]
    fn parameters_that_feed_each_other_and_carry_one_value_are_replaced_together() {
        // 0 and 1 values, 2 condition. The header parameter 3 takes the entry
        // value 0 or the join parameter 4; the join takes the header's value on
        // one arm and 0 on the other. No parameter's arguments are all equal
        // as written, yet both carry 0.
        let types = [WORD, WORD, B, WORD, WORD];
        let build = |header: &[u32],
                     join: &[u32],
                     entry: &[u32],
                     then: &[u32],
                     otherwise: &[u32],
                     back: &[u32],
                     exit: u32| {
            function_of(
                &types,
                3,
                vec![
                    block(&types, &[], vec![], jump(1, entry)),
                    block(&types, header, vec![], branch(2, 2, 5)),
                    block(&types, &[], vec![], branch(2, 3, 4)),
                    block(&types, &[], vec![], jump(6, then)),
                    block(&types, &[], vec![], jump(6, otherwise)),
                    block(&types, &[], vec![], ret(exit)),
                    block(&types, join, vec![], jump(1, back)),
                ],
            )
        };
        let before = build(&[3], &[4], &[0], &[3], &[0], &[4], 3);
        assert_eq!(
            merged(before),
            build(&[], &[], &[], &[], &[], &[], 0),
            "the cycle's two parameters merge together"
        );
        // When the other arm passes a different value, the cycle carries two
        // values and nothing merges.
        let varying = build(&[3], &[4], &[0], &[3], &[1], &[4], 3);
        assert_eq!(merged(varying.clone()), varying);
    }

    #[test]
    fn an_inner_loop_that_only_passes_on_a_swapped_value_merges_into_it() {
        // 0 and 1 values, 2 condition. The outer header parameters 3 and 4
        // swap on every trip, so each varies. The inner header parameter 5 is
        // entered with 3 and gets back the join parameter 6, which only
        // copies it: the two feed each other and carry 3, although the outer
        // parameter they carry varies.
        let types = [WORD, WORD, B, WORD, WORD, WORD, WORD];
        let build = |inner: &[u32], join: &[u32], enter: &[u32], pass: &[u32], back: &[u32]| {
            function_of(
                &types,
                3,
                vec![
                    block(&types, &[], vec![], jump(1, &[0, 1])),
                    block(&types, &[3, 4], vec![], branch(2, 2, 5)),
                    block(&types, &[], vec![], jump(3, enter)),
                    block(&types, inner, vec![], branch(2, 4, 6)),
                    block(&types, &[], vec![], jump(7, pass)),
                    block(&types, &[], vec![], ret(3)),
                    block(&types, &[], vec![], jump(1, &[4, 3])),
                    block(&types, join, vec![], jump(3, back)),
                ],
            )
        };
        let before = build(&[5], &[6], &[3], &[5], &[6]);
        assert_eq!(merged(before), build(&[], &[], &[], &[], &[]));
    }

    #[test]
    fn a_varying_parameter_that_only_copies_another_is_replaced_by_it() {
        // 0 and 1 values, 2 condition, 3 varying join, 4 copy of it.
        let types = [WORD, WORD, B, WORD, WORD];
        let build = |copy: &[u32], from_then: &[u32], from_else: &[u32], exit: u32| {
            function_of(
                &types,
                3,
                vec![
                    block(&types, &[], vec![], branch(2, 1, 2)),
                    block(&types, &[], vec![], jump(3, &[0])),
                    block(&types, &[], vec![], jump(3, &[1])),
                    block(&types, &[3], vec![], branch(2, 4, 5)),
                    block(&types, &[], vec![], jump(6, from_then)),
                    block(&types, &[], vec![], jump(6, from_else)),
                    block(&types, copy, vec![], ret(exit)),
                ],
            )
        };
        let before = build(&[4], &[3], &[3], 4);
        assert_eq!(merged(before), build(&[], &[], &[], 3));
    }

    #[test]
    fn a_parameter_with_another_type_than_its_arguments_is_kept() {
        let types = [WORD, B];
        let function = function_of(
            &types,
            1,
            vec![
                block(&types, &[], vec![], jump(1, &[0])),
                block(&types, &[1], vec![], branch(1, 2, 2)),
                block(&types, &[], vec![], ret(0)),
            ],
        );
        assert_eq!(merged(function.clone()), function);
    }

    #[test]
    fn a_cycle_no_value_enters_keeps_one_parameter_and_an_unreached_block_keeps_its_own() {
        // 0 value; 2 and 3 only feed each other and no value enters them;
        // block 3 is reached by no edge. 3 is a copy of 2, so it is replaced
        // by it; 2 has no value to be replaced by and stays, feeding itself.
        let types = [WORD, B, WORD, WORD, WORD];
        let build = |second: &[u32], forward: &[u32], back: &[u32]| {
            function_of(
                &types,
                2,
                vec![
                    block(&types, &[], vec![], ret(0)),
                    block(&types, &[2], vec![], jump(2, forward)),
                    block(&types, second, vec![], jump(1, back)),
                    block(&types, &[4], vec![], ret(4)),
                ],
            )
        };
        assert_eq!(merged(build(&[3], &[2], &[3])), build(&[], &[], &[2]));
    }

    #[test]
    fn the_target_of_a_branch_or_an_integer_match_keeps_its_parameters() {
        // Block 1 is entered by a jump that always carries 0 and by a match
        // edge that carries nothing: its parameter is not that value.
        let types = [WORD, B, WORD];
        let entries = [
            branch(1, 1, 2),
            IrTerminator::IntegerMatch {
                scrutinee: value(0),
                targets: vec![IrIntegerMatchTarget {
                    value: 0,
                    block: block_id(1),
                }],
                default: Some(block_id(2)),
            },
            IrTerminator::IntegerMatch {
                scrutinee: value(0),
                targets: vec![IrIntegerMatchTarget {
                    value: 0,
                    block: block_id(2),
                }],
                default: Some(block_id(1)),
            },
        ];
        for entry in entries {
            let function = function_of(
                &types,
                2,
                vec![
                    block(&types, &[], vec![], entry),
                    block(&types, &[2], vec![], ret(2)),
                    block(&types, &[], vec![], jump(1, &[0])),
                ],
            );
            assert_eq!(merged(function.clone()), function);
        }
    }

    #[test]
    fn merging_rewrites_cleanup_subjects_and_leaves_their_order_alone() {
        let types = [WORD, WORD, B, WORD];
        let build = |join: &[u32], arguments: &[u32], subject: u32| {
            let drop = |index| IrDrop {
                subject: IrDropSubject::Value(value(index)),
                ty: WORD,
            };
            function_of(
                &types,
                3,
                vec![
                    block(&types, &[], vec![], branch(2, 1, 2)),
                    block(&types, &[], vec![], jump(3, arguments)),
                    block(&types, &[], vec![], jump(3, arguments)),
                    block(
                        &types,
                        join,
                        vec![IrInstruction::Drops(vec![drop(1), drop(subject)])],
                        IrTerminator::Return {
                            value: value(subject),
                            drops: vec![drop(subject), drop(0)],
                        },
                    ),
                ],
            )
        };
        assert_eq!(merged(build(&[3], &[0], 3)), build(&[], &[], 0));
    }
}
