//! Source enum route identities, declaration diagnostics and verified receipts.

use crate::{SemanticIssueKind, SemanticLocation, SemanticOutcome, SemanticRule};

use super::{assert_rule, with_semantics, with_semantics_dark};

#[test]
fn separate_fields_retain_distinct_route_identities_and_verified_summaries() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/fn9-pos-enum-separate-field-clauses.wf"
    );
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("separate field routes must check: {outcome:?}");
        };
        let function = checked
            .data
            .executable_functions()
            .find(|function| function.name == "pair")
            .expect("pair function");
        assert_eq!(function.postconditions.len(), 2);
        let first = &function.postconditions[0].selector;
        let second = &function.postconditions[1].selector;
        assert_eq!(first.ordinal, 0);
        assert_eq!(second.ordinal, 0);
        assert_eq!(first.variant, second.variant);
        let first_route = first.route.expect("first field route");
        let second_route = second.route.expect("second field route");
        assert_eq!(first_route.variant, second_route.variant);
        assert_eq!(first_route.field, 0);
        assert_eq!(second_route.field, 1);
        assert_eq!(first.field.as_ref().expect("first field").index, 0);
        assert_eq!(second.field.as_ref().expect("second field").index, 1);
        assert_eq!(function.entailment.postconditions.len(), 2);
        for proof in &function.entailment.postconditions {
            assert!(proof.aggregate.discharged);
            assert!(proof.summary.is_some());
            assert!(!proof.vacuous);
        }
    });
}

#[test]
fn explicit_enum_routes_keep_their_result_ordinals() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/call4-pos-enum-explicit-ordinals.wf");
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("explicit ordinal routes must check: {outcome:?}");
        };
        let function = checked
            .data
            .executable_functions()
            .find(|function| function.name == "pair")
            .expect("pair function");
        assert_eq!(function.postconditions[0].selector.ordinal, 0);
        assert_eq!(function.postconditions[1].selector.ordinal, 1);
        assert_eq!(
            function.postconditions[0].selector.route,
            function.postconditions[1].selector.route
        );
        assert!(
            function
                .entailment
                .postconditions
                .iter()
                .all(|proof| proof.aggregate.discharged && proof.summary.is_some())
        );
    });
}

#[test]
fn a_missing_source_variant_field_is_reported_at_its_fieldbind() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/fn9-neg-enum-missing-field.wf");
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("missing field must be an FN-9 source issue: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn9);
        assert_eq!(
            issue.kind(),
            &SemanticIssueKind::InvalidPostconditionFields {
                required_fields: vec!["first".to_owned(), "second".to_owned()],
            }
        );
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        assert_eq!(&source[start..end], b"missing: a");
    });
}

#[test]
fn a_variant_owned_by_another_enum_is_an_fn9_selector_issue() {
    assert_rule(
        include_bytes!("../../../../tests/conformance/cases/fn9-neg-enum-foreign-variant.wf"),
        SemanticRule::Fn9,
        SemanticIssueKind::invalid_postcondition_selector(),
    );
}

#[test]
fn a_nondata_generic_instance_is_refused_at_its_route_and_names_its_request() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/fn9-neg-enum-generic-nondata-instance.wf"
    );
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("Bool route admission must be an FN-9 source issue: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn9);
        assert_eq!(
            issue.kind(),
            &SemanticIssueKind::invalid_postcondition_selector()
        );
        let cited = |coordinate: crate::SyntaxCoordinate| {
            let start = usize::try_from(coordinate.start().value()).expect("offset fits");
            let end = usize::try_from(coordinate.end().value()).expect("offset fits");
            &source[start..end]
        };
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        assert_eq!(cited(*coordinate), b"Held(data: payload)");
        let request = issue.request.expect("concrete Bool instance requester");
        assert_eq!(request.source(), coordinate.source());
        assert_eq!(cited(request), b"retain::<Bool>(value: flag)");
    });
}

#[test]
fn unread_routed_call_keeps_its_required_conditional_root() {
    let source = br#"enum Outcome {
  Some(value: u64);
  None();
}

fn make() -> result: Outcome pure contract {
  ensures when Some(value: n): n <= 7_u64;
} {
  return Outcome::Some(value: 7_u64);
}

fn discard(flag: Bool) -> result: unit pure {
  let unread = make();
  if flag {
    set unread = Outcome::None();
  }
  return unit;
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("unread call must check: {outcome:?}");
        };
        let function = checked
            .data
            .executable_functions()
            .find(|function| function.name == "discard")
            .expect("discard");
        assert_eq!(function.entailment.derivations.roots.iter().filter(|root| {
            matches!(root.kind, crate::semantic::entailment::DerivationRootKind::PostconditionConditional { .. })
        }).count(), 1);
    });
}

#[test]
fn future_match_retains_construction_exclusion_through_copy_and_join() {
    let source = br#"enum Outcome {
  Some(value: u64);
  None();
}

fn read(value: u64, unknown: Outcome) -> result: u8 pure {
  let selected = Outcome::None();
  if value <= 7_u64 {
    set selected = unknown;
  }
  let copied = selected;
  match copied {
    Some(value: ignored) => {
      return cvt::<u64, u8>(value);
    }
    None() => {
      return 0_u8;
    }
  }
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "selecting Some must recover value <= 7 from its only possible branch: {outcome:?}"
        );
    });

    // A bound covering all u64 values supplies no evidence for narrowing,
    // even when Some still comes from just one branch of the same join.
    let unbounded = std::str::from_utf8(source)
        .expect("UTF-8 witness")
        .replace("value <= 7_u64", "value <= 18446744073709551615_u64");
    with_semantics(unbounded.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("the unproved narrowing must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op6);
        let SemanticIssueKind::UndischargedConversionDomainObligation {
            disposition,
            residual,
            ..
        } = issue.kind()
        else {
            panic!("expected the narrowing obligation, got {:?}", issue.kind());
        };
        assert_eq!(*disposition, crate::StaticObligationDisposition::Unproved);
        assert_eq!(residual, "cvt.defined::<u64, u8>(value)");
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        assert_eq!(&unbounded.as_bytes()[start..end], b"cvt::<u64, u8>(value)");
    });
}

#[test]
fn a_call_requirement_retains_its_copied_argument_route() {
    let source = br#"struct Span {
  low: u64;
  high: u64;
  invariant ordered(span): span.low <= span.high;
}

fn take(value: Option<Span>) -> result: u64 pure {
  return 0_u64;
}

fn caller() -> result: u64 pure {
  let span = Span(low: 2_u64, high: 9_u64);
  let made = Some<Span>(value: span);
  let copied = made;
  return take(value: copied);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "the call must receive the copied payload invariant: {outcome:?}"
        );
    });

    // Construct a valid Span first, then invalidate it before wrapping and
    // copying. An unknown Option<Span> parameter would instead establish the
    // routed invariant at body entry and would not be a negative control.
    let unordered = std::str::from_utf8(source)
        .expect("UTF-8 witness")
        .replace(
            "  let made = Some<Span>(value: span);",
            "  set span.low = 10_u64;\n  let made = Some<Span>(value: span);",
        );
    with_semantics(unordered.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("the invalid copied payload must reject at its call: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn8);
        let SemanticIssueKind::UndischargedCallRequirement(detail) = issue.kind() else {
            panic!("expected the routed call requirement, got {:?}", issue.kind());
        };
        assert_eq!(detail.concrete_callee, "take");
        let cited = |location: &SemanticLocation| {
            let SemanticLocation::SourceNode(_, coordinate) = location;
            let start = usize::try_from(coordinate.start().value()).expect("offset fits");
            let end = usize::try_from(coordinate.end().value()).expect("offset fits");
            &unordered.as_bytes()[start..end]
        };
        assert_eq!(cited(issue.location()), b"take(value: copied)");
        assert_eq!(
            cited(&detail.requires_clause),
            b"invariant ordered(span): span.low <= span.high;"
        );
    });
}
