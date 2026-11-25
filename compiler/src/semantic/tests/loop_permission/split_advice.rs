//! The split advice of a loop refused for carrying several accumulators:
//! eligible while every accumulator is read only by its own updates, and a
//! dependent-accumulator explanation once one is read anywhere else.

use super::*;

/// The advice of the one loop of `main`, asserted to be refused by condition
/// one for carrying `accumulators` accumulators.
fn advice_of_many(source: &[u8], accumulators: usize) -> LoopPermission {
    let table = permission_of(source);
    let judged = only_loop(&table, "main").clone();
    let LoopDenial::ManyAccumulators { accumulators: found } = denial(&judged, 1) else {
        panic!("expected an accumulator-count denial: {:?}", judged.verdict);
    };
    assert_eq!(*found, accumulators);
    judged
}

/// The item's shape: `entries` adds the running total of `contexts`, so the
/// result of one half depends on the other's, and no combination of the
/// halves under `+sat` reproduces it.
#[test]
fn an_increment_reading_another_accumulator_is_a_dependent_accumulator() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let contexts = 0_u64;
  let paragraphs = 0_u64;
  let entries = 0_u64;
  for @dependent (k in 0_u64..16_u64) {
    set contexts = contexts +sat k;
    set paragraphs = paragraphs +sat 1_u64;
    set entries = entries +sat contexts;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let judged = advice_of_many(source, 3);
    assert_eq!(judged.combines, vec!["+sat"]);
    assert!(
        matches!(
            judged.split_advice,
            SplitAdvice::DependentAccumulator {
                outside_reads: 1,
                ..
            }
        ),
        "{:?}",
        judged.split_advice
    );
}

/// A local that copies the other accumulator carries the same dependence.
#[test]
fn an_increment_reading_another_accumulator_through_a_local_is_a_dependent_accumulator() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  let third = 0_u64;
  for @dependent (i in 0_u64..16_u64) {
    set first = first +sat i;
    let seen = first;
    set second = second +sat seen;
    set third = third +sat i;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let judged = advice_of_many(source, 3);
    assert!(
        matches!(
            judged.split_advice,
            SplitAdvice::DependentAccumulator {
                outside_reads: 1,
                ..
            }
        ),
        "{:?}",
        judged.split_advice
    );
}

/// Observing an accumulator in a condition is a read outside its update too.
#[test]
fn an_accumulator_read_in_a_condition_is_a_dependent_accumulator() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  for @dependent (i in 0_u64..16_u64) {
    set first = first +sat i;
    let big = 3_u64 < first;
    if big {
      set second = second +sat 1_u64;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let judged = advice_of_many(source, 2);
    assert!(
        matches!(
            judged.split_advice,
            SplitAdvice::DependentAccumulator {
                outside_reads: 1,
                ..
            }
        ),
        "{:?}",
        judged.split_advice
    );
}

/// The item's `sum_natural`: each accumulator adds one field of its own
/// element and nothing reads another, so combining the halves per accumulator
/// is the sequential result and the advice stays.
#[test]
fn accumulators_read_only_by_their_own_updates_keep_the_eligible_advice() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let contexts = 0_u64;
  let paragraphs = 0_u64;
  let entries = 0_u64;
  for @natural (k in 0_u64..16_u64) {
    set contexts = contexts +sat k;
    set paragraphs = paragraphs +sat 1_u64;
    set entries = entries +sat k;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let judged = advice_of_many(source, 3);
    assert_eq!(judged.combines, vec!["+sat"]);
    assert_eq!(judged.split_advice, SplitAdvice::Eligible);
}

/// One accumulator updated in both branches of an `if`, read nowhere else,
/// is read once per update and not observed between them: the halves still
/// combine, so the advice stays.
#[test]
fn an_accumulator_with_two_updates_nothing_else_reads_keeps_the_eligible_advice() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  for @branches (i in 0_u64..16_u64) {
    let low = iand(i, 1_u64);
    let odd = low == 1_u64;
    if odd {
      set first = first +sat i;
    } else {
      set first = first +sat 1_u64;
    }
    set second = second +sat i;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let judged = advice_of_many(source, 2);
    assert_eq!(judged.split_advice, SplitAdvice::Eligible);
}
