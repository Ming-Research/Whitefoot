//! The ledger's split advice for a loop refused because it carries several
//! accumulators: given when the halves of a hand-written split could be
//! combined, withheld when an accumulator is read outside its own update.

use super::ledger_of;

/// The item's `sum_dependent`: `entries` adds the running total of
/// `contexts`, so two halves cannot be added under `+sat`.
const SUM_DEPENDENT: &[u8] = b"struct Counts {
  contexts: u64;
  paragraphs: u64;
  entries: u64;
}

fn sum_dependent(results: &[Counts]) -> counts: Counts reads(results) {
  let count = results^.len;
  let contexts = 0_u64;
  let paragraphs = 0_u64;
  let entries = 0_u64;
  for (k in 0_u64..count) {
    let got = results^[k];
    set contexts = contexts +sat got.contexts;
    set paragraphs = paragraphs +sat got.paragraphs;
    set entries = entries +sat contexts;
  }
  return Counts(contexts: contexts, paragraphs: paragraphs, entries: entries);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
";

/// The item's `sum_natural`: each accumulator adds one field of its own
/// element and none reads another.
const SUM_NATURAL: &[u8] = b"struct Counts {
  contexts: u64;
  paragraphs: u64;
  entries: u64;
}

fn sum_natural(results: &[Counts]) -> counts: Counts reads(results) {
  let count = results^.len;
  let contexts = 0_u64;
  let paragraphs = 0_u64;
  let entries = 0_u64;
  for (k in 0_u64..count) {
    let got = results^[k];
    set contexts = contexts +sat got.contexts;
    set paragraphs = paragraphs +sat got.paragraphs;
    set entries = entries +sat got.entries;
  }
  return Counts(contexts: contexts, paragraphs: paragraphs, entries: entries);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
";

/// The same increment reaching the other accumulator through a local.
const DEPENDENT_THROUGH_A_LOCAL: &[u8] = b"fn main() -> status: std::process::ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  let third = 0_u64;
  for @dep (i in 0_u64..16_u64) {
    set first = first +sat i;
    let seen = first;
    set second = second +sat seen;
    set third = third +sat i;
  }
  return std::process::exit_status(code: 0_u8);
}
";

fn denied_by_three_accumulators(ledger: &[String]) {
    assert!(
        ledger.iter().any(|line| line.starts_with("PAR loop")
            && line.contains("denied")
            && line.contains("condition 1: the body carries 3 accumulators")),
        "the loop must be refused for its accumulators: {ledger:#?}"
    );
}

/// The one hint line of a ledger, which must follow the loop's denial line.
fn only_hint(ledger: &[String]) -> &String {
    let hints: Vec<&String> = ledger
        .iter()
        .filter(|line| line.starts_with("PAR hint"))
        .collect();
    assert_eq!(hints.len(), 1, "{ledger:#?}");
    hints[0]
}

#[test]
fn a_loop_whose_increment_reads_another_accumulator_gets_no_eligible_split_hint() {
    let ledger = ledger_of("sum_dependent.wf", SUM_DEPENDENT);
    denied_by_three_accumulators(&ledger);
    assert!(
        !ledger.iter().any(|line| line.contains("would be eligible")),
        "{ledger:#?}"
    );
    // The explanation names the accumulator that is read outside its update
    // and says why the halves cannot be added.
    assert_eq!(
        only_hint(&ledger),
        "PAR hint        sum_dependent.wf:12  loop  refused by condition 1; \
         a recursive split over its index range would not combine under +sat: \
         the accumulator of set contexts = contexts +sat got.contexts; is read \
         1 time(s) outside its own update, so the result of one half depends \
         on the other half's partial value",
        "{ledger:#?}"
    );
}

/// Accumulators that only their own updates read keep the hint, byte for
/// byte, beside the loop's denial line.
#[test]
fn a_loop_whose_accumulators_are_independent_keeps_the_eligible_split_hint() {
    let ledger = ledger_of("sum_natural.wf", SUM_NATURAL);
    denied_by_three_accumulators(&ledger);
    assert_eq!(
        only_hint(&ledger),
        "PAR hint        sum_natural.wf:12  loop  refused by condition 1; \
         a recursive split over its index range would be eligible, \
         combining under +sat",
        "{ledger:#?}"
    );
}

#[test]
fn an_increment_reading_another_accumulator_through_a_local_gets_no_eligible_split_hint() {
    let ledger = ledger_of("dependent_local.wf", DEPENDENT_THROUGH_A_LOCAL);
    denied_by_three_accumulators(&ledger);
    assert!(
        !ledger.iter().any(|line| line.contains("would be eligible")),
        "{ledger:#?}"
    );
}
