//! [SHARE-2] How a statement takes a type group, several targets of one
//! type (compiler/waiting-contexts/state-locks): its first group holds
//! nothing before it, so it waits as a first object's acquire does and a
//! guard's reservation can refuse it; a later group is taken by a holder of
//! an earlier object, which a reservation admits. Each target of a group's
//! record says whether the statement may write it, so the group's release
//! gives up an object it only reads without a sweep [SHARE-3].

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

/// The write flag of each target of the two-target group in `main`, by its
/// position in the record, each site of the statement's paths once.
fn group_write_flags(program: &IrProgram) -> Vec<(u32, bool)> {
    let mut flags = function(program, "main")
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
        .filter_map(|instruction| match instruction {
            IrInstruction::Define {
                operation:
                    IrOperation::AtomicGroupTarget {
                        index, wrote, ..
                    },
                ..
            } => Some((*index, *wrote)),
            _ => None,
        })
        .collect::<Vec<_>>();
    flags.sort_unstable();
    flags.dedup();
    flags
}

/// A statement over two `Alpha` objects whose block is `block`.
fn two_alphas(block: &str) -> String {
    format!(
        r#"struct Alpha {{
  n: u64;
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let a0 = Alpha(n: 0_u64);
  let a1 = Alpha(n: 1_u64);
  let first = shared_new::<Alpha>(value: a0);
  let second = shared_new::<Alpha>(value: a1);
  let seen = 0_u64;
  atomic p = &first, q = &second {{
    {block}
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

#[test]
fn a_group_target_says_whether_the_statement_may_write_it() {
    // Reading both: neither is released as written.
    with_ir(two_alphas("set seen = p^.n +wrap q^.n;").as_bytes(), |program| {
        assert_eq!(group_write_flags(program), [(0, false), (1, false)]);
    });
    // Writing p: two handles of one type may name one object, so the
    // checker counts a write through p as a possible write of q's object.
    with_ir(two_alphas("set p^.n = q^.n;").as_bytes(), |program| {
        assert_eq!(group_write_flags(program), [(0, true), (1, true)]);
    });
}
