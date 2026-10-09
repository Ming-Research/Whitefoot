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
    // certified elements and the loop is permitted [PAR-2].
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

/// One tree level of an inherited pass, as Snowghost's real style stage
/// would write it: each listed element writes its own `sizes` entry and,
/// when it owns a custom set, replaces its `sets` entry, an owned entry
/// list, while it reads its parent's size and, through a reference to that
/// one element, the set of the owner of `source`, the binding `parent` or
/// `element`. `owned` places an owner at or above its element's depth.
fn owned_entries(source: &str) -> Vec<u8> {
    format!(
        "nocopy struct Custom {{
  entries: Box<Slots<u64>>;
}}

fn entry_count(list: &Custom) -> result: u64 reads(list) {{
  let count = list^.entries.inner.len;
  return count;
}}

fn own_set(inherited: u64, element: u64) -> result: Custom pure {{
  let entries = box_slots_new::<u64>(capacity: 1_u64);
  let first = element +wrap inherited;
  place_back(window: &entries.inner, value: first);
  return Custom(entries: move entries);
}}

fn inherit_level(slots: &[u64], positions: &[u64], depths: &[u64], parents: &[u64], owners: &[u64], sets: &[Custom], sizes: &[u64], level: u64) -> result: unit reads(slots), reads(parents), reads(owners), writes(sets), writes(sizes) contract {{
  requires parents^.len == sets^.len;
  requires positions^.len == sets^.len;
  requires depths^.len == sets^.len;
  requires owners^.len == sets^.len;
  requires sizes^.len == sets^.len;
  requires forall listed(k in 0_u64..slots^.len) when slots^[k] < sets^.len: positions^[slots^[k]] == k, depths^[slots^[k]] == level;
  requires forall up(e in 0_u64..parents^.len) when parents^[e] < parents^.len: depths^[parents^[e]] + 1_u64 == depths^[e];
  requires forall owned(e in 0_u64..owners^.len) when owners^[e] < owners^.len: depths^[owners^[e]] <= depths^[e];
}} {{
  let count = slots^.len;
  let total = sets^.len;
  for (
    k in 0_u64..count,
    apart(i, j) {{
    }}
  ) {{
    let element = slots^[k];
    if element < total {{
      let parent = parents^[element];
      let inherited = if parent < total {{
        let source = owners^[{source}];
        if source < total {{
          give entry_count(list: &sets^[source]);
        }} else {{
          give 0_u64;
        }}
      }} else {{
        give 0_u64;
      }}
      let above = if parent < total {{
        give sizes^[parent];
      }} else {{
        give 16_u64;
      }}
      let own = owners^[element];
      if own == element {{
        let made = own_set(inherited: inherited, element: element);
        set sets^[element] = move made;
      }}
      set sizes^[element] = above +wrap 1_u64;
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

#[test]
fn a_certificate_places_owned_elements_of_two_roots_and_an_ancestor_read() {
    // Two roots, an owned entry list replaced by `move` and an element
    // reference handed to a reading helper: the parent's owner lies at most
    // at depth level - 1, so `owned`, `up` and `listed` separate its read
    // from every write at depth level, and both writes are certified [PAR-2].
    with_semantics(&owned_entries("parent"), |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the owned level must check: {outcome:?}");
        };
        let function = program
            .data
            .executable_functions()
            .find(|function| function.name == "inherit_level")
            .expect("inherit_level is checked");
        let certified = &function.range_facts.certified;
        assert_eq!(certified.len(), 1);
        assert_eq!(certified[0].writes.len(), 2, "{certified:?}");
        let table = program
            .data
            .permission
            .named("inherit_level")
            .expect("inherit_level's permissions");
        assert_eq!(table.loops[0].verdict, LoopVerdict::PermittedEligible);
    });
    // The element's own owner may lie at depth level, another iteration's
    // element: the reference read and that write stay unseparated.
    with_semantics(&owned_entries("element"), |outcome| {
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
        assert!(pair.contains("`set sets^[element] = move made;`"), "{pair}");
        assert!(
            pair.contains("`entry_count(list: &sets^[source])`"),
            "{pair}"
        );
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

/// One counted loop writing `out^[k]` under an empty certificate, with
/// `extra` before the write and `callee` beside `fill`.
fn certified_fill(callee: &str, extra: &str) -> Vec<u8> {
    format!(
        "{callee}fn fill(out: &[u64]) -> result: unit writes(out) {{
  let n = out^.len;
  let off = False();
  for (
    k in 0_u64..n,
    apart(i, j) {{
    }}
  ) {{
{extra}    set out^[k] = 1_u64;
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

fn fill_verdict(source: &[u8]) -> LoopVerdict {
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the fill must check: {outcome:?}");
        };
        let table = program
            .data
            .permission
            .named("fill")
            .expect("fill's permissions");
        table.loops[0].verdict.clone()
    })
}

#[test]
fn a_read_the_certificate_skipped_keeps_the_loop_sequential() {
    // The read of out^[0_u64] sits in an arm the entry state excludes, so
    // the certificate's walk never records it while the permission survey
    // sees it: PAR-2 admits only reads the certificate placed.
    let dead = "    if off {\n      let x = out^[0_u64];\n      let y = x +wrap 1_u64;\n    }\n";
    let LoopVerdict::Denied(denial) = fill_verdict(&certified_fill("", dead)) else {
        panic!("an unplaced read of the certified root must deny");
    };
    assert!(
        matches!(denial, LoopDenial::SharedWrite { .. }),
        "{denial:?}"
    );
    assert_eq!(
        fill_verdict(&certified_fill("", "")),
        LoopVerdict::PermittedEligible
    );
}

#[test]
fn a_reference_argument_the_callee_never_touches_is_no_access() {
    // [RANGE-5] only an argument the callee's row reads or writes is an
    // access; here the run of every written element is passed to a callee
    // whose row is pure, and the certificate still holds.
    let callee = "fn ignore(r: &[u64], v: u64) -> result: u64 pure {\n  return v;\n}\n\n";
    let call = "    let y = ignore(r: &out^[0_u64..n], v: k);\n";
    assert_eq!(
        fill_verdict(&certified_fill(callee, call)),
        LoopVerdict::PermittedEligible
    );
}

#[test]
fn a_derivation_past_the_checkers_arithmetic_is_unsupported_not_rejected() {
    // Coefficients near 2^64 make each elimination step's products pass
    // i128; the specified arithmetic is exact, so the verdict is the
    // checker's capability [RANGE-3].
    let source = b"fn scaled(cells: &[u64]) -> result: unit reads(cells) contract {
  requires forall big(a in 0_u64..cells^.len, b in 0_u64..cells^.len): 18446744073709551615_u64 * cells^[a] <= 18446744073709551614_u64 * cells^[b];
} {
  if 0_u64 < cells^.len {
    let first = cells^[0_u64];
  }
  return unit;
}

fn pass(cells: &[u64]) -> result: unit reads(cells) contract {
  requires forall small(a in 0_u64..cells^.len, b in 0_u64..cells^.len): 18446744073709551613_u64 * cells^[a] <= 18446744073709551612_u64 * cells^[b];
} {
  scaled(cells: cells);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
";
    with_semantics(source, |outcome| {
        let SemanticOutcome::Unsupported { unsupported, .. } = outcome else {
            panic!("expected an unsupported capability, got {outcome:?}");
        };
        assert_eq!(
            unsupported.feature(),
            crate::UnsupportedSemanticFeature::RangeArithmetic
        );
    });
}

/// A loop whose write to `cells` waits behind a chain of `flags` flags,
/// each set an iteration after the one before, so each dry walk of the
/// header reaches one more of them. `header` opens the loop; an ordinary
/// `loop` ends its body with a `break`.
fn flag_chain(flags: usize, header: &str) -> Vec<u8> {
    let mut lets = String::new();
    let mut steps = format!(
        "    if f{flags} {{
      set cells.inner[0_u64] = 5_u64;
    }}
"
    );
    for flag in 1..=flags {
        lets.push_str(&format!(
            "  let f{flag} = False();
"
        ));
    }
    for flag in (1..flags).rev() {
        let next = flag + 1;
        steps.push_str(&format!(
            "    if f{flag} {{
      set f{next} = True();
    }}
"
        ));
    }
    steps.push_str(
        "    set f1 = True();
",
    );
    let close = if header.starts_with("  loop") {
        "    break;\n"
    } else {
        ""
    };
    format!(
        "fn zeros(cells: &[u64]) -> result: u64 reads(cells) contract {{
  requires forall zero(k in 0_u64..cells^.len): cells^[k] == 0_u64;
}} {{
  if 0_u64 < cells^.len {{
    let first = cells^[0_u64];
    return first;
  }}
  return 0_u64;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let cells = box_array_filled::<u64>(count: 4_u64, value: 0_u64);
{lets}  let seen = 0_u64;
{header}
    let got = zeros(cells: &cells.inner[0_u64..4_u64]);
    set seen = got;
{steps}{close}  }}
  if seen != 0_u64 {{
    return std::process::exit_status(code: 1_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"
    )
    .into_bytes()
}

#[test]
fn a_header_that_does_not_settle_is_unsupported_at_its_loop() {
    // Six flags settle within the walks a header takes, and the write they
    // guard refutes the requirement; twelve do not, the header forgets
    // everything, and the verdict is the checker's capability, cited at
    // the loop: a counted loop, one whose certificate is walked first, and
    // an ordinary loop.
    const COUNTED: &str = "  for (i in 0_u64..16_u64) {";
    const CERTIFIED: &str = "  for (\n    i in 0_u64..16_u64,\n    apart(p, q) {\n    }\n  ) {";
    const ORDINARY: &str = "  loop {";
    with_semantics(&flag_chain(6, COUNTED), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected a RANGE-3 rejection, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Range3);
    });
    for (header, opening) in [
        (COUNTED, b"for (".as_slice()),
        (CERTIFIED, b"for (".as_slice()),
        (ORDINARY, b"loop {".as_slice()),
    ] {
        let source = flag_chain(12, header);
        with_semantics(&source, |outcome| {
            let SemanticOutcome::Unsupported { unsupported, .. } = outcome else {
                panic!("expected an unsupported capability, got {outcome:?}");
            };
            assert_eq!(
                unsupported.feature(),
                crate::UnsupportedSemanticFeature::RangeLoopNesting
            );
            let crate::SemanticLocation::SourceNode(_, coordinate) = &unsupported.node;
            let start = usize::try_from(coordinate.start().value()).expect("offset fits");
            assert!(
                source[start..].starts_with(opening),
                "cited at the loop: {header}"
            );
        });
    }
}

/// A call owing `zero` after `reads` guards, each on its own cell of
/// `cells`, with the caller's own requirement `known` active; every guarded
/// read selects one value of `known`'s first binder. Unpaired, `zero` is
/// owed over `cells`, and `known`'s instance at the owed read proves it.
/// Paired, `zero` is owed over `other`, which the requirement `held` proves,
/// and `known` has a second binder over `spare`, whose one read no condition
/// names, so no read in the problem selects a value for it.
fn guarded_reads(reads: u64, paired: bool) -> Vec<u8> {
    let (parameters, known, unnamed, owed) = if paired {
        (
            ", other: &[u64], spare: &[u64]) -> result: u64 reads(cells), reads(other), reads(spare)",
            "requires forall known(k in 0_u64..cells^.len, j in 0_u64..spare^.len): cells^[k] == spare^[j];
  requires forall held(k in 0_u64..other^.len): other^[k] == 0_u64;",
            "  if 0_u64 < spare^.len {
    let seen = spare^[0_u64];
  }
",
            "other",
        )
    } else {
        (
            ") -> result: u64 reads(cells)",
            "requires forall known(k in 0_u64..cells^.len): cells^[k] == 0_u64;",
            "",
            "cells",
        )
    };
    let mut guards = String::new();
    for cell in 0..reads {
        guards.push_str(&format!(
            "  if cells^[{cell}_u64] > 5_u64 {{
    return 1_u64;
  }}
"
        ));
    }
    format!(
        "fn zeros(cells: &[u64]) -> result: u64 reads(cells) contract {{
  requires forall zero(k in 0_u64..cells^.len): cells^[k] == 0_u64;
}} {{
  if 0_u64 < cells^.len {{
    let first = cells^[0_u64];
    return first;
  }}
  return 0_u64;
}}

fn probe(cells: &[u64]{parameters} contract {{
  requires cells^.len == 400_u64;
  {known}
}} {{
{guards}{unnamed}  let got = zeros(cells: {owed});
  return got;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    )
    .into_bytes()
}

/// The conclusion a RANGE-3 rejection of `source` names, or `None` when
/// the program is accepted.
fn undischarged(source: &[u8]) -> Option<String> {
    with_semantics(source, |outcome| {
        let issue = match outcome {
            SemanticOutcome::Complete(_) => return None,
            SemanticOutcome::SourceIssue { issue, .. } => issue,
            other => panic!("expected acceptance or a RANGE-3 rejection, got {other:?}"),
        };
        assert_eq!(issue.rule(), SemanticRule::Range3);
        let SemanticIssueKind::UndischargedRangeFact { fact, missing, .. } = issue.kind() else {
            panic!(
                "expected an undischarged range fact, got {:?}",
                issue.kind()
            );
        };
        assert_eq!(fact, "zero");
        Some(missing.clone())
    })
}

#[test]
fn the_instance_ceiling_counts_the_instances_a_fact_forms() {
    // 200 guarded cells and the owed read select 201 values of `known`'s
    // binder, within the 256 instances one fact may form, and its instance
    // at the owed read proves `zero`; 300 select 301, past the ceiling.
    assert_eq!(undischarged(&guarded_reads(200, false)), None);
    let past = undischarged(&guarded_reads(300, false)).expect("a RANGE-3 rejection");
    assert!(
        past.contains("256 instances"),
        "the ceiling is named: {past}"
    );
    // With a second binder no read selects, `known` forms no instance at
    // all, however many values its first binder has, so it reaches no
    // ceiling, and `held` proves `zero` over `other`.
    assert_eq!(undischarged(&guarded_reads(300, true)), None);
}

/// A producer owing `cleared` over the run it writes, which `fault` leaves
/// before the loop clears the run: at the return when `early` is false, and
/// through a propagated error exit before it when `early` is true.
fn unproved_postcondition(early: bool) -> Vec<u8> {
    let exit = if early {
        "  let y = propagate step(x: x);
"
    } else {
        ""
    };
    let tail = if early {
        "  return Ok<i32, StepError>(value: y);"
    } else {
        "  return Ok<i32, StepError>(value: x);"
    };
    format!(
        "enum StepError {{
  Negative();
}}

fn step(x: i32) -> result: Result<i32, StepError> pure {{
  if x < 0_i32 {{
    let negative = StepError::Negative();
    return Err<i32, StepError>(error: negative);
  }}
  return Ok<i32, StepError>(value: x);
}}

fn fault(x: i32, out: &[u64]) -> result: Result<i32, StepError> writes(out) contract {{
  ensures forall cleared(k in 0_u64..out^.len): out^[k] == 0_u64;
}} {{
{exit}  if 0_u64 < out^.len {{
    set out^[0_u64] = 1_u64;
  }}
{tail}
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    )
    .into_bytes()
}

#[test]
fn an_unproved_postcondition_names_the_exit_that_owes_it() {
    for (early, site) in [(false, "a return"), (true, "a propagated error exit")] {
        with_semantics(&unproved_postcondition(early), |outcome| {
            let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("expected a RANGE-3 rejection, got {outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Range3);
            let SemanticIssueKind::UndischargedRangeFact {
                fact,
                site: named,
                missing,
                ..
            } = issue.kind()
            else {
                panic!(
                    "expected an undischarged range fact, got {:?}",
                    issue.kind()
                );
            };
            assert_eq!(fact, "cleared");
            assert_eq!(*named, site);
            assert!(missing.contains("out^[k] == 0_u64"), "{missing}");
        });
    }
}

// These fixtures exercise the selected range-field proposal before its
// specification amendment; they do not change conformance verdicts.
fn field_range_program(body: &str) -> Vec<u8> {
    format!(
        "{body}\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
    ).into_bytes()
}

fn field_range_verdict(source: &[u8], rule: Option<SemanticRule>) {
    with_semantics(source, |outcome| match (rule, outcome) {
        (None, SemanticOutcome::Complete(_)) => {}
        (Some(expected), SemanticOutcome::SourceIssue { issue, .. }) => {
            assert_eq!(issue.rule(), expected, "{issue:?}");
        }
        (Some(expected), SemanticOutcome::Complete(_)) => {
            panic!("expected {expected:?}, got acceptance")
        }
        (expected, outcome) => panic!("expected {expected:?}, got {outcome:?}"),
    });
}

fn field_range_scatter(before: &str, order_effect: &str, read: &str) -> Vec<u8> {
    let before = if before.is_empty() {
        String::new()
    } else {
        format!("{before}\n")
    };
    let read = if read.is_empty() {
        String::new()
    } else {
        format!("{read}\n")
    };
    field_range_program(&format!(
        "struct Block {{
  entry_slot: u64;
  normal_y: i64;
}}

fn need(order: &[u64], targets: &[Block]) -> result: unit pure contract {{
  requires forall inverse(k in 0_u64..order^.len) when order^[k] < targets^.len: targets^[order^[k]].entry_slot == k;
}} {{
  return unit;
}}

fn scatter(order: &[u64], targets: &[Block], unknown: u64) -> result: unit {order_effect}, writes(targets) contract {{
  requires forall inv(k in 0_u64..order^.len) when order^[k] < targets^.len: targets^[order^[k]].entry_slot == k;
}} {{
{before}  let count = order^.len;
  for (
    k in 0_u64..count,
    apart(i, j) {{
    }}
  ) {{
    let at = order^[k];
    if at < targets^.len {{
{read}      set targets^[at].normal_y = 0_i64;
    }}
  }}
  return unit;
}}
"
    ))
}

#[test]
fn field_range_scatter_certifies_sibling_write_and_field_read() {
    let source = field_range_scatter(
        "",
        "reads(order)",
        "      let slot = targets^[at].entry_slot;",
    );
    with_semantics(&source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("field scatter must check: {outcome:?}");
        };
        let function = program
            .data
            .executable_functions()
            .find(|f| f.name == "scatter")
            .unwrap();
        let certified = &function.range_facts.certified;
        assert_eq!(certified.len(), 1);
        assert_eq!(certified[0].writes.len(), 1);
        assert_eq!(certified[0].reads.len(), 1);
        let table = program.data.permission.named("scatter").unwrap();
        assert_eq!(table.loops[0].verdict, LoopVerdict::PermittedEligible);
    });
}

#[test]
fn field_range_sibling_write_preserves_inverse_at_call() {
    let source = field_range_scatter(
        "  if 0_u64 < targets^.len {\n    set targets^[0_u64].normal_y = 9_i64;\n  }\n  need(order: order, targets: targets);",
        "reads(order)",
        "",
    );
    field_range_verdict(&source, None);
}

#[test]
fn field_range_unknown_inverse_write_rejects_at_range3() {
    let source = field_range_scatter(
        "  if 0_u64 < targets^.len {\n    set targets^[0_u64].entry_slot = unknown;\n  }\n  need(order: order, targets: targets);",
        "reads(order)",
        "",
    );
    field_range_verdict(&source, Some(SemanticRule::Range3));
}

#[test]
fn field_range_changed_order_rejects_old_inverse_at_range5() {
    let source = field_range_scatter(
        "  if 1_u64 < order^.len {\n    let first = order^[0_u64];\n    set order^[1_u64] = first;\n  }",
        "writes(order)",
        "",
    );
    field_range_verdict(&source, Some(SemanticRule::Range5));
}

#[test]
fn field_range_congruence_never_equates_different_fields() {
    let source = field_range_program(
        "struct Pair {\n  f: u64;\n  g: u64;\n}\n
fn need_equal(rows: &[Pair]) -> result: unit pure contract {
  requires forall equal(k in 0_u64..rows^.len): rows^[k].f == rows^[k].g;
} {
  return unit;
}

fn forward(rows: &[Pair]) -> result: unit pure {
  need_equal(rows: rows);
  return unit;
}
",
    );
    field_range_verdict(&source, Some(SemanticRule::Range3));
}

fn field_range_measure_source(before: &str, start: &str, measure: &str) -> Vec<u8> {
    let (before, effect) = if before.is_empty() {
        (String::new(), "pure")
    } else {
        (format!("{before}\n"), "writes(rows)")
    };
    field_range_program(&format!(
        "fn need_rows(rows: &[Slots<u8, 8>]) -> result: unit pure contract {{
  requires forall bounded(k in {start}..rows^.len): rows^[k].{measure} <= 4_u64;
}} {{
  return unit;
}}

fn forward(rows: &[Slots<u8, 8>]) -> result: unit {effect} contract {{
  requires forall held(k in {start}..rows^.len): rows^[k].{measure} <= 4_u64;
}} {{
{before}  need_rows(rows: rows);
  return unit;
}}
"
    ))
}

#[test]
fn field_range_element_length_passes_between_contracts() {
    field_range_verdict(&field_range_measure_source("", "0_u64", "len"), None);
}

#[test]
fn field_range_element_capacity_passes_between_contracts() {
    field_range_verdict(&field_range_measure_source("", "0_u64", "cap"), None);
}

const FIELD_RANGE_APPEND: &str = "  if 0_u64 < rows^.len {
    if rows^[0_u64].len < rows^[0_u64].cap {
      place_back(window: &rows^[0_u64], value: 0_u8);
    }
  }";

#[test]
fn field_range_descriptor_write_invalidates_written_element_measure() {
    field_range_verdict(
        &field_range_measure_source(FIELD_RANGE_APPEND, "0_u64", "len"),
        Some(SemanticRule::Range3),
    );
}

#[test]
fn field_range_descriptor_write_keeps_other_element_measures() {
    field_range_verdict(
        &field_range_measure_source(FIELD_RANGE_APPEND, "1_u64", "len"),
        None,
    );
}

#[test]
fn field_range_box_content_and_nested_fields_form_and_preserve_siblings() {
    let source = field_range_scatter(
        "  if 0_u64 < targets^.len {\n    set targets^[0_u64].normal_y = 9_i64;\n  }\n  need(order: order, targets: targets);",
        "reads(order)",
        "",
    );
    let source = String::from_utf8(source)
        .unwrap()
        .replace(
            "struct Block {",
            "struct Header {\n  entry_slot: u64;\n}\n\nstruct Block {",
        )
        .replacen(
            "  entry_slot: u64;\n  normal_y",
            "  header: Header;\n  normal_y",
            1,
        )
        .replace("&[Block]", "&[Box<Block>]")
        .replace("].entry_slot", "].inner.header.entry_slot")
        .replace("].normal_y", "].inner.normal_y");
    field_range_verdict(source.as_bytes(), None);
}

fn field_range_formation(declarations: &str, element: &str, suffix: &str) -> Vec<u8> {
    let declarations = if declarations.is_empty() {
        String::new()
    } else {
        format!("{declarations}\n")
    };
    field_range_program(&format!(
        "{declarations}fn inspect(rows: &[{element}]) -> result: unit pure contract {{
  requires forall held(k in 0_u64..rows^.len): rows^[k]{suffix} == 0_u64;
}} {{
  return unit;
}}
"
    ))
}

#[test]
fn field_range_enum_payload_forms() {
    let source = field_range_formation(
        "enum Entry {\n  Open(slot: u64);\n}\n",
        "Entry",
        ".Open.slot",
    );
    field_range_verdict(&source, None);
}

#[test]
fn field_range_noninteger_field_rejects_at_range1() {
    let source = field_range_formation("struct Entry {\n  flag: Bool;\n}\n", "Entry", ".flag");
    field_range_verdict(&source, Some(SemanticRule::Range1));
}

#[test]
fn field_range_subscript_of_scalar_rejects_at_range1() {
    let source = field_range_formation("", "u64", "[0_u64]");
    field_range_verdict(&source, Some(SemanticRule::Range1));
}

#[test]
fn generic_range_postcondition_is_judged_at_concrete_instances() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/range1-pos-generic-instance.wf");
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("integer and noninteger instances must check: {outcome:?}");
        };
        let instances = program
            .data
            .executable_functions()
            .filter(|function| function.name == "filled_with")
            .collect::<Vec<_>>();
        assert_eq!(instances.len(), 2);
        assert_eq!(
            instances
                .iter()
                .filter(|function| function
                    .range_facts
                    .postconditions
                    .iter()
                    .any(|post| post.owed && post.clause.name == "same"))
                .count(),
            1,
            "only the integer instance owes the content postcondition"
        );
    });
}

#[test]
fn generic_noninteger_range_postcondition_owes_no_selected_exit() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/range1-pos-generic-unformed-postcondition.wf"
    );
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the noninteger instance owes no selected exit: {outcome:?}");
        };
        let instance = program
            .data
            .executable_functions()
            .find(|function| function.name == "first_of")
            .expect("first_of's concrete instance is checked");
        assert!(instance.range_facts.postconditions.is_empty());
    });
}

#[test]
fn generic_call_range_requirement_is_owed_at_the_integer_instance() {
    let source = |ty: &str, value: &str| {
        format!(
            "fn require_same<T: copy>(values: &[T], value: T) -> result: unit pure contract {{
  requires forall same(k in 0_u64..values^.len): values^[k] == value;
}} {{
  return unit;
}}

fn forward<T: copy>(values: &[T], value: T) -> result: unit pure {{
  require_same::<T>(values: values, value: value);
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let value = {value};
  let cells = box_array_filled::<{ty}>(count: 2_u64, value: value);
  forward::<{ty}>(values: &cells.inner[0_u64..2_u64], value: value);
  return std::process::exit_status(code: 0_u8);
}}
"
        )
    };
    with_semantics(source("Bool", "True()").as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "a noninteger instance owes no range requirement: {outcome:?}"
        );
    });
    with_semantics(source("u64", "7_u64").as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the integer forward instance owes the requirement: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Range3);
        let SemanticIssueKind::UndischargedRangeFact { fact, site, .. } = issue.kind() else {
            panic!("expected an undischarged range requirement: {issue:?}");
        };
        assert_eq!(fact, "same");
        assert_eq!(*site, "a call");
    });
}

#[test]
fn field_range_generic_declared_leaf_forms_and_noninteger_instance_is_empty() {
    let source = field_range_program(
        "struct Record<T> {
  value: T;
}

fn accept<T>(rows: &[Record<T>]) -> result: unit pure contract {
  requires forall same(k in 0_u64..rows^.len): rows^[k].value == rows^[k].value;
} {
  return unit;
}

fn forward(numbers: &[Record<u64>], flags: &[Record<Bool>]) -> result: unit pure {
  accept::<u64>(rows: numbers);
  accept::<Bool>(rows: flags);
  return unit;
}
",
    );
    field_range_verdict(&source, None);
}

#[test]
fn field_range_generic_cannot_select_undeclared_field() {
    let source = field_range_program(
        "fn inspect<T>(rows: &[T]) -> result: unit pure contract {
  requires forall held(k in 0_u64..rows^.len): rows^[k].missing == 0_u64;
} {
  return unit;
}
",
    );
    field_range_verdict(&source, Some(SemanticRule::Range1));
}

#[test]
fn field_range_whole_replacement_uses_known_construction_fields() {
    let source = field_range_program(
        "nocopy struct Pair {
  f: u64;
  g: u64;
}

fn need_zero(rows: &[Pair]) -> result: unit pure contract {
  requires forall zero(k in 0_u64..rows^.len): rows^[k].f == 0_u64;
} {
  return unit;
}

fn forward(rows: &[Pair]) -> result: unit writes(rows) contract {
  requires forall held(k in 0_u64..rows^.len): rows^[k].f == 0_u64;
} {
  if 0_u64 < rows^.len {
    let made = Pair(f: 0_u64, g: 7_u64);
    set rows^[0_u64] = move made;
  }
  need_zero(rows: rows);
  return unit;
}
",
    );
    field_range_verdict(&source, None);
}

#[test]
fn field_range_whole_replacement_forgets_unknown_fields() {
    let source = field_range_program(
        "struct Pair {
  f: u64;
  g: u64;
}

fn need_zero(rows: &[Pair]) -> result: unit pure contract {
  requires forall zero(k in 0_u64..rows^.len): rows^[k].f == 0_u64;
} {
  return unit;
}

fn forward(rows: &[Pair], replacement: Pair) -> result: unit writes(rows) contract {
  requires forall held(k in 0_u64..rows^.len): rows^[k].f == 0_u64;
} {
  if 0_u64 < rows^.len {
    set rows^[0_u64] = replacement;
  }
  need_zero(rows: rows);
  return unit;
}
",
    );
    field_range_verdict(&source, Some(SemanticRule::Range3));
}

#[test]
fn field_range_segments_preserve_projection_after_two_indices() {
    let source = field_range_program(
        "struct Pair {
  f: u64;
  g: u64;
}

fn need_segments(rows: &Segments<Pair>) -> result: unit pure contract {
  requires forall zero(d in 0_u64..rows^.len, k in 0_u64..rows^[d].len): rows^[d][k].f == 0_u64;
} {
  return unit;
}

fn forward(rows: &Segments<Pair>) -> result: unit writes(rows) contract {
  requires forall held(d in 0_u64..rows^.len, k in 0_u64..rows^[d].len): rows^[d][k].f == 0_u64;
} {
  if 0_u64 < rows^.len {
    let first = &rows^[0_u64];
    if 0_u64 < first^.len {
      set first^[0_u64].g = 1_u64;
    }
  }
  need_segments(rows: rows);
  return unit;
}
",
    );
    field_range_verdict(&source, None);
}

#[test]
fn field_range_array_element_has_length_but_no_capacity() {
    let source = field_range_formation("", "Array<u8, 8>", ".len");
    field_range_verdict(&source, None);
    let source = field_range_formation("", "Array<u8, 8>", ".cap");
    field_range_verdict(&source, Some(SemanticRule::Range1));
}

fn field_range_enum_scatter(multi: bool, after: bool, wrong: bool) -> Vec<u8> {
    let variants = if multi || wrong {
        "  Close(block: u32);\n  Child(context: u32);\n  Float(context: u32);\n"
    } else {
        ""
    };
    let mut facts = String::from(
        "  requires forall inv(k in first..order^.len) when order^[k].Open.block < targets^.len: targets^[order^[k].Open.block].entry_slot == k;\n",
    );
    if multi {
        for (name, field, store) in [
            ("Close", "block", "targets"),
            ("Child", "context", "children"),
            ("Float", "context", "children"),
        ] {
            let fact_name = name.to_lowercase();
            facts.push_str(&format!("  requires forall inv_{fact_name}(k in first..order^.len) when order^[k].{name}.{field} < {store}^.len: {store}^[order^[k].{name}.{field}].entry_slot == k;\n"));
        }
    }
    let mut arms = String::new();
    for (name, field, store) in [
        ("Open", "block", "targets"),
        ("Close", "block", "targets"),
        ("Child", "context", "children"),
        ("Float", "context", "children"),
    ] {
        if name != "Open" && !multi && !wrong {
            continue;
        }
        arms.push_str(&format!("      {name}({field}: b) => {{\n"));
        if multi || (!wrong && name == "Open") || (wrong && name == "Close") {
            arms.push_str(&format!("        let at = cvt::<u32, u64>(b);\n        if at < {store}^.len {{\n          set {store}^[at].normal_y = {store}^[at].normal_y +sat delta;\n        }}\n"));
        }
        arms.push_str("      }\n");
    }
    let extra_param = if multi { ", children: &[Block]" } else { "" };
    let extra_effect = if multi { "), writes(children" } else { "" };
    let extra_arg = if multi { ", children: children" } else { "" };
    let tail = if after {
        format!("  need(order: order, targets: targets, first: first{extra_arg});\n")
    } else {
        String::new()
    };
    field_range_program(&format!(
        "struct Block {{\n  entry_slot: u32;\n  normal_y: i32;\n}}\n\nenum Flow {{\n  Open(block: u32);\n{variants}}}\n\nfn need(order: &[Flow], targets: &[Block], first: u64{extra_param}) -> result: unit pure contract {{\n{facts}}} {{\n  return unit;\n}}\n\nfn translate_owner_suffix(order: &[Flow], targets: &[Block], first: u64, delta: i32{extra_param}) -> result: unit reads(order), writes(targets{extra_effect}) contract {{\n{facts}}} {{\n  let count = order^.len;\n  for (\n    k in first..count,\n    apart(i, j) {{\n    }}\n  ) {{\n    let item = order^[k];\n    match item {{\n{arms}    }}\n  }}\n{tail}  return unit;\n}}\n"
    ))
}

fn field_range_assert_enum_certificate(source: &[u8], writes: usize) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("enum scatter must check: {outcome:?}");
        };
        let function = program
            .data
            .executable_functions()
            .find(|f| f.name == "translate_owner_suffix")
            .unwrap();
        assert_eq!(function.range_facts.certified.len(), 1);
        assert_eq!(function.range_facts.certified[0].writes.len(), writes);
        assert_eq!(
            program
                .data
                .permission
                .named("translate_owner_suffix")
                .unwrap()
                .loops[0]
                .verdict,
            LoopVerdict::PermittedEligible
        );
    });
}

#[test]
fn field_range_enum_snowghost_certificate_and_permission() {
    field_range_assert_enum_certificate(&field_range_enum_scatter(false, false, false), 1);
}

/// Snowghost-wf `research/investigations/m2-edit-cost/inverse-proof/natural.wf`
/// at 552dcbf, verbatim: the stored inverse reported as the gap.
const FIELD_RANGE_SNOWGHOST_NATURAL: &str = r#"struct Block {
  entry_slot: u32;
  normal_y: i32;
}

enum Flow {
  Open(block: u32);
}

fn translate_owner_suffix(order: &[Flow], targets: &[Block], first: u64, delta: i32) -> result: unit reads(order), writes(targets) contract {
  requires forall inv(k in first..order^.len) when order^[k].Open.block < targets^.len: targets^[order^[k].Open.block].entry_slot == k;
} {
  let count = order^.len;
  for (
    k in first..count,
    apart(i, j) {
    }
  ) {
    let item = order^[k];
    match item {
      Open(block: b) => {
        let at = cvt::<u32, u64>(b);
        if at < targets^.len {
          set targets^[at].normal_y = targets^[at].normal_y +sat delta;
        }
      }
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn field_range_snowghost_natural_witness_is_certified() {
    field_range_assert_enum_certificate(FIELD_RANGE_SNOWGHOST_NATURAL.as_bytes(), 1);
}

#[test]
fn field_range_enum_shared_targets_across_variants() {
    field_range_assert_enum_certificate(&field_range_enum_scatter(true, false, false), 4);
}

#[test]
fn field_range_enum_inverse_survives_projected_loop_header() {
    field_range_assert_enum_certificate(&field_range_enum_scatter(false, true, false), 1);
}

#[test]
fn field_range_enum_wrong_variant_rejects_at_range5() {
    field_range_verdict(
        &field_range_enum_scatter(false, false, true),
        Some(SemanticRule::Range5),
    );
}

#[test]
fn field_range_enum_unknown_variant_cannot_supply_unconditional_fact() {
    let source = field_range_program(
        "enum Flow {\n  Open(block: u32);\n  Close(block: u32);\n}\n\nfn need(targets: &[u32]) -> result: unit pure contract {\n  requires forall wanted(k in 0_u64..targets^.len): targets^[k] == k;\n} {\n  return unit;\n}\n\nfn check(order: &[Flow], targets: &[u32]) -> result: unit pure contract {\n  requires forall inv(k in 0_u64..targets^.len) when order^[k].Open.block == order^[k].Open.block: targets^[k] == k;\n} {\n  need(targets: targets);\n  return unit;\n}\n",
    );
    field_range_verdict(&source, Some(SemanticRule::Range3));
}

fn field_range_enum_copy_source_write(expected: u32) -> Vec<u8> {
    field_range_program(&format!(
        "enum Flow {{\n  Open(block: u32);\n}}\n\nfn need(order: &[Flow], expected: u32) -> result: unit pure contract {{\n  requires forall wanted(k in 0_u64..order^.len): order^[k].Open.block == expected;\n}} {{\n  return unit;\n}}\n\nfn check(order: &[Flow]) -> result: unit writes(order) contract {{\n  requires order^.len == 1_u64;\n}} {{\n  set order^[0_u64] = Flow::Open(block: 3_u32);\n  let item = order^[0_u64];\n  set order^[0_u64] = Flow::Open(block: 9_u32);\n  match item {{\n    Open(block: b) => {{\n      set order^[0_u64] = Flow::Open(block: b);\n    }}\n  }}\n  need(order: order, expected: {expected}_u32);\n  return unit;\n}}\n"
    ))
}

#[test]
fn field_range_enum_copy_keeps_old_payload_after_source_write() {
    field_range_verdict(&field_range_enum_copy_source_write(3), None);
}

#[test]
fn field_range_enum_copy_does_not_acquire_new_payload() {
    field_range_verdict(
        &field_range_enum_copy_source_write(9),
        Some(SemanticRule::Range3),
    );
}

#[test]
fn field_range_enum_replaced_variant_cannot_reuse_old_domain() {
    let source = field_range_program(
        r#"enum Flow {
  Open(block: u32);
  Close(block: u32);
}

fn need(order: &[Flow], saved: u32) -> result: unit pure contract {
  requires forall wanted(k in 0_u64..order^.len): order^[k].Open.block == saved;
} {
  return unit;
}

fn check(order: &[Flow]) -> result: unit writes(order) contract {
  requires order^.len == 1_u64;
  requires forall inv(k in 0_u64..order^.len): order^[k].Open.block == 3_u32;
} {
  set order^[0_u64] = Flow::Close(block: 9_u32);
  let current = order^[0_u64];
  match current {
    Open(block: b) => {
    }
    Close(block: b) => {
      set order^[0_u64] = Flow::Open(block: b);
    }
  }
  need(order: order, saved: 3_u32);
  return unit;
}
"#,
    );
    field_range_verdict(&source, Some(SemanticRule::Range3));
}

#[test]
fn field_range_enum_inactive_requirement_is_vacuous_after_replacement() {
    let source = String::from_utf8(field_range_enum_copy_source_write(3)).unwrap()
        .replace("  Open(block: u32);", "  Open(block: u32);\n  Close(block: u32);")
        .replace("  let item = order^[0_u64];\n  set order^[0_u64] = Flow::Open(block: 9_u32);\n  match item {\n    Open(block: b) => {\n      set order^[0_u64] = Flow::Open(block: b);\n    }\n  }", "  set order^[0_u64] = Flow::Close(block: 9_u32);");
    field_range_verdict(source.as_bytes(), None);
}

#[test]
fn field_range_loop_overlapping_projection_is_forgotten() {
    let source = String::from_utf8(field_range_enum_scatter(false, true, false))
        .unwrap()
        .replace(
            "set targets^[at].normal_y = targets^[at].normal_y +sat delta;",
            "set targets^[at].entry_slot = 0_u32;",
        );
    field_range_verdict(source.as_bytes(), Some(SemanticRule::Range3));
}

#[test]
fn field_range_enum_aggregate_copy_sites_keep_payload_definition() {
    let source = field_range_program(
        r#"struct Payload {
  slot: u32;
}

enum Flow {
  Open(block: Payload);
}

struct Holder {
  value: Payload;
}

fn need(order: &[Flow]) -> result: unit pure contract {
  requires forall wanted(k in 0_u64..order^.len): order^[k].Open.block.slot == 3_u32;
} {
  return unit;
}

fn check(order: &[Flow]) -> result: unit writes(order) contract {
  requires order^.len == 1_u64;
} {
  let initial = Payload(slot: 3_u32);
  set order^[0_u64] = Flow::Open(block: initial);
  let item = order^[0_u64];
  let saved = match item {
    Open(block: b) => {
      give b;
    }
  }
  let held = Holder(value: saved);
  let target = Payload(slot: 9_u32);
  set target = held.value;
  set order^[0_u64] = Flow::Open(block: target);
  need(order: order);
  return unit;
}
"#,
    );
    field_range_verdict(&source, None);
}

#[test]
fn field_range_struct_cannot_ignore_variant_qualifier() {
    let source = field_range_formation("struct Entry {\n  slot: u64;\n}\n", "Entry", ".Open.slot");
    field_range_verdict(&source, Some(SemanticRule::Range1));
}

#[test]
fn field_range_enum_missing_shared_store_variant_fact_rejects() {
    let source = String::from_utf8(field_range_enum_scatter(true, false, false)).unwrap();
    let source = source
        .lines()
        .filter(|line| !line.contains("requires forall inv_float("))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    field_range_verdict(source.as_bytes(), Some(SemanticRule::Range5));
}

fn field_range_copy_join_source(body: &str) -> Vec<u8> {
    field_range_program(&format!(
        "struct Entry {{\n  slot: u32;\n}}\n\nfn need(rows: &[Entry]) -> result: unit pure contract {{\n  requires forall wanted(k in 0_u64..rows^.len): rows^[k].slot == 3_u32;\n}} {{\n  return unit;\n}}\n\nfn check(rows: &[Entry], flag: Bool) -> result: unit writes(rows) contract {{\n  requires rows^.len == 1_u64;\n}} {{\n  set rows^[0_u64] = Entry(slot: 3_u32);\n  let item = rows^[0_u64];\n{body}  need(rows: rows);\n  return unit;\n}}\n"
    ))
}

#[test]
fn field_range_copy_join_never_resurrects_written_field() {
    for branches in [
        "  if flag {\n    set item.slot = 9_u32;\n  }\n",
        "  if flag {\n  } else {\n    set item.slot = 9_u32;\n  }\n",
    ] {
        let source =
            field_range_copy_join_source(&format!("{branches}  set rows^[0_u64] = item;\n"));
        field_range_verdict(&source, Some(SemanticRule::Range3));
    }
}

#[test]
fn field_range_give_join_keeps_aggregate_copy_provenance() {
    let source = field_range_copy_join_source(
        "  let saved = if flag {\n    give item;\n  } else {\n    give item;\n  }\n  set rows^[0_u64] = saved;\n",
    );
    field_range_verdict(&source, None);
}

#[test]
fn field_range_give_join_tracks_each_arms_copy() {
    let source = field_range_copy_join_source(
        "  let saved = if flag {\n    set item.slot = 9_u32;\n    give item;\n  } else {\n    give item;\n  }\n  set rows^[0_u64] = saved;\n",
    );
    field_range_verdict(&source, Some(SemanticRule::Range3));
}

#[test]
fn field_range_copy_join_keeps_nested_alias_overrides() {
    let source = String::from_utf8(field_range_copy_join_source("  let held = Holder(value: item);\n  if flag {\n    set item.slot = 9_u32;\n    set held.value = item;\n  }\n  set rows^[0_u64] = held.value;\n")).unwrap()
        .replace("fn need(", "struct Holder {\n  value: Entry;\n}\n\nfn need(");
    field_range_verdict(source.as_bytes(), Some(SemanticRule::Range3));
}
