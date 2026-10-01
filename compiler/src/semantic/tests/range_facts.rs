//! Range facts [RANGE-1..RANGE-5] and the counted permission a holding
//! certificate grants [PAR-2].
//!
//! The `range*` conformance cases pin which programs are accepted and which
//! rule a rejection cites. These tests observe what the corpus cannot: that
//! a certificate is retained and admits exactly its writes to the counted
//! permission judgment, and that a rejection names the site, the open
//! conclusion or the access pair a writer has to repair [DIAG-1].

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::loop_permission::{LoopDenial, LoopVerdict};
use super::with_semantics;

/// A scatter through a left inverse: iteration k writes `out^[order^[k]]`,
/// and `inv` makes two iterations name two slots. `header` is the counted
/// header's canonical text, `extra` statements after the write.
fn scatter(header: &str, extra: &str) -> Vec<u8> {
    format!(
        "fn scatter(order: &[u64], pos: &[u64], out: &[u64]) -> result: unit reads(order), writes(out) contract {{
  requires pos^.len == out^.len;
  requires forall inv(k in 0_u64..order^.len) when order^[k] < out^.len: pos^[order^[k]] == k;
}} {{
  let count = order^.len;
  for ({header}) {{
    let e = order^[k];
    if e < out^.len {{
      set out^[e] = k;{extra}
    }}
  }}
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    )
    .into_bytes()
}

const PLAIN: &str = "k in 0_u64..count";

const APART: &str = "\n    k in 0_u64..count,\n    apart(i, j) {\n    }\n  ";

#[test]
fn a_holding_certificate_admits_its_write_to_the_counted_judgment() {
    let source = scatter(APART, "");
    with_semantics(&source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the certified scatter must check: {outcome:?}");
        };
        let function = program
            .data
            .executable_functions()
            .find(|function| function.name == "scatter")
            .expect("scatter is checked");
        let certified = &function.range_facts.certified;
        assert_eq!(certified.len(), 1, "{certified:?}");
        assert_eq!(certified[0].writes.len(), 1, "{certified:?}");
        let table = program
            .data
            .permission
            .named("scatter")
            .expect("scatter's permissions");
        assert_eq!(table.loops.len(), 1);
        assert_eq!(table.loops[0].verdict, LoopVerdict::PermittedEligible);
    });
}

#[test]
fn the_same_write_without_a_certificate_stays_sequential() {
    let source = scatter(PLAIN, "");
    with_semantics(&source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the uncertified scatter must check: {outcome:?}");
        };
        let function = program
            .data
            .executable_functions()
            .find(|function| function.name == "scatter")
            .expect("scatter is checked");
        assert!(function.range_facts.certified.is_empty());
        let table = program
            .data
            .permission
            .named("scatter")
            .expect("scatter's permissions");
        let LoopVerdict::Denied(denial) = &table.loops[0].verdict else {
            panic!(
                "a write at a stored index needs a certificate: {:?}",
                table.loops[0]
            );
        };
        // Condition 2: a write of storage another iteration may write.
        assert!(
            matches!(denial, LoopDenial::SharedWrite { .. }),
            "{denial:?}"
        );
    });
}

#[test]
fn an_unseparated_pair_names_both_accesses() {
    // The write of one iteration and the read of out^[0] by another may be
    // one element: nothing states that order never names slot 0.
    let source = scatter(
        APART,
        "\n      let first = out^[0_u64];\n      set out^[e] = first;",
    );
    with_semantics(&source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected a RANGE-5 rejection, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Range5);
        let SemanticIssueKind::UndischargedApart { pair, .. } = issue.kind() else {
            panic!(
                "expected an undischarged certificate, got {:?}",
                issue.kind()
            );
        };
        assert!(pair.contains("`set out^[e] = k;`"), "{pair}");
        assert!(pair.contains("`out^[0_u64]`"), "{pair}");
    });
}

#[test]
fn an_unpreserved_invariant_names_its_back_edge_and_open_conclusion() {
    // Without vals, an earlier slot of order may name the slot pos is
    // written at, so inv is not shown to survive the iteration.
    let source = b"fn build(n: u64) -> result: u64 pure contract {
  requires n <= 1000_u64;
  requires n >= 1_u64;
} {
  let order = box_array_filled::<u64>(count: n, value: 0_u64);
  let pos = box_array_filled::<u64>(count: n, value: 0_u64);
  let last = n - 1_u64;
  for (
    k in 0_u64..n,
    invariant forall inv(q in 0_u64..k): pos.inner[order.inner[q]] == q
  ) {
    let back = last - k;
    set order.inner[k] = back;
    set pos.inner[back] = k;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
";
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected a RANGE-3 rejection, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Range3);
        let SemanticIssueKind::UndischargedRangeFact {
            fact,
            site,
            missing,
            ..
        } = issue.kind()
        else {
            panic!(
                "expected an undischarged range fact, got {:?}",
                issue.kind()
            );
        };
        assert_eq!(fact, "inv");
        assert_eq!(*site, "a loop back edge");
        assert!(
            missing.contains("pos.inner[order.inner[q]] == q"),
            "{missing}"
        );
    });
}

#[test]
fn a_fact_owed_at_a_call_is_judged_in_the_caller() {
    // The callee's requirement holds of a fresh fill and fails once one
    // element is written with another value.
    let program = |value: &str| {
        format!(
            "fn first(cells: &[u64]) -> result: u64 reads(cells) contract {{
  requires forall zero(k in 0_u64..cells^.len): cells^[k] == 0_u64;
}} {{
  if 0_u64 < cells^.len {{
    let got = cells^[0_u64];
    return got;
  }}
  return 0_u64;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let cells = box_array_filled::<u64>(count: 8_u64, value: 0_u64);
  set cells.inner[3_u64] = {value};
  let got = first(cells: &cells.inner[0_u64..8_u64]);
  if got != 0_u64 {{
    return std::process::exit_status(code: 1_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"
        )
        .into_bytes()
    };
    with_semantics(&program("0_u64"), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "an unchanged fill keeps the fact: {outcome:?}"
        );
    });
    with_semantics(&program("7_u64"), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected a RANGE-3 rejection, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Range3);
        let SemanticIssueKind::UndischargedRangeFact { fact, site, .. } = issue.kind() else {
            panic!(
                "expected an undischarged range fact, got {:?}",
                issue.kind()
            );
        };
        assert_eq!(fact, "zero");
        assert_eq!(*site, "a call");
    });
}

#[test]
fn a_certificate_places_an_affine_write_beside_a_scattered_one() {
    // The certificate holds: writes at k are apart from each other, and the
    // writes at order^[k] lie at or above order^.len, above every k, and
    // apart by the left inverse. It places both writes, so both are
    // certified elements and the loop is permitted [PAR-2]; an affine write
    // the certificate did not place cannot arise, since it records every
    // write of storage that exists before the body.
    let source = b"fn split(order: &[u64], pos: &[u64], out: &[u64]) -> result: unit reads(order), writes(out) contract {
  requires pos^.len == out^.len;
  requires order^.len <= out^.len;
  requires forall inv(k in 0_u64..order^.len) when order^[k] < out^.len: pos^[order^[k]] == k;
  requires forall high(k in 0_u64..order^.len) when order^[k] < out^.len: order^[k] >= order^.len;
} {
  let count = order^.len;
  for (
    k in 0_u64..count,
    apart(i, j) {
    }
  ) {
    set out^[k] = 0_u64;
    let e = order^[k];
    if e < out^.len {
      set out^[e] = k;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
";
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the mixed scatter must check: {outcome:?}");
        };
        let function = program
            .data
            .executable_functions()
            .find(|function| function.name == "split")
            .expect("split is checked");
        let certified = &function.range_facts.certified;
        assert_eq!(certified.len(), 1);
        assert_eq!(certified[0].writes.len(), 2, "{certified:?}");
        let table = program
            .data
            .permission
            .named("split")
            .expect("split's permissions");
        assert_eq!(table.loops[0].verdict, LoopVerdict::PermittedEligible);
    });
}

/// `depth` counted loops nested around one call owing `positive`, which
/// the zero fill does not meet.
fn nest(depth: usize) -> Vec<u8> {
    let mut body =
        String::from("let got = positive(cells: &cells.inner[0_u64..4_u64]);\nset seen = got;\n");
    for level in (0..depth).rev() {
        body = format!("for (k{level} in 0_u64..2_u64) {{\n{body}}}\n");
    }
    let indented: String = body
        .lines()
        .scan(1_usize, |depth, line| {
            if line.starts_with('}') {
                *depth -= 1;
            }
            let rendered = format!("{}{line}\n", "  ".repeat(*depth));
            if line.ends_with('{') {
                *depth += 1;
            }
            Some(rendered)
        })
        .collect();
    format!(
        "fn positive(cells: &[u64]) -> result: u64 reads(cells) contract {{
  requires forall pos(k in 0_u64..cells^.len): cells^[k] > 0_u64;
}} {{
  if 0_u64 < cells^.len {{
    let first = cells^[0_u64];
    return first;
  }}
  return 0_u64;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let cells = box_array_filled::<u64>(count: 4_u64, value: 0_u64);
  let seen = 0_u64;
{indented}  if seen != 0_u64 {{
    return std::process::exit_status(code: 1_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"
    )
    .into_bytes()
}

#[test]
fn a_loop_nest_deeper_than_the_checker_follows_is_unsupported_not_rejected() {
    // Within the depth the walk follows, the zero fill refutes the
    // requirement; beyond it the outer header forgets everything, and what
    // it then leaves unproved might hold by RANGE-2, so the verdict is the
    // checker's capability.
    with_semantics(&nest(3), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected a RANGE-3 rejection, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Range3);
    });
    with_semantics(&nest(10), |outcome| {
        let SemanticOutcome::Unsupported { unsupported, .. } = outcome else {
            panic!("expected an unsupported capability, got {outcome:?}");
        };
        assert_eq!(
            unsupported.feature(),
            crate::UnsupportedSemanticFeature::RangeLoopNesting
        );
    });
}
