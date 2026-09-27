//! Static integer-domain obligations for exact `/` and `%` operations [OP-2,
//! ENT-6]. Every exact site must establish its complete typed `.defined`
//! predicate, including nonzero-divisor and signed `MIN / -1` safety. A proof
//! discharges the obligation without adding an effect; an unproved or refuted
//! obligation rejects under OP-2.

use crate::{
    SemanticIssueKind, SemanticLocation, SemanticOutcome, SemanticRule, StaticObligationDisposition,
};

use super::super::entailment::{
    DerivationNode, FunctionEntailment, ObligationFamily, Relation, TermKind,
};
use super::super::goal::{GoalExpression, GoalOperation};
use super::super::model::{CheckedFunction, CheckedIntegerOperation, MeasuredKind};
use super::entailment::validate_derivations;
use super::with_semantics;

/// [OP-2] one rejection's residual and disposition, and the fragment of its
/// repair that shows which routes the goal's disposition and terms selected
/// [DIAG-1]. The complete sentences are pinned with repaired programs in
/// `driver::pinned_repairs`.
fn assert_integer_domain(
    kind: &SemanticIssueKind,
    expected_residual: &str,
    expected_disposition: StaticObligationDisposition,
    route: &str,
) {
    let SemanticIssueKind::UndischargedIntegerDomainObligation {
        residual,
        disposition,
        mechanical_fix,
    } = kind
    else {
        panic!("expected an OP-2 domain rejection, got {kind:?}");
    };
    assert_eq!(
        (residual.as_str(), *disposition),
        (expected_residual, expected_disposition)
    );
    assert!(mechanical_fix.contains(route), "{mechanical_fix}");
}

fn named<'functions>(
    functions: &'functions [CheckedFunction],
    name: &str,
) -> &'functions CheckedFunction {
    functions
        .iter()
        .find(|function| function.name == name)
        .expect("named function is checked")
}

fn division_outcomes(
    function: &CheckedFunction,
) -> Vec<&super::super::entailment::ObligationOutcome> {
    function
        .entailment
        .obligations
        .iter()
        .filter(|outcome| {
            outcome.family == ObligationFamily::IntegerDomain
                && matches!(
                    &outcome.canonical_goal,
                    Some(GoalExpression::Operation {
                        row: GoalOperation::Integer {
                            operation: CheckedIntegerOperation::DivideDefined
                                | CheckedIntegerOperation::RemainderDefined,
                            ..
                        },
                        ..
                    })
                )
        })
        .collect()
}

/// A positive-divisor requirement enters the body through S4 and discharges
/// the zero-divisor conjunct of an unsigned exact site.
#[test]
fn a_positive_requirement_discharges_an_unsigned_site() {
    let source = br#"fn ratio(n: u64, divisor: u64) -> result: u64 pure contract {
  requires divisor > 0_u64;
} {
  let q = n / divisor;
  return q;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a requirement-dominated unsigned site must be accepted: {outcome:?}");
        };
        let ratio = named(&checked.data.functions, "ratio");
        let division = division_outcomes(ratio);
        assert_eq!(division.len(), 1, "one source occurrence, one obligation");
        assert_eq!(division[0].components.len(), 2);
        assert!(
            division.iter().all(|outcome| outcome.discharged),
            "the positive requirement derives `divisor != 0` and the unsigned overflow conjunct is ground true",
        );
    });
}

/// A dominating branch spells the divisor's canonical disequality and
/// discharges the exact operation only on the taken edge.
#[test]
fn a_canonical_branch_discharges_the_site() {
    let source = br#"fn ratio(n: u64, divisor: u64) -> result: u64 pure {
  if divisor != 0_u64 {
    let q = n / divisor;
    return q;
  } else {
    return 0_u64;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a branch-backed site must be accepted: {outcome:?}");
        };
        let ratio = named(&checked.data.functions, "ratio");
        assert!(
            division_outcomes(ratio)
                .iter()
                .all(|outcome| outcome.discharged),
            "the branch's established disequality discharges the obligation",
        );
    });
}

/// A divisor nothing bounds leaves the zero-divisor conjunct underivable:
/// the program rejects citing OP-2 at the `infix` node with the exact
/// residual `d != 0`. Exact division contributes no runtime effect, so the
/// `pure` row is correct for this body.
#[test]
fn an_unconstrained_divisor_rejects_citing_op2_with_the_exact_residual() {
    let source = br#"fn ratio(n: u64, d: u64) -> result: u64 pure {
  let q = n / d;
  return q;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an unconstrained divisor must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "n /defined d",
            StaticObligationDisposition::Unproved,
            "add `requires n /defined d;` to the `contract` of `ratio`",
        );
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        assert_eq!(
            std::str::from_utf8(&source[start..end]).expect("cited bytes are text"),
            "n / d",
            "the rejection lands on the infix node",
        );
    });
}

/// The remainder row carries the identical obligation: both `/` and `%`
/// fail on exactly the same two inputs, so one class and one pair of
/// conjuncts serve both.
#[test]
fn the_remainder_row_carries_the_same_obligation() {
    let source = br#"fn residue(n: u64, d: u64) -> result: u64 pure {
  let r = n % d;
  return r;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an unconstrained remainder divisor must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "n %defined d",
            StaticObligationDisposition::Unproved,
            "add `requires n %defined d;` to the `contract` of `residue`",
        );
    });
}

/// A nonzero constant divisor makes the zero-divisor conjunct ground true
/// and, over a signed type, decides the `-1` half of the exceptional pair, so
/// the site discharges with no fact source.
#[test]
fn a_nonzero_constant_divisor_discharges_with_no_fact_source() {
    let source = br#"fn halve(n: i32) -> result: i32 pure {
  let q = n / 2_i32;
  return q;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a nonzero constant divisor must be accepted: {outcome:?}");
        };
        let halve = named(&checked.data.functions, "halve");
        assert!(
            division_outcomes(halve)
                .iter()
                .all(|outcome| outcome.discharged),
        );
    });
}

/// A constant zero divisor instantiates a ground false zero-divisor
/// conjunct and is therefore rejected at every non-contradictory point.
#[test]
fn a_constant_zero_divisor_is_rejected_everywhere() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let x = 10_i32;
  let q = x / 0_i32;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("a constant zero divisor must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "x /defined 0_i32",
            StaticObligationDisposition::Refuted,
            "make `x /defined 0_i32` false",
        );
    });
}

/// The one expressible signed-overflow shape: a constant `-1` divisor
/// reduces the disjunction to `dividend != iK::MIN`, one disequality the
/// fragment states. Nothing bounds the dividend, so conjunct one is the
/// rejection.
#[test]
fn a_minus_one_divisor_demands_the_dividend_disequality() {
    let source = br#"fn negate(n: i32) -> result: i32 pure {
  let q = n / -1_i32;
  return q;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an unconstrained dividend over -1 must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "n /defined -1_i32",
            StaticObligationDisposition::Unproved,
            "add `requires n /defined -1_i32;` to the `contract` of `negate`",
        );
    });
}

/// The same site with a verified entry requirement that bounds the dividend
/// away from the type minimum discharges both conjuncts.
#[test]
fn a_bounded_dividend_over_minus_one_discharges() {
    let source = br#"fn negate(n: i32) -> result: i32 pure contract {
  requires n > -100_i32;
} {
  let q = n / -1_i32;
  return q;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a bounded dividend over -1 must be accepted: {outcome:?}");
        };
        let negate = named(&checked.data.functions, "negate");
        assert!(
            division_outcomes(negate)
                .iter()
                .all(|outcome| outcome.discharged),
        );
    });
}

/// `/checked` is total: no exact-domain obligation attaches, the row stays a
/// `Result`-returning operation, and a zero divisor remains a recoverable
/// value rather than a source rejection.
#[test]
fn a_checked_division_attaches_no_obligation() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let n = 10_i64;
  let d = 0_i64;
  match n /checked d {
    Ok(value: v) => {
      return std::process::exit_status(code: 0_u8);
    }
    Err(error: e) => {
      return std::process::exit_status(code: 0_u8);
    }
  }
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a checked division must be accepted: {outcome:?}");
        };
        let main = named(&checked.data.functions, "main");
        assert!(
            division_outcomes(main).is_empty(),
            "no division obligation attaches to a checked site",
        );
    });
}

/// On the default checker, a declared effect entry the body does not exhibit
/// is judged under EFF-2 before the undischarged exact-division obligation is
/// reported. v0.59 wrote that entry `allocates(heap)`; v0.60 has no allocation
/// category [EFF-1, STOR-8], so it is an ordinary unexhibited `writes`.
#[test]
fn effect_mismatch_precedes_static_division_rejection() {
    let source = br#"fn ratio(cell: &u64, n: u64, d: u64) -> result: u64 writes(cell) {
  let q = n / d;
  let r = n % d;
  return q;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("exact division does not exhibit the declared write: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Eff2);
    });
}

/// The default checker reaches the same ground-false zero-divisor conjunct as
/// the obligation-focused test entry.
#[test]
fn the_default_checker_rejects_a_constant_zero_divisor() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let x = 10_i32;
  let q = x / 0_i32;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the default path rejects a constant zero divisor: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "x /defined 0_i32",
            StaticObligationDisposition::Refuted,
            "make `x /defined 0_i32` false",
        );
    });
}

/// [ENT-3.S7] the retained postcondition proof stands on the division row's
/// order relation `quotient <= count`: the quotient, bound by a `let` or as a
/// commit value, against the dividend's place.
fn quotient_order_is_retained(summary: &FunctionEntailment) -> bool {
    summary.derivations.nodes.iter().any(|node| {
        matches!(
            node,
            DerivationNode::OperationFact {
                relation: Relation::Bound { left, right, bound: 0 },
                ..
            } if matches!(
                summary.inventory.terms[left.0 as usize],
                TermKind::Place(..) | TermKind::CommitValue { .. }
            ) && matches!(summary.inventory.terms[right.0 as usize], TermKind::Place(..))
        )
    })
}

#[test]
fn unsigned_literal_division_publishes_the_quotient_bound() {
    let source = br#"fn half_floor(count: u64) -> result: u64 pure contract {
  ensures result <= count;
} {
  let quotient = count / 2_u64;
  return quotient;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the unsigned quotient bound must discharge the postcondition: {outcome:?}");
        };
        let function = named(&checked.data.functions, "half_floor");
        validate_derivations(&function.entailment);
        assert!(function.entailment.postconditions[0].aggregate.discharged);
        assert!(quotient_order_is_retained(&function.entailment));
    });
}

#[test]
fn runtime_division_publishes_its_quotient_bound_without_a_later_product() {
    for transfer in [
        "let quotient = count / divisor;",
        "let quotient = 0_u64;\n  set quotient = count / divisor;",
    ] {
        let source = format!(
            "fn quotient_bound(count: u64, divisor: u64) -> result: u64 pure contract {{
  requires 1_u64 <= divisor;
  ensures result <= count;
}} {{
  {transfer}
  return quotient;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("runtime quotient bound must establish the postcondition: {outcome:?}");
            };
            let function = named(&checked.data.functions, "quotient_bound");
            validate_derivations(&function.entailment);
            assert!(function.entailment.postconditions[0].aggregate.discharged);
            assert!(quotient_order_is_retained(&function.entailment));
        });
    }
}

#[test]
fn unsigned_literal_division_publishes_the_scaled_quotient_image() {
    let source = br#"fn doubled_floor(count: u64) -> result: u64 pure contract {
  ensures result <= count;
} {
  let quotient = count / 2_u64;
  let doubled = quotient * 2_u64;
  return doubled;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!(
                "the scaled quotient image must prove both multiplication and return: {outcome:?}"
            );
        };
        let function = named(&checked.data.functions, "doubled_floor");
        let domains = function
            .entailment
            .obligations
            .iter()
            .filter(|outcome| outcome.family == ObligationFamily::IntegerDomain)
            .collect::<Vec<_>>();
        assert_eq!(
            domains.len(),
            2,
            "division and multiplication each own one domain"
        );
        assert!(domains.iter().all(|outcome| outcome.discharged));
        assert!(function.entailment.postconditions[0].aggregate.discharged);
    });
}

/// The scaled quotient image proves the doubled midpoint relation
/// `2*mid + 1 <= 2*hi` automatically, and AUTO then halves that published fact
/// over the integers, so the midpoint subscript needs no written certificate.
#[test]
fn the_scaled_quotient_image_halves_into_an_automatic_midpoint_bound() {
    let source = br#"fn probe(table: &[u8], lo: u64, hi: u64) -> found: u8 reads(table) contract {
  define spare = deref(table).len;
  requires lo < hi;
  requires hi <= spare;
} {
  let span = hi - lo;
  let half = span / 2_u64;
  let mid = lo + half;
  invariant inside: 2_u64 * mid + 1_u64 <= 2_u64 * hi;
  let byte = deref(table)[mid];
  return byte;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the halved midpoint fact must discharge the subscript: {outcome:?}");
        };
        let function = named(&checked.data.functions, "probe");
        validate_derivations(&function.entailment);
        assert!(
            function.entailment.source_proofs.is_empty()
                || function
                    .entailment
                    .source_proofs
                    .iter()
                    .all(|proof| proof.check.premises.is_empty()),
            "the automatic route uses no written premise"
        );
        let bounds = function
            .entailment
            .obligations
            .iter()
            .filter(|outcome| outcome.family == ObligationFamily::Bounds)
            .collect::<Vec<_>>();
        assert_eq!(bounds.len(), 1, "the one subscript owns one bounds goal");
        assert!(bounds[0].discharged);
    });
}

#[test]
fn signed_literal_division_does_not_publish_unsigned_ordering_images() {
    let source = br#"fn signed_half(value: i32) -> result: i32 pure contract {
  ensures result <= value;
} {
  let quotient = value / 2_i32;
  return quotient;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("signed truncation does not imply quotient <= dividend: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn9);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::UndischargedPostcondition(_)
        ));
    });
}

#[test]
fn unsigned_zero_literal_still_fails_the_division_domain() {
    let source = br#"fn invalid_divisor(value: u64) -> result: u64 pure {
  let quotient = value / 0_u64;
  return quotient;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("zero remains outside the exact unsigned division domain: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::UndischargedIntegerDomainObligation {
                disposition: StaticObligationDisposition::Refuted,
                ..
            }
        ));
    });
}

#[test]
fn replacing_the_dividend_does_not_retarget_the_old_division_image() {
    let source = br#"fn replace_dividend(count: u64, replacement: u64) -> result: u64 pure {
  let quotient = count / 2_u64;
  set count = replacement;
  let doubled = quotient * 2_u64;
  let difference = count - doubled;
  return difference;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the old dividend value must not constrain its replacement: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        assert_eq!(&source[start..end], b"count - doubled");
    });
}

/// [OP-2]'s mechanical fixes are writable at a signed type. A verified
/// requirement establishes the complete body fact through S4, while a branch
/// establishes the original divisor's disequality on its taken edge.
#[test]
fn the_signed_zero_divisor_conjunct_is_discharged_by_its_own_mechanical_fix() {
    let required = br#"fn ratio(divisor: i32) -> result: i32 pure contract {
  requires divisor != 0_i32;
} {
  let q = 100_i32 / divisor;
  return q;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(required, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the required disequality must discharge the conjunct: {outcome:?}");
        };
        let ratio = named(&checked.data.functions, "ratio");
        assert!(
            division_outcomes(ratio)
                .iter()
                .all(|outcome| outcome.discharged),
        );
    });
    let branched = br#"fn ratio(d: i32) -> result: i32 pure {
  if d != 0_i32 {
    let q = 100_i32 / d;
    return q;
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(branched, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the dominating branch must discharge the conjunct: {outcome:?}");
        };
        // The site sits inside the taken arm, so the obligation outcomes,
        // not the top-level statement walk, carry the verdict here.
        let ratio = named(&checked.data.functions, "ratio");
        let discharged = division_outcomes(ratio);
        assert_eq!(discharged.len(), 1, "one source occurrence, one obligation");
        assert!(discharged.iter().all(|outcome| outcome.discharged));
    });
    let unproved = br#"fn ratio(d: i32) -> result: i32 pure {
  let q = 100_i32 / d;
  return q;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(unproved, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("without either route the conjunct must stay undischarged: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "100_i32 /defined d",
            StaticObligationDisposition::Unproved,
            "add `requires 100_i32 /defined d;` to the `contract` of `ratio`",
        );
    });
}

#[test]
fn active_invariants_prove_signed_division_and_remainder_domains() {
    let source = br#"fn exact_pairs(dividend_start: i32, divisor_start: i32, limit: u64) -> result: unit pure contract {
  requires -10_i32 <= dividend_start;
  requires dividend_start <= 100_i32;
  requires 1_i32 <= divisor_start;
  requires divisor_start <= 100_i32;
  requires limit <= 10_u64;
} {
  let dividend = dividend_start;
  let divisor = divisor_start;
  for @items (
    i in 0_u64..limit,
    invariant dividend_lower: -10_i32 <= dividend,
    invariant dividend_progress: dividend <= dividend_start + i,
    invariant divisor_positive: 1_i32 <= divisor,
    invariant divisor_progress: divisor <= divisor_start + i
  ) {
    let quotient_positive = -2147483648_i32 / divisor;
    let remainder_positive = -2147483648_i32 % divisor;
    let quotient_negative_one = dividend / -1_i32;
    let remainder_negative_one = dividend % -1_i32;
    set dividend = dividend + 1_i32;
    set divisor = divisor + 1_i32;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the active invariants must prove all signed domains: {outcome:?}");
        };
        let function = named(&checked.data.functions, "exact_pairs");
        let domains = function
            .entailment
            .obligations
            .iter()
            .filter(|outcome| {
                outcome.family == ObligationFamily::IntegerDomain
                    && matches!(
                        &outcome.canonical_goal,
                        Some(GoalExpression::Operation {
                            row: GoalOperation::Integer {
                                operation: CheckedIntegerOperation::DivideDefined
                                    | CheckedIntegerOperation::RemainderDefined,
                                ..
                            },
                            ..
                        })
                    )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            domains.len(),
            4,
            "two division and two remainder sites remain distinct"
        );
        for domain in domains {
            assert_eq!(domain.components.len(), 3);
            assert!(domain.discharged);
            assert!(domain.residual.is_none());

            let root = domain
                .derivation
                .expect("the accepted signed operation retains a derivation root");
            let mut seen = vec![false; function.entailment.derivations.nodes.len()];
            let mut stack = vec![root];
            let mut used_invariant = false;
            while let Some(node) = stack.pop() {
                let index = node.0 as usize;
                if seen[index] {
                    continue;
                }
                seen[index] = true;
                let retained = &function.entailment.derivations.nodes[index];
                used_invariant |= matches!(
                    retained,
                    DerivationNode::AffineConsequence {
                        premises,
                        ..
                    } if !premises.is_empty()
                );
                stack.extend(retained.parent_ids());
            }
            assert!(
                used_invariant,
                "each signed domain proof must consume a source invariant"
            );
        }
    });
}

#[test]
fn a_fixed_run_indexed_defined_guard_discharges_the_same_structural_exact_operation() {
    // A `Slots<u8, 1>` window is the v0.60 spelling of the run this case used
    // to write `FixedVector<u8, 1>`. A window's length is a descriptor word
    // rather than a fact of its type [WIN-1], so the subscript's [OP-4] domain
    // is stated as a requirement; the subject, one canonical index goal reused
    // by the `+defined` guard and the exact addition, is unchanged, and the
    // goal row is still `RunIndex`. An `own` parameter carries no effect entry
    // [EFF-1], so the row is `pure` where it declared a read.
    let source = br#"fn increment(values: Slots<u8, 1>) -> result: u8 pure contract {
  requires values.len >= 1_u64;
} {
  if values[0_u64] +defined 1_u8 {
    let result = values[0_u64] + 1_u8;
    return result;
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let filled = array_filled::<u8, 1>(value: 0_u8);
  let values = slots_from_array::<u8, 1>(values: filled);
  let result = increment(values: move values);
  return std::process::exit_status(code: result);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the identical indexed value must retain one structural goal: {outcome:?}");
        };
        let increment = named(&checked.data.functions, "increment");
        let exact = increment
            .entailment
            .obligations
            .iter()
            .find(|outcome| outcome.family == ObligationFamily::IntegerDomain)
            .expect("the exact addition retains its OP-2 obligation");
        assert!(exact.discharged);
        let Some(GoalExpression::Operation { arguments, .. }) = &exact.canonical_goal else {
            panic!("every OP-2 occurrence must retain its canonical goal");
        };
        assert!(matches!(
            arguments.as_slice(),
            [
                GoalExpression::Operation {
                    row: GoalOperation::RunIndex { .. },
                    ..
                },
                GoalExpression::Datum(_)
            ]
        ));
    });
}

#[test]
fn writing_the_indexed_collection_invalidates_its_old_defined_fact() {
    let source = br#"fn increment_after_write(values: Slots<u8, 1>) -> result: u8 pure contract {
  requires values.len >= 1_u64;
} {
  if values[0_u64] +defined 1_u8 {
    set values[0_u64] = 255_u8;
    let result = values[0_u64] + 1_u8;
    return result;
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let filled = array_filled::<u8, 1>(value: 0_u8);
  let values = slots_from_array::<u8, 1>(values: filled);
  let result = increment_after_write(values: move values);
  return std::process::exit_status(code: result);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("a collection write must kill the old indexed goal: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::UndischargedIntegerDomainObligation {
                disposition: StaticObligationDisposition::Unproved,
                ..
            }
        ));
    });
}

#[test]
fn a_referenced_run_indexed_defined_guard_discharges_the_same_structural_exact_operation() {
    // The run reached through a reference parameter replaces the retiring
    // store-resident `&Vector<'heap, u8>`. A constant `Array` takes the
    // `ArrayIndex` row where the window above takes `RunIndex` [MSR-1], so
    // the two cases still pin two distinct index rows.
    let source = br#"fn increment(values: &Array<u8, 2>) -> result: u8 reads(values) {
  let spare = deref(values).len;
  if 0_u64 < spare {
    if deref(values)[0_u64] +defined 1_u8 {
      let result = deref(values)[0_u64] + 1_u8;
      return result;
    } else {
      return 0_u8;
    }
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!(
                "the identical referenced-run element must retain one structural goal: {outcome:?}"
            );
        };
        let increment = named(&checked.data.functions, "increment");
        let exact = increment
            .entailment
            .obligations
            .iter()
            .rfind(|outcome| outcome.family == ObligationFamily::IntegerDomain)
            .expect("the exact addition retains its OP-2 obligation");
        assert!(exact.discharged);
        let Some(GoalExpression::Operation { arguments, .. }) = &exact.canonical_goal else {
            panic!("every OP-2 occurrence must retain its canonical goal");
        };
        assert!(matches!(
            arguments.as_slice(),
            [
                GoalExpression::Operation {
                    row: GoalOperation::ArrayIndex { .. },
                    ..
                },
                GoalExpression::Datum(_)
            ]
        ));
    });
}

#[test]
fn a_range_reference_indexed_defined_guard_discharges_the_same_structural_exact_operation() {
    // The [REF-4] range reference `&[T]` replaces the retiring `Slice<T>`
    // view: the parameter kind is a reference kind and not a type, its one
    // measure is `len`, and its element is reached through `deref` [REF-1].
    // Its measured kind is `Range`, which is the third index row this file
    // pins beside the constant window and the constant array.
    let source = br#"fn increment(values: &[u8]) -> result: u8 reads(values) {
  let spare = deref(values).len;
  if 0_u64 < spare {
    if deref(values)[0_u64] +defined 1_u8 {
      let result = deref(values)[0_u64] + 1_u8;
      return result;
    } else {
      return 0_u8;
    }
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!(
                "the identical range-reference element must retain one structural goal: {outcome:?}"
            );
        };
        let increment = named(&checked.data.functions, "increment");
        let exact = increment
            .entailment
            .obligations
            .iter()
            .rfind(|outcome| outcome.family == ObligationFamily::IntegerDomain)
            .expect("the exact addition retains its OP-2 obligation");
        assert!(exact.discharged);
        let Some(GoalExpression::Operation { arguments, .. }) = &exact.canonical_goal else {
            panic!("every OP-2 occurrence must retain its canonical goal");
        };
        assert!(matches!(
            arguments.as_slice(),
            [
                GoalExpression::Operation {
                    row: GoalOperation::RunIndex {
                        measured: MeasuredKind::Range,
                        ..
                    },
                    ..
                },
                GoalExpression::Datum(_)
            ]
        ));
    });
}

#[test]
fn different_index_offsets_do_not_share_a_defined_fact() {
    let source = br#"const values: Array<u8, 2> =[0_u8, 0_u8];

fn increment_other() -> result: u8 pure {
  if values[0_u64] +defined 1_u8 {
    let result = values[1_u64] + 1_u8;
    return result;
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("different index identities must not share a defined fact: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::UndischargedIntegerDomainObligation {
                disposition: StaticObligationDisposition::Unproved,
                residual,
                ..
            } if residual == "values[1_u64] +defined 1_u8"
        ));
    });
}

#[test]
fn writing_the_index_binding_invalidates_its_old_indexed_defined_fact() {
    let source = br#"const values: Array<u8, 2> =[0_u8, 0_u8];

fn increment_after_index_write() -> result: u8 pure {
  let offset = 1_u64;
  if values[offset] +defined 1_u8 {
    set offset = 0_u64;
    let result = values[offset] + 1_u8;
    return result;
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an index write must kill the old indexed goal: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::UndischargedIntegerDomainObligation {
                disposition: StaticObligationDisposition::Unproved,
                residual,
                ..
            } if residual == "values[offset] +defined 1_u8"
        ));
    });
}

#[test]
fn runtime_unsigned_division_retains_the_checked_product_bound_in_either_order() {
    for product in ["quotient * divisor", "divisor * quotient"] {
        let source = format!(
            r#"fn covered(count: u64, divisor: u64) -> result: u64 pure contract {{
  requires count <= 16777216_u64;
  requires 1_u64 <= divisor;
  requires divisor <= 65536_u64;
  ensures result <= count;
}} {{
  let quotient = count / divisor;
  let product = {product};
  return product;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("runtime quotient-product consequence: {product}: {outcome:?}");
            };
            let function = named(&checked.data.functions, "covered");
            validate_derivations(&function.entailment);
            assert!(function.entailment.postconditions[0].aggregate.discharged);
            assert!(
                function
                    .entailment
                    .derivations
                    .nodes
                    .iter()
                    .any(|node| { matches!(node, DerivationNode::UnsignedDivisionProduct { .. }) })
            );
        });
    }
}

#[test]
fn captured_runtime_division_values_survive_only_in_their_unchanged_aliases() {
    let source = br#"fn covered(count: u64, divisor: u64) -> result: u64 pure contract {
  requires count <= 4096_u64;
  requires 1_u64 <= divisor;
  requires divisor <= 64_u64;
} {
  let quotient = count / divisor;
  let old_count = count;
  let old_divisor = divisor;
  let old_quotient = quotient;
  set count = 0_u64;
  set divisor = 1_u64;
  set quotient = 4096_u64;
  let product = old_divisor * old_quotient;
  invariant covered_input: product <= old_count;
  return product;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("unchanged aliases retain captured values: {outcome:?}");
        };
        validate_derivations(&named(&checked.data.functions, "covered").entailment);
    });
}
