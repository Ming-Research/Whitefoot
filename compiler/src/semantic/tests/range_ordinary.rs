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
    for expression in [
        "    let y = xs^[i] + xs^[i];",
        // The at-site binding is a range term, whatever initialized it.
        "    let x = cvt::<u16, u16>(xs^[i]);\n    let y = x + x;",
    ] {
        let source = "fn probe(xs: &[u16]) -> result: unit reads(xs) contract {
  requires forall small(k in 0_u64..xs^.len): xs^[k] <= 1000_u16;
} {
  for (i in 0_u64..xs^.len) {
    let y = xs^[i] + xs^[i];
  }
  return unit;
}"
        .replace("    let y = xs^[i] + xs^[i];", expression);
        check(&source, None);
    }
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
    // Even an exactly known range value does not make x * x a range term.
    // The dead-arm twin also distinguishes non-deferral from the former
    // opaque mathematical goal: a range-excluded site cannot discharge it.
    for (body, expected_residual) in [
        ("  let x = xs^[0_u64];\n  let y = x * x;", "x *defined x"),
        (
            "  let route = Route::Live();\n  match route {\n    Live() => {\n    }\n    Dead() => {\n      let x = xs^[0_u64];\n      let y = x * x;\n    }\n  }",
            "x *defined x",
        ),
        (
            "  let x = xs^[0_u64];\n  let scale = 2_u64;\n  let y = x * scale;",
            "x *defined scale",
        ),
    ] {
        let source = format!(
            "enum Route {{\n  Live();\n  Dead();\n}}\n\nfn probe(xs: &[u64]) -> result: unit reads(xs) contract {{\n  requires 0_u64 < xs^.len;\n  requires forall two(k in 0_u64..xs^.len): xs^[k] == 2_u64;\n}} {{\n{body}\n  return unit;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        let ordinary = source.replace(
            "  requires forall two(k in 0_u64..xs^.len): xs^[k] == 2_u64;\n",
            "",
        );
        let rejection = |source: &str| {
            with_semantics(source.as_bytes(), |outcome| {
                let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                    panic!("{outcome:?}");
                };
                assert_eq!(issue.rule(), SemanticRule::Op2, "{issue:?}");
                let crate::SemanticIssueKind::UndischargedIntegerDomainObligation {
                    residual, ..
                } = issue.kind()
                else {
                    panic!("{issue:?}");
                };
                assert_eq!(residual, expected_residual);
                issue.kind().clone()
            })
        };
        assert_eq!(rejection(&source), rejection(&ordinary));
    }
}

#[test]
fn a_false_boolean_requirement_keeps_fn8_when_only_array_filled_participates() {
    check(
        "fn need(flag: Bool) -> result: unit pure contract {\n  requires flag;\n} {\n  return unit;\n}\n\nfn probe() -> result: unit pure {\n  let xs = array_filled::<u64, 1>(value: 0_u64);\n  need(flag: False());\n  return unit;\n}",
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn a_type_invariant_is_not_an_ordinary_call_requirement() {
    // TYPE-11 shares the call-goal record representation, but it is not one
    // of RANGE-2's deferred ordinary families, even in an excluded arm.
    check(
        "enum Route {\n  Live();\n  Dead();\n}\n\nstruct Guard {\n  value: u64;\n  invariant small(g): g.value < 4_u64;\n}\n\nfn probe(value: u64) -> result: unit pure {\n  let xs = array_filled::<u64, 1>(value: 0_u64);\n  let route = Route::Live();\n  match route {\n    Live() => {\n    }\n    Dead() => {\n      let guarded = Guard(value: value);\n    }\n  }\n  return unit;\n}",
        Some(SemanticRule::Type11),
    );
}

#[test]
fn a_scalar_field_requirement_keeps_ordinary_fn8() {
    // RANGE-1 admits an integer field below an element, not a bare struct's
    // scalar field. The range state knowing the field does not change its shape.
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
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn a_scalar_field_subscript_keeps_ordinary_op4() {
    // The field is outside RANGE-1 even when its value comes from a range fact.
    check(
        "struct Holder {\n  index: u64;\n}\n\nfn probe(xs: &Array<u64, 1>, table: &Array<u8, 4>) -> result: unit reads(xs), reads(table) contract {\n  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;\n} {\n  let holder = Holder(index: xs^[0_u64]);\n  let value = table^[holder.index];\n  return unit;\n}",
        Some(SemanticRule::Op4),
    );
}

#[test]
fn call_requirements_select_ring_elements_after_substitution() {
    // Instantiation encodes the projected actual as a datum, without the
    // intermediate Ring type. RANGE-1 still excludes this first element
    // selection; exclusion by the range walk cannot discharge its FN-8 goal.
    check(
        "enum Route {\n  Live();\n  Dead();\n}\n\nfn need(value: u64) -> result: unit pure contract {\n  requires value < 4_u64;\n} {\n  return unit;\n}\n\nfn probe(rows: &Ring<Slots<u8, 8>, 8>, xs: &Array<u64, 1>) -> result: unit reads(rows) contract {\n  requires 0_u64 < rows^.len;\n  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;\n} {\n  let route = Route::Live();\n  match route {\n    Live() => {\n    }\n    Dead() => {\n      need(value: rows^[0_u64].len);\n    }\n  }\n  return unit;\n}",
        Some(SemanticRule::Fn8),
    );
    // After substitution the Ring subscript follows an Array element, which
    // RANGE-1 admits. A selected goal in the excluded arm holds vacuously.
    check(
        "enum Route {\n  Live();\n  Dead();\n}\n\nfn need(rows: &Ring<Slots<u8, 8>, 8>) -> result: unit reads(rows) contract {\n  requires 0_u64 < rows^.len;\n  requires rows^[0_u64].len < 4_u64;\n} {\n  let length = rows^[0_u64].len;\n  return unit;\n}\n\nfn probe(table: &Array<Ring<Slots<u8, 8>, 8>, 1>, xs: &Array<u64, 1>) -> result: unit reads(table) contract {\n  requires 0_u64 < table^[0_u64].len;\n  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;\n} {\n  let route = Route::Live();\n  match route {\n    Live() => {\n    }\n    Dead() => {\n      need(rows: &table^[0_u64]);\n    }\n  }\n  return unit;\n}",
        None,
    );
}

#[test]
fn a_requirement_subscript_keeps_its_actuals_shape() {
    // A projected actual becomes an unknown captured offset in the ordinary
    // goal. Selection must still see that holder.index is not a range term.
    check(
        "enum Route {\n  Live();\n  Dead();\n}\n\nstruct Holder {\n  index: u64;\n}\n\nfn need(rows: &Array<Slots<u8, 8>, 4>, i: u64) -> result: unit reads(rows) contract {\n  requires i < 4_u64;\n  requires rows^[i].len < 4_u64;\n} {\n  let length = rows^[i].len;\n  return unit;\n}\n\nfn probe(rows: &Array<Slots<u8, 8>, 4>, holder: Holder, xs: &Array<u64, 1>) -> result: unit reads(rows) contract {\n  requires holder.index < 4_u64;\n  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;\n} {\n  let route = Route::Live();\n  match route {\n    Live() => {\n    }\n    Dead() => {\n      need(rows: rows, i: holder.index);\n    }\n  }\n  return unit;\n}",
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn a_reference_requirement_preserves_the_actuals_binding_mode() {
    // A reference scalar read is not a live own integer binding. An own
    // scalar reached by the explicit borrow actual is still a range term.
    let need = "fn need(value: &u64) -> result: unit pure contract {\n  requires value^ < 4_u64;\n} {\n  return unit;\n}\n";
    check(
        &format!(
            "{need}\nenum Route {{\n  Live();\n  Dead();\n}}\n\nfn probe(value: &u64, xs: &Array<u64, 1>) -> result: unit pure contract {{\n  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;\n}} {{\n  let route = Route::Live();\n  match route {{\n    Live() => {{\n    }}\n    Dead() => {{\n      need(value: value);\n    }}\n  }}\n  return unit;\n}}"
        ),
        Some(SemanticRule::Fn8),
    );
    check(
        &format!(
            "{need}\nfn probe(xs: &Array<u64, 1>) -> result: unit reads(xs) contract {{\n  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;\n}} {{\n  let value = xs^[0_u64];\n  need(value: &value);\n  return unit;\n}}"
        ),
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

/// A callee whose range postcondition makes its caller take part, a callee
/// whose affine requirement the caller defers, and a writer through a
/// reference: a deferred requirement on a value the walk must have forgotten.
fn stale(body: &str) -> String {
    format!(
        "struct Holder {{
  value: u64;
}}

struct Owner {{
  value: u64;
  spare: Box<u64>;
}}

fn observe(value: u64) -> result: u64 pure contract {{
  ensures result == value;
}} {{
  return value;
}}

fn guard(left: u64, right: u64) -> result: unit pure contract {{
  requires left == right;
}} {{
  return unit;
}}

fn bump(target: &u64) -> result: unit writes(target) {{
  set target^ = 9_u64;
  return unit;
}}

{body}"
    )
}

#[test]
fn scalar_written_by_a_call_through_its_reference() {
    check(
        &stale(
            "fn caller() -> result: unit pure {
  let source = 1_u64;
  let observed = observe(value: source);
  bump(target: &source);
  guard(left: observed, right: source);
  return unit;
}",
        ),
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn reference_parameter_written_by_a_call() {
    check(
        &stale(
            "fn caller(source: &u64) -> result: unit writes(source) {
  let observed = observe(value: source^);
  bump(target: source);
  guard(left: observed, right: source^);
  return unit;
}",
        ),
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn reference_parameter_written_through_a_reborrow() {
    check(
        &stale(
            "fn caller(source: &u64) -> result: unit writes(source) {
  let observed = observe(value: source^);
  let again = &source^;
  set again^ = 9_u64;
  guard(left: observed, right: source^);
  return unit;
}",
        ),
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn owned_locals_exchanged_by_swap() {
    check(
        &stale(
            "fn caller() -> result: unit pure {
  let first_box = box_new::<u64>(value: 0_u64);
  let second_box = box_new::<u64>(value: 0_u64);
  let left = Owner(value: 1_u64, spare: move first_box);
  let right = Owner(value: 2_u64, spare: move second_box);
  let observed = observe(value: left.value);
  swap(first: &left, second: &right);
  guard(left: observed, right: left.value);
  return unit;
}",
        ),
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn scalar_written_through_a_reference_the_loop_body_takes() {
    check(
        &stale(
            "fn caller() -> result: unit pure {
  let source = 1_u64;
  let observed = observe(value: source);
  for (i in 0_u64..2_u64) {
    guard(left: observed, right: source);
    let writer = &source;
    set writer^ = 2_u64;
  }
  return unit;
}",
        ),
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn scalar_written_through_a_reference_rebound_to_it_in_the_loop() {
    check(
        &stale(
            "fn caller() -> result: unit pure {
  let source = 1_u64;
  let other = 1_u64;
  let observed = observe(value: source);
  let writer = &other;
  for (i in 0_u64..2_u64) {
    guard(left: observed, right: source);
    set writer^ = 2_u64;
    set writer = &source;
  }
  return unit;
}",
        ),
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn scalar_written_through_a_joined_reference() {
    check(
        &stale(
            "fn caller(flag: Bool) -> result: unit pure {
  let a = 1_u64;
  let b = 1_u64;
  let observed = observe(value: b);
  let p = if flag {
    give &a;
  } else {
    give &b;
  }
  set p^ = 2_u64;
  guard(left: observed, right: b);
  return unit;
}",
        ),
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn scalar_written_through_a_reference_inside_an_atomic_statement() {
    check(
        &stale(
            "fn caller(state: Shared<Holder>) -> result: unit pure waits {
  let source = 1_u64;
  let observed = observe(value: source);
  let writer = &source;
  atomic held = &state {
    set writer^ = held^.value;
  }
  guard(left: observed, right: source);
  return unit;
}",
        ),
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn struct_written_after_a_copy_of_it() {
    check(
        &stale(
            "fn caller() -> result: unit pure {
  let held = Holder(value: 1_u64);
  let twin = held;
  let observed = observe(value: twin.value);
  set held.value = 2_u64;
  guard(left: observed, right: held.value);
  return unit;
}",
        ),
        Some(SemanticRule::Fn8),
    );
}

#[test]
fn struct_field_written_through_a_reference() {
    check(
        &stale(
            "fn caller() -> result: unit pure {
  let held = Holder(value: 1_u64);
  let observed = observe(value: held.value);
  let writer = &held;
  set writer^.value = 2_u64;
  guard(left: observed, right: held.value);
  return unit;
}",
        ),
        Some(SemanticRule::Fn8),
    );
}

/// A condition the ordinary checker keeps no origin for, because one path
/// sets it, and the walk still knows as a comparison.
fn condition(write: &str) -> String {
    format!(
        "fn probe(xs: &[u64], table: &Array<u64, 4>, x: u64, flag: Bool) -> result: unit reads(table) contract {{
  requires forall small(k in 0_u64..xs^.len): xs^[k] < 4_u64;
}} {{
  let inside = x < 4_u64;
  if flag {{
    set inside = x < 4_u64;
  }}
{write}  if inside {{
    let v = table^[x];
  }}
  return unit;
}}"
    )
}

#[test]
fn condition_kept_across_a_join() {
    check(&condition(""), None);
}

#[test]
fn condition_written_through_a_reference() {
    check(
        &condition("  let writer = &inside;\n  set writer^ = True();\n"),
        Some(SemanticRule::Op4),
    );
}

/// [RANGE-3] the same staleness, without deferral, at a range requirement.
#[test]
fn range_requirement_bound_written_through_a_reference() {
    check(
        "fn need(xs: &Array<u64, 4>, n: u64) -> result: unit pure contract {
  requires forall small(k in 0_u64..n): xs^[k] < 4_u64;
} {
  return unit;
}

fn caller(xs: &Array<u64, 4>) -> result: unit writes(xs) {
  let n = 1_u64;
  let w = &n;
  set w^ = 4_u64;
  set xs^[0_u64] = 0_u64;
  need(xs: xs, n: n);
  return unit;
}",
        Some(SemanticRule::Range3),
    );
}
