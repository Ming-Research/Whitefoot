//! Bound contexts join at the owning block's planned statement or normal end.

use super::*;

fn context_awaits(function: &IrFunction) -> Vec<(usize, usize, IrValueId)> {
    function
        .blocks()
        .iter()
        .enumerate()
        .flat_map(|(block, body)| {
            body.instructions()
                .iter()
                .enumerate()
                .filter_map(move |(index, instruction)| match instruction {
                    IrInstruction::Define {
                        operation: IrOperation::ContextAwait { start },
                        ..
                    } => Some((block, index, *start)),
                    _ => None,
                })
        })
        .collect()
}

#[test]
fn an_unused_bound_context_awaits_after_the_atomic_statement_at_block_end() {
    let source = include_bytes!("../../../../tests/programs/bound_context_block_end.wf");
    with_checked(source, |checked| {
        let main = checked
            .data
            .executable_functions()
            .find(|function| function.name == "main")
            .expect("main");
        let [planned] = main.waiting.context_awaits.as_slice() else {
            panic!("one bound context: {:?}", main.waiting.context_awaits);
        };
        assert_eq!(planned.before, None, "the loop body never names pending");

        let program = lower_checked(checked, OverlapLowering::Off).expect("witness lowers");
        let main = function(&program, "main");
        let awaits = context_awaits(main);
        let [(await_block, await_index, start)] = awaits.as_slice() else {
            panic!("one block-end await: {awaits:?}");
        };
        assert!(main.blocks().iter().any(|block| {
            block.instructions().iter().any(|instruction| {
                matches!(
                    instruction,
                    IrInstruction::Define {
                        result,
                        operation: IrOperation::ContextStartBound { .. },
                        ..
                    } if result == start
                )
            })
        }));
        assert!(
            main.blocks()[*await_block].instructions()[..*await_index]
                .iter()
                .any(|instruction| matches!(
                    instruction,
                    IrInstruction::Define {
                        operation: IrOperation::SnapshotAtomicEnd,
                        ..
                    }
                )),
            "the atomic statement must finish before the block-end await"
        );

        let unlocks = main
            .blocks()
            .iter()
            .filter(|block| {
                block.instructions().iter().any(|instruction| {
                    matches!(
                        instruction,
                        IrInstruction::Define {
                            operation: IrOperation::SharedUnlock { wrote: true, .. },
                            ..
                        }
                    )
                })
            })
            .collect::<Vec<_>>();
        let [unlock] = unlocks.as_slice() else {
            panic!("one written-state unlock: {unlocks:?}");
        };
        assert!(
            matches!(
                unlock.terminator(),
                IrTerminator::Jump { target, .. } if target.0 as usize == *await_block
            ),
            "the state is unlocked before reaching the await's block"
        );
        let state = main
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
            .find_map(|instruction| match instruction {
                IrInstruction::Define {
                    result,
                    operation: IrOperation::SharedState { .. },
                    ..
                } => Some(*result),
                _ => None,
            })
            .expect("the atomic statement addresses the shared state");
        assert!(main.blocks().iter().any(|block| {
            block.instructions().iter().any(|instruction| {
                matches!(instruction, IrInstruction::Store { address, .. } if *address == state)
            })
        }));
    });
}

#[test]
fn block_end_awaits_every_local_context_and_preserves_an_outer_pending_context() {
    let source = br#"fn finish() -> result: unit pure waits {
  return unit;
}

fn marker() -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let outer = spawn finish();
  for @once (index in 0_u64..1_u64) {
    let first = spawn finish();
    let second = spawn finish();
    marker();
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_checked(source, |checked| {
        let main = checked
            .data
            .executable_functions()
            .find(|function| function.name == "main")
            .expect("main");
        assert_eq!(
            main.waiting
                .context_awaits
                .iter()
                .filter(|planned| planned.before.is_none())
                .count(),
            2,
            "both contexts local to the loop body join at its end"
        );
        assert_eq!(
            main.waiting
                .context_awaits
                .iter()
                .filter_map(|planned| planned.before)
                .collect::<Vec<_>>(),
            [2],
            "the outer context joins before the return, after the loop"
        );
        let program = lower_checked(checked, OverlapLowering::Off).expect("nested witness lowers");
        let main = function(&program, "main");
        let starts = main
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
            .filter_map(|instruction| match instruction {
                IrInstruction::Define {
                    result,
                    operation: IrOperation::ContextStartBound { .. },
                    ..
                } => Some(*result),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(starts.len(), 3);
        let awaits = context_awaits(main);
        assert_eq!(awaits.len(), starts.len());
        for start in &starts {
            assert_eq!(
                awaits.iter().filter(|(_, _, joined)| joined == start).count(),
                1,
                "each context is awaited exactly once"
            );
        }
        let marker = program
            .functions()
            .iter()
            .position(|function| function.name() == "marker")
            .expect("marker function") as u32;
        let (body_block, marker_index) = main
            .blocks()
            .iter()
            .enumerate()
            .find_map(|(block, body)| {
                body.instructions()
                    .iter()
                    .position(|instruction| {
                        matches!(
                            instruction,
                            IrInstruction::Define {
                                operation: IrOperation::Call { function, .. },
                                ..
                            } if *function == marker
                        )
                    })
                    .map(|index| (block, index))
            })
            .expect("the loop body's later statement");
        for start in &starts[1..] {
            assert!(awaits.iter().any(|(block, index, joined)| {
                *joined == *start && *block == body_block && *index > marker_index
            }));
        }
        assert!(awaits.iter().any(|(block, _, joined)| {
            *joined == starts[0] && *block != body_block
        }));
    });
}
