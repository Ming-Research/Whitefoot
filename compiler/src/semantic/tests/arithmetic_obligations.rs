//! Static integer-domain obligations for exact `+`, `-`, and `*` operations
//! [OP-2, ENT-6]. Every exact site must establish its canonical `.defined`
//! predicate. A proof discharges the obligation without adding an effect; an
//! unproved or refuted obligation rejects under OP-2. Total suffixed modes do
//! not carry this obligation.

use crate::{
    SemanticIssueKind, SemanticLocation, SemanticOutcome, SemanticRule, StaticObligationDisposition,
};

use super::super::entailment::{
    DerivationNode, ObligationFamily, OverflowConjuncts, overflow_conjuncts_for_values,
};
use super::super::goal::{GoalExpression, GoalOperation};
use super::super::model::{CheckedFunction, CheckedIntegerOperation, IntegerType};
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

/// The repair fragment for a goal over an element read, which a condition
/// naming the same admitted expression establishes [ENT-3] although the
/// read is no term [ENT-2].
const ELEMENT_ROUTES: &str = "is not proved here: when facts that reach the operation imply it";

fn named<'functions>(
    functions: &'functions [CheckedFunction],
    name: &str,
) -> &'functions CheckedFunction {
    functions
        .iter()
        .find(|function| function.name == name)
        .expect("named function is checked")
}

/// A verified callable-boundary requirement enters the body as an ordinary S4
/// fact and discharges both conjuncts of the exact addition.
#[test]
fn a_verified_requirement_discharges_the_literal_site() {
    let source = br#"fn bump(x: u64) -> result: u64 pure contract {
  requires x < 1000_u64;
} {
  let y = x + 1_u64;
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a requirement-dominated literal site must be accepted: {outcome:?}");
        };
        let bump = named(&checked.data.functions, "bump");
        let overflow: Vec<_> = bump
            .entailment
            .obligations
            .iter()
            .filter(|outcome| outcome.family == ObligationFamily::IntegerDomain)
            .collect();
        assert_eq!(overflow.len(), 1, "one source occurrence, one obligation");
        assert_eq!(overflow[0].components.len(), 2);
        assert!(
            overflow.iter().all(|outcome| outcome.discharged),
            "both conjuncts discharge: the requirement bounds the operand and the \
             implicit type bound closes the trivial side",
        );
        super::entailment::validate_derivations(&bump.entailment);
        let root = overflow[0]
            .derivation
            .expect("the accepted OP-2 site retains one proof root");
        let DerivationNode::IntegerDomain {
            goal: Some(_),
            parents,
        } = &bump.entailment.derivations.nodes[root.0 as usize]
        else {
            panic!("the finite OP-2 route concludes the canonical integer-domain goal");
        };
        let [parent] = parents.as_slice() else {
            panic!("the finite OP-2 route retains one normalized-goal proof");
        };
        assert!(matches!(
            bump.entailment.derivations.nodes[parent.0 as usize],
            DerivationNode::GoalNormalization { .. }
        ));
    });
}

/// R1 bridges the ordinary true-edge difference fact `right <= left` into
/// the affine domain goal for two nonconstant subtraction operands.
#[test]
fn a_guarded_two_value_subtraction_uses_the_l0_affine_bridge() {
    let source = br#"fn distance(left: u64, right: u64) -> result: u64 pure {
  let ordered = right <= left;
  if ordered {
    let difference = left - right;
    return difference;
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
            panic!("the guarded subtraction must use its true-edge fact: {outcome:?}");
        };
        let distance = named(&checked.data.functions, "distance");
        let subtractions = distance
            .entailment
            .obligations
            .iter()
            .filter(|outcome| outcome.family == ObligationFamily::IntegerDomain)
            .collect::<Vec<_>>();
        let [subtraction] = subtractions.as_slice() else {
            panic!("the exact subtraction retains one integer-domain obligation");
        };
        assert!(subtraction.discharged);
        super::entailment::validate_derivations(&distance.entailment);
    });

    let unguarded = br#"fn distance(left: u64, right: u64) -> result: u64 pure {
  let difference = left - right;
  return difference;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(unguarded, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the unguarded subtraction must remain unproved: {outcome:?}");
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

/// The loop-counter shape from the recorded measurements: the counted
/// binder's `binder < upper_capture` body fact plus the capture's implicit
/// type bound discharge `i + 1_u64` by the same transitive closure the
/// index obligation uses.
#[test]
fn the_counted_binder_increment_discharges_by_transitive_closure() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let n = 10_u64;
  for @steps (i in 0_u64..n) {
    let next = i + 1_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a counted-binder increment must be accepted: {outcome:?}");
        };
        let main = named(&checked.data.functions, "main");
        assert!(
            main.entailment
                .obligations
                .iter()
                .filter(|outcome| outcome.family == ObligationFamily::IntegerDomain)
                .all(|outcome| outcome.discharged),
            "the binder increment discharges inside the counted body",
        );
    });
}

/// An operand nothing bounds leaves the binding conjunct underivable: the
/// program rejects citing OP-2 at the `infix` node with the exact folded
/// residual. Exact arithmetic contributes no runtime effect, so `pure` is the
/// correct row for this body.
#[test]
fn an_unbounded_literal_site_rejects_citing_op2_with_the_folded_residual() {
    let source = br#"fn bump(x: u64) -> result: u64 pure {
  let y = x + 1_u64;
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an unbounded literal site must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "x +defined 1_u64",
            StaticObligationDisposition::Unproved,
            "add `requires x +defined 1_u64;` to the `contract` of `bump`",
        );
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        assert_eq!(
            std::str::from_utf8(&source[start..end]).expect("cited bytes are text"),
            "x + 1_u64",
            "the rejection lands on the infix node",
        );
    });
}

/// The taken edge of a written comparison is an ordinary S1 fact, so the
/// exact operation is accepted only in the branch where its domain holds.
#[test]
fn a_dominating_branch_discharges_the_site() {
    let source = br#"fn bump(x: u64) -> result: u64 pure {
  if x <= 100_u64 {
    let y = x + 1_u64;
    return y;
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
            panic!("a branch-dominated literal site must be accepted: {outcome:?}");
        };
        let bump = named(&checked.data.functions, "bump");
        assert!(
            bump.entailment
                .obligations
                .iter()
                .filter(|outcome| outcome.family == ObligationFamily::IntegerDomain)
                .all(|outcome| outcome.discharged),
            "the taken branch's established fact discharges the obligation",
        );
    });
}

/// `.wrap` is total: no exact-domain obligation attaches, the operation stays
/// pure, and the checked program keeps its wrap identity.
#[test]
fn a_wrap_site_attaches_no_obligation() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let x = 6_u64;
  let y = x +wrap 1_u64;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a wrap site must be accepted: {outcome:?}");
        };
        let main = named(&checked.data.functions, "main");
        assert!(
            main.entailment
                .obligations
                .iter()
                .all(|outcome| outcome.family == ObligationFamily::Bounds),
            "no overflow obligation attaches to a wrap site",
        );
    });
}

/// Two local operands retain their exact affine values, so a fixed in-range
/// sum discharges without a written goal. Unbounded parameters retain no such
/// upper interval and still require proof at the exact site.
#[test]
fn exact_local_values_discharge_a_two_variable_sum_but_parameters_remain_bounded() {
    let exact_locals = br#"fn main() -> status: std::process::ExitStatus pure {
  let a = 6_u64;
  let b = 7_u64;
  let c = a + b;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(exact_locals, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the retained exact values prove the local sum: {outcome:?}");
        };
        let main = named(&checked.data.functions, "main");
        let addition = main
            .entailment
            .obligations
            .iter()
            .find(|outcome| outcome.family == ObligationFamily::IntegerDomain)
            .expect("the exact addition still owns one OP-2 obligation");
        assert!(addition.discharged);
    });

    let unbounded_parameters = br#"fn add(a: u64, b: u64) -> result: u64 pure {
  let result = a + b;
  return result;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(unbounded_parameters, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("unbounded parameter intervals cannot prove the sum: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
    });
}

/// Two constant operands make the obligation ground: an in-range result
/// discharges, while an inevitable overflow is a compile-time rejection.
#[test]
fn a_ground_obligation_discharges_in_range_and_rejects_on_inevitable_overflow() {
    let in_range = br#"fn main() -> status: std::process::ExitStatus pure {
  let x = 254_u8 + 1_u8;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(in_range, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("an in-range constant fold must be accepted: {outcome:?}");
        };
        let main = named(&checked.data.functions, "main");
        let overflow: Vec<_> = main
            .entailment
            .obligations
            .iter()
            .filter(|outcome| outcome.family == ObligationFamily::IntegerDomain)
            .collect();
        assert_eq!(overflow.len(), 1, "one exact site, one obligation");
        assert!(overflow[0].discharged, "the ground obligation is true");
    });
    let overflowing = br#"fn main() -> status: std::process::ExitStatus pure {
  let x = 255_u8 + 1_u8;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(overflowing, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an inevitable constant overflow must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "255_u8 +defined 1_u8",
            StaticObligationDisposition::Refuted,
            "make `255_u8 +defined 1_u8` false",
        );
    });
}

/// A subscripted class operand is outside ENT-2's term vocabulary: the
/// relation is underivable, never ill-formed, and the rejection renders the
/// subscripted operand exactly — the one-`let` rebinding fallback then
/// makes the operand a term, mirroring the subscript-offset fallback.
#[test]
fn a_subscripted_class_operand_is_underivable_and_rejects() {
    let source = br#"const a: Array<u8, 2> =[7_u8, 7_u8];

fn main() -> status: std::process::ExitStatus pure {
  let y = a[0_u64] + 1_u8;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("a non-term class operand must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "a[0_u64] +defined 1_u8",
            StaticObligationDisposition::Unproved,
            ELEMENT_ROUTES,
        );
    });
}

/// [TYPE-9] a `Box`'s content is its field `inner`, reached and rendered by
/// that ordinary field step. The indexed operand remains no term.
#[test]
fn an_owning_box_index_renders_its_content_step_as_a_dereference() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 2>(value: 7_u8);
  let boxed = box_new::<Array<u8, 2>>(value: values);
  let result = boxed.inner[0_u64] + 1_u8;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the indexed operand remains no term: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "boxed.inner[0_u64] +defined 1_u8",
            StaticObligationDisposition::Unproved,
            ELEMENT_ROUTES,
        );
    });
}

/// [REF-1] "A reference variable denotes the reference, and the storage it
/// names is reached only through `deref` [TYPE-7]: every place expression,
/// subscript, field selection, payload step, and measure read that goes
/// through a reference variable `p` is written under that step -- `deref(p)`,
/// `deref(p).field`, `deref(part)[i]`, `deref(part).len`, and
/// `deref(p).Some.value`." The residual therefore spells the indexed operand
/// under its `deref` step, which is what the writer wrote and the only
/// spelling the writer can write. Resolving that step away is what [OWN-7]
/// does to decide overlap, not what a diagnostic prints. The operand is still
/// no term, which is what the rejection states.
#[test]
fn a_reference_parameter_index_renders_under_its_deref_step() {
    let source = br#"fn increment(values: &Array<u8, 2>) -> result: u8 reads(values) {
  return deref(values)[0_u64] + 1_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the indexed operand remains no term: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "deref(values)[0_u64] +defined 1_u8",
            StaticObligationDisposition::Unproved,
            ELEMENT_ROUTES,
        );
    });
}

/// Rule precedence is stable on the default semantic path: a declared effect
/// entry the body does not exhibit rejects under EFF-2 before an unproved
/// exact-site obligation, while the matching `pure` row reaches OP-2.
///
/// v0.59 wrote that unexhibited entry as `allocates(heap)`. There is no
/// allocation category in v0.60: allocation and release carry no effect entry
/// at all [EFF-1, STOR-8], so the unexhibited entry is an ordinary
/// `writes(cell)` of a reference parameter the body never writes.
#[test]
fn effect_mismatch_precedes_static_integer_domain_rejection() {
    let extra_effect_row = br#"fn bump(cell: &u64, x: u64) -> result: u64 writes(cell) {
  let y = x + 1_u64;
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(extra_effect_row, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an exact site does not exhibit the declared write: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Eff2);
        assert!(
            matches!(issue.kind(), SemanticIssueKind::EffectMismatch { .. }),
            "expected EffectMismatch, got {:?}",
            issue.kind()
        );
    });
    let pure_row = br#"fn bump(x: u64) -> result: u64 pure {
  let y = x + 1_u64;
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(pure_row, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the unbounded class site must reject on OP-2: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op2);
        assert_integer_domain(
            issue.kind(),
            "x +defined 1_u64",
            StaticObligationDisposition::Unproved,
            "add `requires x +defined 1_u64;` to the `contract` of `bump`",
        );
    });
    let ground = br#"fn main() -> status: std::process::ExitStatus pure {
  let x = 255_u8 + 1_u8;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(ground, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an inevitable constant overflow must reject: {outcome:?}");
        };
        assert_eq!(
            issue.rule(),
            SemanticRule::Op2,
            "there is no accepted always-trapping bare spelling",
        );
    });
}

#[test]
fn a_defined_guard_reuses_the_complete_identity_of_an_exact_let_operand() {
    let source = br#"fn combine(start: u64, parent: u64) -> result: u64 pure contract {
  requires parent <= 9223372036854775807_u64;
} {
  let doubled = parent * 2_u64;
  let addition_defined = start +defined doubled;
  if addition_defined {
    let result = start + doubled;
    return result;
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
            panic!("the matching defined guard must admit the later exact addition: {outcome:?}");
        };
        let combine = named(&checked.data.functions, "combine");
        let domains = combine
            .entailment
            .obligations
            .iter()
            .filter(|outcome| outcome.family == ObligationFamily::IntegerDomain)
            .collect::<Vec<_>>();
        let [multiply, addition] = domains.as_slice() else {
            panic!("the exact multiply and addition retain two OP-2 obligations");
        };
        assert!(multiply.discharged);
        assert!(addition.discharged);
        let Some(GoalExpression::Operation { arguments, .. }) = &addition.canonical_goal else {
            panic!("the addition retains its canonical domain goal");
        };
        assert!(matches!(
            arguments.as_slice(),
            [
                GoalExpression::Datum(_),
                GoalExpression::Operation {
                    row: GoalOperation::Integer {
                        operation: CheckedIntegerOperation::MultiplyExact,
                        ..
                    },
                    ..
                }
            ]
        ));
    });
}

/// The folded conjunct constants over the signed corners, checked directly
/// against the [ENT-6] table: the sign reversals of negative multiplication
/// constants and the exact floor/ceil quotients are where the arithmetic
/// can silently go wrong.
#[test]
fn the_conjunct_fold_matches_the_ent6_table() {
    let conjuncts = |operation: CheckedIntegerOperation,
                     constant: i128,
                     constant_is_left: bool,
                     ty: IntegerType| {
        let OverflowConjuncts {
            upper,
            lower,
            ground,
            ..
        } = overflow_conjuncts_for_values(
            operation,
            constant_is_left.then_some(constant),
            (!constant_is_left).then_some(constant),
            ty,
        )
        .expect("one constant operand has a fixed normalization");
        (upper, lower, ground)
    };
    // t + 200 over u8: t <= 55; the lower side is the implicit type bound.
    assert_eq!(
        conjuncts(
            CheckedIntegerOperation::AddExact,
            200,
            false,
            IntegerType::U8,
        ),
        (55, 200, false),
    );
    // t + (-3) over i8: t >= -125 binds; the upper side is relaxed.
    assert_eq!(
        conjuncts(
            CheckedIntegerOperation::AddExact,
            -3,
            false,
            IntegerType::I8,
        ),
        (130, 125, false),
    );
    // t - 3 over u8: 3 <= t binds.
    assert_eq!(
        conjuncts(
            CheckedIntegerOperation::SubtractExact,
            3,
            false,
            IntegerType::U8,
        ),
        (258, -3, false),
    );
    // (-100) - t over i8: t <= 28 binds.
    assert_eq!(
        conjuncts(
            CheckedIntegerOperation::SubtractExact,
            -100,
            true,
            IntegerType::I8,
        ),
        (28, 227, false),
    );
    // t * 2 over i8: -64 <= t <= 63.
    assert_eq!(
        conjuncts(
            CheckedIntegerOperation::MultiplyExact,
            2,
            false,
            IntegerType::I8,
        ),
        (63, 64, false),
    );
    // t * (-2) over i8: the bounds swap ends: -63 <= t <= 64.
    assert_eq!(
        conjuncts(
            CheckedIntegerOperation::MultiplyExact,
            -2,
            false,
            IntegerType::I8,
        ),
        (64, 63, false),
    );
    // t * 0 is zero for every operand: ground and in range.
    assert_eq!(
        conjuncts(
            CheckedIntegerOperation::MultiplyExact,
            0,
            false,
            IntegerType::I8,
        ),
        (0, 0, true),
    );
    // Ground u64 * u64 at the magnitude ceiling: exact, and out of range.
    let max = i128::from(u64::MAX);
    let result = overflow_conjuncts_for_values(
        CheckedIntegerOperation::MultiplyExact,
        Some(max),
        Some(max),
        IntegerType::U64,
    )
    .expect("two constants have a ground normalization");
    assert_eq!((result.upper, result.lower, result.ground), (-1, -1, true));
    assert_eq!(
        result
            .ground_result
            .expect("ground obligations retain their exact result")
            .render(),
        "340282366920938463426481119284349108225",
    );
}

/// [OP-2, ENT-6, INV-1, DIAG-1] A body operand nothing bounds leaves its own
/// `.defined` obligation underivable, and ENT-6 then forms the binding it
/// initializes as a fresh full-range atom. That demoted value is what breaks
/// the enclosing header invariant at the backedge, so two obligations fail for
/// one cause.
///
/// DIAG-1 admits exactly one rule and one location, and the one reported is
/// the cause rather than its consequence: every judgment is ordered where the
/// checker decides it, and INV-1's backedge judgment is decided only after the
/// entire loop body has been walked.
#[test]
fn a_body_domain_failure_precedes_the_backedge_it_breaks() {
    let source = br#"fn accumulate(step: u32) -> total: u32 pure {
  let sum = 0_u32;
  for (
    i in 0_u64..4_u64,
    invariant carried: sum <= 255_u32
  ) {
    let doubled = step + step;
    set sum = doubled;
  }
  return sum;
}

fn main() -> status: std::process::ExitStatus pure {
  let total = accumulate(step: 1_u32);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the unbounded body addition must reject: {outcome:?}");
        };
        assert_eq!(
            issue.rule(),
            SemanticRule::Op2,
            "the body obligation that demoted the value is the reported rejection",
        );
        assert_integer_domain(
            issue.kind(),
            "step +defined step",
            StaticObligationDisposition::Unproved,
            "add `requires step +defined step;` to the `contract` of `accumulate`",
        );
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        assert_eq!(
            std::str::from_utf8(&source[start..end]).expect("cited bytes are text"),
            "step + step",
            "the rejection lands on the body infix node, not on the loop header",
        );
    });
}
