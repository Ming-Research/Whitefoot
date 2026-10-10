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

/// The same scatter over run references into `Paged` storage [REF-4]: the
/// elements of a run are integer storage terms exactly as a range's are
/// [RANGE-1], so the certificate places its writes alike [PAR-2].
fn run_scatter(header: &str) -> Vec<u8> {
    String::from_utf8(scatter(header, ""))
        .expect("scatter source is UTF-8")
        .replacen(
            "fn scatter(order: &[u64], pos: &[u64], out: &[u64])",
            "fn scatter(order: &Run<u64>, pos: &Run<u64>, out: &Run<u64>)",
            1,
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
fn a_callee_relation_over_an_entry_image_is_not_taken_as_a_range_fact() {
    // `place_back`'s relation `window^.len == entry(window)^.len + 1` names
    // an entry image, which no range term reads [RANGE-1]. Read as a range
    // fact over the current length it is `len == len + 1`, a contradiction
    // after which the caller proved every owed range fact, including this
    // false one.
    let source = b"fn need_sevens(xs: &Slots<u64, 8>) -> result: unit pure contract {
  requires forall seven(k in 0_u64..xs^.len): xs^[k] == 7_u64;
} {
  return unit;
}

fn extend_by_five(xs: &Slots<u64, 8>) -> result: unit writes(xs) contract {
  requires forall small(k in 0_u64..xs^.len): xs^[k] < 100_u64;
} {
  if xs^.len < xs^.cap {
    place_back(window: xs, value: 5_u64);
    need_sevens(xs: xs);
  }
  return unit;
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
        let SemanticIssueKind::UndischargedRangeFact { fact, .. } = issue.kind() else {
            panic!(
                "expected an undischarged range fact, got {:?}",
                issue.kind()
            );
        };
        assert_eq!(fact, "seven");
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

#[test]
fn the_instance_ceiling_counts_both_rounds_without_recounting_duplicates() {
    // Each first-round read cells[c] instantiates relay at c, whose read
    // cells[next[c]] selects one new value of each binder in round two.
    // 127 guarded reads plus the owed read give 128 tuples in round one
    // and 256 in their union with round two; 128 guards give 258 instead.
    for reads in [127, 128] {
        let source = String::from_utf8(guarded_reads(reads, false))
            .unwrap()
            .replacen(
                "fn probe(cells: &[u64]) -> result: u64 reads(cells)",
                "fn probe(cells: &[u64], next: &[u64]) -> result: u64 reads(cells), reads(next)",
                1,
            )
            .replacen(
                "requires forall known(k in 0_u64..cells^.len): cells^[k] == 0_u64;",
                "requires forall known(k in 0_u64..cells^.len): cells^[k] == 0_u64;
  requires next^.len == cells^.len;
  requires forall relay(k in 0_u64..cells^.len) when next^[k] < cells^.len: cells^[next^[k]] == cells^[k];",
                1,
            )
            .replacen(
                "  let got = zeros(",
                "  if 0_u64 < next^.len {
    let hop = next^[0_u64];
  }
  let got = zeros(",
                1,
            );
        assert!(source.contains("forall relay(") && source.contains("let hop = next^[0_u64];"));
        let outcome = undischarged(source.as_bytes());
        if reads == 127 {
            assert_eq!(outcome, None);
        } else {
            let missing = outcome.expect("the second round exceeds the instance ceiling");
            assert!(missing.contains("256 instances"), "{missing}");
        }
    }
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

// Wrap range-field fixtures in a complete program with a trivial entry point.
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
            2,
            "integer and copy aggregate instances both owe the content postcondition"
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
fn generic_call_range_requirement_is_owed_at_integer_and_copy_aggregate_instances() {
    let source = |ty: &str, value: &str, comparison: &str| {
        format!(
            "struct Block {{
  entry_slot: u64;
}}

fn require_same<T: copy>(values: &[T], value: T) -> result: unit pure contract {{
  requires forall same(k in 0_u64..values^.len): values^[k] {comparison} value;
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
    // The forwarding body has no requirement of its own. The integer,
    // prelude Bool enum and copy Block instances each owe the equality.
    for (ty, value) in [
        ("u64", "7_u64"),
        ("Bool", "True()"),
        ("Block", "Block(entry_slot: 7_u64)"),
    ] {
        let source = source(ty, value, "==");
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("the {ty} forward instance owes the requirement: {outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Range3);
            let SemanticIssueKind::UndischargedRangeFact { fact, site, .. } = issue.kind() else {
                panic!("expected an undischarged range requirement: {issue:?}");
            };
            assert_eq!(fact, "same");
            assert_eq!(*site, "a call");
            let crate::SemanticLocation::SourceNode(_, coordinate) = issue.location();
            let start = usize::try_from(coordinate.start().value()).unwrap();
            assert!(
                source[start..].starts_with("require_same::<T>("),
                "{issue:?}"
            );
        });
    }
    // A non-equality over the same copy struct still states nothing at this
    // concrete instance; it must neither be owed nor become an active fact.
    with_semantics(
        source("Block", "Block(entry_slot: 7_u64)", "!=").as_bytes(),
        |outcome| {
            let SemanticOutcome::Complete(program) = outcome else {
                panic!("Block disequality owes no range requirement: {outcome:?}");
            };
            let instance = program
                .data
                .executable_functions()
                .find(|function| function.name == "require_same")
                .unwrap();
            assert!(instance.range_facts.requirements.is_empty());
        },
    );
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

/// A callee owing `small` over its first `n` elements, and a caller with
/// `parameters` and `body` that passes it a bound written through a
/// reference.
fn bound_through_reference(parameters: &str, effects: &str, body: &str) -> Vec<u8> {
    format!(
        "struct Holder {{
  value: u64;
}}

fn need(xs: &Array<u64, 4>, n: u64) -> result: unit pure contract {{
  requires forall small(k in 0_u64..n): xs^[k] < 4_u64;
}} {{
  return unit;
}}

fn caller(xs: &Array<u64, 4>{parameters}) -> result: unit {effects} {{
{body}  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    )
    .into_bytes()
}

/// Asserts that `source` is refused because `small` is not proved.
fn small_is_undischarged(source: &[u8]) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected a RANGE-3 rejection, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Range3);
        let SemanticIssueKind::UndischargedRangeFact { fact, .. } = issue.kind() else {
            panic!(
                "expected an undischarged range fact, got {:?}",
                issue.kind()
            );
        };
        assert_eq!(fact, "small");
    });
}

#[test]
fn a_bound_written_by_a_call_through_its_reference_is_forgotten() {
    // The callee writes `n` through the reference argument; the walk
    // cannot place that write in `n`, so it must forget `n` = 0, under
    // which `small` would hold vacuously.
    small_is_undischarged(
        b"fn need(xs: &Array<u64, 4>, n: u64) -> result: unit pure contract {
  requires forall small(k in 0_u64..n): xs^[k] < 4_u64;
} {
  return unit;
}

fn bump(x: &u64) -> result: unit writes(x) {
  set x^ = 4_u64;
  return unit;
}

fn caller(xs: &Array<u64, 4>) -> result: unit pure {
  let n = 0_u64;
  bump(x: &n);
  need(xs: xs, n: n);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
",
    );
}

#[test]
fn a_bound_written_through_a_reference_the_loop_body_takes() {
    // The first iteration sees `n` = 0, a later one what the previous one
    // wrote through `w`.
    small_is_undischarged(&bound_through_reference(
        "",
        "pure",
        "  let n = 0_u64;
  for (i in 0_u64..2_u64) {
    need(xs: xs, n: n);
    let w = &n;
    set w^ = 4_u64;
  }
",
    ));
}

#[test]
fn a_bound_written_through_a_reference_rebound_to_it_in_the_loop() {
    // `w` reaches `n` only from the second iteration on.
    small_is_undischarged(&bound_through_reference(
        "",
        "pure",
        "  let n = 0_u64;
  let other = 0_u64;
  let w = &other;
  for (i in 0_u64..2_u64) {
    need(xs: xs, n: n);
    set w^ = 4_u64;
    set w = &n;
  }
",
    ));
}

#[test]
fn a_bound_written_through_a_joined_reference() {
    small_is_undischarged(&bound_through_reference(
        ", flag: Bool",
        "pure",
        "  let other = 0_u64;
  let n = 0_u64;
  let p = if flag {
    give &other;
  } else {
    give &n;
  }
  set p^ = 4_u64;
  need(xs: xs, n: n);
",
    ));
}

#[test]
fn a_bound_written_through_a_reference_inside_an_atomic_statement() {
    small_is_undischarged(&bound_through_reference(
        ", state: Shared<Holder>",
        "pure waits",
        "  let n = 0_u64;
  let w = &n;
  atomic held = &state {
    set w^ = held^.value;
  }
  need(xs: xs, n: n);
",
    ));
}

#[test]
fn paged_page_count_remains_its_own_range_atom() {
    let source = b"fn bounded(p: &Paged<u64>) -> result: unit pure contract {\n  requires forall page(k in 0_u64..p^.pages.len) when k < p^.pages.len: k < p^.pages.len;\n} {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("Paged page-count range terms must check: {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "bounded")
            .expect("bounded function");
        let [clause] = function.range_facts.requirements.as_slice() else {
            panic!("one retained range clause");
        };
        let expected = &clause.binders[0].end;
        assert!(matches!(
            expected,
            super::super::range_facts::CheckedRangeTerm::Measure {
                measure: super::super::CheckedMeasure::Pages,
                ..
            }
        ));
        assert_eq!(&clause.guards[0].right, expected);
        assert_eq!(&clause.conclusions[0].right, expected);
    });
}

#[test]
fn a_certificate_admits_a_scatter_through_paged_runs() {
    for (header, certified) in [(APART, true), (PLAIN, false)] {
        let source = run_scatter(header);
        with_semantics(&source, |outcome| {
            let SemanticOutcome::Complete(program) = outcome else {
                panic!("the run scatter must check: {outcome:?}");
            };
            let function = program
                .data
                .executable_functions()
                .find(|function| function.name == "scatter")
                .expect("scatter is checked");
            assert_eq!(
                function.range_facts.certified.len(),
                usize::from(certified),
                "{:?}",
                function.range_facts.certified
            );
            let table = program
                .data
                .permission
                .named("scatter")
                .expect("scatter's permissions");
            assert_eq!(table.loops.len(), 1);
            if certified {
                assert_eq!(table.loops[0].verdict, LoopVerdict::PermittedEligible);
            } else {
                assert!(
                    matches!(
                        &table.loops[0].verdict,
                        LoopVerdict::Denied(LoopDenial::SharedWrite { .. })
                    ),
                    "{:?}",
                    table.loops[0]
                );
            }
        });
    }
}

#[test]
fn paged_bases_and_runs_preserve_element_field_certificates() {
    for (order, targets) in [
        ("&Paged<u64>", "&Paged<Block>"),
        ("&Run<u64>", "&Run<Block>"),
    ] {
        let source = String::from_utf8(field_range_scatter(
            "",
            "reads(order)",
            "      let slot = targets^[at].entry_slot;",
        ))
        .unwrap()
        .replace("&[u64]", order)
        .replace("&[Block]", targets);
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::Complete(program) = outcome else {
                panic!("Paged element projections must check: {outcome:?}");
            };
            let function = program
                .data
                .executable_functions()
                .find(|function| function.name == "scatter")
                .expect("scatter is checked");
            assert_eq!(function.range_facts.certified.len(), 1);
            assert_eq!(function.range_facts.certified[0].writes.len(), 1);
            assert_eq!(function.range_facts.certified[0].reads.len(), 1);
            let table = program.data.permission.named("scatter").unwrap();
            assert_eq!(table.loops[0].verdict, LoopVerdict::PermittedEligible);
        });
    }
}

#[test]
fn paged_element_page_counts_follow_length_writes_only_at_the_written_tuple() {
    for (before, start, expected) in [
        ("", "0_u64", None),
        (FIELD_RANGE_APPEND, "0_u64", Some(SemanticRule::Range3)),
        (FIELD_RANGE_APPEND, "1_u64", None),
        (
            "  if 0_u64 < rows^.len {\n    if 0_u64 < rows^[0_u64].len {\n      let removed = take_back(window: &rows^[0_u64]);\n    }\n  }",
            "0_u64",
            Some(SemanticRule::Range3),
        ),
        (
            "  if 0_u64 < rows^.len {\n    if 0_u64 < rows^[0_u64].len {\n      set rows^[0_u64][0_u64] = 0_u8;\n    }\n  }",
            "0_u64",
            None,
        ),
    ] {
        let source = String::from_utf8(field_range_measure_source(before, start, "pages.len"))
            .unwrap()
            .replace("Slots<u8, 8>", "Paged<u8>");
        field_range_verdict(source.as_bytes(), expected);
    }
}

#[test]
fn paged_range_element_measures_form_in_values_and_affine_proofs() {
    for rows in ["[Paged<u8>]", "Run<Paged<u8>>"] {
        let source = field_range_program(&format!(
            "fn inspect(rows: &{rows}) -> result: u64 reads(rows) {{
  if 0_u64 < rows^.len {{
    let count = rows^[0_u64].pages.len;
    invariant same: count == rows^[0_u64].pages.len;
    return count;
  }}
  return 0_u64;
}}
"
        ));
        field_range_verdict(&source, None);
    }
}

#[test]
fn a_pages_field_below_a_range_element_remains_an_ordinary_field() {
    let source = field_range_program(
        "struct Row {
  pages: Slots<u8, 8>;
}

fn inspect(rows: &[Row]) -> result: u64 reads(rows) {
  if 0_u64 < rows^.len {
    let count = rows^[0_u64].pages.len;
    invariant same: count == rows^[0_u64].pages.len;
    return count;
  }
  return 0_u64;
}
",
    );
    field_range_verdict(&source, None);
}

#[test]
fn a_ring_is_not_an_element_projection_base() {
    let source = field_range_program(
        "fn inspect(rows: &Ring<u64, 8>) -> result: unit pure contract {
  requires forall held(k in 0_u64..rows^.len): rows^[k] == 0_u64;
} {
  return unit;
}
",
    );
    field_range_verdict(&source, Some(SemanticRule::Range1));
}

#[test]
fn range_facts_discharge_page_and_segment_borrow_bounds_at_their_sites() {
    // [EFF-2] forming a page reads the Paged's length to capture its
    // initialized extent, while forming a segment borrow reads nothing.
    for (storage, row, bound, borrowed, expected) in [
        (
            "Paged<u8>",
            "reads(data.len), reads(indices)",
            "data^.pages.len",
            "data^.pages[i]",
            None,
        ),
        (
            "Paged<u8>",
            "reads(data.len), reads(indices)",
            "data^.len",
            "data^.pages[i]",
            Some(SemanticRule::Op4),
        ),
        (
            "Segments<u8>",
            "reads(indices)",
            "data^.len",
            "data^[i]",
            None,
        ),
    ] {
        let source = field_range_program(&format!(
            "fn inspect(data: &{storage}, indices: &Array<u64, 1>) -> result: unit {row} contract {{
  requires forall valid(k in 0_u64..1_u64): indices^[k] < {bound};
}} {{
  let i = indices^[0_u64];
  let part = &{borrowed};
  return unit;
}}
"
        ));
        field_range_verdict(&source, expected);
    }
}

#[test]
fn a_page_below_a_range_element_uses_its_projected_page_count_for_the_bound() {
    let source = field_range_program(
        "fn inspect(rows: &[Paged<u8>], indices: &Array<u64, 1>) -> result: unit reads(rows), reads(indices) contract {
  requires rows^.len == 1_u64;
  requires forall valid(k in 0_u64..1_u64): indices^[k] < rows^[0_u64].pages.len;
} {
  let i = indices^[0_u64];
  let part = &rows^[0_u64].pages[i];
  return unit;
}
",
    );
    field_range_verdict(&source, None);
}

/// The corpus owns the verdicts; this pins each negative to the intended
/// post-match call rather than an earlier type, effect or reference error.
#[test]
fn reference_match_payload_writes_reach_the_container() {
    for (source, rejected) in [
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-deref-scalar-write.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-deref-scalar-call.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-pos-match-deref-scalar-read.wf"
            )
            .as_slice(),
            false,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-deref-struct-write.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-deref-struct-call.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-pos-match-deref-struct-read.wf"
            )
            .as_slice(),
            false,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-borrow-scalar-write.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-borrow-scalar-call.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-pos-match-borrow-scalar-read.wf"
            )
            .as_slice(),
            false,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-borrow-struct-write.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-borrow-struct-call.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-pos-match-borrow-struct-read.wf"
            )
            .as_slice(),
            false,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-deref-field-write.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-deref-field-call.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-neg-match-deref-deep-field-write.wf"
            )
            .as_slice(),
            true,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range2-pos-match-deref-field-read.wf"
            )
            .as_slice(),
            false,
        ),
    ] {
        with_semantics(source, |outcome| match (rejected, outcome) {
            (false, SemanticOutcome::Complete(_)) => {}
            (true, SemanticOutcome::SourceIssue { issue, .. }) => {
                assert_eq!(issue.rule(), SemanticRule::Range3, "{issue:?}");
                assert!(
                    matches!(issue.kind(), SemanticIssueKind::UndischargedRangeFact { fact, site, .. } if fact == "zero" && *site == "a call"),
                    "{issue:?}"
                );
                let crate::SemanticLocation::SourceNode(_, coordinate) = issue.location();
                let start = usize::try_from(coordinate.start().value()).unwrap();
                assert!(source[start..].starts_with(b"need("), "{issue:?}");
            }
            (expected, outcome) => panic!("rejected={expected}: {outcome:?}"),
        });
    }
}

#[test]
fn range_facts_discharge_every_integer_domain_family() {
    for (ty, bound, expression) in [
        ("u64", "0_u64 < xs^[k]", "9_u64 / x"),
        ("u64", "0_u64 < xs^[k]", "9_u64 % x"),
        ("u64", "0_u64 < xs^[k]", "x / x"),
        ("u64", "0_u64 < xs^[k]", "x % x"),
        // The unsigned domain mentions only x, not the scalar-field numerator.
        ("u64", "0_u64 < xs^[k]", "holder.value / x"),
        ("u64", "0_u64 < xs^[k]", "holder.value % x"),
        ("i64", "-9223372036854775808_i64 < xs^[k]", "ineg(x)"),
        ("i64", "-9223372036854775808_i64 < xs^[k]", "iabs(x)"),
        ("i64", "0_i64 < xs^[k]", "-9223372036854775808_i64 / x"),
        ("i64", "0_i64 < xs^[k]", "-9223372036854775808_i64 % x"),
        ("u32", "xs^[k] < 64_u32", "ishl(1_u64, x)"),
        ("u32", "xs^[k] < 64_u32", "ishr(1_u64, x)"),
        ("u32", "xs^[k] < 32_u32", "ishl(x, x)"),
        ("u32", "xs^[k] < 32_u32", "ishr(x, x)"),
        ("u64", "xs^[k] < 256_u64", "cvt::<u64, u8>(x)"),
    ] {
        let source = field_range_program(&format!(
            "struct Holder {{\n  value: {ty};\n}}\n\nfn probe(xs: &[{ty}], holder: Holder) -> result: unit reads(xs) contract {{\n  requires forall domain(k in 0_u64..xs^.len): {bound};\n}} {{\n  if 0_u64 < xs^.len {{\n    let x = xs^[0_u64];\n    let result = {expression};\n  }}\n  return unit;\n}}\n"
        ));
        field_range_verdict(&source, None);
    }
}

#[test]
fn range_integer_domains_refuse_their_boundary_twins() {
    for (ty, bound, expression, rule) in [
        ("u64", "xs^[k] == 0_u64", "9_u64 / x", SemanticRule::Op2),
        ("u64", "xs^[k] == 0_u64", "9_u64 % x", SemanticRule::Op2),
        ("u64", "xs^[k] == 0_u64", "x / x", SemanticRule::Op2),
        ("u64", "xs^[k] == 0_u64", "x % x", SemanticRule::Op2),
        ("u32", "xs^[k] == 32_u32", "ishl(x, x)", SemanticRule::Op2),
        ("u32", "xs^[k] == 32_u32", "ishr(x, x)", SemanticRule::Op2),
        (
            "i64",
            "xs^[k] == -9223372036854775808_i64",
            "ineg(x)",
            SemanticRule::Op2,
        ),
        (
            "i64",
            "xs^[k] == -9223372036854775808_i64",
            "iabs(x)",
            SemanticRule::Op2,
        ),
        (
            "i64",
            "xs^[k] == -1_i64",
            "-9223372036854775808_i64 / x",
            SemanticRule::Op2,
        ),
        (
            "i64",
            "xs^[k] == -1_i64",
            "-9223372036854775808_i64 % x",
            SemanticRule::Op2,
        ),
        (
            "u32",
            "xs^[k] == 64_u32",
            "ishl(1_u64, x)",
            SemanticRule::Op2,
        ),
        (
            "u32",
            "xs^[k] == 64_u32",
            "ishr(1_u64, x)",
            SemanticRule::Op2,
        ),
        (
            "u64",
            "xs^[k] == 256_u64",
            "cvt::<u64, u8>(x)",
            SemanticRule::Op6,
        ),
    ] {
        let source = field_range_program(&format!(
            "fn probe(xs: &[{ty}]) -> result: unit reads(xs) contract {{\n  requires forall domain(k in 0_u64..xs^.len): {bound};\n}} {{\n  if 0_u64 < xs^.len {{\n    let x = xs^[0_u64];\n    let result = {expression};\n  }}\n  return unit;\n}}\n"
        ));
        field_range_verdict(&source, Some(rule));
    }
}

#[test]
fn signed_division_with_two_variable_operands_keeps_ordinary_op2() {
    // OP-2's signed corner exclusion is (x != MIN or y != -1).
    // Unlike a constant-operand instance, this is not a conjunction of
    // comparisons, even when the range fact proves both operands positive.
    for operation in ["/", "%"] {
        let source = field_range_program(&format!(
            "fn probe(xs: &Array<i64, 2>) -> result: unit reads(xs) contract {{\n  requires forall positive(k in 0_u64..xs^.len): xs^[k] > 0_i64;\n}} {{\n  let x = xs^[0_u64];\n  let y = xs^[1_u64];\n  let result = x {operation} y;\n  return unit;\n}}\n"
        ));
        field_range_verdict(&source, Some(SemanticRule::Op2));
    }
}

#[test]
fn range_requirements_accept_only_conjunctions_of_comparisons() {
    // RANGE-2 leaves every bor/bnot/bxor goal to ordinary entailment,
    // including nested ones and ones the range fact would imply.
    for (goal, expected) in [
        ("below", true),
        ("band(below, different)", true),
        ("band(below, both)", true),
        ("band(below, two)", false),
        ("bor(one, seven)", false),
        ("bnot(either)", false),
        ("bxor(below, seven)", false),
        ("bor(two, seven)", false),
        ("band(below, oneorseven)", false),
        ("band(below, nottwo)", false),
        ("band(below, exclusive)", false),
    ] {
        let source = field_range_program(&format!(
            "fn need(x: u64) -> result: unit pure contract {{\n  define below = x < 4_u64;\n  define different = x != 2_u64;\n  define one = x == 1_u64;\n  define two = x == 2_u64;\n  define seven = x == 7_u64;\n  define either = bor(two, seven);\n  define both = band(different, one);\n  define oneorseven = bor(one, seven);\n  define nottwo = bnot(two);\n  define exclusive = bxor(one, seven);\n  requires {goal};\n}} {{\n  return unit;\n}}\n\nfn probe(xs: &[u64]) -> result: unit reads(xs) contract {{\n  requires forall one(k in 0_u64..xs^.len): xs^[k] == 1_u64;\n}} {{\n  if 0_u64 < xs^.len {{\n    let x = xs^[0_u64];\n    need(x: x);\n  }}\n  return unit;\n}}\n"
        ));
        field_range_verdict(&source, (!expected).then_some(SemanticRule::Fn8));
    }
}

#[test]
fn a_float_conversion_keeps_its_ordinary_domain_rejection() {
    // Exact float representability is not a comparison conjunction over
    // integer range terms, so RANGE-2 does not defer this OP-6 domain.
    let source = field_range_program(
        "fn probe(xs: &[u64]) -> result: unit reads(xs) contract {\n  requires forall value(k in 0_u64..xs^.len): xs^[k] == 1_u64;\n} {\n  if 0_u64 < xs^.len {\n    let x = xs^[0_u64];\n    let result = cvt::<u64, f32>(x);\n  }\n  return unit;\n}\n",
    );
    field_range_verdict(&source, Some(SemanticRule::Op6));
}

#[test]
fn a_deferred_obligation_reports_the_instance_ceiling_it_reached() {
    let source = String::from_utf8(guarded_reads(300, false))
        .unwrap()
        .replace(
            "  let got = zeros(cells: cells);",
            "  let got = cells^[399_u64];\n  need(value: got);",
        );
    let source = format!(
        "fn need(value: u64) -> result: unit pure contract {{\n  requires value == 0_u64;\n}} {{\n  return unit;\n}}\n\n{source}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Range3, "{issue:?}");
        assert!(
            matches!(issue.kind(), SemanticIssueKind::UndischargedRangeFact { missing, .. } if missing.contains("256 instances")),
            "{issue:?}"
        );
    });
}

#[test]
fn deferred_arithmetic_capacity_is_never_an_invariant_rejection() {
    let source = field_range_program(
        "fn probe(cells: &[u64]) -> result: unit reads(cells) contract {\n  requires 2_u64 <= cells^.len;\n  requires forall small(a in 0_u64..cells^.len, b in 0_u64..cells^.len): 18446744073709551613_u64 * cells^[a] <= 18446744073709551612_u64 * cells^[b];\n} {\n  let a = cells^[0_u64];\n  let b = cells^[1_u64];\n  invariant scaled: 18446744073709551615_u64 * a <= 18446744073709551614_u64 * b;\n  return unit;\n}\n",
    );
    with_semantics(&source, |outcome| {
        let SemanticOutcome::Unsupported { unsupported, .. } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(
            unsupported.feature(),
            crate::UnsupportedSemanticFeature::RangeArithmetic
        );
    });
}

#[test]
fn an_imprecise_walk_cannot_reject_a_deferred_requirement() {
    let depth = 10;
    let indent = "  ".repeat(depth + 1);
    let source = String::from_utf8(nest(depth)).unwrap().replace(
        "let got = positive(cells: &cells.inner[0_u64..4_u64]);",
        &format!("let x = cells.inner[0_u64];\n{indent}let got = need(value: x);"),
    );
    let source = format!(
        "fn need(value: u64) -> result: u64 pure contract {{\n  requires value > 0_u64;\n}} {{\n  return value;\n}}\n\n{source}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Unsupported { unsupported, .. } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(
            unsupported.feature(),
            crate::UnsupportedSemanticFeature::RangeLoopNesting
        );
    });
}

#[test]
fn an_unvisited_page_element_bound_is_an_explicit_capability() {
    let source = field_range_program(
        "fn probe(data: &Paged<u64>, indices: &Array<u64, 1>) -> result: unit reads(data), reads(indices) contract {\n  requires 0_u64 < data^.pages.len;\n  requires forall zero(k in 0_u64..1_u64): indices^[k] == 0_u64;\n} {\n  let page = &data^.pages[0_u64];\n  let i = indices^[0_u64];\n  let value = page^[i];\n  return unit;\n}\n",
    );
    with_semantics(&source, |outcome| {
        let SemanticOutcome::Unsupported { unsupported, .. } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(
            unsupported.feature(),
            crate::UnsupportedSemanticFeature::RangeOrdinaryGoal
        );
    });
}

#[test]
fn a_disjunction_keeps_fn8_even_when_a_range_disequality_implies_it() {
    // A true disjunction is still outside RANGE-2's selected goal shape.
    let source = field_range_program(
        "fn need(x: u64) -> result: unit pure contract {\n  define below = x < 2_u64;\n  define above = x > 2_u64;\n  requires bor(below, above);\n} {\n  return unit;\n}\n\nfn probe(xs: &[u64]) -> result: unit reads(xs) contract {\n  requires forall different(k in 0_u64..xs^.len): xs^[k] != 2_u64;\n} {\n  if 0_u64 < xs^.len {\n    let x = xs^[0_u64];\n    need(x: x);\n  }\n  return unit;\n}\n",
    );
    field_range_verdict(&source, Some(SemanticRule::Fn8));
}

#[test]
fn a_boolean_requirement_leaf_keeps_fn8_even_when_its_value_is_a_comparison() {
    // The written requirement is a Boolean leaf; the range walk does not
    // replace its shape with the comparison held by its actual argument.
    let source = field_range_program(
        "fn need(flag: Bool) -> result: unit pure contract {\n  requires flag;\n} {\n  return unit;\n}\n\nfn probe(xs: &[u64]) -> result: unit reads(xs) contract {\n  requires forall one(k in 0_u64..xs^.len): xs^[k] == 1_u64;\n} {\n  if 0_u64 < xs^.len {\n    let x = xs^[0_u64];\n    let flag = x == 1_u64;\n    need(flag: flag);\n  }\n  return unit;\n}\n",
    );
    field_range_verdict(&source, Some(SemanticRule::Fn8));
}

#[test]
fn range_comparison_conjunction_conformance() {
    with_semantics(
        include_bytes!("../../../../tests/conformance/cases/range3-pos-ordinary-conjunction.wf"),
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{outcome:?}"
            )
        },
    );
}

/// The bad return is checked on a path independent of the unsupported
/// page-element bound. FN-9 is not a deferred family and must remain the verdict.
#[test]
fn a_nondeferrable_error_precedes_an_unsupported_deferred_goal() {
    for generic in ["", "<T>"] {
        let source = field_range_program(&format!(
            "fn probe{generic}(xs: &[u64], data: &Paged<u64>, flag: Bool) -> result: u64 reads(xs), reads(data) contract {{\n  requires 0_u64 < data^.pages.len;\n  requires forall one(k in 0_u64..xs^.len): xs^[k] == 1_u64;\n  ensures result == 0_u64;\n}} {{\n  if flag {{\n    return 1_u64;\n  }}\n  if 0_u64 < xs^.len {{\n    let x = xs^[0_u64];\n    let page = &data^.pages[0_u64];\n    let value = page^[x];\n  }}\n  return 0_u64;\n}}\n"
        ));
        field_range_verdict(&source, Some(SemanticRule::Fn9));
    }
}

/// Both functions participate. An unsupported function visited first must
/// not hide a representable ordinary goal the other function refutes.
#[test]
fn an_unproved_ordinary_goal_precedes_another_functions_capability_gap() {
    let source = field_range_program(
        "fn unsupported(xs: &[u64], data: &Paged<u64>) -> result: unit reads(xs), reads(data) contract {\n  requires 0_u64 < data^.pages.len;\n  requires forall one(k in 0_u64..xs^.len): xs^[k] == 1_u64;\n} {\n  if 0_u64 < xs^.len {\n    let x = xs^[0_u64];\n    let page = &data^.pages[0_u64];\n    let value = page^[x];\n  }\n  return unit;\n}\n\nfn wrong(xs: &[u64]) -> result: unit reads(xs) contract {\n  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;\n} {\n  if 0_u64 < xs^.len {\n    let x = xs^[0_u64];\n    let quotient = 9_u64 / x;\n  }\n  return unit;\n}\n",
    );
    for generic in [false, true] {
        let source = String::from_utf8(source.clone()).unwrap();
        let source = if generic {
            source.replace("fn unsupported(xs:", "fn unsupported<T>(xs:")
        } else {
            source
        };
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("{outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Op2, "{issue:?}");
            let crate::SemanticLocation::SourceNode(_, coordinate) = issue.location();
            let start = usize::try_from(coordinate.start().value()).unwrap();
            assert!(
                source.as_bytes()[start..].starts_with(b"9_u64 / x"),
                "{issue:?}"
            );
        });
    }
}

#[test]
fn deferred_constant_array_bounds_visit_their_offsets() {
    for (index, verdict) in [(0, None), (1, Some(SemanticRule::Op4))] {
        let source = field_range_program(&format!(
            "const values: Array<u8, 1> =[0_u8];

fn probe() -> result: u8 pure {{
  let indices = array_filled::<u64, 1>(value: {index}_u64);
  return values[indices[0_u64]];
}}
"
        ));
        field_range_verdict(&source, verdict);
    }
}

/// The range state excludes the dead arm and discharges selected bounds
/// there. A float domain is outside RANGE-2 and keeps its ordinary verdict
/// even at a site the range walk excludes.
#[test]
fn an_excluded_variant_only_discharges_selected_range_goals() {
    // EFF-2 counts the read syntactically, even in the excluded arm; the
    // conversion reads no formal storage and therefore exhibits pure.
    for (operation, row, verdict) in [
        ("xs^[i]", "reads(xs)", None),
        ("cvt::<u64, f32>(i)", "pure", Some(SemanticRule::Op6)),
    ] {
        let source = field_range_program(&format!(
            "enum Route {{\n  Live();\n  Dead();\n}}\n\nfn probe(xs: &[u64], i: u64) -> result: unit {row} contract {{\n  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;\n}} {{\n  let route = Route::Live();\n  match route {{\n    Live() => {{\n    }}\n    Dead() => {{\n      let value = {operation};\n    }}\n  }}\n  return unit;\n}}\n"
        ));
        field_range_verdict(&source, verdict);
    }
}

/// The dry walk initially excludes Dead, but the written variant is
/// forgotten at the real header. A dry exclusion cannot discharge its
/// reachable page bound, for which the real walk has no site handler.
#[test]
fn a_dry_walk_exclusion_does_not_hide_a_live_unhandled_site() {
    let source = field_range_program(
        "enum Route {\n  Live();\n  Dead();\n}\n\nfn probe(data: &Paged<u64>, indices: &Array<u64, 1>) -> result: unit reads(data), reads(indices) contract {\n  requires 0_u64 < data^.pages.len;\n  requires forall zero(k in 0_u64..1_u64): indices^[k] == 0_u64;\n} {\n  let route = Route::Live();\n  for (k in 0_u64..2_u64) {\n    match route {\n      Live() => {\n      }\n      Dead() => {\n        let page = &data^.pages[0_u64];\n        let i = indices^[0_u64];\n        let value = page^[i];\n      }\n    }\n    set route = Route::Dead();\n  }\n  return unit;\n}\n",
    );
    super::assert_unsupported(
        &source,
        crate::UnsupportedSemanticFeature::RangeOrdinaryGoal,
    );
}

#[test]
fn a_state_excluded_match_continuation_discharges_its_deferred_sites() {
    let source = field_range_program(
        "enum Route {\n  Live();\n  Dead();\n}\n\nfn probe(xs: &[u64], i: u64) -> result: unit reads(xs) contract {\n  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;\n} {\n  let route = Route::Live();\n  match route {\n    Live() => {\n      return unit;\n    }\n    Dead() => {\n    }\n  }\n  let value = xs^[i];\n  return unit;\n}\n",
    );
    field_range_verdict(&source, None);
}

#[test]
fn range_const_generic_substitution_keeps_concrete_and_symbolic_length_facts() {
    use super::super::model::{CheckedExpression, CheckedStatement};
    use super::super::range_facts::CheckedRangeTerm as Term;
    let source =
        b"fn filled<const count: u64>(value: u32) -> result: Array<u32, count> pure contract {
  ensures result.len == count;
  ensures forall same(k in 0_u64..count): result[k] == value;
} {
  let made = array_filled::<u32, count>(value: value);
  return made;
}

fn need(values: &[u32]) -> result: unit pure contract {
  requires forall wanted(k in 0_u64..values^.len): values^[k] == 5_u32;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = filled::<2>(value: 5_u32);
  need(values: &values[0_u64..2_u64]);
  return std::process::exit_status(code: 0_u8);
}
";
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("const-generic range terms must form in both scopes: {outcome:?}");
        };
        let functions = &program.data.functions;
        let mut concrete = false;
        let mut symbolic = false;
        for function in functions.iter().filter(|f| f.name == "filled") {
            let clause = &function
                .range_facts
                .postconditions
                .iter()
                .find(|post| post.clause.name == "same")
                .unwrap()
                .clause;
            let end = &clause.binders[0].end;
            match end {
                Term::Constant(2) => concrete = true,
                Term::ConstGeneric { .. } => symbolic = true,
                other => panic!("unexpected count: {other:?}"),
            }
            let CheckedStatement::Let {
                value:
                    CheckedExpression::UserCall {
                        function: callee, ..
                    },
                ..
            } = &function.body.as_ref().unwrap()[0]
            else {
                panic!("filled calls the prelude through an ordinary call")
            };
            let callee = &functions[callee.0 as usize];
            assert!(
                callee
                    .range_facts
                    .postconditions
                    .iter()
                    .any(|post| !post.owed
                        && post
                            .clause
                            .conclusions
                            .iter()
                            .any(|relation| &relation.right == end)),
                "the callee's n must become the caller's count in its length fact"
            );
        }
        assert!(
            concrete && symbolic,
            "both instantiation scopes were inspected"
        );
    });
}

#[test]
fn aggregate_range_negatives_name_the_consumer_call() {
    for (source, callee) in [
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range3-neg-aggregate-filled-struct.wf"
            )
            .as_slice(),
            "need(",
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range3-neg-aggregate-filled-enum.wf"
            )
            .as_slice(),
            "need(",
        ),
        (
            include_bytes!("../../../../tests/conformance/cases/range3-neg-aggregate-enum-tag.wf")
                .as_slice(),
            "same::<Flow>(",
        ),
        (
            include_bytes!("../../../../tests/conformance/cases/range3-neg-aggregate-bool-tag.wf")
                .as_slice(),
            "same::<Bool>(",
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/range3-neg-aggregate-bool-write.wf"
            )
            .as_slice(),
            "same::<Bool>(",
        ),
        (
            include_bytes!("../../../../tests/conformance/cases/range3-neg-aggregate-noncopy.wf")
                .as_slice(),
            "need(",
        ),
    ] {
        with_semantics(source, |outcome| {
            let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("expected the consumer's range rejection: {outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Range3, "{issue:?}");
            assert!(
                matches!(issue.kind(), SemanticIssueKind::UndischargedRangeFact { site, .. } if *site == "a call"),
                "{issue:?}"
            );
            let crate::SemanticLocation::SourceNode(_, coordinate) = issue.location();
            let start = usize::try_from(coordinate.start().value()).unwrap();
            assert!(source[start..].starts_with(callee.as_bytes()), "{issue:?}");
        });
    }
}

#[test]
fn aggregate_range_enum_expansion_retains_separate_payload_domains() {
    use super::super::range_facts::{
        CheckedRangeProjection as Projection, CheckedRangeTerm as Term,
    };
    let source = include_bytes!(
        "../../../../tests/conformance/cases/range1-pos-aggregate-generic-requirement.wf"
    );
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the enum requirement must check: {outcome:?}");
        };
        let function = program
            .data
            .executable_functions()
            .find(|function| function.name == "same")
            .unwrap();
        let clause = &function.range_facts.requirements[0];
        assert_eq!(
            clause.conclusions.len(),
            3,
            "one tag and both declared payloads"
        );
        for (index, relation) in clause.conclusions.iter().enumerate() {
            assert!(relation.projected);
            let Term::Read {
                projection,
                guarded_from,
                ..
            } = &relation.left
            else {
                panic!("{relation:?}")
            };
            assert_eq!(*guarded_from, Some(0));
            let Term::ValueProjection {
                projection: value_path,
                ..
            } = &relation.right
            else {
                panic!("{relation:?}")
            };
            assert_eq!(
                projection, value_path,
                "both sides select the same integer path"
            );
            match (index, projection.as_slice()) {
                (0, [Projection::Tag(2)]) => {}
                (
                    1,
                    [
                        Projection::Payload {
                            variant: 0,
                            field: 0,
                            variants: 2,
                        },
                    ],
                ) => {}
                (
                    2,
                    [
                        Projection::Payload {
                            variant: 1,
                            field: 0,
                            variants: 2,
                        },
                    ],
                ) => {}
                _ => panic!("{relation:?}"),
            }
        }
    });
}

#[test]
fn aggregate_range_postconditions_read_the_parameter_at_entry() {
    let good =
        include_str!("../../../../tests/conformance/cases/range1-pos-aggregate-entry-value.wf");
    let bad = good.replace(
        "  let made = array_filled::<T, 1>(value: value);\n  set value = other;",
        "  set value = other;\n  let made = array_filled::<T, 1>(value: value);",
    );
    assert_ne!(
        bad, good,
        "the negative must move replacement before the fill"
    );
    for (source, rejected) in [(good.as_bytes(), false), (bad.as_bytes(), true)] {
        with_semantics(source, |outcome| match (rejected, outcome) {
            (false, SemanticOutcome::Complete(_)) => {}
            (true, SemanticOutcome::SourceIssue { issue, .. }) => {
                assert_eq!(issue.rule(), SemanticRule::Range3, "{issue:?}");
                let crate::SemanticLocation::SourceNode(_, coordinate) = issue.location();
                let start = usize::try_from(coordinate.start().value()).unwrap();
                assert!(
                    source[start..].starts_with(b"return made;"),
                    "the changed value must fail at its producer's return: {issue:?}"
                );
            }
            (_, outcome) => panic!("rejected={rejected}: {outcome:?}"),
        });
    }
}

#[test]
fn aggregate_range_large_array_reports_the_atom_ceiling() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/range3-neg-aggregate-atom-ceiling.wf");
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(issue.rule(), SemanticRule::Range3, "{issue:?}");
        assert!(
            matches!(issue.kind(), SemanticIssueKind::UndischargedRangeFact { site, missing, .. } if *site == "a call" && missing.contains("4096 atoms")),
            "{issue:?}"
        );
    });
}

#[test]
fn aggregate_range_large_array_without_integer_projections_is_empty() {
    let source = field_range_program(
        "struct Flag {\n  value: f32;\n}\n\nfn same<T: copy>(targets: &[T], value: T) -> result: unit pure contract {\n  requires forall same(k in 0_u64..targets^.len): targets^[k] == value;\n} {\n  return unit;\n}\n\nfn forward(targets: &[Array<Flag, 1000000000>], value: Array<Flag, 1000000000>) -> result: unit pure {\n  same::<Array<Flag, 1000000000>>(targets: targets, value: value);\n  return unit;\n}\n",
    );
    with_semantics(&source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("{outcome:?}")
        };
        let function = program
            .data
            .executable_functions()
            .find(|function| function.name == "same")
            .unwrap();
        assert_eq!(
            function.range_facts.requirements.len(),
            1,
            "the aggregate equality forms even when its projection conjunction is empty"
        );
        assert!(function.range_facts.requirements[0].conclusions.is_empty());
    });
}

#[test]
fn aggregate_range_vacuity_does_not_form_projected_reads() {
    let source = include_str!(
        "../../../../tests/conformance/cases/range3-pos-vacuous-aggregate-expansion.wf"
    );
    // The minimal witness crosses the former instance limit; the larger
    // expansion also crosses the atom limit if any reads are formed.
    for length in [257, 1000000000] {
        field_range_verdict(source.replace("257", &length.to_string()).as_bytes(), None);
    }
}

#[test]
fn aggregate_range_symbolic_arrays_defer_the_whole_expansion() {
    use super::super::model::{CheckedConst, CheckedNominalKind, CheckedType};
    let source = include_bytes!(
        "../../../../tests/conformance/cases/range1-pos-aggregate-symbolic-array.wf"
    );
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("symbolic Array lengths must not panic or partially expand: {outcome:?}");
        };
        let mut observed = [false; 4];
        for function in program
            .data
            .functions
            .iter()
            .filter(|function| function.name == "fill")
        {
            let (length, fields) = match function.parameters[0].ty {
                CheckedType::Array { length, .. } => (length, 0),
                CheckedType::Nominal(id) => {
                    let CheckedNominalKind::Struct { fields } =
                        &program.data.nominals[id.0 as usize].kind
                    else {
                        continue;
                    };
                    let CheckedType::Array { length, .. } = fields[1].ty else {
                        panic!("Row's second field is an Array");
                    };
                    (length, 1)
                }
                _ => continue,
            };
            let clause = function
                .range_facts
                .postconditions
                .iter()
                .find(|post| post.clause.name == "same");
            match length {
                CheckedConst::Value(2) => {
                    assert_eq!(
                        clause.expect("concrete expansion").clause.conclusions.len(),
                        2 + fields
                    );
                    observed[fields] = true;
                }
                CheckedConst::Parameter(_) => {
                    assert!(
                        clause.is_none(),
                        "even a concrete sibling field must wait for the complete shape"
                    );
                    observed[2 + fields] = true;
                }
                other => panic!("unexpected Array length: {other:?}"),
            }
        }
        assert!(
            observed.into_iter().all(|seen| seen),
            "direct/nested, symbolic/concrete instances must all be inspected"
        );
    });
}
