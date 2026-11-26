//! Corners of the split advice for a loop refused for carrying several
//! accumulators. The reference is the sequential meaning of the loop [FN-1]:
//! a hand-written split combines the halves' accumulators per accumulator,
//! which reproduces the sequential result exactly when no accumulator's
//! running value is observed anywhere but by its own update. Every shape here
//! observes it through a different form (a call argument, a reference, a
//! subscript guard, a nested loop's endpoint, a Boolean condition) or through
//! none, and the advice must follow that, not the spelling.

use super::*;

/// The advice of the one loop of `main`, refused by condition one for carrying
/// `accumulators` accumulators.
fn advice_of_many(source: &[u8], accumulators: usize) -> LoopPermission {
    let table = permission_of(source);
    let judged = only_loop(&table, "main").clone();
    let LoopDenial::ManyAccumulators {
        accumulators: found,
    } = denial(&judged, 1)
    else {
        panic!("expected an accumulator-count denial: {:?}", judged.verdict);
    };
    assert_eq!(*found, accumulators);
    judged
}

fn outside_reads_of(judged: &LoopPermission) -> usize {
    match &judged.split_advice {
        SplitAdvice::DependentAccumulator { outside_reads, .. } => *outside_reads,
        other => panic!("expected a dependent accumulator, got {other:?}"),
    }
}

/// The accumulator passes through a pure call on its way to the other one.
#[test]
fn an_accumulator_passed_to_a_call_is_a_dependent_accumulator() {
    let source = b"fn twice(x: u64) -> result: u64 pure {
  return x *wrap 2_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  for @call (i in 0_u64..16_u64) {
    set first = first +sat i;
    let seen = twice(x: first);
    set second = second +sat seen;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let judged = advice_of_many(source, 2);
    assert_eq!(outside_reads_of(&judged), 1);
}

/// A reference formed to the running value observes it as much as a copy.
#[test]
fn an_accumulator_read_through_a_reference_is_a_dependent_accumulator() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  for @view (i in 0_u64..16_u64) {
    set first = first +sat i;
    let view = &first;
    let seen = view^;
    set second = second +sat seen;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let judged = advice_of_many(source, 2);
    assert!(outside_reads_of(&judged) >= 1);
}

/// Each read of the running value counts: two reads in one local are two.
#[test]
fn each_read_outside_the_update_is_counted() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  for @twice (i in 0_u64..16_u64) {
    set first = first +sat i;
    let doubled = first +wrap first;
    set second = second +sat doubled;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let judged = advice_of_many(source, 2);
    assert_eq!(outside_reads_of(&judged), 2);
}

/// A guarded subscript names the running value twice, in its guard and in the
/// subscript; neither is an update of it.
#[test]
fn an_accumulator_used_as_a_guarded_subscript_is_a_dependent_accumulator() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let table = array_filled::<u64, 64>(value: 1_u64);
  let first = 0_u64;
  let second = 0_u64;
  for @pick (i in 0_u64..16_u64) {
    set first = first +sat i;
    let inside = first < 64_u64;
    if inside {
      let picked = table[first];
      set second = second +sat picked;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let judged = advice_of_many(source, 2);
    assert!(outside_reads_of(&judged) >= 2);
}

/// A Boolean accumulator observed by a branch condition is read outside its
/// update, whatever its operation.
#[test]
fn a_boolean_accumulator_read_by_a_branch_is_a_dependent_accumulator() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let seen = False();
  let count = 0_u64;
  for @flag (i in 0_u64..16_u64) {
    let low = iand(i, 1_u64);
    let bit = low == 0_u64;
    set seen = bor(seen, bit);
    if seen {
      set count = count +sat 1_u64;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let judged = advice_of_many(source, 2);
    assert_eq!(judged.combines, vec!["bor", "+sat"]);
    assert_eq!(outside_reads_of(&judged), 1);
}

/// Nested loops: the inner loop's updates are the outer loop's, so both
/// loops carry the dependence and neither may be advised.
#[test]
fn a_dependence_inside_a_nested_loop_is_dependent_for_both_loops() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  for @rows (r in 0_u64..4_u64) {
    for @cols (c in 0_u64..4_u64) {
      set first = first +sat c;
      set second = second +sat first;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let table = permission_of(source);
    let judged = loops(&table, "main");
    assert_eq!(judged.len(), 2);
    for item in judged {
        assert!(
            matches!(denial(item, 1), LoopDenial::ManyAccumulators { .. }),
            "{:?}",
            item.verdict
        );
        assert_eq!(outside_reads_of(item), 1);
    }
}

/// Nested loops with independent accumulators keep the advice at both depths:
/// the nesting adds no read of any running value.
#[test]
fn independent_accumulators_in_a_nested_loop_keep_the_advice_for_both_loops() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  for @rows (r in 0_u64..4_u64) {
    for @cols (c in 0_u64..4_u64) {
      set first = first +sat c;
      set second = second +sat r;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let table = permission_of(source);
    let judged = loops(&table, "main");
    assert_eq!(judged.len(), 2);
    for item in judged {
        assert!(matches!(
            denial(item, 1),
            LoopDenial::ManyAccumulators { accumulators: 2 }
        ));
        assert_eq!(item.split_advice, SplitAdvice::Eligible);
    }
}

/// The inner loop's endpoint is the outer accumulator's running value: the
/// outer loop is dependent, the inner loop (one accumulator, which nothing
/// else reads) is permitted and needs no advice.
#[test]
fn an_inner_endpoint_reading_an_outer_accumulator_is_dependent_only_for_the_outer_loop() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  for @rows (r in 0_u64..4_u64) {
    set first = first +sat 1_u64;
    for @cols (c in 0_u64..first) {
      set second = second +sat c;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let table = permission_of(source);
    let judged = loops(&table, "main");
    assert_eq!(judged.len(), 2);
    assert!(matches!(
        denial(judged[0], 1),
        LoopDenial::ManyAccumulators { accumulators: 2 }
    ));
    assert_eq!(outside_reads_of(judged[0]), 1);
    assert_eq!(judged[1].verdict, LoopVerdict::PermittedEligible);
    assert_eq!(judged[1].split_advice, SplitAdvice::None);
}

/// A loop refused for a reason other than the accumulator count carries no
/// advice, whatever its accumulators read.
#[test]
fn a_loop_not_refused_for_its_accumulator_count_carries_no_advice() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  for @plain (i in 0_u64..16_u64) {
    set first = first +sat i;
    let seen = first;
    set second = seen;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let table = permission_of(source);
    let judged = only_loop(&table, "main");
    assert!(matches!(
        denial(judged, 1),
        LoopDenial::NotAReduction { .. }
    ));
    assert_eq!(judged.split_advice, SplitAdvice::None);
}
