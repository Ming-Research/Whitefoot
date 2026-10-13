//! [SHARE-2] How a statement takes a type group, several targets of one
//! type (compiler/waiting-contexts/state-locks): its first group holds
//! nothing before it, so it waits as a first object's acquire does and a
//! guard's reservation can refuse it; a later group is taken by a holder of
//! an earlier object, which a reservation admits. The record's write flags
//! are checked where the emitter stores them (backend/tests/wake.rs).

use super::*;
use crate::{IrRecord, IrRecordKind};

/// The distinct group takes in `main`, each site a statement's acquisition
/// path reaches once: whether it may park, and its target count.
fn group_takes(program: &IrProgram) -> Vec<(bool, u32)> {
    fn count(record: IrRecord) -> u32 {
        match record.kind() {
            IrRecordKind::AtomicGroup { count } => count,
            other => panic!("a group take names a group record: {other:?}"),
        }
    }
    let mut takes = function(program, "main")
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
        .filter_map(|instruction| match instruction {
            IrInstruction::Define {
                operation: IrOperation::AtomicGroupAcquire { record },
                ..
            } => Some((true, count(*record))),
            IrInstruction::Define {
                operation: IrOperation::AtomicGroupTake { record },
                ..
            } => Some((false, count(*record))),
            _ => None,
        })
        .collect::<Vec<_>>();
    takes.sort_unstable_by_key(|take| (take.1, take.0));
    takes.dedup();
    takes
}

/// A statement over two `Alpha` objects and, when `later`, three `Beta`
/// objects, a group after the first in the type order.
fn grouped(later: bool) -> String {
    let (targets, block) = if later {
        (
            ", r = &third, s = &fourth, t = &fifth",
            "\n    set r^.m = s^.m;\n    set t^.m = 3_u64;",
        )
    } else {
        ("", "")
    };
    format!(
        r#"struct Alpha {{
  n: u64;
}}

struct Beta {{
  m: u64;
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let a0 = Alpha(n: 0_u64);
  let a1 = Alpha(n: 1_u64);
  let b0 = Beta(m: 0_u64);
  let b1 = Beta(m: 1_u64);
  let b2 = Beta(m: 2_u64);
  let first = shared_new::<Alpha>(value: a0);
  let second = shared_new::<Alpha>(value: a1);
  let third = shared_new::<Beta>(value: b0);
  let fourth = shared_new::<Beta>(value: b1);
  let fifth = shared_new::<Beta>(value: b2);
  atomic p = &first, q = &second{targets} {{
    set p^.n = q^.n;{block}
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

#[test]
fn the_first_group_parks_and_a_later_group_does_not() {
    with_ir(grouped(false).as_bytes(), |program| {
        assert_eq!(group_takes(program), [(true, 2)]);
    });
    with_ir(grouped(true).as_bytes(), |program| {
        assert_eq!(group_takes(program), [(true, 2), (false, 3)]);
    });
}

#[test]
fn map_first_counts_one_boundary_before_taking_any_target() {
    with_ir(
        include_bytes!("../../../../tests/conformance/cases/share-pos-shards-beside-map-entry.wf"),
        |program| {
            let lookup = function(program, "lookup");
            let operations = lookup
                .blocks()
                .iter()
                .flat_map(IrBlock::instructions)
                .filter_map(|instruction| match instruction {
                    IrInstruction::Define { operation, .. } => Some(operation),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                operations
                    .iter()
                    .filter(|operation| matches!(operation, IrOperation::AtomicPass))
                    .count(),
                1
            );
            assert!(!operations.iter().any(|operation| matches!(
                operation,
                IrOperation::SharedAcquire { .. } | IrOperation::AtomicGroupAcquire { .. }
            )));
            let pass = operations
                .iter()
                .position(|operation| matches!(operation, IrOperation::AtomicPass))
                .unwrap();
            let entry = operations
                .iter()
                .position(|operation| matches!(operation, IrOperation::TableLockEntry { .. }))
                .unwrap();
            let shard = operations
                .iter()
                .position(|operation| matches!(operation, IrOperation::ShardsTake { .. }))
                .unwrap();
            assert!(pass < entry && entry < shard);
        },
    );
    with_ir(
        include_bytes!("../../../../tests/conformance/cases/share-pos-map-two-handles-one-entry.wf"),
        |program| {
            assert_eq!(group_takes(program), [(false, 2)]);
            let operations = function(program, "main")
                .blocks()
                .iter()
                .flat_map(IrBlock::instructions)
                .filter_map(|instruction| match instruction {
                    IrInstruction::Define { operation, .. } => Some(operation),
                    _ => None,
                })
                .collect::<Vec<_>>();
            // The eager grouped statement counts once before either target.
            // Later whole-map uses have separate static take sites behind
            // one lazy held flag; they need not have one static pass site.
            let grouped = function(program, "main")
                .blocks()
                .iter()
                .find(|block| {
                    block.instructions().iter().any(|instruction| {
                        matches!(
                            instruction,
                            IrInstruction::Define {
                                operation: IrOperation::AtomicGroupTake { .. },
                                ..
                            }
                        )
                    })
                })
                .expect("the fixture has one eager grouped map take");
            let passes = grouped
                .instructions()
                .iter()
                .enumerate()
                .filter_map(|(index, instruction)| {
                    matches!(
                        instruction,
                        IrInstruction::Define {
                            operation: IrOperation::AtomicPass,
                            ..
                        }
                    )
                    .then_some(index)
                })
                .collect::<Vec<_>>();
            let [pass] = passes.as_slice() else {
                panic!("the grouped statement counts exactly one boundary");
            };
            let take = grouped
                .instructions()
                .iter()
                .position(|instruction| {
                    matches!(
                        instruction,
                        IrInstruction::Define {
                            operation: IrOperation::AtomicGroupTake { .. },
                            ..
                        }
                    )
                })
                .unwrap();
            assert!(*pass < take);
            assert!(!operations.iter().any(|operation| matches!(
                operation,
                IrOperation::SharedAcquire { .. } | IrOperation::AtomicGroupAcquire { .. }
            )));
        },
    );
}
