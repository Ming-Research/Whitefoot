//! Ordinary obligations left open for the range walk at their own sites.
use super::with_semantics;
use crate::{SemanticOutcome, SemanticRule};

fn check(body: &str, rule: Option<SemanticRule>) {
    let source = format!(
        "{body}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
    );
    with_semantics(source.as_bytes(), |outcome| match (&outcome, rule) {
        (SemanticOutcome::Complete(_), None) => {}
        (SemanticOutcome::SourceIssue { issue, .. }, Some(rule)) => {
            assert_eq!(issue.rule(), rule, "{issue:?}")
        }
        _ => panic!("expected {rule:?}: {outcome:?}"),
    });
}

fn rows(bound: u64, start: u64, body: &str) -> String {
    format!(
        "fn probe(rows: &[Slots<u8, 8>], table: &Array<u64, 5>) -> result: unit reads(rows), reads(table) contract {{\n  requires forall all(k in {start}_u64..rows^.len): rows^[k].len <= {bound}_u64;\n}} {{\n{body}\n  return unit;\n}}"
    )
}

#[test]
fn input_rows() {
    check(
        &rows(
            4,
            0,
            "  for (i in 0_u64..rows^.len) {\n    let t = table^[rows^[i].len];\n  }",
        ),
        None,
    );
}

#[test]
fn rows_one_too_weak() {
    check(
        &rows(
            5,
            0,
            "  for (i in 0_u64..rows^.len) {\n    let t = table^[rows^[i].len];\n  }",
        ),
        Some(SemanticRule::Op4),
    );
}

#[test]
fn rows_outside_range() {
    check(
        &rows(
            4,
            1,
            "  if rows^.len > 0_u64 {\n    let t = table^[rows^[0_u64].len];\n  }",
        ),
        Some(SemanticRule::Op4),
    );
}

#[test]
fn stored_position() {
    check("fn probe(slots: &[u64], blocks: &[u64]) -> result: unit reads(slots), reads(blocks) contract {
  requires forall bounded(j in 0_u64..slots^.len): slots^[j] < blocks^.len;
} {
  for (k in 0_u64..slots^.len) {
    let at = slots^[k];
    let b = blocks^[at];
  }
  return unit;
}", None);
}

fn written(target: &str, step: &str) -> String {
    format!(
        "fn probe(targets: &Array<u64, 4>, table: &Array<u64, 4>, i: u64) -> result: unit reads(targets), reads(table) contract {{\n  requires i < 4_u64;\n  requires forall valid(j in 0_u64..4_u64): targets^[j] < 4_u64;\n}} {{\n  let t = targets^[i];\n  invariant bounded: {target} {{\n    {step}\n  }}\n  let v = table^[t];\n  return unit;\n}}"
    )
}

#[test]
fn written_instance() {
    check(&written("t < 4_u64", "use valid(i);"), None);
}

#[test]
fn written_wrong_arity() {
    check(
        &written("t < 4_u64", "use valid(i, i);"),
        Some(SemanticRule::Range4),
    );
}

#[test]
fn false_local_target() {
    check(
        &written("t < 0_u64", "use valid(i);").replace(" {\n    use valid(i);\n  }", ";"),
        Some(SemanticRule::Inv1),
    );
}

#[test]
fn exact_overflow() {
    check(
        "fn probe(xs: &[u16]) -> result: unit reads(xs) contract {
  requires forall small(k in 0_u64..xs^.len): xs^[k] <= 1000_u16;
} {
  for (i in 0_u64..xs^.len) {
    let y = xs^[i] + xs^[i];
  }
  return unit;
}",
        None,
    );
}

#[test]
fn loop_backedge() {
    check("fn probe(targets: &[u64], code: &[u8], leave: Bool) -> result: unit reads(targets) contract {
  requires 0_u64 < code^.len;
  requires targets^.len == code^.len;
  requires forall ok(j in 0_u64..targets^.len): targets^[j] < code^.len;
} {
  let pc = 0_u64;
  loop (
    invariant inside: pc < code^.len
  ) {
    if leave {
      break;
    }
    set pc = targets^[pc];
    continue;
  }
  return unit;
}", None);
}

#[test]
fn write_breaks_fact() {
    check("fn probe(slots: &[u64], blocks: &[u64], replacement: u64) -> result: unit reads(blocks), writes(slots) contract {
  requires forall bounded(j in 0_u64..slots^.len): slots^[j] < blocks^.len;
} {
  for (k in 0_u64..slots^.len) {
    set slots^[k] = replacement;
    let b = blocks^[slots^[k]];
  }
  return unit;
}", Some(SemanticRule::Op4));
}

#[test]
fn exact_overflow_negative() {
    check(
        "fn probe(xs: &[u16]) -> result: unit reads(xs) contract {
  requires forall small(k in 0_u64..xs^.len): xs^[k] <= 40000_u16;
} {
  for (i in 0_u64..xs^.len) {
    let y = xs^[i] + xs^[i];
  }
  return unit;
}",
        Some(SemanticRule::Op2),
    );
}

#[test]
fn exact_conversion() {
    check(
        "fn probe(xs: &[u64]) -> result: unit reads(xs) contract {
  requires forall small(k in 0_u64..xs^.len): xs^[k] <= 255_u64;
} {
  for (i in 0_u64..xs^.len) {
    let y = cvt::<u64, u8>(xs^[i]);
  }
  return unit;
}",
        None,
    );
}

#[test]
fn affine_call_requirement() {
    check(
        "fn consume(x: u64) -> result: unit pure contract {
  requires x * 2_u64 < 8_u64;
} {
  return unit;
}

fn probe(xs: &[u64]) -> result: unit reads(xs) contract {
  requires forall small(k in 0_u64..xs^.len): xs^[k] < 4_u64;
} {
  for (i in 0_u64..xs^.len) {
    consume(x: xs^[i]);
  }
  return unit;
}",
        None,
    );
}

#[test]
fn wrong_fact_name() {
    check(
        &written("t < 4_u64", "use previous(i);")
            .replace("  let t =", "  invariant previous: i < 4_u64;\n  let t ="),
        Some(SemanticRule::Range4),
    );
}

#[test]
fn instance_multiplicity() {
    check(
        &written("t < 4_u64", "use 2 times valid(i);"),
        Some(SemanticRule::Range4),
    );
}

#[test]
fn no_range_keeps_diagnostic() {
    let source =
        "fn probe(slots: &[u64], blocks: &[u64]) -> result: unit reads(slots), reads(blocks) {
  for (k in 0_u64..slots^.len) {
    let at = slots^[k];
    let b = blocks^[at];
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
";
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(issue.rule(), SemanticRule::Op4);
        let crate::SemanticIssueKind::UndischargedBoundsObligation {
            residual,
            mechanical_fix,
            ..
        } = issue.kind()
        else {
            panic!("{issue:?}")
        };
        assert_eq!(residual, "at < blocks^.len");
        assert_eq!(
            mechanical_fix,
            "`at < blocks^.len` is not proved here: when facts that reach the access imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); or guard the access with `if at < blocks^.len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare"
        );
    });
}

#[test]
fn callee_postcondition_only_participates() {
    check(
        "fn seed(xs: &Array<u64, 1>) -> result: unit writes(xs) contract {
  ensures forall valid(k in 0_u64..1_u64): xs^[k] < 4_u64;
} {
  set xs^[0_u64] = 0_u64;
  return unit;
}

fn probe(xs: &Array<u64, 1>, table: &Array<u64, 4>) -> result: unit reads(table), writes(xs) {
  seed(xs: xs);
  let v = table^[xs^[0_u64]];
  return unit;
}",
        None,
    );
}

#[test]
fn invalid_ordinary_certificate_stays_rejected() {
    check(
        &written("i < 4_u64", "use 0_u64 < 0_u64;")
            .replace("use 0_u64 < 0_u64;", "use (0_u64 < 0_u64);"),
        Some(SemanticRule::Prf1),
    );
}

#[test]
fn nonlinear_exact_result_stays_unproved() {
    check(
        "fn probe(xs: &[u16]) -> result: unit reads(xs) contract {
  requires forall small(k in 0_u64..xs^.len): xs^[k] < 10_u16;
} {
  for (i in 0_u64..xs^.len) {
    let y = xs^[i] * xs^[i];
  }
  return unit;
}",
        Some(SemanticRule::Op2),
    );
}

#[test]
fn projected_affine_call_requirement() {
    check(
        "struct Bound {
  value: u64;
}

fn consume(x: Bound) -> result: unit pure contract {
  requires x.value < 4_u64;
} {
  return unit;
}

fn probe(xs: &[u64]) -> result: unit reads(xs) contract {
  requires forall small(k in 0_u64..xs^.len): xs^[k] < 4_u64;
} {
  for (i in 0_u64..xs^.len) {
    let item = Bound(value: xs^[i]);
    consume(x: item);
  }
  return unit;
}",
        None,
    );
}

#[test]
fn loop_fallthrough_and_entry() {
    check("fn probe(targets: &[u64], code: &[u8], leave: Bool) -> result: unit reads(targets) contract {
  requires 0_u64 < code^.len;
  requires targets^.len == code^.len;
  requires forall ok(j in 0_u64..targets^.len): targets^[j] < code^.len;
} {
  let pc = targets^[0_u64];
  loop (
    invariant inside: pc < code^.len
  ) {
    if leave {
      break;
    }
    set pc = targets^[pc];
  }
  return unit;
}", None);
}

#[test]
fn bad_continue_is_not_assumed() {
    check("fn probe(targets: &[u64], code: &[u8], leave: Bool, replacement: u64) -> result: unit reads(targets) contract {
  requires 0_u64 < code^.len;
  requires targets^.len == code^.len;
  requires forall ok(j in 0_u64..targets^.len): targets^[j] < code^.len;
} {
  let pc = 0_u64;
  loop (
    invariant inside: pc < code^.len
  ) {
    if leave {
      set pc = replacement;
      continue;
    }
    set pc = targets^[pc];
  }
  return unit;
}", Some(SemanticRule::Inv1));
}

#[test]
fn bad_loop_entry_is_not_assumed() {
    check(
        "fn probe(xs: &[u64], replacement: u64, leave: Bool) -> result: unit reads(xs) contract {
  requires xs^.len > 0_u64;
  requires forall small(k in 0_u64..xs^.len): xs^[k] < 4_u64;
} {
  let pc = replacement;
  loop (
    invariant inside: pc < 4_u64
  ) {
    if leave {
      break;
    }
    set pc = xs^[0_u64];
  }
  return unit;
}",
        Some(SemanticRule::Inv1),
    );
}

#[test]
fn first_still_unproved_record_keeps_ordinary_diagnostic() {
    let source = "fn probe(xs: &[u64], wide: &Array<u8, 5>, narrow: &Array<u8, 4>) -> result: unit reads(xs), reads(wide), reads(narrow) contract {
  requires forall small(k in 0_u64..xs^.len): xs^[k] <= 4_u64;
} {
  for (i in 0_u64..xs^.len) {
    let at = xs^[i];
    let first = wide^[at];
    let second = narrow^[at];
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
";
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(issue.rule(), SemanticRule::Op4);
        let crate::SemanticIssueKind::UndischargedBoundsObligation { residual, .. } = issue.kind()
        else {
            panic!("{issue:?}")
        };
        assert_eq!(residual, "at < narrow^.len");
    });
}

#[test]
fn element_reference_affine_call_requirement() {
    check(
        "struct Bound {
  value: u64;
}

fn consume(x: &Bound) -> result: u64 reads(x) contract {
  requires x^.value < 4_u64;
} {
  return x^.value;
}

fn probe(rows: &[Bound]) -> result: unit reads(rows) contract {
  requires forall small(k in 0_u64..rows^.len): rows^[k].value < 4_u64;
} {
  for (i in 0_u64..rows^.len) {
    let got = consume(x: &rows^[i]);
  }
  return unit;
}",
        None,
    );
}

#[test]
fn indexed_formal_affine_call_requirement() {
    check(
        "fn consume(rows: &[Slots<u8, 8>], i: u64) -> result: unit pure contract {
  requires i < rows^.len;
  requires rows^[i].len <= 4_u64;
} {
  return unit;
}

fn probe(rows: &[Slots<u8, 8>]) -> result: unit reads(rows) contract {
  requires forall small(k in 0_u64..rows^.len): rows^[k].len <= 4_u64;
} {
  for (i in 0_u64..rows^.len) {
    let length = rows^[i].len;
    consume(rows: rows, i: i);
  }
  return unit;
}",
        None,
    );
}

#[test]
fn symbolic_generic_body_defers() {
    check(
        "fn probe<T>(xs: &[u16], unused: T) -> result: T reads(xs) contract {
  requires forall small(k in 0_u64..xs^.len): xs^[k] <= 1000_u16;
} {
  for (i in 0_u64..xs^.len) {
    let y = xs^[i] + xs^[i];
  }
  return move unused;
}",
        None,
    );
}

#[test]
fn written_enclosing_loop_fact() {
    check("fn probe(targets: &[u64], table: &Array<u64, 4>) -> result: unit reads(targets), reads(table) contract {
  requires forall all(j in 0_u64..targets^.len): targets^[j] < 4_u64;
} {
  for (
    i in 0_u64..targets^.len,
    invariant forall valid(j in 0_u64..targets^.len): targets^[j] < 4_u64
  ) {
    let t = targets^[i];
    invariant bounded: t < 4_u64 {
      use valid(i);
    }
    let v = table^[t];
  }
  return unit;
}", None);
}
