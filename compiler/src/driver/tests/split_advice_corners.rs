//! Which accumulator the split hint names, and how it counts reads: the
//! reference is the sequential meaning of the loop, in which a split's halves
//! combine per accumulator only while nothing but its own update reads a
//! running value.

use super::ledger_of;

fn hints(ledger: &[String]) -> Vec<&String> {
    ledger
        .iter()
        .filter(|line| line.starts_with("PAR hint"))
        .collect()
}

/// The first accumulator in source order that is read outside its update is
/// the one named, even when an earlier accumulator is read by nobody.
#[test]
fn the_hint_names_the_first_accumulator_that_is_read_outside_its_update() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let alpha = 0_u64;
  let beta = 0_u64;
  let gamma = 0_u64;
  for @three (i in 0_u64..16_u64) {
    set alpha = alpha +sat i;
    set beta = beta +sat i;
    set gamma = gamma +sat beta;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let ledger = ledger_of("which.wf", source);
    let hints = hints(&ledger);
    assert_eq!(hints.len(), 1, "{ledger:#?}");
    assert!(
        hints[0].contains(
            "the accumulator of set beta = beta +sat i; is read 1 time(s) outside its own update"
        ),
        "{ledger:#?}"
    );
    assert!(
        !ledger.iter().any(|line| line.contains("would be eligible")),
        "{ledger:#?}"
    );
}

/// The count is of reads outside the update, not of reads in all.
#[test]
fn the_hint_counts_each_read_outside_the_update() {
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
    let ledger = ledger_of("twice.wf", source);
    let hints = hints(&ledger);
    assert_eq!(hints.len(), 1, "{ledger:#?}");
    assert!(
        hints[0].contains(
            "the accumulator of set first = first +sat i; is read 2 time(s) outside its own update"
        ),
        "{ledger:#?}"
    );
}

/// Each loop of a nest is judged on its own: both loops are refused for
/// their accumulators and both explain why, none says it would be eligible.
#[test]
fn a_nested_dependence_is_explained_at_each_loop() {
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
    let ledger = ledger_of("nest.wf", source);
    let hints = hints(&ledger);
    assert_eq!(hints.len(), 2, "{ledger:#?}");
    for hint in hints {
        assert!(hint.contains("would not combine under +sat"), "{hint}");
    }
    assert!(
        !ledger.iter().any(|line| line.contains("would be eligible")),
        "{ledger:#?}"
    );
}
