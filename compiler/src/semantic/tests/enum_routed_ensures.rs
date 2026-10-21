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
        }
    });
}

#[test]
fn explicit_enum_routes_keep_their_result_ordinals() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/call4-pos-enum-explicit-ordinals.wf"
    );
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
