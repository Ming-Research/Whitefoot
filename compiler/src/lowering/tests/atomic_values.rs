//! Placement and lifetime obligations of lock-free atomic-value targets.
use super::*;

fn operations(function: &IrFunction) -> Vec<&IrOperation> {
    function
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
        .filter_map(|instruction| match instruction {
            IrInstruction::Define { operation, .. } => Some(operation),
            _ => None,
        })
        .collect()
}

#[test]
fn atomic_value_operation_takes_later_groups_before_its_effect_point() {
    let source = br#"struct Metadata {
  count: u64;
}

fn write(sequence: &Shared<AtomicValue>, early: &Shared<u64>, late: &Shared<Metadata>) -> result: unit reads(sequence), reads(early), reads(late) waits {
  atomic number = &sequence^, first = &early^, last = &late^ {
    let before = atomic_add(cell: number, value: 1_u64);
    set first^ = 4_u64;
    set last^.count = 8_u64;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let sequence = shared_atomic_new(value: 0_u64);
  let early = shared_new::<u64>(value: 0_u64);
  let metadata = Metadata(count: 0_u64);
  let late = shared_new::<Metadata>(value: metadata);
  write(sequence: &sequence, early: &early, late: &late);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let ops = operations(function(program, "write"));
        assert!(
            ops.iter().all(|op| !matches!(op, IrOperation::ContextPass)),
            "mixed statements retain the pass already supplied by ordinary acquisition"
        );
        let effect = ops
            .iter()
            .position(|op| matches!(op, IrOperation::AtomicValueAdd { .. }))
            .unwrap();
        let takes = ops
            .iter()
            .enumerate()
            .filter(|(_, op)| {
                matches!(
                    op,
                    IrOperation::SharedAcquire { .. } | IrOperation::SharedTake { .. }
                )
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(
            takes.iter().filter(|index| **index < effect).count(),
            2,
            "both ordinary groups are taken before the effect: {ops:?}"
        );
        assert!(
            ops[..effect]
                .iter()
                .all(|op| !matches!(op, IrOperation::SharedUnlock { .. })),
            "no release precedes the effect"
        );
        assert!(
            ops.iter()
                .all(|op| !matches!(op, IrOperation::SharedRetain { .. })),
            "read-only handle places retain their existing lifetime"
        );
    });
}

#[test]
fn replacing_an_atomic_target_handle_retains_the_original_state_until_release() {
    let source = br#"fn replace() -> result: Option<u64> pure waits {
  let cell = shared_atomic_new(value: 7_u64);
  let before = None<u64>();
  atomic state = &cell {
    let fresh = shared_atomic_new(value: 40_u64);
    set cell = move fresh;
    set before = atomic_add(cell: state, value: 1_u64);
  }
  return before;
}

fn main() -> status: std::process::ExitStatus pure waits {
  match replace() {
    Some(value: before) => {
      if before == 7_u64 {
        return std::process::exit_status(code: 0_u8);
      }
    }
    None() => {}
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    with_ir(source, |program| {
        let ops = operations(function(program, "replace"));
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op, IrOperation::SharedRetain { .. }))
                .count(),
            1,
            "replacing the only owning handle must not free the bound state: {ops:?}"
        );
        assert!(
            ops.iter().all(|op| !matches!(
                op,
                IrOperation::SharedAcquire { .. }
                    | IrOperation::SharedTake { .. }
                    | IrOperation::SharedUnlock { .. }
            )),
            "lifetime protection is not an exclusion lock: {ops:?}"
        );
    });
}

#[test]
fn a_branch_without_an_atomic_value_operation_never_reaches_the_shared_word() {
    let source = br#"fn conditional(counter: &Shared<AtomicValue>, logging: Bool) -> result: Option<u64> reads(counter) waits {
  let before = None<u64>();
  atomic state = &counter^ {
    if logging {
      set before = atomic_add(cell: state, value: 1_u64);
    }
  }
  return before;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let counter = shared_atomic_new(value: 0_u64);
  let disabled = False();
  let before = conditional(counter: &counter, logging: disabled);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let function = function(program, "conditional");
        let ops = operations(function);
        let passes = ops
            .iter()
            .enumerate()
            .filter_map(|(index, op)| matches!(op, IrOperation::ContextPass).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(
            passes.len(),
            1,
            "even the disabled branch reaches one waiting boundary"
        );
        let snapshot = ops
            .iter()
            .position(|op| matches!(op, IrOperation::SnapshotAtomicBegin))
            .unwrap();
        let state = ops
            .iter()
            .position(|op| matches!(op, IrOperation::SharedState { .. }))
            .unwrap();
        assert!(
            passes[0] < snapshot && snapshot < state,
            "the scheduling pass precedes snapshot preparation and every target: {ops:?}"
        );
        let false_arm = function
            .blocks()
            .iter()
            .find_map(|block| {
                if let IrTerminator::Match { targets, .. } = block.terminator() {
                    targets
                        .iter()
                        .find(|target| target.tag() == 0)
                        .map(|target| target.block())
                } else {
                    None
                }
            })
            .expect("the logging condition has a false arm");
        let mut pending = vec![false_arm];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(block) = pending.pop() {
            if !visited.insert(block) {
                continue;
            }
            let block = &function.blocks()[block.index()];
            for instruction in block.instructions() {
                assert!(
                    !matches!(
                        instruction,
                        IrInstruction::Define {
                            operation: IrOperation::AtomicValueLoad { .. }
                                | IrOperation::AtomicValueAdd { .. }
                                | IrOperation::AtomicValueRaise { .. }
                                | IrOperation::SharedRetain { .. }
                                | IrOperation::SharedAcquire { .. }
                                | IrOperation::SharedTake { .. },
                            ..
                        }
                    ),
                    "the disabled path must not touch the shared line: {instruction:?}"
                );
            }
            match block.terminator() {
                IrTerminator::Jump { target, .. } => pending.push(*target),
                IrTerminator::Match { targets, .. } => {
                    pending.extend(targets.iter().map(|target| target.block()))
                }
                IrTerminator::Return { .. } | IrTerminator::Unreachable => {}
                other => panic!("unexpected control flow in logging witness: {other:?}"),
            }
        }
    });
}
