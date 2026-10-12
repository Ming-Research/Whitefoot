//! [SHARE-2] How a statement takes a type group, several targets of one
//! type (compiler/waiting-contexts/state-locks): its first group holds
//! nothing before it, so it waits as a first object's acquire does and a
//! guard's reservation can refuse it; a later group is taken by a holder of
//! an earlier object, which a reservation admits.

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
