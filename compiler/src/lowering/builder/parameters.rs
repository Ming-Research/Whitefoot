//! Dead block-parameter elimination shared by ordinary lowering and chunk captures.

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

#[cfg(test)]
mod tests {
    use super::prune_block_parameters;
    use crate::{
        IrAddressed, IrBlock, IrBlockId, IrDrop, IrDropSubject, IrEnumType, IrFunction,
        IrInstruction, IrMatchTarget, IrOperation, IrOverlap, IrTerminator, IrType, IrValueId,
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
            box_keeping_reference_parameters: Vec::new(),
            source_signature: None,
            source_calls: Vec::new(),
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
}
