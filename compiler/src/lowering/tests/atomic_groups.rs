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
fn a_proof_only_object_target_is_acquired_and_released_without_a_write() {
    let source = br#"struct Counter {
  count: u64;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let initial = Counter(count: 0_u64);
  let counter = shared_new::<Counter>(value: initial);
  atomic cell = &counter {
    invariant nonnegative: 0_u64 <= cell^.count;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let mut acquired = Vec::new();
        let mut released = Vec::new();
        for instruction in function(program, "main")
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
        {
            match instruction {
                IrInstruction::Define {
                    operation: IrOperation::SharedAcquire { object },
                    ..
                } => acquired.push(*object),
                IrInstruction::Define {
                    operation: IrOperation::SharedUnlock { object, wrote },
                    ..
                } => released.push((*object, *wrote)),
                _ => {}
            }
        }
        assert_eq!(acquired.len(), 1, "proof erasure must not erase the take");
        assert_eq!(released, [(acquired[0], false)]);
    });
}

#[test]
fn a_proof_only_key_set_target_keeps_its_descriptor_and_hold() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/share2-pos-binding-named-only-in-invariant-stmt.wf"
    );
    with_ir(source, |program| {
        let mut filled = Vec::new();
        let mut acquired = Vec::new();
        let mut released = Vec::new();
        for instruction in function(program, "main")
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
        {
            match instruction {
                IrInstruction::Define {
                    operation: IrOperation::EntriesFill { hold, .. },
                    ..
                } => filled.push(*hold),
                IrInstruction::Define {
                    operation: IrOperation::TableHoldTake { record },
                    ..
                } => acquired.push(*record),
                IrInstruction::Define {
                    operation: IrOperation::TableHoldRelease { record, .. },
                    ..
                } => released.push(*record),
                _ => {}
            }
        }
        assert_eq!(
            filled.len(),
            1,
            "the target still forms its Entries descriptor"
        );
        assert_eq!(acquired, filled);
        assert_eq!(released, acquired);
    });
}
