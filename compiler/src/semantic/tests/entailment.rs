//! Unit tests for the L0 entailment engine, one family per [ENT] rule,
//! including the stale-fact and fresh-binding boundary shapes the spec
//! text was reviewed against.
//!
//! Derivation tests observe accepted proof roots through checked-program
//! metadata.

use crate::{
    BindingId, CallRequirementDisposition, NodePath, SemanticIssueKind, SemanticOutcome,
    SemanticRule, SourceInput, StaticObligationDisposition,
};

use super::super::entailment::affine::{AffineCheckState, AffineInequality};
use super::super::entailment::{
    CallGoalDisposition, CallGoalEvidence, CallGoalOutcome, CountedAtomicDerivation,
    CountedCaptureSide, CountedDerivationSet, CountedProofPoint, CountedRootAtom, DerivationId,
    DerivationNode, DerivationRootKind, FlowEvent, FlowEventId, FlowEventKind, FunctionEntailment,
    GoalId, GoalSign, ImplicitBoundKind, JoinParent, MeasureBound, ObligationFamily,
    ObligationOutcome, PlaceRoot, PostconditionCallDetail, PostconditionDeliveryJoinDetail,
    PostconditionDisposition, RangeSeparationOrdering, Relation, SourceAffineFactRef, TermId,
    TermKind, ZERO, type_range,
};
use super::super::goal::{GoalExpression, GoalOperation};
use super::super::model::{
    CheckedBodyDisposition, CheckedConversionMode, CheckedExpression, CheckedIntegerOperation,
    CheckedMeasure, CheckedProgramData, CheckedStatement, CheckedValue, FunctionId, IntegerType,
};
// [REF-1] the v0.59 `PlaceProjection` is retired; one resolved path step is a
// `PlaceStep`, and a term's place carries the whole resolved path rather than
// a separate deref flag and field list.
use super::super::places::{CapturedRange, PlaceStep};
use super::{assert_rule_kind, with_semantics, with_semantics_dark};

fn obligations(source: &[u8], function: &str) -> Vec<ObligationOutcome> {
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("entailment test source must check completely: {outcome:?}");
        };
        let function = checked
            .data
            .functions
            .iter()
            .find(|candidate| candidate.name == function)
            .unwrap_or_else(|| panic!("function {function} must exist"));
        function.entailment.obligations.clone()
    })
}

fn call_goals(source: &[u8], function: &str) -> Vec<CallGoalOutcome> {
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("call-goal test source must check completely: {outcome:?}");
        };
        let function = checked
            .data
            .functions
            .iter()
            .find(|candidate| candidate.name == function)
            .unwrap_or_else(|| panic!("function {function} must exist"));
        function.entailment.call_goals.clone()
    })
}

fn entailment(source: &[u8], function: &str) -> FunctionEntailment {
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("entailment test source must check completely: {outcome:?}");
        };
        checked
            .data
            .functions
            .iter()
            .find(|candidate| candidate.name == function)
            .unwrap_or_else(|| panic!("function {function} must exist"))
            .entailment
            .clone()
    })
}

/// Reads a summary only after the ordinary acceptance path succeeds.
fn accepted_entailment(source: &[u8], function: &str) -> FunctionEntailment {
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("accepted entailment fixture must check completely: {outcome:?}");
        };
        checked
            .data
            .functions
            .iter()
            .find(|candidate| candidate.name == function)
            .unwrap_or_else(|| panic!("function {function} must exist"))
            .entailment
            .clone()
    })
}

fn entailments(source: &[u8], function: &str) -> Vec<FunctionEntailment> {
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("entailment test source must check completely: {outcome:?}");
        };
        checked
            .data
            .functions
            .iter()
            .filter(|candidate| candidate.name == function)
            .map(|candidate| candidate.entailment.clone())
            .collect()
    })
}

fn collect_direct_calls<'checked>(
    statements: &'checked [CheckedStatement],
    callee: FunctionId,
    calls: &mut Vec<(&'checked NodePath, &'checked [CheckedExpression])>,
) {
    fn record<'checked>(
        expression: &'checked CheckedExpression,
        callee: FunctionId,
        calls: &mut Vec<(&'checked NodePath, &'checked [CheckedExpression])>,
    ) {
        if let CheckedExpression::UserCall {
            function,
            call,
            arguments,
            ..
        } = expression
            && *function == callee
        {
            calls.push((call, arguments));
        }
    }

    for statement in statements {
        match statement {
            CheckedStatement::Let { value, .. }
            | CheckedStatement::DestructuringLet { value, .. }
            | CheckedStatement::Set { value, .. }
            | CheckedStatement::Return { value, .. }
            | CheckedStatement::Give { value, .. }
            | CheckedStatement::DropExpression { value, .. } => record(value, callee, calls),
            CheckedStatement::PropagateLet { scrutinee, .. } => record(scrutinee, callee, calls),
            CheckedStatement::Evaluate {
                value: expression, ..
            } => record(expression, callee, calls),
            CheckedStatement::Match {
                scrutinee, arms, ..
            }
            | CheckedStatement::ValueMatchLet {
                scrutinee, arms, ..
            } => {
                record(scrutinee, callee, calls);
                for arm in arms {
                    collect_direct_calls(&arm.body, callee, calls);
                }
            }
            CheckedStatement::Loop { body, .. } => {
                collect_direct_calls(body, callee, calls);
            }
            CheckedStatement::CountedRange {
                lower, upper, body, ..
            } => {
                record(lower, callee, calls);
                record(upper, callee, calls);
                collect_direct_calls(body, callee, calls);
            }
            CheckedStatement::Break { .. } | CheckedStatement::Proof(_) => {}
        }
    }
}

fn discharge_flags(source: &[u8], function: &str) -> Vec<bool> {
    obligations(source, function)
        .iter()
        .map(|outcome| outcome.discharged)
        .collect()
}

fn accepted_discharge_flags(source: &[u8], function: &str) -> Vec<bool> {
    accepted_entailment(source, function)
        .obligations
        .iter()
        .map(|outcome| outcome.discharged)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DerivationConclusion {
    Relation(Relation),
    Goal {
        goal: GoalId,
        sign: GoalSign,
    },
    IntegerDomain(Option<GoalId>),
    AffineConsequence,
    RangeSeparation {
        left: CapturedRange,
        right: CapturedRange,
        ordering: RangeSeparationOrdering,
    },
    IndexSeparation {
        left: crate::semantic::places::CapturedValue,
        right: crate::semantic::places::CapturedValue,
    },
    UnsignedDivisionProduct,
    RequirementAffineImage,
    ContractCall,
    Contradiction,
    PostconditionAggregate,
}

fn retained_term(summary: &FunctionEntailment, id: TermId) -> &TermKind {
    summary
        .inventory
        .terms
        .get(id.0 as usize)
        .unwrap_or_else(|| panic!("retained term ID {id:?} must resolve"))
}

fn assert_relation_terms_resolve(summary: &FunctionEntailment, relation: &Relation) {
    for term in relation.terms() {
        retained_term(summary, term);
    }
}

fn relation_has_bare_binding(
    summary: &FunctionEntailment,
    relation: &Relation,
    binding: super::super::BindingId,
) -> bool {
    relation.terms().into_iter().any(|term| {
        matches!(
            retained_term(summary, term),
            TermKind::Place(place, _)
                if place.root == PlaceRoot::Binding(binding)
                    && place.path.is_empty()
        )
    })
}

fn retained_conclusion(
    conclusions: &[DerivationConclusion],
    id: DerivationId,
) -> &DerivationConclusion {
    conclusions
        .get(id.0 as usize)
        .unwrap_or_else(|| panic!("retained derivation ID {id:?} must resolve"))
}

fn retained_bound(
    conclusions: &[DerivationConclusion],
    id: DerivationId,
    left: TermId,
    right: TermId,
) -> i128 {
    match retained_conclusion(conclusions, id) {
        DerivationConclusion::Relation(Relation::Bound {
            left: held_left,
            right: held_right,
            bound,
        }) if (*held_left, *held_right) == (left, right) => *bound,
        DerivationConclusion::Relation(Relation::Equal {
            left: held_left,
            right: held_right,
            difference,
        }) if *held_left == left && *held_right == right => *difference,
        DerivationConclusion::Relation(Relation::Equal {
            left: held_left,
            right: held_right,
            difference,
        }) if *held_left == right && *held_right == left => -difference,
        conclusion => panic!(
            "retained derivation {id:?} does not prove {left:?} - {right:?} <= k: {conclusion:?}"
        ),
    }
}

fn retained_event(summary: &FunctionEntailment, id: FlowEventId) -> &FlowEvent {
    summary
        .derivations
        .events
        .get(id.0 as usize)
        .unwrap_or_else(|| panic!("retained flow event ID {id:?} must resolve"))
}

fn node_event(node: &DerivationNode) -> Option<FlowEventId> {
    match node {
        DerivationNode::SourceBound { event, .. }
        | DerivationNode::SourceDistinct { event, .. }
        | DerivationNode::SourceGoal { event, .. }
        | DerivationNode::OperationFact { event, .. }
        | DerivationNode::JoinBound { event, .. }
        | DerivationNode::JoinDistinct { event, .. }
        | DerivationNode::JoinGoal { event, .. }
        | DerivationNode::JoinContradiction { event, .. }
        | DerivationNode::MaterializedBound { event, .. }
        | DerivationNode::MaterializedDistinct { event, .. }
        | DerivationNode::MaterializedGoal { event, .. }
        | DerivationNode::MaterializedContradiction { event, .. } => Some(*event),
        _ => None,
    }
}

fn root_contains(
    summary: &FunctionEntailment,
    root: DerivationId,
    predicate: impl Fn(&DerivationNode) -> bool,
) -> bool {
    let mut seen = vec![false; summary.derivations.nodes.len()];
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let index = id.0 as usize;
        if seen[index] {
            continue;
        }
        seen[index] = true;
        let node = &summary.derivations.nodes[index];
        if predicate(node) {
            return true;
        }
        stack.extend(node.parent_ids());
    }
    false
}

fn obligation_root(summary: &FunctionEntailment, ordinal: usize) -> DerivationId {
    summary.obligations[ordinal]
        .derivation
        .unwrap_or_else(|| panic!("obligation {ordinal} must have one exact root"))
}

fn call_root(summary: &FunctionEntailment, ordinal: usize) -> DerivationId {
    summary.call_goals[ordinal]
        .derivation
        .unwrap_or_else(|| panic!("call goal {ordinal} must have one exact root"))
}

fn assert_root_contains(
    summary: &FunctionEntailment,
    root: DerivationId,
    predicate: impl Fn(&DerivationNode) -> bool,
    description: &str,
) {
    assert!(
        root_contains(summary, root, predicate),
        "root {root:?} must contain {description}: {:#?}",
        summary.derivations.nodes
    );
}

fn root_has_event_kind(
    summary: &FunctionEntailment,
    root: DerivationId,
    kind: FlowEventKind,
) -> bool {
    root_contains(summary, root, |node| {
        node_event(node).is_some_and(|event| retained_event(summary, event).kind == kind)
    })
}

fn assert_root_has_event_kind(
    summary: &FunctionEntailment,
    root: DerivationId,
    kind: FlowEventKind,
) {
    assert!(
        root_has_event_kind(summary, root, kind),
        "root {root:?} must descend from {kind:?}: {:#?}",
        summary.derivations.nodes
    );
}

fn normalized_derivation_dump(summary: &FunctionEntailment) -> Vec<u8> {
    format!(
        "{:#?}",
        (
            &summary.derivations.events,
            &summary.derivations.nodes,
            &summary.derivations.roots,
            &summary.counted_derivations,
            &summary.loop_invariants,
            &summary.inventory,
        )
    )
    .into_bytes()
}

fn assert_source_event(summary: &FunctionEntailment, id: FlowEventId, used: &mut [bool]) {
    let event = retained_event(summary, id);
    used[id.0 as usize] = true;
    assert!(matches!(
        event.kind,
        FlowEventKind::S1
            | FlowEventKind::S4
            | FlowEventKind::S5
            | FlowEventKind::S6
            | FlowEventKind::S7
            | FlowEventKind::S9
            | FlowEventKind::S11
            | FlowEventKind::S13
    ));
}

fn assert_synthetic_event(
    summary: &FunctionEntailment,
    id: FlowEventId,
    kind: FlowEventKind,
    used: &mut [bool],
) {
    let event = retained_event(summary, id);
    used[id.0 as usize] = true;
    assert_eq!(event.kind, kind);
    assert!(event.node_path.is_none());
}

fn assert_materialization_event(
    summary: &FunctionEntailment,
    id: FlowEventId,
    used: &mut [bool],
) -> FlowEventKind {
    let event = retained_event(summary, id);
    used[id.0 as usize] = true;
    match event.kind {
        FlowEventKind::Snapshot => {
            assert!(event.node_path.is_none());
        }
        other => panic!("a materialized goal has an invalid event kind {other:?}"),
    }
    event.kind
}

fn assert_join_parents(
    parents: &[JoinParent],
    conclusions: &[DerivationConclusion],
    accepts: impl Fn(&DerivationConclusion) -> bool,
    require_contributor: bool,
) {
    let mut has_noncontradictory_parent = false;
    for (ordinal, parent) in parents.iter().enumerate() {
        assert_eq!(
            parent.ordinal,
            u32::try_from(ordinal).expect("test join ordinal fits u32")
        );
        let conclusion = retained_conclusion(conclusions, parent.parent);
        has_noncontradictory_parent |= !matches!(conclusion, DerivationConclusion::Contradiction);
        assert!(
            matches!(conclusion, DerivationConclusion::Contradiction) || accepts(conclusion),
            "join predecessor {parent:?} has incompatible conclusion {conclusion:?}"
        );
    }
    if require_contributor {
        assert!(
            has_noncontradictory_parent,
            "a noncontradictory joined fact needs a contributing predecessor"
        );
    }
}

fn term_integer_range(kind: &TermKind) -> Option<(i128, i128)> {
    match kind {
        TermKind::Place(_, ty) | TermKind::ConstParameter(_, ty) => Some(type_range(*ty)),
        TermKind::Measure(..)
        | TermKind::CountedCapture { .. }
        | TermKind::IndexCapture { .. }
        | TermKind::EntryDatum { .. }
        | TermKind::MeasureDatum { .. } => Some(type_range(IntegerType::U64)),
        TermKind::ResultPayload(ty)
        | TermKind::CommitValue { ty, .. }
        | TermKind::CallDatum { ty, .. } => Some(type_range(*ty)),
        TermKind::Zero | TermKind::Constant(_) => None,
    }
}

fn assert_run_length_bound(summary: &FunctionEntailment, left: TermId, right: TermId, bound: i128) {
    let relation_matches = |length: TermId, measure_bound: MeasureBound| match measure_bound {
        MeasureBound::Constant(value) => {
            (left == length && right == ZERO && bound == value)
                || (left == ZERO && right == length && bound == -value)
        }
        MeasureBound::Equal(parameter) => {
            ((left == length && right == parameter) || (left == parameter && right == length))
                && bound == 0
        }
    };
    let matched = [left, right].into_iter().any(|candidate| {
        matches!(retained_term(summary, candidate), TermKind::Measure(..))
            && summary.inventory.measure_bounds[candidate.0 as usize]
                .is_some_and(|measure_bound| relation_matches(candidate, measure_bound))
    });
    assert!(matched, "run-length implicit bound must resolve exactly");
}

fn counted_atoms(
    counted: &CountedDerivationSet,
) -> [(CountedRootAtom, &CountedAtomicDerivation); 8] {
    [
        (
            CountedRootAtom::LowerCaptureToEndpoint,
            &counted.lower_capture_eq_endpoint.forward,
        ),
        (
            CountedRootAtom::LowerEndpointToCapture,
            &counted.lower_capture_eq_endpoint.reverse,
        ),
        (
            CountedRootAtom::UpperCaptureToEndpoint,
            &counted.upper_capture_eq_endpoint.forward,
        ),
        (
            CountedRootAtom::UpperEndpointToCapture,
            &counted.upper_capture_eq_endpoint.reverse,
        ),
        (
            CountedRootAtom::BinderToLowerCapture,
            &counted.binder_eq_lower_capture.forward,
        ),
        (
            CountedRootAtom::LowerCaptureToBinder,
            &counted.binder_eq_lower_capture.reverse,
        ),
        (
            CountedRootAtom::LowerCaptureLeBinder,
            &counted.lower_capture_le_binder.atomic,
        ),
        (
            CountedRootAtom::BinderLtUpperCapture,
            &counted.binder_lt_upper_capture.atomic,
        ),
    ]
}

const fn counted_atom_index(atom: CountedRootAtom) -> usize {
    match atom {
        CountedRootAtom::LowerCaptureToEndpoint => 0,
        CountedRootAtom::LowerEndpointToCapture => 1,
        CountedRootAtom::UpperCaptureToEndpoint => 2,
        CountedRootAtom::UpperEndpointToCapture => 3,
        CountedRootAtom::BinderToLowerCapture => 4,
        CountedRootAtom::LowerCaptureToBinder => 5,
        CountedRootAtom::LowerCaptureLeBinder => 6,
        CountedRootAtom::BinderLtUpperCapture => 7,
    }
}

fn assert_counted_atomic_parent(
    summary: &FunctionEntailment,
    conclusions: &[DerivationConclusion],
    counted: &CountedDerivationSet,
    atomic: &CountedAtomicDerivation,
) {
    let Relation::Bound { left, right, bound } = atomic.relation else {
        panic!("every counted atomic root must name one normalized bound");
    };
    retained_term(summary, left);
    retained_term(summary, right);
    match retained_conclusion(conclusions, atomic.parent) {
        DerivationConclusion::Relation(Relation::Bound {
            left: parent_left,
            right: parent_right,
            bound: parent_bound,
        }) => {
            assert_eq!((*parent_left, *parent_right), (left, right));
            assert_eq!(
                *parent_bound, bound,
                "the retained parent must prove the exact normalized S11 relation"
            );
        }
        DerivationConclusion::Contradiction => {}
        conclusion => panic!("counted atomic parent is incompatible: {conclusion:?}"),
    }
    let parent = &summary.derivations.nodes[atomic.parent.0 as usize];
    match atomic.proof_point {
        CountedProofPoint::PreheaderSnapshot => assert!(matches!(
            parent,
            DerivationNode::MaterializedBound { .. }
                | DerivationNode::MaterializedContradiction { .. }
        )),
        CountedProofPoint::BodyEntry => {
            let DerivationNode::SourceBound { event, .. } = parent else {
                panic!("a non-reconstructed body-entry root must be its S11 source bound");
            };
            let event = retained_event(summary, *event);
            assert_eq!(event.kind, FlowEventKind::S11);
            assert_eq!(event.node_path.as_ref(), Some(&counted.counted_node_path));
        }
    }
}

fn validate_counted_derivation_set(
    summary: &FunctionEntailment,
    conclusions: &[DerivationConclusion],
    counted: &CountedDerivationSet,
) {
    assert!(!counted.counted_node_path.components().is_empty());
    let Relation::Equal {
        left: lower_capture,
        right: lower_endpoint,
        difference: 0,
    } = counted.lower_capture_eq_endpoint.relation
    else {
        panic!("the first counted semantic root must be the lower capture equality");
    };
    let Relation::Equal {
        left: upper_capture,
        right: upper_endpoint,
        difference: 0,
    } = counted.upper_capture_eq_endpoint.relation
    else {
        panic!("the second counted semantic root must be the upper capture equality");
    };
    let Relation::Equal {
        left: binder,
        right: binder_lower_capture,
        difference: 0,
    } = counted.binder_eq_lower_capture.relation
    else {
        panic!("the third counted semantic root must be binder initialization");
    };
    assert_eq!(binder_lower_capture, lower_capture);
    assert!(matches!(
        retained_term(summary, lower_capture),
        TermKind::CountedCapture {
            range_path,
            side: CountedCaptureSide::Lower,
        } if range_path == counted.counted_node_path.components()
    ));
    assert!(matches!(
        retained_term(summary, upper_capture),
        TermKind::CountedCapture {
            range_path,
            side: CountedCaptureSide::Upper,
        } if range_path == counted.counted_node_path.components()
    ));
    assert!(matches!(
        retained_term(summary, binder),
        TermKind::Place(_, IntegerType::U64)
    ));
    assert!(!matches!(
        retained_term(summary, lower_endpoint),
        TermKind::CountedCapture { .. }
    ));
    assert!(!matches!(
        retained_term(summary, upper_endpoint),
        TermKind::CountedCapture { .. }
    ));

    let equality_atoms = [
        (
            &counted.lower_capture_eq_endpoint,
            lower_capture,
            lower_endpoint,
        ),
        (
            &counted.upper_capture_eq_endpoint,
            upper_capture,
            upper_endpoint,
        ),
        (&counted.binder_eq_lower_capture, binder, lower_capture),
    ];
    for (equality, left, right) in equality_atoms {
        assert_eq!(
            equality.forward.relation,
            Relation::Bound {
                left,
                right,
                bound: 0,
            }
        );
        assert_eq!(
            equality.reverse.relation,
            Relation::Bound {
                left: right,
                right: left,
                bound: 0,
            }
        );
        assert_eq!(
            equality.forward.proof_point,
            CountedProofPoint::PreheaderSnapshot
        );
        assert_eq!(
            equality.reverse.proof_point,
            CountedProofPoint::PreheaderSnapshot
        );
    }
    let lower_bound = Relation::Bound {
        left: lower_capture,
        right: binder,
        bound: 0,
    };
    let upper_bound = Relation::Bound {
        left: binder,
        right: upper_capture,
        bound: -1,
    };
    assert_eq!(counted.lower_capture_le_binder.relation, lower_bound);
    assert_eq!(counted.lower_capture_le_binder.atomic.relation, lower_bound);
    assert_eq!(counted.binder_lt_upper_capture.relation, upper_bound);
    assert_eq!(counted.binder_lt_upper_capture.atomic.relation, upper_bound);
    assert_eq!(
        counted.lower_capture_le_binder.atomic.proof_point,
        CountedProofPoint::BodyEntry
    );
    assert_eq!(
        counted.binder_lt_upper_capture.atomic.proof_point,
        CountedProofPoint::BodyEntry
    );
    for (_, atomic) in counted_atoms(counted) {
        assert_counted_atomic_parent(summary, conclusions, counted, atomic);
    }
}

fn assert_source_affine_fact_resolves(summary: &FunctionEntailment, source: SourceAffineFactRef) {
    let mut pending = vec![source];
    let mut seen = std::collections::HashSet::new();
    while let Some(source) = pending.pop() {
        if !seen.insert(source) {
            continue;
        }
        match source {
            SourceAffineFactRef::LoopInvariant(source) => {
                let mut matching = summary.loop_invariants.iter().filter(|invariant| {
                    invariant.loop_id == source.loop_id
                        && invariant.source_ordinal == source.source_ordinal
                });
                let invariant = matching
                    .next()
                    .expect("affine consequence source invariant must resolve");
                assert!(
                    matching.next().is_none(),
                    "source invariant reference is unique"
                );
                assert!(invariant.proof.discharged());
            }
            SourceAffineFactRef::SourceProof { source_ordinal } => {
                let proof = summary
                    .source_proofs
                    .iter()
                    .find(|proof| proof.source_ordinal == source_ordinal)
                    .expect("affine consequence source proof must resolve");
                assert!(proof.check.discharged());
            }
            SourceAffineFactRef::JoinedSourceProof { join_ordinal } => {
                let provenance = summary
                    .joined_source_proofs
                    .get(join_ordinal as usize)
                    .expect("joined source proof reference must resolve");
                assert!(
                    provenance.predecessors.len() >= 2,
                    "a diagnostic join records every structural predecessor"
                );
                for predecessor in provenance.predecessors.iter().copied() {
                    if let SourceAffineFactRef::JoinedSourceProof {
                        join_ordinal: predecessor_ordinal,
                    } = predecessor
                    {
                        assert!(
                            predecessor_ordinal < join_ordinal,
                            "joined source proof provenance is a backward DAG"
                        );
                    }
                    pending.push(predecessor);
                }
            }
        }
    }
}

pub(super) fn validate_derivations(summary: &FunctionEntailment) {
    assert_eq!(
        summary.inventory.terms.len(),
        summary.inventory.measure_bounds.len(),
        "term and length-bound inventories stay densely aligned"
    );
    assert_eq!(retained_term(summary, ZERO), &TermKind::Zero);

    let mut conclusions = Vec::with_capacity(summary.derivations.nodes.len());
    let mut depths = Vec::with_capacity(summary.derivations.nodes.len());
    let mut used_events = vec![false; summary.derivations.events.len()];
    let mut parent_edges = 0usize;

    for (index, node) in summary.derivations.nodes.iter().enumerate() {
        let parents = node.parent_ids();
        parent_edges += parents.len();
        assert!(
            parents.iter().all(|parent| parent.0 < index as u32),
            "every derivation parent must precede its child"
        );
        let depth = parents
            .iter()
            .map(|parent| depths[parent.0 as usize])
            .max()
            .map_or(0, |depth: u32| depth + 1);
        depths.push(depth);

        let conclusion = match node {
            DerivationNode::SourceBound {
                relation,
                left,
                right,
                bound,
                event,
            } => {
                assert_relation_terms_resolve(summary, relation);
                retained_term(summary, *left);
                retained_term(summary, *right);
                if retained_event(summary, *event).kind == FlowEventKind::Entry {
                    // MSR-3 admits exactly an immutable measure datum equated
                    // to its entering live measure at this source placement.
                    used_events[event.0 as usize] = true;
                    let Relation::Equal {
                        left,
                        right,
                        difference: 0,
                    } = relation
                    else {
                        panic!("an entry datum is established by equality");
                    };
                    let TermKind::EntryDatum { measure, .. } = retained_term(summary, *left) else {
                        panic!("entry equality must name an immutable entry datum");
                    };
                    assert!(matches!(retained_term(summary, *right),
                        TermKind::Measure(actual, _) if actual == measure));
                } else {
                    assert_source_event(summary, *event, &mut used_events);
                }
                match relation {
                    Relation::Bound {
                        left: source_left,
                        right: source_right,
                        bound: source_bound,
                    } => assert_eq!(
                        (left, right, bound),
                        (source_left, source_right, source_bound)
                    ),
                    // An equality's two bounds are its own difference in
                    // each direction [ENT-4].
                    Relation::Equal {
                        left: source_left,
                        right: source_right,
                        difference,
                    } => {
                        if *left == *source_left && *right == *source_right {
                            assert_eq!(*bound, *difference);
                        } else {
                            assert!(*left == *source_right && *right == *source_left);
                            assert_eq!(*bound, -difference);
                        }
                    }
                    Relation::Distinct { .. } => panic!("a distinct source is not a bound"),
                }
                DerivationConclusion::Relation(Relation::Bound {
                    left: *left,
                    right: *right,
                    bound: *bound,
                })
            }
            DerivationNode::SourceDistinct { left, right, event } => {
                assert!(left <= right, "disequality identities are normalized");
                retained_term(summary, *left);
                retained_term(summary, *right);
                assert_source_event(summary, *event, &mut used_events);
                DerivationConclusion::Relation(Relation::Distinct {
                    left: *left,
                    right: *right,
                    difference: 0,
                })
            }
            DerivationNode::SourceGoal { goal, sign, event } => {
                assert!(summary.inventory.goals.get(goal.0 as usize).is_some());
                assert_source_event(summary, *event, &mut used_events);
                DerivationConclusion::Goal {
                    goal: *goal,
                    sign: *sign,
                }
            }
            // [ENT-3.S7, DIAG-2] one table fact stands on the closed operand
            // bounds its row read, each a bound of one term against Z, or on
            // the discharged domain whose interval-product measurement it
            // states.
            DerivationNode::OperationFact {
                relation,
                event,
                parents,
            } => {
                assert_relation_terms_resolve(summary, relation);
                assert!(matches!(relation, Relation::Bound { .. }));
                assert_eq!(retained_event(summary, *event).kind, FlowEventKind::S7);
                assert_source_event(summary, *event, &mut used_events);
                for parent in parents {
                    match retained_conclusion(&conclusions, *parent) {
                        DerivationConclusion::Relation(bound) => assert!(
                            bound.terms().contains(&ZERO),
                            "an operand bound relates its term to Z"
                        ),
                        DerivationConclusion::IntegerDomain(_)
                        | DerivationConclusion::Contradiction => {}
                        other => panic!("an operation fact cannot stand on {other:?}"),
                    }
                }
                DerivationConclusion::Relation(relation.clone())
            }
            DerivationNode::BooleanLiteral { goal, sign } => {
                assert!(summary.inventory.goals.get(goal.0 as usize).is_some());
                DerivationConclusion::Goal {
                    goal: *goal,
                    sign: *sign,
                }
            }
            DerivationNode::ImplicitBound {
                left,
                right,
                bound,
                kind,
            } => {
                let left_kind = retained_term(summary, *left);
                let right_kind = retained_term(summary, *right);
                match kind {
                    ImplicitBoundKind::Reflexive => {
                        assert_eq!(left, right);
                        assert_eq!(*bound, 0);
                    }
                    ImplicitBoundKind::Constant => match (left_kind, right_kind) {
                        (TermKind::Constant(value), TermKind::Zero) => assert_eq!(bound, value),
                        (TermKind::Zero, TermKind::Constant(value)) => assert_eq!(*bound, -value),
                        _ => panic!("constant implicit bound must relate that constant to Z"),
                    },
                    ImplicitBoundKind::TypeMaximum => {
                        assert_eq!(*right, ZERO);
                        let (_, maximum) = term_integer_range(left_kind)
                            .expect("type maximum requires an integer-like term");
                        assert_eq!(*bound, maximum);
                    }
                    ImplicitBoundKind::TypeMinimum => {
                        assert_eq!(*left, ZERO);
                        let (minimum, _) = term_integer_range(right_kind)
                            .expect("type minimum requires an integer-like term");
                        assert_eq!(*bound, -minimum);
                    }
                    ImplicitBoundKind::StandingMeasure => {
                        assert_run_length_bound(summary, *left, *right, *bound);
                    }
                    // [MSR-2] `P.len <= P.cap` and `P.head <= P.cap`,
                    // emitted from the capacity term of one place.
                    ImplicitBoundKind::MeasureOrdering => {
                        assert_eq!(*bound, 0);
                        assert!(matches!(
                            retained_term(summary, *right),
                            TermKind::Measure(CheckedMeasure::Capacity, _)
                        ));
                        assert!(matches!(
                            retained_term(summary, *left),
                            TermKind::Measure(CheckedMeasure::Length | CheckedMeasure::Head, _)
                        ));
                    }
                }
                DerivationConclusion::Relation(Relation::Bound {
                    left: *left,
                    right: *right,
                    bound: *bound,
                })
            }
            DerivationNode::TransitiveBound {
                left,
                middle,
                right,
                bound,
                first,
                second,
            } => {
                let first_bound = retained_bound(&conclusions, *first, *left, *middle);
                let second_bound = retained_bound(&conclusions, *second, *middle, *right);
                assert_eq!(*bound, first_bound.saturating_add(second_bound));
                DerivationConclusion::Relation(Relation::Bound {
                    left: *left,
                    right: *right,
                    bound: *bound,
                })
            }
            DerivationNode::StrengthenedBound {
                left,
                right,
                bound,
                weak,
                distinct,
            } => {
                assert_eq!(*bound, -1);
                assert_eq!(retained_bound(&conclusions, *weak, *left, *right), 0);
                let DerivationConclusion::Relation(Relation::Distinct {
                    left: distinct_left,
                    right: distinct_right,
                    difference: 0,
                }) = retained_conclusion(&conclusions, *distinct)
                else {
                    panic!("strengthening's second parent must be a disequality");
                };
                assert!(
                    (*distinct_left == *left && *distinct_right == *right)
                        || (*distinct_left == *right && *distinct_right == *left)
                );
                DerivationConclusion::Relation(Relation::Bound {
                    left: *left,
                    right: *right,
                    bound: *bound,
                })
            }
            DerivationNode::SubsumedBound {
                left,
                right,
                held,
                requested,
                parent,
            } => {
                assert!(*held < *requested);
                assert_eq!(retained_bound(&conclusions, *parent, *left, *right), *held);
                DerivationConclusion::Relation(Relation::Bound {
                    left: *left,
                    right: *right,
                    bound: *requested,
                })
            }
            DerivationNode::Equality {
                left,
                right,
                forward,
                reverse,
            } => {
                // [ENT-4] an equality is its two bounds, and a displaced one
                // carries its difference in each direction.
                let difference = retained_bound(&conclusions, *forward, *left, *right);
                assert_eq!(
                    retained_bound(&conclusions, *reverse, *right, *left),
                    -difference
                );
                DerivationConclusion::Relation(Relation::Equal {
                    left: *left,
                    right: *right,
                    difference,
                })
            }
            DerivationNode::DisequalityFromStrictBound {
                left,
                right,
                parent,
            } => {
                assert!(left < right, "strict-derived disequalities are normalized");
                let DerivationConclusion::Relation(Relation::Bound {
                    left: parent_left,
                    right: parent_right,
                    bound: parent_bound,
                }) = retained_conclusion(&conclusions, *parent)
                else {
                    panic!("strict-bound disequality requires a bound parent");
                };
                assert!(*parent_bound <= -1);
                assert!(
                    (*parent_left == *left && *parent_right == *right)
                        || (*parent_left == *right && *parent_right == *left)
                );
                DerivationConclusion::Relation(Relation::Distinct {
                    left: *left,
                    right: *right,
                    difference: 0,
                })
            }
            DerivationNode::GoalProjection {
                goal,
                sign,
                relation,
                parent,
            } => {
                assert_relation_terms_resolve(summary, relation);
                let DerivationConclusion::Relation(parent_relation) =
                    retained_conclusion(&conclusions, *parent)
                else {
                    panic!("a goal projection requires a relation parent");
                };
                match (parent_relation, relation) {
                    (
                        Relation::Distinct {
                            left: parent_left,
                            right: parent_right,
                            ..
                        },
                        Relation::Distinct { left, right, .. },
                    ) => assert!(
                        (parent_left == left && parent_right == right)
                            || (parent_left == right && parent_right == left),
                        "disequality parents use unordered fact identity"
                    ),
                    (_, Relation::Bound { left, right, bound }) => assert!(
                        retained_bound(&conclusions, *parent, *left, *right) <= *bound,
                        "a projected bound's parent must imply it in the requested direction"
                    ),
                    _ => assert_eq!(parent_relation, relation),
                }
                let retained_goal = summary
                    .inventory
                    .goals
                    .get(goal.0 as usize)
                    .expect("projected goal ID must resolve");
                let projection = retained_goal
                    .projection
                    .as_ref()
                    .expect("projection node requires a projected goal");
                let expected = match sign {
                    GoalSign::Positive => projection.clone(),
                    GoalSign::Negative => projection.negated(),
                };
                match (relation, &expected) {
                    (
                        Relation::Distinct { left, right, .. },
                        Relation::Distinct {
                            left: expected_left,
                            right: expected_right,
                            ..
                        },
                    ) => assert!(
                        (left == expected_left && right == expected_right)
                            || (left == expected_right && right == expected_left),
                        "disequality identity is independent of endpoint rendering order"
                    ),
                    _ => assert_eq!(*relation, expected),
                }
                DerivationConclusion::Goal {
                    goal: *goal,
                    sign: *sign,
                }
            }
            DerivationNode::L0Contradiction { term, parent } => {
                retained_term(summary, *term);
                let DerivationConclusion::Relation(Relation::Bound { left, right, bound }) =
                    retained_conclusion(&conclusions, *parent)
                else {
                    panic!("L0 contradiction requires a bound parent");
                };
                assert_eq!((left, right), (term, term));
                assert!(*bound < 0);
                DerivationConclusion::Contradiction
            }
            DerivationNode::GoalContradiction {
                goal,
                positive,
                negative,
            } => {
                assert!(summary.inventory.goals.get(goal.0 as usize).is_some());
                assert_eq!(
                    retained_conclusion(&conclusions, *positive),
                    &DerivationConclusion::Goal {
                        goal: *goal,
                        sign: GoalSign::Positive,
                    }
                );
                assert_eq!(
                    retained_conclusion(&conclusions, *negative),
                    &DerivationConclusion::Goal {
                        goal: *goal,
                        sign: GoalSign::Negative,
                    }
                );
                DerivationConclusion::Contradiction
            }
            DerivationNode::RequirementAffineImage { goal, sign, parent } => {
                assert!(
                    summary.inventory.goals[goal.0 as usize]
                        .projection
                        .is_none()
                );
                assert_eq!(
                    retained_conclusion(&conclusions, *parent),
                    &DerivationConclusion::Goal {
                        goal: *goal,
                        sign: *sign
                    }
                );
                let event = summary
                    .derivations
                    .node_event(*parent)
                    .expect("an S4 image keeps its established-goal source");
                assert_eq!(
                    summary.derivations.events[event.0 as usize].kind,
                    FlowEventKind::S4
                );
                DerivationConclusion::RequirementAffineImage
            }
            DerivationNode::UnsignedDivisionProduct {
                product,
                division,
                domain,
            } => {
                assert!(!product.components().is_empty());
                // [DIAG-2] the division parent is the S7 fact bounding the
                // quotient by its dividend.
                assert!(matches!(
                    summary.derivations.nodes[division.0 as usize],
                    DerivationNode::OperationFact {
                        relation: Relation::Bound { .. },
                        ..
                    }
                ));
                assert!(matches!(
                    retained_conclusion(&conclusions, *domain),
                    DerivationConclusion::IntegerDomain(_)
                ));
                DerivationConclusion::UnsignedDivisionProduct
            }
            DerivationNode::IntegerDomain { goal, parents } => {
                if let Some(goal) = goal {
                    assert!(summary.inventory.goals.get(goal.0 as usize).is_some());
                }
                assert!(!parents.is_empty());
                DerivationConclusion::IntegerDomain(*goal)
            }
            DerivationNode::ConversionDomain { goal, parents } => {
                let retained = &summary.inventory.goals[goal.0 as usize];
                assert!(matches!(
                    retained.expression,
                    GoalExpression::Operation {
                        row: GoalOperation::NumericConversion {
                            mode: CheckedConversionMode::Defined,
                            ..
                        },
                        ..
                    }
                ));
                assert_eq!(
                    parents.len(),
                    2,
                    "conversion interval proves upper then lower"
                );
                for parent in parents {
                    assert!(matches!(
                        retained_conclusion(&conclusions, *parent),
                        DerivationConclusion::Relation(_) | DerivationConclusion::AffineConsequence
                    ));
                }
                DerivationConclusion::Goal {
                    goal: *goal,
                    sign: GoalSign::Positive,
                }
            }
            DerivationNode::AffineConsequence {
                relation,
                premises,
                parents,
            } => {
                for parent in parents {
                    assert!(matches!(
                        retained_conclusion(&conclusions, *parent),
                        DerivationConclusion::Relation(_)
                            | DerivationConclusion::Contradiction
                            | DerivationConclusion::UnsignedDivisionProduct
                            | DerivationConclusion::RequirementAffineImage
                    ));
                }
                for premise in premises {
                    assert!(premise.factor > 0);
                    assert_source_affine_fact_resolves(summary, premise.source);
                }
                if let Some(relation) = relation {
                    assert_relation_terms_resolve(summary, relation);
                    DerivationConclusion::Relation(relation.as_ref().clone())
                } else {
                    DerivationConclusion::AffineConsequence
                }
            }
            DerivationNode::GoalNormalization {
                goal,
                sign,
                clause,
                parents,
            } => {
                let retained = summary
                    .inventory
                    .goals
                    .get(goal.0 as usize)
                    .expect("normalization proof goal is retained");
                let relations = retained
                    .normalization
                    .as_ref()
                    .expect("normalization proof goal has fixed components")
                    .clause_relations(*sign, *clause)
                    .expect("normalization proof selects a complete fixed clause");
                assert_eq!(parents.len(), relations.len());
                for (parent, relation) in parents.iter().zip(relations) {
                    assert_eq!(
                        retained_conclusion(&conclusions, *parent),
                        &DerivationConclusion::Relation(relation),
                    );
                }
                DerivationConclusion::Goal {
                    goal: *goal,
                    sign: *sign,
                }
            }
            DerivationNode::GoalAffineConsequence { goal, sign, parent } => {
                assert!(summary.inventory.goals.get(goal.0 as usize).is_some());
                assert_eq!(
                    retained_conclusion(&conclusions, *parent),
                    &DerivationConclusion::AffineConsequence,
                    "an affine-goal conclusion must retain its owning affine proof",
                );
                DerivationConclusion::Goal {
                    goal: *goal,
                    sign: *sign,
                }
            }
            DerivationNode::RangeSeparation { detail } => {
                assert!(matches!(
                    retained_conclusion(&conclusions, detail.parent),
                    DerivationConclusion::AffineConsequence | DerivationConclusion::Contradiction
                ));
                DerivationConclusion::RangeSeparation {
                    left: detail.left,
                    right: detail.right,
                    ordering: detail.ordering,
                }
            }
            DerivationNode::IndexSeparation { detail } => {
                let left = &detail.left;
                let right = &detail.right;
                let parent = detail.parent;
                let term_for = |captured: &crate::semantic::places::CapturedValue| {
                    use crate::semantic::places::CapturedTerm;
                    summary
                        .inventory
                        .terms
                        .iter()
                        .position(|term| match captured.term {
                            CapturedTerm::Literal(value) => {
                                (*term == TermKind::Zero && value == 0)
                                    || *term == TermKind::Constant(i128::from(value))
                            }
                            CapturedTerm::Const(declaration) => {
                                matches!(term, TermKind::ConstParameter(candidate, _) if *candidate == declaration)
                            }
                            CapturedTerm::Binding(_) | CapturedTerm::Superseded(_) => matches!(
                                term,
                                TermKind::IndexCapture { capture } if *capture == captured.capture
                            ),
                            CapturedTerm::Opaque => false,
                        })
                        .map(|term| TermId(u32::try_from(term).expect("term index fits u32")))
                        .expect("an indexed-separation capture has a retained datum term")
                };
                let (left_term, right_term) = (term_for(left), term_for(right));
                let expected = if left_term <= right_term {
                    Relation::Distinct {
                        left: left_term,
                        right: right_term,
                        difference: 0,
                    }
                } else {
                    Relation::Distinct {
                        left: right_term,
                        right: left_term,
                        difference: 0,
                    }
                };
                if let Some(substitution) = &detail.substitution {
                    use crate::semantic::places::{CapturedTerm, PlaceRoot};
                    let (CapturedTerm::Binding(left_binding), CapturedTerm::Binding(right_binding)) =
                        (left.term, right.term)
                    else {
                        panic!("capture substitution is restricted to direct binding captures");
                    };
                    for (source, binding) in [
                        (substitution.source_left, left_binding),
                        (substitution.source_right, right_binding),
                    ] {
                        assert!(matches!(
                            retained_term(summary, source),
                            TermKind::Place(place, IntegerType::U64)
                                if place.root == PlaceRoot::Binding(binding) && place.path.is_empty()
                        ));
                    }
                    let source_expected = if substitution.source_left <= substitution.source_right {
                        Relation::Distinct {
                            left: substitution.source_left,
                            right: substitution.source_right,
                            difference: 0,
                        }
                    } else {
                        Relation::Distinct {
                            left: substitution.source_right,
                            right: substitution.source_left,
                            difference: 0,
                        }
                    };
                    assert_eq!(
                        retained_conclusion(&conclusions, parent),
                        &DerivationConclusion::Relation(source_expected)
                    );
                    for (identity, capture, source) in [
                        (
                            substitution.left_identity,
                            left_term,
                            substitution.source_left,
                        ),
                        (
                            substitution.right_identity,
                            right_term,
                            substitution.source_right,
                        ),
                    ] {
                        let DerivationConclusion::Relation(Relation::Equal {
                            left,
                            right,
                            difference: 0,
                        }) = retained_conclusion(&conclusions, identity)
                        else {
                            panic!("capture substitution retains an exact identity proof");
                        };
                        assert!(
                            (*left == capture && *right == source)
                                || (*left == source && *right == capture)
                        );
                    }
                } else {
                    match retained_conclusion(&conclusions, parent) {
                        DerivationConclusion::Relation(relation) => assert_eq!(relation, &expected),
                        DerivationConclusion::AffineConsequence => {
                            let DerivationNode::AffineConsequence { relation: None, .. } =
                                &summary.derivations.nodes[parent.0 as usize]
                            else {
                                panic!(
                                    "an indexed affine proof retains its targetless affine parent"
                                );
                            };
                            let target = detail.affine_target.as_deref().expect(
                                "an indexed affine proof retains its selected strict target",
                            );
                            let (left_image, right_image) = detail.affine_images.as_deref().expect(
                                "an indexed affine proof retains the exact captured images",
                            );
                            let expected_targets = [
                                AffineInequality::from_bounded_forms(
                                    left_image,
                                    right_image,
                                    -1,
                                    &mut AffineCheckState::new(),
                                ),
                                AffineInequality::from_bounded_forms(
                                    right_image,
                                    left_image,
                                    -1,
                                    &mut AffineCheckState::new(),
                                ),
                            ];
                            assert!(expected_targets.iter().any(|expected| {
                                expected.as_ref().is_ok_and(|expected| expected == target)
                            }));
                        }
                        DerivationConclusion::Contradiction => {}
                        other => panic!("invalid indexed-separation parent: {other:?}"),
                    }
                }
                if !matches!(
                    retained_conclusion(&conclusions, parent),
                    DerivationConclusion::AffineConsequence
                ) {
                    assert!(detail.affine_target.is_none());
                    assert!(detail.affine_images.is_none());
                }
                DerivationConclusion::IndexSeparation {
                    left: *left,
                    right: *right,
                }
            }
            DerivationNode::BooleanIntroduction {
                goal,
                sign,
                parents,
            } => {
                assert!(summary.inventory.goals.get(goal.0 as usize).is_some());
                assert!(!parents.is_empty());
                for parent in parents {
                    assert!(matches!(
                        retained_conclusion(&conclusions, *parent),
                        DerivationConclusion::Goal { .. }
                    ));
                }
                DerivationConclusion::Goal {
                    goal: *goal,
                    sign: *sign,
                }
            }
            DerivationNode::JoinBound {
                left,
                right,
                bound,
                event,
                parents,
            } => {
                retained_term(summary, *left);
                retained_term(summary, *right);
                assert_synthetic_event(summary, *event, FlowEventKind::Join, &mut used_events);
                assert_join_parents(
                    parents,
                    &conclusions,
                    |conclusion| {
                        matches!(
                            conclusion,
                            DerivationConclusion::Relation(Relation::Bound {
                                left: parent_left,
                                right: parent_right,
                                bound: parent_bound,
                            }) if parent_left == left && parent_right == right && parent_bound <= bound
                        )
                    },
                    true,
                );
                DerivationConclusion::Relation(Relation::Bound {
                    left: *left,
                    right: *right,
                    bound: *bound,
                })
            }
            DerivationNode::JoinDistinct {
                left,
                right,
                event,
                parents,
            } => {
                assert!(left <= right, "joined disequalities are normalized");
                retained_term(summary, *left);
                retained_term(summary, *right);
                assert_synthetic_event(summary, *event, FlowEventKind::Join, &mut used_events);
                assert_join_parents(
                    parents,
                    &conclusions,
                    |conclusion| {
                        conclusion
                            == &DerivationConclusion::Relation(Relation::Distinct {
                                left: *left,
                                right: *right,
                                difference: 0,
                            })
                    },
                    true,
                );
                DerivationConclusion::Relation(Relation::Distinct {
                    left: *left,
                    right: *right,
                    difference: 0,
                })
            }
            DerivationNode::JoinGoal {
                goal,
                sign,
                event,
                parents,
            } => {
                assert!(summary.inventory.goals.get(goal.0 as usize).is_some());
                assert_synthetic_event(summary, *event, FlowEventKind::Join, &mut used_events);
                assert_join_parents(
                    parents,
                    &conclusions,
                    |conclusion| {
                        conclusion
                            == &DerivationConclusion::Goal {
                                goal: *goal,
                                sign: *sign,
                            }
                    },
                    true,
                );
                DerivationConclusion::Goal {
                    goal: *goal,
                    sign: *sign,
                }
            }
            DerivationNode::JoinContradiction { event, parents } => {
                assert_synthetic_event(summary, *event, FlowEventKind::Join, &mut used_events);
                assert_join_parents(
                    parents,
                    &conclusions,
                    |conclusion| matches!(conclusion, DerivationConclusion::Contradiction),
                    false,
                );
                assert!(parents.iter().all(|parent| matches!(
                    retained_conclusion(&conclusions, parent.parent),
                    DerivationConclusion::Contradiction
                )));
                DerivationConclusion::Contradiction
            }
            DerivationNode::MaterializedBound {
                left,
                right,
                bound,
                event,
                parent,
            } => {
                assert_synthetic_event(summary, *event, FlowEventKind::Snapshot, &mut used_events);
                let conclusion = DerivationConclusion::Relation(Relation::Bound {
                    left: *left,
                    right: *right,
                    bound: *bound,
                });
                // An equality is its two non-strict bounds [ENT-2], so a
                // materialization of either direction may name the equality
                // node itself as its parent. Both readings are the one
                // retained conclusion; nothing else is admitted here.
                let parent_conclusion = retained_conclusion(&conclusions, *parent);
                let equality_parent = DerivationConclusion::Relation(Relation::Equal {
                    left: *left,
                    right: *right,
                    difference: 0,
                }) == *parent_conclusion
                    || DerivationConclusion::Relation(Relation::Equal {
                        left: *right,
                        right: *left,
                        difference: 0,
                    }) == *parent_conclusion;
                assert!(
                    *parent_conclusion == conclusion || (equality_parent && *bound == 0),
                    "materialized bound {conclusion:?} has parent {parent_conclusion:?}"
                );
                conclusion
            }
            DerivationNode::MaterializedDistinct {
                left,
                right,
                event,
                parent,
            } => {
                assert_synthetic_event(summary, *event, FlowEventKind::Snapshot, &mut used_events);
                let conclusion = DerivationConclusion::Relation(Relation::Distinct {
                    left: *left,
                    right: *right,
                    difference: 0,
                });
                assert_eq!(retained_conclusion(&conclusions, *parent), &conclusion);
                conclusion
            }
            DerivationNode::MaterializedGoal {
                goal,
                sign,
                event,
                parent,
            } => {
                assert!(summary.inventory.goals.get(goal.0 as usize).is_some());
                let kind = assert_materialization_event(summary, *event, &mut used_events);
                let conclusion = DerivationConclusion::Goal {
                    goal: *goal,
                    sign: *sign,
                };
                assert_eq!(kind, FlowEventKind::Snapshot);
                assert_eq!(retained_conclusion(&conclusions, *parent), &conclusion);
                conclusion
            }
            DerivationNode::MaterializedContradiction { event, parent } => {
                assert_synthetic_event(summary, *event, FlowEventKind::Snapshot, &mut used_events);
                assert_eq!(
                    retained_conclusion(&conclusions, *parent),
                    &DerivationConclusion::Contradiction
                );
                DerivationConclusion::Contradiction
            }
            DerivationNode::PostconditionExit {
                relation, parent, ..
            } => {
                let relation = relation.as_ref();
                assert_relation_terms_resolve(summary, relation);
                let conclusion = DerivationConclusion::Relation(relation.clone());
                assert!(matches!(
                    retained_conclusion(&conclusions, *parent),
                    parent if parent == &conclusion
                        || parent == &DerivationConclusion::AffineConsequence
                        || parent == &DerivationConclusion::Contradiction
                ));
                conclusion
            }
            DerivationNode::SignatureContract { .. } => {
                DerivationConclusion::PostconditionAggregate
            }
            DerivationNode::PostconditionAggregate { parents, .. } => {
                assert!(!parents.is_empty());
                let statements = parents
                    .iter()
                    .map(|parent| {
                        let DerivationNode::PostconditionExit { statement, .. } =
                            &summary.derivations.nodes[parent.0 as usize]
                        else {
                            panic!("aggregate parents must be postcondition exit roots");
                        };
                        statement
                    })
                    .collect::<Vec<_>>();
                assert!(
                    statements
                        .windows(2)
                        .all(|pair| { pair[0].components().cmp(pair[1].components()).is_lt() })
                );
                DerivationConclusion::PostconditionAggregate
            }
            DerivationNode::PostconditionCall { detail } => {
                let PostconditionCallDetail {
                    relation,
                    summary: reference,
                    substitutions,
                    transfer_events,
                    parents,
                    ..
                } = detail.as_ref();
                assert_relation_terms_resolve(summary, relation);
                match &reference.summary {
                    crate::semantic::entailment::RelationProvenance::Verified(published) => {
                        assert!(!published.block.components().is_empty());
                    }
                    crate::semantic::entailment::RelationProvenance::FormalBoundary {
                        premises,
                        ..
                    } => {
                        assert!(
                            premises
                                .iter()
                                .all(|premise| { !premise.block.components().is_empty() })
                        );
                    }
                }
                for parent in parents {
                    assert!(summary.derivations.nodes.get(parent.0 as usize).is_some());
                }
                for substitution in substitutions {
                    retained_term(summary, substitution.term);
                }
                for event in transfer_events {
                    let retained = retained_event(summary, *event);
                    used_events[event.0 as usize] = true;
                    assert!(matches!(
                        retained.kind,
                        FlowEventKind::PostconditionCallConsume
                            | FlowEventKind::PostconditionCallWrite
                    ));
                }
                DerivationConclusion::Relation(relation.clone())
            }
            DerivationNode::ContractCall { parents, .. } => {
                assert!(
                    parents.iter().all(|parent| summary
                        .derivations
                        .nodes
                        .get(parent.0 as usize)
                        .is_some())
                );
                DerivationConclusion::ContractCall
            }
            DerivationNode::ResultTransport {
                from,
                to,
                relation,
                parent,
                ..
            } => {
                assert_relation_terms_resolve(summary, relation);
                let DerivationConclusion::Relation(source) =
                    retained_conclusion(&conclusions, *parent)
                else {
                    panic!("Result substitution needs a numeric premise");
                };
                assert!(
                    matches!(retained_term(summary, *from), TermKind::ResultPayload(_))
                        || matches!(retained_term(summary, *to), TermKind::ResultPayload(_))
                );
                let replace = |term| if term == *from { *to } else { term };
                let expected = match source {
                    Relation::Bound { left, right, bound } => Relation::Bound {
                        left: replace(*left),
                        right: replace(*right),
                        bound: *bound,
                    },
                    Relation::Equal {
                        left,
                        right,
                        difference,
                    } => Relation::Equal {
                        left: replace(*left),
                        right: replace(*right),
                        difference: *difference,
                    },
                    Relation::Distinct {
                        left,
                        right,
                        difference,
                    } => {
                        let (left, right) = (replace(*left), replace(*right));
                        if left <= right {
                            Relation::Distinct {
                                left,
                                right,
                                difference: *difference,
                            }
                        } else {
                            Relation::Distinct {
                                left: right,
                                right: left,
                                difference: -*difference,
                            }
                        }
                    }
                };
                assert_eq!(relation.as_ref(), &expected);
                DerivationConclusion::Relation(expected)
            }
            DerivationNode::ResultErr { statement } => {
                assert!(!statement.components().is_empty());
                DerivationConclusion::Contradiction
            }
            DerivationNode::PostconditionDirectResult {
                relation, parent, ..
            } => {
                let relation = relation.as_ref();
                assert_relation_terms_resolve(summary, relation);
                assert_eq!(
                    retained_conclusion(&conclusions, *parent),
                    &DerivationConclusion::Relation(relation.clone())
                );
                DerivationConclusion::Relation(relation.clone())
            }
            DerivationNode::PostconditionDirectReceiver {
                relation,
                target_event,
                parent,
                ..
            } => {
                let relation = relation.as_ref();
                assert_relation_terms_resolve(summary, relation);
                assert_eq!(
                    retained_conclusion(&conclusions, *parent),
                    &DerivationConclusion::Relation(relation.clone())
                );
                let event = retained_event(summary, *target_event);
                used_events[target_event.0 as usize] = true;
                assert_eq!(event.kind, FlowEventKind::PostconditionReceiverWrite);
                DerivationConclusion::Relation(relation.clone())
            }
            DerivationNode::PostconditionGive {
                statement,
                carrier,
                receiver,
                relation,
                event,
                parent,
                ..
            } => {
                let relation = relation.as_ref();
                assert_relation_terms_resolve(summary, relation);
                let DerivationConclusion::Relation(source) =
                    retained_conclusion(&conclusions, *parent)
                else {
                    panic!("a reachable give relation has one exact source relation");
                };
                assert!(relation_has_bare_binding(summary, source, *carrier));
                assert!(!relation_has_bare_binding(summary, source, *receiver));
                assert!(!relation_has_bare_binding(summary, relation, *carrier));
                assert!(relation_has_bare_binding(summary, relation, *receiver));
                let retained = retained_event(summary, *event);
                used_events[event.0 as usize] = true;
                assert_eq!(retained.kind, FlowEventKind::PostconditionGive);
                assert_eq!(retained.node_path.as_ref(), Some(statement));
                DerivationConclusion::Relation(relation.clone())
            }
            DerivationNode::PostconditionDeliveryJoin { detail } => {
                let PostconditionDeliveryJoinDetail {
                    statement,
                    receiver,
                    relation,
                    event,
                    parents,
                } = detail.as_ref();
                assert_relation_terms_resolve(summary, relation);
                assert!(relation_has_bare_binding(summary, relation, *receiver));
                assert!(!parents.is_empty());
                let mut prior_edge: Option<&crate::NodePath> = None;
                for (ordinal, parent) in parents.iter().enumerate() {
                    assert_eq!(parent.ordinal as usize, ordinal);
                    match retained_conclusion(&conclusions, parent.parent) {
                        DerivationConclusion::Contradiction => {}
                        DerivationConclusion::Relation(edge_relation) => {
                            let DerivationNode::PostconditionGive {
                                statement: edge, ..
                            } = &summary.derivations.nodes[parent.parent.0 as usize]
                            else {
                                panic!("each positive delivery edge parent is one Give node");
                            };
                            if let Some(prior) = prior_edge {
                                assert!(
                                    prior.components().cmp(edge.components()).is_lt(),
                                    "delivery edge parents stay in source NodePath order"
                                );
                            }
                            prior_edge = Some(edge);
                            match (relation, edge_relation) {
                                (
                                    Relation::Bound { left, right, bound },
                                    Relation::Bound {
                                        left: edge_left,
                                        right: edge_right,
                                        bound: edge_bound,
                                    },
                                ) => {
                                    assert_eq!((left, right), (edge_left, edge_right));
                                    assert!(edge_bound <= bound);
                                }
                                (
                                    Relation::Distinct { left, right, .. },
                                    Relation::Distinct {
                                        left: edge_left,
                                        right: edge_right,
                                        ..
                                    },
                                ) => assert_eq!((left, right), (edge_left, edge_right)),
                                _ => panic!("delivery join preserves the L0 relation class"),
                            }
                        }
                        DerivationConclusion::Goal { .. }
                        | DerivationConclusion::IntegerDomain(_)
                        | DerivationConclusion::AffineConsequence
                        | DerivationConclusion::UnsignedDivisionProduct
                        | DerivationConclusion::RequirementAffineImage
                        | DerivationConclusion::ContractCall
                        | DerivationConclusion::RangeSeparation { .. }
                        | DerivationConclusion::IndexSeparation { .. }
                        | DerivationConclusion::PostconditionAggregate => {
                            panic!("delivery join parent must be a relation or contradiction")
                        }
                    }
                }
                let retained = retained_event(summary, *event);
                used_events[event.0 as usize] = true;
                assert_eq!(retained.kind, FlowEventKind::PostconditionDeliveryJoin);
                assert_eq!(retained.node_path.as_ref(), Some(statement));
                DerivationConclusion::Relation(relation.clone())
            }
        };
        conclusions.push(conclusion);
    }

    for postcondition in &summary.postconditions {
        for exit in &postcondition.exits {
            for image in &exit.entry_images {
                if let Some(event) = image.invalidation {
                    let retained = retained_event(summary, event);
                    used_events[event.0 as usize] = true;
                    assert!(matches!(
                        retained.kind,
                        FlowEventKind::PostconditionEntryImageInvalidation
                            | FlowEventKind::Snapshot
                            | FlowEventKind::PostconditionCallConsume
                            | FlowEventKind::PostconditionCallWrite
                    ));
                }
            }
        }
    }
    assert!(
        used_events.iter().all(|used| *used),
        "finish must prune every unreferenced flow event"
    );
    assert_eq!(
        summary.derivations.metrics.unique_nodes as usize,
        summary.derivations.nodes.len()
    );
    assert_eq!(
        summary.derivations.metrics.parent_edges as usize,
        parent_edges
    );
    assert_eq!(
        summary.derivations.metrics.maximum_depth,
        depths.iter().copied().max().unwrap_or(0)
    );
    if !summary.derivations.nodes.is_empty() {
        assert!(summary.derivations.metrics.retained_bytes > 0);
    }

    for counted in &summary.counted_derivations {
        validate_counted_derivation_set(summary, &conclusions, counted);
    }

    let mut seen_obligations = vec![false; summary.obligations.len()];
    let mut seen_calls = vec![false; summary.call_goals.len()];
    let mut seen_contracts = vec![false; summary.contract_goals.len()];
    let mut seen_counted = vec![[false; 8]; summary.counted_derivations.len()];
    let mut seen_postcondition_exits = summary
        .postconditions
        .iter()
        .map(|proof| vec![false; proof.exits.len()])
        .collect::<Vec<_>>();
    let mut seen_postcondition_aggregates = vec![false; summary.postconditions.len()];
    let mut seen_s12 = 0u32;
    let mut seen_s12_nodes = vec![false; summary.derivations.nodes.len()];
    let mut seen_delivery_gives = 0u32;
    let mut seen_delivery_joins = 0u32;
    let mut seen_delivery_nodes = vec![false; summary.derivations.nodes.len()];
    let mut counted_root_order = Vec::new();
    let mut class_counts = [0u32; 4];
    for root in &summary.derivations.roots {
        let conclusion = retained_conclusion(&conclusions, root.node);
        match root.kind {
            DerivationRootKind::BodyEntryContradiction => {
                let CheckedBodyDisposition::Uninhabited { contradiction } =
                    summary.body_disposition
                else {
                    panic!("a body-entry contradiction root requires uninhabited metadata");
                };
                assert_eq!(contradiction, root.node);
                assert_eq!(conclusion, &DerivationConclusion::Contradiction);
            }
            DerivationRootKind::BoundsObligation(ordinal) => {
                let ordinal = ordinal as usize;
                let outcome = summary
                    .obligations
                    .get(ordinal)
                    .expect("bounds-root ordinal must resolve");
                assert!(!seen_obligations[ordinal], "one exact root per obligation");
                seen_obligations[ordinal] = true;
                assert!(outcome.discharged);
                // Retired with [BLK-0]: the KernelRequirement obligation family had the kernel declaration domain as its subject and has no v0.60 one; its successor is the ordinary PRE-1 record call and the UndischargedCallRequirement it submits under FN-8.
                // Retired with [LIV-2]: the former multi-target-commit
                // IndexSeparation family has no v0.60 subject; indexed and
                // ranged EFF-5 pairs now share CallSeparation at a call.
                match outcome.family {
                    ObligationFamily::Bounds => assert_eq!(outcome.conjunct, 0),
                    ObligationFamily::AllocationFit => assert_eq!(outcome.conjunct, 0),
                    // Separation for a call, exchange or reference preservation
                    // is one occurrence, without a conjunct of its own.
                    ObligationFamily::CallSeparation(_)
                    | ObligationFamily::ExchangeSeparation(_)
                    | ObligationFamily::ReferencePreservation(_) => {
                        assert_eq!(outcome.conjunct, 0)
                    }
                    // [REF-4] the two formation goals `lo <= hi` and
                    // `hi <= x.len` are conjuncts zero and one.
                    ObligationFamily::RangeFormation => {
                        assert!(outcome.conjunct <= 1)
                    }
                    ObligationFamily::IntegerDomain | ObligationFamily::ConversionDomain => {
                        panic!("integer-domain roots use their own root class")
                    }
                }
                assert_eq!(outcome.derivation, Some(root.node));
                assert!(!outcome.node_path.components().is_empty());
                // Call and reference-preservation separations have no canonical
                // goal or single normalized component; their retained wrapper
                // names the exact pair and selected ordering. Another family
                // may lack an L0 component when its source operands have only
                // the canonical goal plus an affine normalization. That root must conclude
                // the exact retained positive goal (or the actual entering
                // contradiction), rather than being accepted by shape alone.
                if matches!(
                    outcome.family,
                    ObligationFamily::CallSeparation(_)
                        | ObligationFamily::ReferencePreservation(_)
                ) {
                    assert!(outcome.components.is_empty());
                    assert!(outcome.canonical_goal.is_none());
                    assert!(matches!(
                        conclusion,
                        DerivationConclusion::RangeSeparation { .. }
                            | DerivationConclusion::IndexSeparation { .. }
                    ));
                } else if outcome.components.is_empty() {
                    let canonical = outcome
                        .canonical_goal
                        .as_ref()
                        .expect("an affine-only bounds obligation retains its canonical goal");
                    match conclusion {
                        DerivationConclusion::Goal {
                            goal,
                            sign: GoalSign::Positive,
                        } => {
                            let retained = summary
                                .inventory
                                .goals
                                .get(goal.0 as usize)
                                .expect("affine-only bounds goal ID must resolve");
                            assert_eq!(&retained.expression, canonical);
                            assert!(!outcome.contradictory);
                        }
                        DerivationConclusion::Contradiction => assert!(outcome.contradictory),
                        DerivationConclusion::Relation(_)
                        | DerivationConclusion::Goal { .. }
                        | DerivationConclusion::IntegerDomain(_)
                        | DerivationConclusion::AffineConsequence
                        | DerivationConclusion::UnsignedDivisionProduct
                        | DerivationConclusion::RequirementAffineImage
                        | DerivationConclusion::ContractCall
                        | DerivationConclusion::RangeSeparation { .. }
                        | DerivationConclusion::IndexSeparation { .. }
                        | DerivationConclusion::PostconditionAggregate => {
                            panic!("an affine-only bounds root must conclude its exact goal")
                        }
                    }
                } else {
                    let [requested] = outcome.components.as_slice() else {
                        panic!("a bounds obligation has one normalized relation");
                    };
                    retained_term(summary, requested.right);
                    match conclusion {
                        DerivationConclusion::Relation(relation) => {
                            let left = requested
                                .left
                                .expect("noncontradictory accepted bound has a tracked offset");
                            retained_term(summary, left);
                            let expected = if requested.distinct {
                                Relation::Distinct {
                                    left,
                                    right: requested.right,
                                    difference: 0,
                                }
                            } else {
                                Relation::Bound {
                                    left,
                                    right: requested.right,
                                    bound: requested.bound,
                                }
                            };
                            assert_eq!(relation, &expected);
                            assert!(!outcome.contradictory);
                        }
                        DerivationConclusion::Contradiction => assert!(outcome.contradictory),
                        DerivationConclusion::Goal {
                            goal,
                            sign: GoalSign::Positive,
                        } if matches!(
                            outcome.family,
                            ObligationFamily::AllocationFit | ObligationFamily::RangeFormation
                        ) =>
                        {
                            let retained = summary
                                .inventory
                                .goals
                                .get(goal.0 as usize)
                                .expect("bounds goal ID must resolve");
                            assert_eq!(Some(&retained.expression), outcome.canonical_goal.as_ref());
                            assert!(!outcome.contradictory);
                        }
                        DerivationConclusion::Goal { .. }
                        | DerivationConclusion::IntegerDomain(_)
                        | DerivationConclusion::AffineConsequence
                        | DerivationConclusion::UnsignedDivisionProduct
                        | DerivationConclusion::RequirementAffineImage
                        | DerivationConclusion::ContractCall
                        | DerivationConclusion::RangeSeparation { .. }
                        | DerivationConclusion::IndexSeparation { .. }
                        | DerivationConclusion::PostconditionAggregate => {
                            panic!("this obligation root cannot conclude that goal")
                        }
                    }
                }
            }
            DerivationRootKind::RangePartition {
                obligation,
                partition,
                base,
            } => {
                let outcome = &summary.obligations[obligation as usize];
                assert_eq!(outcome.family, ObligationFamily::RangeFormation);
                assert!(outcome.discharged);
                let partition = &outcome.range_partitions[partition as usize];
                assert_eq!(
                    root.node,
                    if base {
                        partition.base_nonnegative
                    } else {
                        partition.stride_nonnegative
                    }
                );
                assert!(matches!(
                    conclusion,
                    DerivationConclusion::AffineConsequence | DerivationConclusion::Contradiction
                ));
            }
            DerivationRootKind::AllocationUpperBound(ordinal) => {
                let outcome = summary
                    .obligations
                    .get(ordinal as usize)
                    .expect("allocation-ceiling root ordinal must resolve");
                assert_eq!(outcome.family, ObligationFamily::AllocationFit);
                assert!(outcome.discharged);
                assert_eq!(
                    outcome.allocation_length_upper_bound_derivation,
                    Some(root.node)
                );
                let left = outcome.components[0]
                    .left
                    .expect("a retained numeric allocation ceiling has one length term");
                let upper = i128::from(
                    outcome
                        .allocation_length_upper_bound
                        .expect("a retained numeric allocation ceiling has one value"),
                );
                assert_eq!(
                    conclusion,
                    &DerivationConclusion::Relation(Relation::Bound {
                        left,
                        right: ZERO,
                        bound: upper,
                    })
                );
            }
            DerivationRootKind::IntegerDomainObligation(ordinal) => {
                let ordinal = ordinal as usize;
                let outcome = summary
                    .obligations
                    .get(ordinal)
                    .expect("integer-domain root ordinal must resolve");
                assert!(!seen_obligations[ordinal], "one exact root per obligation");
                seen_obligations[ordinal] = true;
                assert_eq!(outcome.family, ObligationFamily::IntegerDomain);
                assert!(outcome.discharged);
                assert_eq!(outcome.derivation, Some(root.node));
                assert!(matches!(conclusion, DerivationConclusion::IntegerDomain(_)));
            }
            DerivationRootKind::ConversionDomainObligation(ordinal) => {
                let ordinal = ordinal as usize;
                let outcome = &summary.obligations[ordinal];
                assert!(!seen_obligations[ordinal], "one exact root per obligation");
                seen_obligations[ordinal] = true;
                assert_eq!(outcome.family, ObligationFamily::ConversionDomain);
                assert!(outcome.discharged);
                assert_eq!(outcome.conjunct, 0);
                assert_eq!(outcome.derivation, Some(root.node));
                match conclusion {
                    DerivationConclusion::Goal {
                        goal,
                        sign: GoalSign::Positive,
                    } => {
                        assert_eq!(
                            Some(&summary.inventory.goals[goal.0 as usize].expression),
                            outcome.canonical_goal.as_ref()
                        );
                    }
                    DerivationConclusion::Contradiction => assert!(outcome.contradictory),
                    other => panic!("a conversion root must conclude its domain: {other:?}"),
                }
            }
            DerivationRootKind::CallGoal(ordinal) => {
                let ordinal = ordinal as usize;
                let outcome = summary
                    .call_goals
                    .get(ordinal)
                    .expect("call-root ordinal must resolve");
                assert!(!seen_calls[ordinal], "one exact root per discharged call");
                seen_calls[ordinal] = true;
                assert_eq!(outcome.disposition, CallGoalDisposition::Discharged);
                assert_eq!(outcome.derivation, Some(root.node));
                assert!(!outcome.node_path.components().is_empty());
                assert!(!outcome.requires_clause.components().is_empty());
                match conclusion {
                    DerivationConclusion::Goal {
                        goal,
                        sign: GoalSign::Positive,
                    } => {
                        let retained_goal = summary
                            .inventory
                            .goals
                            .get(goal.0 as usize)
                            .expect("call goal ID must resolve");
                        assert_eq!(retained_goal.expression, outcome.goal.root);
                    }
                    DerivationConclusion::Contradiction => {}
                    DerivationConclusion::Goal {
                        sign: GoalSign::Negative,
                        ..
                    }
                    | DerivationConclusion::Relation(_) => {
                        panic!("a discharged call root must be positive or contradictory")
                    }
                    DerivationConclusion::IntegerDomain(_)
                    | DerivationConclusion::AffineConsequence
                    | DerivationConclusion::UnsignedDivisionProduct
                    | DerivationConclusion::RequirementAffineImage
                    | DerivationConclusion::PostconditionAggregate
                    | DerivationConclusion::RangeSeparation { .. }
                    | DerivationConclusion::IndexSeparation { .. }
                    | DerivationConclusion::ContractCall => {
                        panic!("a discharged call root cannot be a postcondition aggregate")
                    }
                }
            }
            DerivationRootKind::CallContract(_) => {
                assert_eq!(conclusion, &DerivationConclusion::ContractCall);
                assert!(matches!(
                    &summary.derivations.nodes[root.node.0 as usize],
                    DerivationNode::ContractCall { .. }
                ));
            }
            DerivationRootKind::ContractGoal(ordinal) => {
                let ordinal = ordinal as usize;
                let outcome = summary
                    .contract_goals
                    .get(ordinal)
                    .expect("contract-root ordinal must resolve");
                assert!(!seen_contracts[ordinal], "one exact root per FN-4 query");
                seen_contracts[ordinal] = true;
                assert_eq!(outcome.disposition, CallGoalDisposition::Discharged);
                assert_eq!(outcome.derivation, Some(root.node));
                match conclusion {
                    DerivationConclusion::Goal {
                        goal,
                        sign: GoalSign::Positive,
                    } => {
                        let retained_goal = summary
                            .inventory
                            .goals
                            .get(goal.0 as usize)
                            .expect("contract goal ID must resolve");
                        assert_eq!(retained_goal.expression, outcome.goal.root);
                    }
                    DerivationConclusion::Contradiction => {}
                    _ => panic!("a discharged FN-4 root must be positive or contradictory"),
                }
            }
            DerivationRootKind::PermissionSeparation { query, occurrence } => {
                let proof = summary
                    .permission_separations
                    .get(query as usize)
                    .expect("PAR-1 range-query root ordinal must resolve");
                assert!(proof.discharged);
                assert_eq!(
                    proof.derivations.get(occurrence as usize),
                    Some(&root.node),
                    "each successful query visit retains its exact root"
                );
                let DerivationConclusion::RangeSeparation { left, right, .. } = conclusion else {
                    panic!("a discharged PAR-1 range query retains its exact conclusion");
                };
                assert_eq!((*left, *right), (proof.query.left, proof.query.right));
            }
            DerivationRootKind::CountedS11 { occurrence, atom } => {
                counted_root_order.push((occurrence, atom));
                let occurrence = occurrence as usize;
                let counted = summary
                    .counted_derivations
                    .get(occurrence)
                    .expect("counted-root occurrence must resolve");
                let index = counted_atom_index(atom);
                assert!(
                    !seen_counted[occurrence][index],
                    "one exact root per counted atomic relation"
                );
                seen_counted[occurrence][index] = true;
                let expected = counted_atoms(counted)
                    .into_iter()
                    .find_map(|(candidate, atomic)| (candidate == atom).then_some(atomic.parent))
                    .expect("the fixed counted atom must exist");
                assert_eq!(root.node, expected);
            }
            DerivationRootKind::RequirementAffineImage { .. } => {
                assert!(matches!(
                    &summary.derivations.nodes[root.node.0 as usize],
                    DerivationNode::RequirementAffineImage { .. }
                ));
                assert_eq!(conclusion, &DerivationConclusion::RequirementAffineImage);
            }
            DerivationRootKind::UnsignedDivisionProduct(ordinal) => {
                let obligation = &summary.obligations[ordinal as usize];
                assert!(obligation.discharged);
                let DerivationNode::UnsignedDivisionProduct {
                    product, domain, ..
                } = &summary.derivations.nodes[root.node.0 as usize]
                else {
                    panic!("product root must name its constructor");
                };
                assert_eq!(product, &obligation.node_path);
                assert_eq!(Some(*domain), obligation.derivation);
                assert_eq!(conclusion, &DerivationConclusion::UnsignedDivisionProduct);
            }
            DerivationRootKind::PostconditionExit {
                relation_ordinal,
                occurrence,
                ..
            } => {
                let proof = summary
                    .postconditions
                    .get(relation_ordinal as usize)
                    .expect("a postcondition root requires retained proof metadata");
                let occurrence = occurrence as usize;
                let exit = proof
                    .exits
                    .get(occurrence)
                    .expect("postcondition exit root occurrence must resolve");
                assert_eq!(exit.disposition, PostconditionDisposition::Discharged);
                assert_eq!(exit.derivation, Some(root.node));
                let seen = &mut seen_postcondition_exits[relation_ordinal as usize][occurrence];
                assert!(!*seen, "one exact root per discharged exit");
                *seen = true;
                assert!(matches!(conclusion, DerivationConclusion::Relation(_)));
                let DerivationNode::PostconditionExit {
                    statement,
                    relation,
                    ..
                } = &summary.derivations.nodes[root.node.0 as usize]
                else {
                    panic!("postcondition exit root must name an exit node");
                };
                assert_eq!(statement, &exit.statement);
                assert_eq!(relation.as_ref(), &exit.relation);
            }
            DerivationRootKind::PostconditionAggregate {
                relation_ordinal, ..
            } => {
                let proof = summary
                    .postconditions
                    .get(relation_ordinal as usize)
                    .expect("an aggregate root requires retained proof metadata");
                let aggregate = &proof.aggregate;
                assert!(aggregate.discharged);
                assert_eq!(aggregate.derivation, Some(root.node));
                let seen = &mut seen_postcondition_aggregates[relation_ordinal as usize];
                assert!(!*seen, "one exact root per discharged aggregate");
                *seen = true;
                assert_eq!(conclusion, &DerivationConclusion::PostconditionAggregate);
                match &summary.derivations.nodes[root.node.0 as usize] {
                    DerivationNode::PostconditionAggregate {
                        block,
                        relation_ordinal: node_ordinal,
                        parents,
                    } => {
                        assert_eq!(block, &proof.block);
                        assert_eq!(*node_ordinal, relation_ordinal);
                        let expected = proof
                            .exits
                            .iter()
                            .map(|exit| {
                                exit.derivation
                                    .expect("a discharged aggregate requires every exit root")
                            })
                            .collect::<Vec<_>>();
                        assert_eq!(parents, &expected);
                    }
                    DerivationNode::SignatureContract {
                        block,
                        relation_ordinal: node_ordinal,
                    } => {
                        // PRE-1 declarations publish their written signature,
                        // with no fabricated source return or proof edge.
                        assert_eq!(block, &proof.block);
                        assert_eq!(*node_ordinal, relation_ordinal);
                        assert!(proof.exits.is_empty());
                    }
                    _ => panic!(
                        "postcondition aggregate root must name its ordinary proof or signature premise"
                    ),
                }
            }
            DerivationRootKind::PostconditionState { occurrence }
            | DerivationRootKind::PostconditionConditional { occurrence } => {
                assert_eq!(occurrence, seen_s12);
                seen_s12 += 1;
                assert!(!seen_s12_nodes[root.node.0 as usize]);
                seen_s12_nodes[root.node.0 as usize] = true;
                assert!(matches!(conclusion, DerivationConclusion::Relation(_)));
                assert!(matches!(
                    summary.derivations.nodes[root.node.0 as usize],
                    DerivationNode::PostconditionCall { .. }
                ));
            }
            DerivationRootKind::PostconditionDirectResult { occurrence, .. } => {
                assert_eq!(occurrence, seen_s12);
                seen_s12 += 1;
                assert!(!seen_s12_nodes[root.node.0 as usize]);
                seen_s12_nodes[root.node.0 as usize] = true;
                assert!(matches!(conclusion, DerivationConclusion::Relation(_)));
                assert!(matches!(
                    summary.derivations.nodes[root.node.0 as usize],
                    DerivationNode::PostconditionDirectResult { .. }
                ));
            }
            DerivationRootKind::PostconditionDirectReceiver { occurrence, .. } => {
                assert_eq!(occurrence, seen_s12);
                seen_s12 += 1;
                assert!(!seen_s12_nodes[root.node.0 as usize]);
                seen_s12_nodes[root.node.0 as usize] = true;
                assert!(matches!(conclusion, DerivationConclusion::Relation(_)));
                assert!(matches!(
                    summary.derivations.nodes[root.node.0 as usize],
                    DerivationNode::PostconditionDirectReceiver { .. }
                ));
            }
            DerivationRootKind::PostconditionGive { occurrence, .. } => {
                assert_eq!(occurrence, seen_delivery_gives);
                seen_delivery_gives += 1;
                assert!(!seen_delivery_nodes[root.node.0 as usize]);
                seen_delivery_nodes[root.node.0 as usize] = true;
                assert!(matches!(conclusion, DerivationConclusion::Relation(_)));
                assert!(matches!(
                    summary.derivations.nodes[root.node.0 as usize],
                    DerivationNode::PostconditionGive { .. }
                ));
            }
            DerivationRootKind::PostconditionDeliveryJoin { occurrence, .. } => {
                assert_eq!(occurrence, seen_delivery_joins);
                seen_delivery_joins += 1;
                assert!(!seen_delivery_nodes[root.node.0 as usize]);
                seen_delivery_nodes[root.node.0 as usize] = true;
                assert!(matches!(conclusion, DerivationConclusion::Relation(_)));
                assert!(matches!(
                    summary.derivations.nodes[root.node.0 as usize],
                    DerivationNode::PostconditionDeliveryJoin { .. }
                ));
            }
        }

        match &summary.derivations.nodes[root.node.0 as usize] {
            DerivationNode::SourceGoal { .. }
            | DerivationNode::JoinGoal { .. }
            | DerivationNode::MaterializedGoal { .. } => class_counts[1] += 1,
            DerivationNode::GoalProjection { .. }
            | DerivationNode::GoalAffineConsequence { .. } => class_counts[2] += 1,
            DerivationNode::L0Contradiction { .. }
            | DerivationNode::GoalContradiction { .. }
            | DerivationNode::JoinContradiction { .. }
            | DerivationNode::MaterializedContradiction { .. } => class_counts[3] += 1,
            _ => class_counts[0] += 1,
        }
    }
    let mut reachable = vec![false; summary.derivations.nodes.len()];
    let mut stack: Vec<_> = summary
        .derivations
        .roots
        .iter()
        .map(|root| root.node)
        .collect();
    while let Some(id) = stack.pop() {
        let index = id.0 as usize;
        if reachable[index] {
            continue;
        }
        reachable[index] = true;
        stack.extend(summary.derivations.nodes[index].parent_ids());
    }
    assert!(
        reachable.iter().all(|is_reachable| *is_reachable),
        "finish must prune every node outside the mandatory-root sub-DAG"
    );
    for (index, node) in summary.derivations.nodes.iter().enumerate() {
        let is_state_root = matches!(node, DerivationNode::PostconditionCall { .. })
            && summary.derivations.roots.iter().any(|root| {
                root.node.0 as usize == index
                    && matches!(
                        root.kind,
                        DerivationRootKind::PostconditionState { .. }
                            | DerivationRootKind::PostconditionConditional { .. }
                    )
            });
        let is_route = is_state_root
            || matches!(
                node,
                DerivationNode::PostconditionDirectResult { .. }
                    | DerivationNode::PostconditionDirectReceiver { .. }
            );
        assert_eq!(
            seen_s12_nodes[index], is_route,
            "every retained S12 route node must have exactly one matching required root"
        );
        let is_delivery = matches!(
            node,
            DerivationNode::PostconditionGive { .. }
                | DerivationNode::PostconditionDeliveryJoin { .. }
        );
        assert_eq!(
            seen_delivery_nodes[index], is_delivery,
            "every retained delivery node must have exactly one matching required root"
        );
    }

    for (ordinal, outcome) in summary.obligations.iter().enumerate() {
        assert_eq!(outcome.derivation.is_some(), outcome.discharged);
        assert_eq!(seen_obligations[ordinal], outcome.discharged);
    }
    for (ordinal, outcome) in summary.call_goals.iter().enumerate() {
        let discharged = outcome.disposition == CallGoalDisposition::Discharged;
        assert_eq!(outcome.derivation.is_some(), discharged);
        assert_eq!(seen_calls[ordinal], discharged);
    }
    for (ordinal, outcome) in summary.contract_goals.iter().enumerate() {
        let discharged = outcome.disposition == CallGoalDisposition::Discharged;
        assert_eq!(outcome.derivation.is_some(), discharged);
        assert_eq!(seen_contracts[ordinal], discharged);
    }
    assert!(
        seen_counted
            .iter()
            .all(|occurrence| occurrence.iter().all(|seen| *seen)),
        "every counted statement must retain all eight atomic roots"
    );
    for ((proof, seen), seen_aggregates) in summary
        .postconditions
        .iter()
        .zip(seen_postcondition_exits)
        .zip(seen_postcondition_aggregates)
    {
        for (ordinal, exit) in proof.exits.iter().enumerate() {
            let discharged = exit.disposition == PostconditionDisposition::Discharged;
            assert_eq!(exit.derivation.is_some(), discharged);
            assert_eq!(seen[ordinal], discharged);
        }
        assert_eq!(
            proof.aggregate.derivation.is_some(),
            proof.aggregate.discharged
        );
        assert_eq!(seen_aggregates, proof.aggregate.discharged);
    }
    let expected_counted_order: Vec<_> = summary
        .counted_derivations
        .iter()
        .enumerate()
        .flat_map(|(occurrence, counted)| {
            counted_atoms(counted).map(|(atom, _)| {
                (
                    u32::try_from(occurrence).expect("counted test occurrence fits u32"),
                    atom,
                )
            })
        })
        .collect();
    assert_eq!(
        counted_root_order, expected_counted_order,
        "counted ledger roots stay grouped in source occurrence and normative atom order"
    );
    assert_eq!(
        class_counts,
        [
            summary.derivations.metrics.bounds_roots,
            summary.derivations.metrics.opaque_goal_roots,
            summary.derivations.metrics.projected_goal_roots,
            summary.derivations.metrics.contradiction_roots,
        ]
    );
}

fn assert_derivation_metadata_edit_rejected(
    summary: &FunctionEntailment,
    edit: impl FnOnce(&mut FunctionEntailment),
) {
    let mut edited = summary.clone();
    edit(&mut edited);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            validate_derivations(&edited);
        }))
        .is_err(),
        "an inconsistent counted-root edit must fail the metadata checker"
    );
}

#[derive(Debug, Default, Eq, PartialEq)]
struct DistinctGroundCounts {
    source: usize,
    strict: usize,
    contradiction: usize,
    joins: usize,
    join_edges: usize,
    join_parent_counts: Vec<usize>,
}

fn collect_distinct_grounds(
    summary: &FunctionEntailment,
    id: DerivationId,
    counts: &mut DistinctGroundCounts,
) {
    match &summary.derivations.nodes[id.0 as usize] {
        DerivationNode::SourceDistinct { .. } => counts.source += 1,
        DerivationNode::DisequalityFromStrictBound { .. } => counts.strict += 1,
        DerivationNode::JoinDistinct { parents, .. } => {
            counts.joins += 1;
            counts.join_edges += parents.len();
            counts.join_parent_counts.push(parents.len());
            for parent in parents {
                collect_distinct_grounds(summary, parent.parent, counts);
            }
        }
        DerivationNode::MaterializedDistinct { parent, .. } => {
            collect_distinct_grounds(summary, *parent, counts);
        }
        DerivationNode::L0Contradiction { .. }
        | DerivationNode::GoalContradiction { .. }
        | DerivationNode::JoinContradiction { .. }
        | DerivationNode::MaterializedContradiction { .. } => counts.contradiction += 1,
        node => panic!("distinct proof has incompatible ground {node:?}"),
    }
}

fn projected_call_parent(summary: &FunctionEntailment, ordinal: usize) -> DerivationId {
    let root = summary.call_goals[ordinal]
        .derivation
        .expect("discharged call must have one exact root");
    let DerivationNode::GoalProjection { parent, .. } = &summary.derivations.nodes[root.0 as usize]
    else {
        panic!("call root must be its exact L0 goal projection");
    };
    *parent
}

#[test]
fn accepted_transitive_bounds_and_discharged_calls_retain_exact_parent_roots() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

struct Pair {
  count: u64;
}

fn below(value: u64) -> result: unit pure contract {
  requires value < 4_u64;
} {
  return unit;
}

fn read(p: Pair, i: u64) -> result: i32 pure {
  if i <= p.count {
    if p.count < 4_u64 {
      let item = values[i];
      below(value: i);
      return item;
    } else {
      return 0_i32;
    }
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(summary.obligations.len(), 1);
    assert_eq!(summary.call_goals.len(), 1);
    assert!(summary.obligations[0].discharged);
    assert_eq!(
        summary.call_goals[0].disposition,
        CallGoalDisposition::Discharged
    );
    let bounds_root = summary.obligations[0]
        .derivation
        .expect("accepted subscript must retain an exact root");
    let call_root = summary.call_goals[0]
        .derivation
        .expect("discharged ordinary call must retain an exact root");
    assert_ne!(bounds_root, call_root);
    assert_eq!(summary.derivations.roots.len(), 2);
    assert!(
        summary
            .derivations
            .nodes
            .iter()
            .any(|node| matches!(node, DerivationNode::TransitiveBound { .. }))
    );
    for (index, node) in summary.derivations.nodes.iter().enumerate() {
        let parents: Vec<_> = match node {
            DerivationNode::TransitiveBound { first, second, .. } => vec![*first, *second],
            _ => Vec::new(),
        };
        assert!(parents.iter().all(|parent| parent.0 < index as u32));
    }
}

#[test]
fn normalized_derivations_are_byte_identical_across_twenty_analyses() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64, left: Bool) -> result: i32 pure {
  if left {
    if i < 4_u64 {
    } else {
      return 0_i32;
    }
  } else if i < 4_u64 {
  } else {
    return 0_i32;
  }
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let first = entailment(source, "read");
    validate_derivations(&first);
    assert!(
        first
            .derivations
            .nodes
            .iter()
            .any(|node| matches!(node, DerivationNode::JoinBound { .. }))
    );
    let expected = normalized_derivation_dump(&first);
    for run in 1..20 {
        let actual = entailment(source, "read");
        validate_derivations(&actual);
        assert_eq!(
            normalized_derivation_dump(&actual),
            expected,
            "normalized function-local ledger changed on run {run}"
        );
    }
}

// ---------------------------------------------------------------------
// [ENT-3] S1 branch facts and their exact negation
// ---------------------------------------------------------------------

#[test]
fn a_dominating_branch_discharges_the_guarded_index_and_not_the_other_arm() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  if i < 4_u64 {
    return values[i];
  } else {
    return values[i];
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    let outcomes = &summary.obligations;
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes[0].discharged, "True arm carries i < 4 = len");
    assert!(!outcomes[1].discharged, "False arm carries only i >= 4");
    assert_eq!(outcomes[1].residual.as_deref(), Some("i < values.len"));
}

#[test]
fn a_projected_bool_scrutinee_retains_its_exact_s1_carrier() {
    let source = br#"struct Flags {
  ready: Bool;
}

fn need_ready(value: Bool) -> result: unit pure contract {
  requires value;
} {
  return unit;
}

fn caller(flags: Flags) -> result: unit pure {
  if flags.ready {
    need_ready(value: flags.ready);
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("projected Bool S1 fixture must check: {outcome:?}");
        };
        let caller = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "caller")
            .expect("caller function");
        let CheckedStatement::Match { scrutinee, .. } =
            &caller.body.as_deref().expect("WF body")[0]
        else {
            panic!("projected Bool branch must retain one checked match");
        };
        assert!(matches!(scrutinee, CheckedExpression::Project { .. }));
        let expected = scrutinee
            .carrier()
            .expect("projected Bool has an exact carrier");
        validate_derivations(&caller.entailment);
        let call = &caller.entailment.call_goals[0];
        assert_eq!(call.disposition, CallGoalDisposition::Discharged);
        let root = call.derivation.expect("S1 whole-goal call root");
        let DerivationNode::SourceGoal { event, .. } =
            &caller.entailment.derivations.nodes[root.0 as usize]
        else {
            panic!("the exact projected whole goal must select its S1 opaque root");
        };
        let retained = retained_event(&caller.entailment, *event);
        assert_eq!(retained.kind, FlowEventKind::S1);
        assert_eq!(retained.node_path.as_ref(), Some(expected));
    });
}

#[test]
fn s1_true_and_false_edges_retain_their_exact_comparison_roots() {
    let source = br#"fn need_below(value: u64) -> result: unit pure contract {
  requires value < 4_u64;
} {
  return unit;
}

fn need_at_least(value: u64) -> result: unit pure contract {
  requires value >= 4_u64;
} {
  return unit;
}

fn caller(value: u64) -> result: unit pure {
  if value < 4_u64 {
    need_below(value: value);
  } else {
    need_at_least(value: value);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "caller");
    validate_derivations(&summary);
    assert_eq!(summary.call_goals.len(), 2);
    for ordinal in 0..2 {
        assert_root_has_event_kind(&summary, call_root(&summary, ordinal), FlowEventKind::S1);
    }
    assert!(matches!(
        summary.derivations.nodes[call_root(&summary, 0).0 as usize],
        DerivationNode::SourceGoal { .. }
    ));
    assert!(matches!(
        summary.derivations.nodes[call_root(&summary, 1).0 as usize],
        DerivationNode::GoalProjection { .. }
    ));
    assert!(matches!(
        summary.derivations.nodes[projected_call_parent(&summary, 1).0 as usize],
        DerivationNode::SourceBound { .. }
    ));
}

#[test]
fn a_constant_offset_discharges_against_a_const_run_and_a_too_large_one_reports() {
    let source = br#"const count: u64 = 4_u64;

const table: Array<u8, count> =[10_u8, 20_u8, 30_u8, 40_u8];

fn read() -> result: u8 pure {
  let inside = table[2_u64];
  let outside = table[9_u64];
  return inside;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    let outcomes = &summary.obligations;
    assert_eq!(outcomes.len(), 2);
    assert!(
        outcomes[0].discharged,
        "2 < 4 by the implicit length equality"
    );
    assert!(!outcomes[1].discharged, "9 < 4 is not derivable");
    assert_eq!(outcomes[1].residual.as_deref(), Some("9_u64 < table.len"));
    assert!(outcomes[1].derivation.is_none());
    let root = obligation_root(&summary, 0);
    assert_root_contains(
        &summary,
        root,
        |node| {
            matches!(
                node,
                DerivationNode::ImplicitBound {
                    kind: ImplicitBoundKind::Constant,
                    ..
                }
            )
        },
        "the literal constant implicit fact",
    );
    assert_root_contains(
        &summary,
        root,
        |node| {
            matches!(
                node,
                DerivationNode::ImplicitBound {
                    kind: ImplicitBoundKind::StandingMeasure,
                    ..
                }
            )
        },
        "the named constant run's implicit length",
    );
    assert_root_contains(
        &summary,
        root,
        |node| matches!(node, DerivationNode::SubsumedBound { .. }),
        "requested-bound subsumption",
    );
}

// ---------------------------------------------------------------------
// [ENT-3] comparison origin (b) and its path validity
// ---------------------------------------------------------------------

#[test]
fn a_bool_binding_carries_its_comparison_to_the_match_when_no_kill_intervenes() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  let flag = i < 4_u64;
  if flag {
    return values[i];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(discharge_flags(source, "read"), vec![true]);
}

#[test]
fn a_set_between_initializer_and_use_invalidates_the_comparison_origin() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  let flag = i < 4_u64;
  set i = i +wrap 1_u64;
  if flag {
    return values[i];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![false],
        "the origin's operand fact was killed by the assignment"
    );
}

// ---------------------------------------------------------------------
// [ENT-4] closure: transitivity, strengthening, contradiction, and the
// flow/closure boundary
// ---------------------------------------------------------------------

#[test]
fn transitivity_composes_branch_facts_through_a_middle_term() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

struct Pair {
  count: u64;
  other: u64;
}

fn read(p: Pair, i: u64) -> result: i32 pure {
  if i <= p.count {
    if p.count < 4_u64 {
      return values[i];
    } else {
      return 0_i32;
    }
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true],
        "i <= p.count and p.count < 4 compose to i < values.len"
    );
}

#[test]
fn disequality_strengthens_a_weak_bound_to_a_strict_one() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  if i <= 4_u64 {
    if i == 4_u64 {
      return 0_i32;
    } else {
      return values[i];
    }
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true],
        "i <= 4 with i != 4 strengthens to i <= 3 < values.len"
    );
}

#[test]
fn equality_retains_both_directed_parents_and_reflexive_implicit_support() {
    let source = br#"fn need_equal(left: u64, right: u64) -> result: unit pure contract {
  requires left == right;
} {
  return unit;
}

fn directed(left: u64, right: u64) -> result: unit pure {
  if left <= right {
    if right <= left {
      need_equal(left: left, right: right);
    } else {
      return unit;
    }
  } else {
    return unit;
  }
  return unit;
}

fn reflexive(value: u64) -> result: unit pure {
  need_equal(left: value, right: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let directed = accepted_entailment(source, "directed");
    validate_derivations(&directed);
    let equality = projected_call_parent(&directed, 0);
    let DerivationNode::Equality {
        forward, reverse, ..
    } = &directed.derivations.nodes[equality.0 as usize]
    else {
        panic!("the exact equality projection must name both directed bounds");
    };
    for parent in [*forward, *reverse] {
        let DerivationNode::SourceBound { event, .. } =
            &directed.derivations.nodes[parent.0 as usize]
        else {
            panic!("each directed equality parent comes from its branch fact");
        };
        assert_eq!(retained_event(&directed, *event).kind, FlowEventKind::S1);
    }

    let reflexive = accepted_entailment(source, "reflexive");
    validate_derivations(&reflexive);
    assert_root_contains(
        &reflexive,
        call_root(&reflexive, 0),
        |node| {
            matches!(
                node,
                DerivationNode::ImplicitBound {
                    kind: ImplicitBoundKind::Reflexive,
                    ..
                }
            )
        },
        "the reflexive implicit bound",
    );
}

#[test]
fn a_contradictory_state_discharges_every_obligation() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn below_minimum(i: u64) -> result: i32 pure {
  if i < 0_u64 {
    return values[9_u64];
  } else {
    return 0_i32;
  }
}

fn above_maximum(i: u64) -> result: i32 pure {
  if i > 18446744073709551615_u64 {
    return values[9_u64];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for (function, kind) in [
        ("below_minimum", ImplicitBoundKind::TypeMinimum),
        ("above_maximum", ImplicitBoundKind::TypeMaximum),
    ] {
        let summary = entailment(source, function);
        validate_derivations(&summary);
        assert_eq!(summary.obligations.len(), 1, "{function}");
        assert!(summary.obligations[0].discharged, "{function}");
        assert!(summary.obligations[0].contradictory, "{function}");
        let root = obligation_root(&summary, 0);
        assert!(matches!(
            summary.derivations.nodes[root.0 as usize],
            DerivationNode::L0Contradiction { .. }
        ));
        assert_root_contains(
            &summary,
            root,
            |node| matches!(node, DerivationNode::ImplicitBound { kind: actual, .. } if *actual == kind),
            "the matching integer type range",
        );
    }
}

#[test]
fn consuming_a_middle_vertex_preserves_its_survivor_consequence() {
    // The move kills p.count, but it cannot retroactively change the already
    // proved independent integer fact i < 4. Killed-vertex projection fixes
    // that one survivor relation before the middle term disappears; this is a
    // deliberate capability increase over query-only closure.
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

nocopy struct Pair {
  count: u64;
  other: u64;
}

fn eat(p: Pair) -> result: unit pure {
  return unit;
}

fn read(p: Pair, i: u64) -> result: i32 pure {
  if i <= p.count {
    if p.count < 4_u64 {
      eat(p: move p);
      return values[i];
    } else {
      return 0_i32;
    }
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "consuming the middle projects the still-true i <= 3 conclusion"
    );
    assert_root_contains(
        &summary,
        obligation_root(&summary, 0),
        |node| matches!(node, DerivationNode::TransitiveBound { .. }),
        "the killed-middle projection",
    );
}

// ---------------------------------------------------------------------
// [ENT-5] kills: assignment overlap and effect-row write projection
// ---------------------------------------------------------------------

#[test]
fn assigning_a_middle_vertex_projects_a_bound_needed_after_the_write() {
    let source = br#"fn increment(x: u8, middle: u8, replacement: u8) -> result: u8 pure contract {
  requires x <= middle;
  requires middle <= 254_u8;
} {
  set middle = replacement;
  let result = x + 1_u8;
  return result;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "increment");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "x <= middle <= 254 must project x <= 254 before middle is assigned"
    );
    assert_root_contains(
        &summary,
        obligation_root(&summary, 0),
        |node| matches!(node, DerivationNode::TransitiveBound { .. }),
        "the killed-middle projection",
    );
}

#[test]
fn projection_does_not_preserve_a_bound_when_the_final_endpoint_is_written() {
    let source = br#"fn increment(x: u8, middle: u8, replacement: u8) -> result: u8 pure contract {
  requires x <= middle;
  requires middle <= 254_u8;
} {
  set middle = replacement;
  set x = 255_u8;
  let result = x + 1_u8;
  return result;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "increment"),
        vec![false],
        "the later endpoint write kills the projected x <= 254 fact"
    );
}

#[test]
fn projection_does_not_preserve_a_bound_through_an_aliasing_write_to_its_endpoint() {
    // [REF-1] the alias is a local name for the path `x`, and [EFF-5]
    // substitutes the callee row's `writes(value)` onto that path, so the
    // call kills every fact supported by x itself.
    let source = br#"fn overwrite(value: &u8) -> result: unit writes(value) {
  set deref(value) = 255_u8;
  return unit;
}

fn increment(x: u8, middle: u8) -> result: u8 pure contract {
  requires x <= middle;
  requires middle <= 254_u8;
} {
  let holder = &x;
  overwrite(value: holder);
  let result = x + 1_u8;
  return result;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "increment"),
        vec![false],
        "the aliasing reference resolves to and kills x itself, so no survivor endpoint exists"
    );
}

#[test]
fn an_assignment_to_a_sibling_field_keeps_facts_and_to_the_fact_field_kills_them() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

struct Pair {
  count: u64;
  other: u64;
}

fn read(p: Pair) -> result: i32 pure {
  if p.count < 4_u64 {
    set p.other = 9_u64;
    let kept = values[p.count];
    set p.count = 9_u64;
    let lost = values[p.count];
    return kept;
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true, false],
        "OWN-7 overlap: p.other is disjoint from p.count; p.count is not"
    );
    assert_root_has_event_kind(&summary, obligation_root(&summary, 0), FlowEventKind::S1);
    assert!(summary.obligations[1].derivation.is_none());
}

#[test]
fn a_callee_writing_through_a_reference_kills_facts_on_that_place() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn bump(p: &u64) -> result: unit writes(p) {
  set deref(p) = 9_u64;
  return unit;
}

fn read(i: u64) -> result: i32 pure {
  if i < 4_u64 {
    bump(p: &i);
    return values[i];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![false],
        "the callee's writes row projects onto the reference actual's path"
    );
    assert!(summary.obligations[0].derivation.is_none());
}

#[test]
fn a_callee_with_no_writes_row_kills_nothing() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn peek(p: &u64) -> result: u64 reads(p) {
  return deref(p);
}

fn read(i: u64) -> result: i32 pure {
  if i < 4_u64 {
    let seen = peek(p: &i);
    return values[i];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "a call whose row carries no writes kills nothing"
    );
    assert_root_has_event_kind(&summary, obligation_root(&summary, 0), FlowEventKind::S1);
}

// ---------------------------------------------------------------------
// [ENT-5] joins and scope-exit ordering
// ---------------------------------------------------------------------

#[test]
fn a_join_keeps_the_weakest_bound_held_on_every_continuing_arm() {
    let source = br#"const two: u64 = 2_u64;

const count: u64 = 4_u64;

const wide: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

const narrow: Array<i32, two> =[0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  if i < 2_u64 {
  } else if i < 4_u64 {
  } else {
    return 0_i32;
  }
  let in_wide = wide[i];
  let in_narrow = narrow[i];
  return in_wide;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true, false],
        "the join keeps i <= 3 (weakest across arms), not the True arm's i <= 1"
    );
    assert_root_contains(
        &summary,
        obligation_root(&summary, 0),
        |node| matches!(node, DerivationNode::JoinBound { parents, .. } if parents.len() == 2),
        "the predecessor-complete joined bound",
    );
}

#[test]
fn a_join_keeps_a_disequality_derived_in_opposite_strict_orientations() {
    let source = br#"fn need_distinct(left: u64, right: u64) -> result: unit pure contract {
  requires left != right;
} {
  return unit;
}

fn need_left_below_right(left: u64, right: u64) -> result: unit pure contract {
  requires left < right;
} {
  return unit;
}

fn need_right_below_left(left: u64, right: u64) -> result: unit pure contract {
  requires right < left;
} {
  return unit;
}

fn caller(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left < right {
    } else {
      return unit;
    }
  } else if right < left {
  } else {
    return unit;
  }
  need_distinct(left: left, right: right);
  need_left_below_right(left: left, right: right);
  need_right_below_left(left: left, right: right);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "caller");
    validate_derivations(&summary);
    let outcomes = &summary.call_goals;
    assert_eq!(outcomes.len(), 3);
    assert_eq!(
        outcomes[0].disposition,
        CallGoalDisposition::Discharged,
        "each strict orientation derives the same disequality, so ENT-5 keeps it"
    );
    assert_eq!(
        outcomes[0].evidence,
        vec![CallGoalEvidence::ExactL0Projection]
    );
    assert_eq!(outcomes[1].disposition, CallGoalDisposition::Unproved);
    assert_eq!(outcomes[2].disposition, CallGoalDisposition::Unproved);
    assert!(outcomes[1].evidence.is_empty());
    assert!(outcomes[2].evidence.is_empty());
    let mut counts = DistinctGroundCounts::default();
    collect_distinct_grounds(&summary, projected_call_parent(&summary, 0), &mut counts);
    // Each strict-derived disequality first becomes independently live at
    // its one-edge join; the merging join retains both of those boundaries.
    assert_eq!(
        counts,
        DistinctGroundCounts {
            strict: 2,
            joins: 3,
            join_edges: 4,
            join_parent_counts: vec![2, 1, 1],
            ..DistinctGroundCounts::default()
        },
        "the normalized joined disequality names both opposite strict parents"
    );
}

#[test]
fn a_joined_derived_disequality_strengthens_a_later_weak_bound() {
    let source =
        br#"fn need_left_below_right(left: u64, right: u64) -> result: unit pure contract {
  requires left < right;
} {
  return unit;
}

fn caller(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left < right {
    } else {
      return unit;
    }
  } else if right < left {
  } else {
    return unit;
  }
  if left <= right {
  } else {
    return unit;
  }
  need_left_below_right(left: left, right: right);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "caller");
    validate_derivations(&summary);
    let outcomes = &summary.call_goals;
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].disposition, CallGoalDisposition::Discharged);
    assert_eq!(
        outcomes[0].evidence,
        vec![CallGoalEvidence::ExactL0Projection],
        "the joined disequality and later weak bound strengthen to the strict requirement"
    );
    let parent = projected_call_parent(&summary, 0);
    let parent_node = &summary.derivations.nodes[parent.0 as usize];
    let distinct = match parent_node {
        DerivationNode::StrengthenedBound { distinct, .. } => *distinct,
        DerivationNode::JoinBound { parents, .. } if parents.len() == 1 => {
            let nested = &summary.derivations.nodes[parents[0].parent.0 as usize];
            let DerivationNode::StrengthenedBound { distinct, .. } = nested else {
                panic!("single-edge continuation must retain the strengthened bound: {nested:?}");
            };
            *distinct
        }
        _ => panic!(
            "post-join weak bound must be strengthened by the joined disequality: {parent_node:?}"
        ),
    };
    let mut counts = DistinctGroundCounts::default();
    collect_distinct_grounds(&summary, distinct, &mut counts);
    assert_eq!(counts.strict, 2);
    // Keep the two strict-derived facts' independence boundaries as well as
    // the join that combines their paths.
    assert_eq!(counts.joins, 3);
    assert_eq!(counts.join_edges, 4);
}

#[test]
fn a_write_kills_a_disequality_materialized_by_a_join() {
    let source = br#"fn need_distinct(left: u64, right: u64) -> result: unit pure contract {
  requires left != right;
} {
  return unit;
}

fn kept(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left < right {
    } else {
      return unit;
    }
  } else if right < left {
  } else {
    return unit;
  }
  need_distinct(left: left, right: right);
  return unit;
}

fn killed(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left < right {
    } else {
      return unit;
    }
  } else if right < left {
  } else {
    return unit;
  }
  set left = left +wrap 1_u64;
  need_distinct(left: left, right: right);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let kept_summary = entailment(source, "kept");
    validate_derivations(&kept_summary);
    let kept = &kept_summary.call_goals;
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].disposition, CallGoalDisposition::Discharged);
    assert_eq!(kept[0].evidence, vec![CallGoalEvidence::ExactL0Projection]);
    let mut kept_counts = DistinctGroundCounts::default();
    collect_distinct_grounds(
        &kept_summary,
        projected_call_parent(&kept_summary, 0),
        &mut kept_counts,
    );
    assert_eq!(kept_counts.strict, 2);
    // The derived facts retain their one-edge independence boundaries.
    assert_eq!(kept_counts.join_edges, 4);

    let killed_summary = entailment(source, "killed");
    validate_derivations(&killed_summary);
    let killed = &killed_summary.call_goals;
    assert_eq!(killed.len(), 1);
    assert_eq!(killed[0].disposition, CallGoalDisposition::Unproved);
    assert!(killed[0].evidence.is_empty());
    assert!(killed_summary.derivations.roots.is_empty());
    assert!(killed_summary.derivations.nodes.is_empty());
    assert!(killed_summary.derivations.events.is_empty());
}

#[test]
fn joins_keep_disequality_across_same_strict_explicit_and_mixed_grounds() {
    let source = br#"fn need_distinct(left: u64, right: u64) -> result: unit pure contract {
  requires left != right;
} {
  return unit;
}

fn same_strict(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left < right {
    } else {
      return unit;
    }
  } else if left < right {
  } else {
    return unit;
  }
  need_distinct(left: left, right: right);
  return unit;
}

fn both_explicit(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left != right {
    } else {
      return unit;
    }
  } else if right != left {
  } else {
    return unit;
  }
  need_distinct(left: left, right: right);
  return unit;
}

fn mixed(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left != right {
    } else {
      return unit;
    }
  } else if right < left {
  } else {
    return unit;
  }
  need_distinct(left: left, right: right);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for function in ["same_strict", "both_explicit", "mixed"] {
        let summary = entailment(source, function);
        validate_derivations(&summary);
        let outcomes = &summary.call_goals;
        assert_eq!(outcomes.len(), 1, "{function}");
        assert_eq!(
            outcomes[0].disposition,
            CallGoalDisposition::Discharged,
            "{function} establishes the same normalized disequality on every input"
        );
        assert_eq!(
            outcomes[0].evidence,
            vec![CallGoalEvidence::ExactL0Projection],
            "{function} discharges through the common L0 relation"
        );
        if function == "mixed" {
            let mut counts = DistinctGroundCounts::default();
            collect_distinct_grounds(&summary, projected_call_parent(&summary, 0), &mut counts);
            assert_eq!(
                counts,
                // The source fact is already live; only the strict-derived
                // fact needs its one-edge independence boundary.
                DistinctGroundCounts {
                    source: 1,
                    strict: 1,
                    joins: 2,
                    join_edges: 3,
                    join_parent_counts: vec![2, 1],
                    ..DistinctGroundCounts::default()
                },
                "the mixed join names its explicit and strict-derived predecessor roots"
            );
        }
    }
}

#[test]
fn a_many_way_join_keeps_mixed_disequality_and_ignores_a_contradictory_input() {
    let source = br#"fn need_distinct(left: u64, right: u64) -> result: unit pure contract {
  requires left != right;
} {
  return unit;
}

fn caller(left: u64, right: u64, first: Bool, second: Bool, third: Bool) -> result: unit pure {
  if first {
    if left < right {
    } else {
      return unit;
    }
  } else if second {
    if left != right {
    } else {
      return unit;
    }
  } else if third {
    if right < left {
    } else {
      return unit;
    }
  } else if 0_u64 < 0_u64 {
  } else {
    return unit;
  }
  need_distinct(left: left, right: right);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "caller");
    validate_derivations(&summary);
    let outcomes = &summary.call_goals;
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].disposition, CallGoalDisposition::Discharged);
    assert_eq!(
        outcomes[0].evidence,
        vec![CallGoalEvidence::ExactL0Projection]
    );
    let mut counts = DistinctGroundCounts::default();
    collect_distinct_grounds(&summary, projected_call_parent(&summary, 0), &mut counts);
    assert_eq!(counts.source, 1);
    assert_eq!(counts.strict, 2);
    assert_eq!(counts.contradiction, 1);
    // The two strict-derived inputs need independence boundaries; the
    // explicit source input can be reused directly.
    assert_eq!(counts.joins, 5);
    counts.join_parent_counts.sort_unstable();
    assert_eq!(counts.join_parent_counts, vec![1, 1, 2, 2, 2]);
    assert_eq!(
        counts.join_edges, 8,
        "the guarded inputs retain all four reaching grounds through the nested joins"
    );
}

#[test]
fn equality_missing_relation_and_a_kill_each_prevent_disequality_survival() {
    let source = br#"fn need_distinct(left: u64, right: u64) -> result: unit pure contract {
  requires left != right;
} {
  return unit;
}

fn equality_input(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left < right {
    } else {
      return unit;
    }
  } else if left == right {
  } else {
    return unit;
  }
  need_distinct(left: left, right: right);
  return unit;
}

fn missing_input(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left < right {
    } else {
      return unit;
    }
  }
  need_distinct(left: left, right: right);
  return unit;
}

fn killed_input(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left < right {
    } else {
      return unit;
    }
    set left = left +wrap 1_u64;
  } else if right < left {
  } else {
    return unit;
  }
  need_distinct(left: left, right: right);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for function in ["equality_input", "missing_input", "killed_input"] {
        let summary = entailment(source, function);
        validate_derivations(&summary);
        let outcomes = &summary.call_goals;
        assert_eq!(outcomes.len(), 1, "{function}");
        assert_eq!(
            outcomes[0].disposition,
            CallGoalDisposition::Unproved,
            "{function} has at least one reaching input without the same live disequality"
        );
        assert!(outcomes[0].evidence.is_empty(), "{function}");
    }
}

#[test]
fn derived_disequality_closure_preserves_contradiction_and_no_loop_induction() {
    let source = br#"fn impossible() -> result: unit pure contract {
  requires 1_u64 < 0_u64;
} {
  return unit;
}

fn reverse_weak_transitivity_control(left: u64, right: u64) -> result: unit pure {
  if left <= right {
  } else {
    return unit;
  }
  if right < left {
  } else {
    return unit;
  }
  impossible();
  return unit;
}

fn both_strict(left: u64, right: u64) -> result: unit pure {
  if left < right {
  } else {
    return unit;
  }
  if right < left {
  } else {
    return unit;
  }
  impossible();
  return unit;
}

fn all_contradictory(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left < left {
    } else {
      return unit;
    }
  } else if right < right {
  } else {
    return unit;
  }
  impossible();
  return unit;
}

fn no_induction(left: u64, right: u64, leave: Bool) -> result: unit pure {
  loop @again {
    need_distinct(left: left, right: right);
    if left < right {
    } else {
      return unit;
    }
    if leave {
      break @again;
    }
  }
  return unit;
}

fn need_distinct(left: u64, right: u64) -> result: unit pure contract {
  requires left != right;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for function in [
        "reverse_weak_transitivity_control",
        "both_strict",
        "all_contradictory",
    ] {
        let summary = entailment(source, function);
        validate_derivations(&summary);
        let outcomes = &summary.call_goals;
        assert_eq!(outcomes.len(), 1, "{function}");
        assert_eq!(
            outcomes[0].disposition,
            CallGoalDisposition::Discharged,
            "{function} reaches an all-derivable state; the reverse-weak case is a transitivity contradiction control"
        );
        assert_eq!(
            outcomes[0].evidence,
            vec![CallGoalEvidence::AllDerivable],
            "{function}"
        );
        if function == "all_contradictory" {
            assert_root_contains(
                &summary,
                call_root(&summary, 0),
                |node| {
                    matches!(
                        node,
                        DerivationNode::JoinContradiction { parents, .. }
                            if parents.len() == 2
                    )
                },
                "the binary all-contradictory join with both predecessor roots",
            );
        }
    }
    let loop_summary = entailment(source, "no_induction");
    validate_derivations(&loop_summary);
    let loop_outcomes = &loop_summary.call_goals;
    assert_eq!(loop_outcomes.len(), 1);
    assert_eq!(
        loop_outcomes[0].disposition,
        CallGoalDisposition::Unproved,
        "a relation established inside one ordinary iteration is not induced at its head"
    );
    assert!(loop_outcomes[0].evidence.is_empty());
}

#[test]
fn an_arm_that_leaves_by_return_contributes_nothing_to_the_join() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true],
        "only the True arm reaches the continuation, so its fact survives"
    );
}

#[test]
fn a_fresh_binding_reusing_an_expired_spelling_inherits_no_stale_fact() {
    // The stale-fact/fresh-binding edge case: each arm declares its own
    // `j`; the second is a distinct declaration event [ENT-2] and no fact
    // established for the first may attach to it.
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(pick: Bool) -> result: i32 pure {
  if pick {
    let j = 0_u64;
    if j < 4_u64 {
      return values[j];
    } else {
      return 0_i32;
    }
  } else {
    let j = 9_u64;
    return values[j];
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(summary.obligations.len(), 2);
    assert!(
        summary.obligations[0].discharged,
        "the first j is branch-guarded"
    );
    assert_root_has_event_kind(&summary, obligation_root(&summary, 0), FlowEventKind::S5);
    assert!(
        !summary.obligations[1].discharged,
        "the second j is a fresh declaration event with no facts"
    );
    assert!(summary.obligations[1].derivation.is_none());
}

#[test]
fn a_fact_about_an_outer_binding_survives_an_inner_block_exit() {
    // The v0.59 vehicle was a `region` block, which v0.60 does not have; the
    // subject is [ENT-5] scope exit, so the inner scope is now the `if`
    // block whose own binding dies at the edge while the outer binding's
    // branch fact survives it.
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  if i < 4_u64 {
    let scoped = i;
  } else {
    return 0_i32;
  }
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "scope-exit kills reach only bindings whose scope ends at the edge"
    );
    assert_root_has_event_kind(&summary, obligation_root(&summary, 0), FlowEventKind::S1);
}

// ---------------------------------------------------------------------
// [ENT-5] break, give, and propagate edges
// ---------------------------------------------------------------------

#[test]
fn a_break_edge_carries_surviving_facts_to_the_loop_continuation() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  loop @l {
    if i < 4_u64 {
      break @l;
    } else {
      return 0_i32;
    }
  }
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true],
        "the break edge exits only loop-local scopes; the fact on i survives"
    );
}

#[test]
fn a_kill_before_the_break_edge_leaves_the_continuation_unproved() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  loop @l {
    if i < 4_u64 {
      set i = i +wrap 1_u64;
      break @l;
    } else {
      return 0_i32;
    }
  }
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![false],
        "the assignment killed the branch fact before the break edge"
    );
}

#[test]
fn give_edges_join_at_the_value_match_continuation_with_arm_facts_dead() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  let picked = if i < 4_u64 {
    give values[i];
  } else {
    give 0_i32;
  }
  let after = values[i];
  return picked;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outcomes = obligations(source, "read");
    assert_eq!(outcomes.len(), 2);
    assert!(
        outcomes[0].discharged,
        "inside the True arm the S1 fact discharges the given index"
    );
    assert!(
        !outcomes[1].discharged,
        "after the give join neither arm's exclusive fact survives"
    );
}

#[test]
fn value_if_delivery_joins_unequal_bounds_through_direct_edge_parents() {
    let source = br#"fn guard(value: i32) -> result: unit pure contract {
  requires value < 128_i32;
} {
  return unit;
}

fn choose(value: i32, narrow: Bool) -> result: unit pure {
  let picked = if narrow {
    if value < 8_i32 {
      give value;
    } else {
      return unit;
    }
  } else if value < 128_i32 {
    give value;
  } else {
    return unit;
  }
  guard(value: picked);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "choose");
    validate_derivations(&summary);
    assert_eq!(summary.call_goals.len(), 1);
    assert_eq!(
        summary.call_goals[0].disposition,
        CallGoalDisposition::Discharged
    );
    let joins = summary
        .derivations
        .nodes
        .iter()
        .filter_map(|node| {
            let DerivationNode::PostconditionDeliveryJoin { detail } = node else {
                return None;
            };
            let PostconditionDeliveryJoinDetail {
                receiver,
                relation,
                parents,
                ..
            } = detail.as_ref();
            let Relation::Bound {
                left,
                right: ZERO,
                bound: 127,
            } = relation
            else {
                return None;
            };
            Some((*receiver, *left, parents))
        })
        .collect::<Vec<_>>();
    assert_eq!(joins.len(), 1);
    assert_eq!(
        summary
            .derivations
            .nodes
            .iter()
            .filter(|node| {
                matches!(
                    node,
                    DerivationNode::PostconditionGive { relation, .. }
                        if matches!(
                            **relation,
                            Relation::Bound {
                                right: ZERO,
                                bound: 7 | 127,
                                ..
                            }
                        )
                )
            })
            .count(),
        2,
        "the target bound retains exactly its two selected edge facts"
    );
    assert_eq!(
        summary
            .derivations
            .roots
            .iter()
            .filter(|root| {
                match &summary.derivations.nodes[root.node.0 as usize] {
                    DerivationNode::PostconditionGive { relation, .. } => matches!(
                        **relation,
                        Relation::Bound {
                            right: ZERO,
                            bound: 7 | 127,
                            ..
                        }
                    ),
                    DerivationNode::PostconditionDeliveryJoin { detail } => matches!(
                        detail.relation,
                        Relation::Bound {
                            right: ZERO,
                            bound: 127,
                            ..
                        }
                    ),
                    _ => false,
                }
            })
            .count(),
        3,
        "the target has two direct Give roots and one joined root"
    );
    let (receiver, left, parents) = joins[0];
    assert!(matches!(
        retained_term(&summary, left),
        TermKind::Place(place, IntegerType::I32)
            if place.root == PlaceRoot::Binding(receiver)
                && place.path.is_empty()
    ));
    assert_eq!(parents.len(), 2);
    let mut edge_bounds = parents
        .iter()
        .map(|parent| {
            let node = &summary.derivations.nodes[parent.parent.0 as usize];
            let DerivationNode::PostconditionGive { relation, .. } = node else {
                panic!("delivery join must directly parent a give relation: {node:?}");
            };
            let Relation::Bound { bound, .. } = **relation else {
                panic!("delivery join must directly parent a give relation: {node:?}");
            };
            bound
        })
        .collect::<Vec<_>>();
    edge_bounds.sort_unstable();
    assert_eq!(edge_bounds, vec![7, 127]);
    assert!(
        summary.derivations.nodes.iter().all(|node| match node {
            DerivationNode::PostconditionGive { relation, .. } => {
                !matches!(**relation, Relation::Bound { left, right, .. } if left == right)
            }
            DerivationNode::PostconditionDeliveryJoin { detail } => {
                !matches!(detail.relation, Relation::Bound { left, right, .. } if left == right)
            }
            _ => true,
        }),
        "the fresh receiver contributes no reflexive source fact"
    );
}

#[test]
fn value_if_delivery_retains_the_ordinary_fallback_and_shared_give_root() {
    // Acceptance alone does not observe the fallback. Inspect both retained
    // derivations: a later candidate kill must be able to expose the ordinary
    // bound even when the selected full bound came from an S12 call.
    let source = br#"fn limit(value: i32) -> result: i32 pure contract {
  requires value < 8_i32;
  ensures result < 8_i32;
} {
  return value;
}

fn guard(value: i32) -> result: unit pure contract {
  requires value < 32_i32;
} {
  return unit;
}

fn choose(value: i32, narrow: Bool) -> result: unit pure contract {
  requires value < 8_i32;
} {
  let picked = if narrow {
    let bounded = limit(value: value);
    if bounded < 16_i32 {
      give bounded;
    } else {
      return unit;
    }
  } else if value < 32_i32 {
    give value;
  } else {
    return unit;
  }
  guard(value: picked);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "choose");
    validate_derivations(&summary);
    let mut call_dependent = Vec::new();
    for node in &summary.derivations.nodes {
        call_dependent.push(
            matches!(node, DerivationNode::PostconditionCall { .. })
                || node
                    .parent_ids()
                    .iter()
                    .any(|parent| call_dependent[parent.0 as usize]),
        );
    }
    let joins = summary
        .derivations
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            let DerivationNode::PostconditionDeliveryJoin { detail } = node else {
                return None;
            };
            let Relation::Bound {
                right: ZERO, bound, ..
            } = detail.relation
            else {
                return None;
            };
            Some((bound, call_dependent[index], &detail.parents))
        })
        .collect::<Vec<_>>();
    let full = joins
        .iter()
        .find(|(bound, call, _)| *bound == 7 && *call)
        .expect("delivery keeps the strongest call-dependent bound");
    let ordinary = joins
        .iter()
        .find(|(bound, call, _)| *bound == 15 && !*call)
        .expect("delivery keeps the weaker ordinary fallback");
    assert_eq!(full.2.len(), 2);
    assert_eq!(ordinary.2.len(), 2);
    assert_eq!(
        full.2[1].parent, ordinary.2[1].parent,
        "both layers reuse the ordinary second edge, with one required Give root"
    );
}

#[test]
fn value_match_delivers_common_bounds_while_a_missing_branch_does_not() {
    let source = br#"enum Choice {
  Narrow();
  Wide();
}

fn missing(value: i32, narrow: Bool) -> result: i32 pure {
  let picked = if narrow {
    if value < 8_i32 {
      give value;
    } else {
      return value;
    }
  } else {
    give value;
  }
  return picked;
}

fn matched(value: i32, choice: Choice) -> result: i32 pure {
  let picked = match choice {
    Narrow() => {
      if value < 8_i32 {
        give value;
      } else {
        return value;
      }
    }
    Wide() => {
      if value < 128_i32 {
        give value;
      } else {
        return value;
      }
    }
  }
  return picked;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    {
        let function = "missing";
        let summary = entailment(source, function);
        validate_derivations(&summary);
        assert!(
            summary.derivations.nodes.iter().all(|node| !matches!(
                node,
                DerivationNode::PostconditionGive { .. }
                    | DerivationNode::PostconditionDeliveryJoin { .. }
            )),
            "{function} retained a delivery node"
        );
        assert!(summary.derivations.roots.iter().all(|root| !matches!(
            root.kind,
            DerivationRootKind::PostconditionGive { .. }
                | DerivationRootKind::PostconditionDeliveryJoin { .. }
        )));
        assert!(summary.derivations.events.iter().all(|event| !matches!(
            event.kind,
            FlowEventKind::PostconditionGive | FlowEventKind::PostconditionDeliveryJoin
        )));
    }
    let summary = entailment(source, "matched");
    validate_derivations(&summary);
    assert!(
        summary.derivations.nodes.iter().any(|node| matches!(
            node, DerivationNode::PostconditionDeliveryJoin { detail }
                if matches!(detail.relation, Relation::Bound { bound: 127, .. })
        )),
        "all delivering arms prove the weaker upper bound"
    );
}

#[test]
fn nonbare_carriers_and_branch_local_support_create_no_delivery_roots() {
    let source = br#"fn computed(value: i32, narrow: Bool) -> result: i32 pure {
  let picked = if narrow {
    if value < 8_i32 {
      give value +wrap 0_i32;
    } else {
      return value;
    }
  } else if value < 128_i32 {
    give value +wrap 0_i32;
  } else {
    return value;
  }
  return picked;
}

fn scoped(value: i32, narrow: Bool) -> result: i32 pure {
  let picked = if narrow {
    let limit = ixor(value, 1_i32);
    if value != limit {
      give value;
    } else {
      return value;
    }
  } else {
    let limit = ixor(value, 2_i32);
    if value != limit {
      give value;
    } else {
      return value;
    }
  }
  return picked;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for function in ["computed", "scoped"] {
        let summary = entailment(source, function);
        validate_derivations(&summary);
        assert!(
            summary.derivations.nodes.iter().all(|node| !matches!(
                node,
                DerivationNode::PostconditionGive { .. }
                    | DerivationNode::PostconditionDeliveryJoin { .. }
            )),
            "{function} retained a delivery node"
        );
        assert!(summary.derivations.roots.iter().all(|root| !matches!(
            root.kind,
            DerivationRootKind::PostconditionGive { .. }
                | DerivationRootKind::PostconditionDeliveryJoin { .. }
        )));
        assert!(summary.derivations.events.iter().all(|event| !matches!(
            event.kind,
            FlowEventKind::PostconditionGive | FlowEventKind::PostconditionDeliveryJoin
        )));
    }
}

#[test]
fn a_contradictory_first_delivery_edge_cannot_launder_the_fresh_receiver() {
    let source = br#"fn choose(value: i32, impossible: Bool) -> result: i32 pure {
  let picked = if impossible {
    if value < value {
      give value;
    } else {
      return value;
    }
  } else if value < 128_i32 {
    give value;
  } else {
    return value;
  }
  return picked;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "choose");
    validate_derivations(&summary);
    let joined = summary
        .derivations
        .nodes
        .iter()
        .filter_map(|node| {
            let DerivationNode::PostconditionDeliveryJoin { detail } = node else {
                return None;
            };
            let PostconditionDeliveryJoinDetail {
                relation, parents, ..
            } = detail.as_ref();
            let Relation::Bound {
                left,
                right: ZERO,
                bound: 127,
            } = relation
            else {
                return None;
            };
            Some((*left, parents))
        })
        .collect::<Vec<_>>();
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].1.len(), 2);
    assert!(matches!(
        summary.derivations.nodes[joined[0].1[0].parent.0 as usize],
        DerivationNode::L0Contradiction { .. }
            | DerivationNode::GoalContradiction { .. }
            | DerivationNode::JoinContradiction { .. }
            | DerivationNode::MaterializedContradiction { .. }
    ));
    assert!(matches!(
        summary.derivations.nodes[joined[0].1[1].parent.0 as usize],
        DerivationNode::PostconditionGive { .. }
    ));
    assert!(summary.derivations.nodes.iter().all(|node| match node {
        DerivationNode::PostconditionGive { relation, .. } => {
            !matches!(**relation, Relation::Bound { left, right, .. } if left == right)
        }
        DerivationNode::PostconditionDeliveryJoin { detail } => {
            !matches!(detail.relation, Relation::Bound { left, right, .. } if left == right)
        }
        _ => true,
    }));
}

#[test]
fn one_structural_value_if_delivery_retains_exact_originating_edge_order() {
    let source = br#"fn guard(value: i32) -> result: unit pure contract {
  requires value < 128_i32;
} {
  return unit;
}

fn choose(value: i32, side: Bool) -> result: unit pure {
  if value < 128_i32 {
    let picked = if side {
      give value;
    } else {
      give value;
    }
    guard(value: picked);
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "choose");
    validate_derivations(&summary);
    let joins = summary
        .derivations
        .nodes
        .iter()
        .filter_map(|node| {
            let DerivationNode::PostconditionDeliveryJoin { detail } = node else {
                return None;
            };
            let PostconditionDeliveryJoinDetail {
                relation, parents, ..
            } = detail.as_ref();
            let Relation::Bound {
                right: ZERO,
                bound: 127,
                ..
            } = relation
            else {
                return None;
            };
            Some(
                parents
                    .iter()
                    .map(|parent| parent.ordinal)
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(joins, vec![vec![0, 1]]);
}

#[test]
fn a_local_invariant_does_not_discard_an_unrelated_no_ensures_value_if_delivery() {
    let source = br#"const values: Array<u8, 4> =[0_u8, 0_u8, 0_u8, 0_u8];

fn choose(value: i32, side: Bool) -> result: i32 pure {
  if value < 128_i32 {
    let picked = if side {
      give value;
    } else {
      give value;
    }
    return picked;
  } else {
    return value;
  }
}

fn read(position: u64) -> result: u8 pure contract {
  requires position <= 3_u64;
} {
  invariant bounded: position <= 3_u64;
  return values[position];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(_) = outcome else {
            panic!("local invariant checking must preserve the unrelated delivery: {outcome:?}");
        };
    });
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(_) = outcome else {
            panic!("dark checking must preserve the unrelated delivery: {outcome:?}");
        };
    });
}

#[test]
fn a_propagate_continuation_keeps_prior_facts_when_the_call_writes_nothing() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

enum Fail {
  Bad();
}

fn source(flag: Bool) -> result: Result<u64, Fail> pure {
  if flag {
    return Ok<u64, Fail>(value: 1_u64);
  } else {
    let bad = Fail::Bad();
    return Err<u64, Fail>(error: bad);
  }
}

fn read(i: u64, flag: Bool) -> result: Result<i32, Fail> pure {
  if i < 4_u64 {
    let v = propagate source(flag: flag);
    let a = values[i];
    return Ok<i32, Fail>(value: a);
  } else {
    return Ok<i32, Fail>(value: 0_i32);
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true],
        "the Err edge leaves the function; the normal continuation keeps i < 4"
    );
}

// ---------------------------------------------------------------------
// [ENT-5] the no-induction loop rule
// ---------------------------------------------------------------------

#[test]
fn a_loop_body_kill_removes_the_fact_from_every_iteration_head() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  let before = values[i];
  loop @l {
    let inside = values[i];
    set i = i +wrap 1_u64;
    if i < 4_u64 {
    } else {
      break @l;
    }
  }
  return before;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true, false],
        "the head state subtracts every fact the body's assignment may kill"
    );
    assert_root_has_event_kind(&summary, obligation_root(&summary, 0), FlowEventKind::S1);
    assert!(summary.obligations[1].derivation.is_none());
}

#[test]
fn a_kill_free_loop_body_keeps_the_entry_fact_at_the_head() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  loop @l {
    let inside = values[i];
    break @l;
  }
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true, true],
        "no kill event in the body touches i, so the fact holds at the front"
    );
}

#[test]
fn d1h_and_d1i_distinguish_a_return_inside_the_loop_from_one_after_it() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn return_inside(i: u64, stop: Bool) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  loop @l {
    let at_head = values[i];
    if stop {
      return at_head;
    }
    break @l;
  }
  return 0_i32;
}

fn return_after(i: u64, stop: Bool) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  loop @l {
    let at_head = values[i];
    break @l;
  }
  if stop {
    return 1_i32;
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "return_inside"),
        vec![true],
        "D1h: a return edge reaches no later front, so it cannot erase the entry fact"
    );
    assert_eq!(
        discharge_flags(source, "return_after"),
        vec![true],
        "D1i control: moving the same return after the loop stays discharged"
    );
}

#[test]
fn a_kill_followed_only_by_the_current_loop_break_does_not_poison_the_head() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  loop @l {
    let at_head = values[i];
    set i = i +wrap 1_u64;
    break @l;
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true],
        "the set's only successor leaves this loop, so no later front observes it"
    );
}

#[test]
fn a_kill_followed_only_by_an_enclosing_break_does_not_poison_the_inner_head() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64, leave_outer: Bool, leave_inner: Bool) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  loop @outer {
    loop @inner {
      let at_head = values[i];
      if leave_outer {
        set i = i +wrap 1_u64;
        break @outer;
      }
      if leave_inner {
        break @inner;
      }
    }
    break @outer;
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true],
        "the assignment reaches only the enclosing loop's continuation, not the inner front"
    );
}

#[test]
fn a_propagate_error_edge_does_not_poison_the_loop_head() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

enum Fail {
  Bad();
}

fn source(fail: Bool) -> result: Result<u64, Fail> pure {
  if fail {
    let bad = Fail::Bad();
    return Err<u64, Fail>(error: bad);
  }
  return Ok<u64, Fail>(value: 1_u64);
}

fn read(i: u64, fail: Bool, leave: Bool) -> result: Result<i32, Fail> pure {
  if i < 4_u64 {
  } else {
    return Ok<i32, Fail>(value: 0_i32);
  }
  loop @l {
    let value = propagate source(fail: fail);
    let at_head = values[i];
    if leave {
      break @l;
    }
  }
  return Ok<i32, Fail>(value: 0_i32);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true],
        "only propagate's Ok edge can return to the front; its Err edge leaves the function"
    );
}

#[test]
fn an_else_free_continuing_kill_still_poisons_the_loop_head() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64, mutate: Bool, leave: Bool) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  loop @l {
    if mutate {
      set i = i +wrap 1_u64;
    }
    let at_head = values[i];
    if leave {
      break @l;
    }
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![false],
        "the mutating arm and the else-free false edge both remain inside the body"
    );
}

#[test]
fn a_give_to_an_initializer_inside_the_loop_carries_its_kill_to_the_head() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64, mutate: Bool, leave: Bool) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  loop @l {
    let picked = if mutate {
      set i = i +wrap 1_u64;
      give 1_i32;
    } else {
      give 0_i32;
    }
    let at_head = values[i];
    if leave {
      break @l;
    }
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![false],
        "the give reaches an initializer continuation inside the body and then the backedge"
    );
}

#[test]
fn a_mixed_branch_ignores_the_return_only_kill_but_keeps_the_continuing_one() {
    let source = br#"const count: u64 = 4_u64;

const left: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

const right: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64, j: u64, stop: Bool, leave: Bool) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  if j < 4_u64 {
  } else {
    return 0_i32;
  }
  loop @l {
    if stop {
      set i = i +wrap 1_u64;
      return 0_i32;
    } else {
      set j = j +wrap 1_u64;
    }
    let left_value = left[i];
    let right_value = right[j];
    if leave {
      break @l;
    }
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true, false],
        "the returning arm's i kill is non-continuing, while the other arm's j kill reaches the backedge"
    );
}

#[test]
fn a_nested_loop_own_break_carries_kills_to_the_outer_loop_head() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64, leave_outer: Bool) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  loop @outer {
    let at_head = values[i];
    loop @inner {
      set i = i +wrap 1_u64;
      break @inner;
    }
    if leave_outer {
      break @outer;
    }
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![false],
        "the inner break reaches a continuation inside the outer body and then its backedge"
    );
}

// ---------------------------------------------------------------------
// [ENT-3] S11 counted-range structural facts
// ---------------------------------------------------------------------

#[test]
fn a_counted_range_discharges_its_binder_and_safe_predecessor_indices() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read() -> result: i32 pure {
  let total = 0_i32;
  for @items (i in 1_u64..4_u64) {
    let previous = i -wrap 1_u64;
    let current_value = values[i];
    let previous_value = values[previous];
    set total = total +wrap current_value;
    set total = total +wrap previous_value;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true, true],
        "S11 proves i < upper and lower <= i; S7 derives the safe predecessor"
    );
}

#[test]
fn a_counted_range_does_not_prove_the_next_index_or_an_unrelated_carried_index() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(j: u64) -> result: i32 pure {
  let total = 0_i32;
  for @items (i in 0_u64..4_u64) {
    let next = i +wrap 1_u64;
    let current_value = values[i];
    let next_value = values[next];
    let unrelated = values[j];
    set total = total +wrap current_value;
    set total = total +wrap next_value;
    set total = total +wrap unrelated;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true, false, false],
        "S11 is not general induction: next may equal upper and j is unrelated"
    );
}

#[test]
fn a_counted_upper_needs_an_independent_relation_to_the_storage_length() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(upper: u64) -> result: i32 pure {
  let total = 0_i32;
  for @items (i in 0_u64..upper) {
    let value = values[i];
    set total = total +wrap value;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![false],
        "i < captured upper does not imply upper <= values.len"
    );
}

#[test]
fn killed_middles_preserve_survivor_consequences_in_counted_and_ordinary_flow() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read() -> result: i32 pure {
  let upper = 4_u64;
  let total = 0_i32;
  for @items (i in 0_u64..upper) {
    set upper = 0_u64;
    let value = values[i];
    set total = total +wrap value;
  }
  return total;
}

fn ordinary(i: u64) -> result: i32 pure {
  let upper = 4_u64;
  if i < upper {
    set upper = 0_u64;
    return values[i];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "the capture equals four before the mutable source changes; that closed snapshot consequence remains true"
    );
    let root = obligation_root(&summary, 0);
    assert_root_has_event_kind(&summary, root, FlowEventKind::S11);
    assert_root_contains(
        &summary,
        root,
        |node| matches!(node, DerivationNode::MaterializedBound { .. }),
        "the counted preheader materialization marker",
    );
    let ordinary = entailment(source, "ordinary");
    validate_derivations(&ordinary);
    assert_eq!(
        ordinary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "the ordinary write projects i < 4 before killing the mutable middle upper"
    );
    // The survivor consequence must be derived through the killed middle
    // `upper`. Which closed endpoint the step reaches — the constant four or Z
    // through that constant's implicit bound — is an equal-bound derivation
    // choice [ENT-4] leaves open, and a seeded closure retains whichever it
    // reaches first.
    assert_root_contains(
        &ordinary,
        obligation_root(&ordinary, 0),
        |node| match node {
            DerivationNode::TransitiveBound {
                left,
                middle,
                right,
                ..
            } => {
                matches!(
                    (
                        retained_term(&ordinary, *left),
                        retained_term(&ordinary, *middle),
                        retained_term(&ordinary, *right),
                    ),
                    (
                        TermKind::Place(i, IntegerType::U64),
                        TermKind::Place(upper, IntegerType::U64),
                        TermKind::Constant(4) | TermKind::Zero,
                    ) if i.root == PlaceRoot::Binding(BindingId(0))
                        && upper.root == PlaceRoot::Binding(BindingId(1))
                )
            }
            _ => false,
        },
        "a projection of i - upper <= -1 through the killed middle upper",
    );
}

#[test]
fn a_write_preserves_a_survivor_bound_derived_through_disequality_strengthening() {
    let source = br#"const count: u64 = 3_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32];

fn read(index: u64, middle: u64) -> result: i32 pure contract {
  requires index <= middle;
  requires index != middle;
  requires middle <= 3_u64;
} {
  set middle = 0_u64;
  return values[index];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "read");
    validate_derivations(&summary);
    let [obligation] = summary.obligations.as_slice() else {
        panic!("the one subscript must retain exactly one bounds obligation");
    };
    assert!(obligation.discharged);
    let root = obligation_root(&summary, 0);
    assert_root_contains(
        &summary,
        root,
        |node| matches!(node, DerivationNode::StrengthenedBound { .. }),
        "index <= middle and index != middle strengthen to index < middle",
    );
    assert_root_contains(
        &summary,
        root,
        |node| matches!(node, DerivationNode::MaterializedBound { bound: 2, .. }),
        "the complete pre-write closure publishes index <= 2 before middle changes",
    );
}

#[test]
fn a_write_preserves_a_survivor_bound_derived_through_an_implicit_type_edge() {
    let source = br#"fn increment(value: u8, byte_limit: u8) -> result: u8 pure contract {
  requires value < byte_limit;
} {
  set byte_limit = 0_u8;
  return value + 1_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "increment");
    validate_derivations(&summary);
    let [obligation] = summary.obligations.as_slice() else {
        panic!("the one exact addition must retain exactly one domain obligation");
    };
    assert!(obligation.discharged);
    let root = obligation_root(&summary, 0);
    assert_root_contains(
        &summary,
        root,
        |node| matches!(node, DerivationNode::ImplicitBound { .. }),
        "the u8 middle contributes its implicit upper bound before it changes",
    );
    assert_root_contains(
        &summary,
        root,
        |node| matches!(node, DerivationNode::MaterializedBound { bound: 254, .. }),
        "the complete pre-write closure publishes value <= 254 independently",
    );
}

#[test]
fn counted_roots_cover_mixed_control_edges_and_unused_s11_facts() {
    let source = br#"enum Stop {
  Failed();
}

fn maybe(fail: Bool) -> result: Result<unit, Stop> pure {
  if fail {
    let stopped = Stop::Failed();
    return Err<unit, Stop>(error: stopped);
  }
  return Ok<unit, Stop>(value: unit);
}

fn mixed_edges(lower: u64, upper: u64, leave: Bool, fail: Bool) -> result: Result<unit, Stop> pure {
  for @zero (zero in 0_u64..0_u64) {
  }
  for @reversed (reversed in 2_u64..1_u64) {
  }
  for @singleton (singleton in 0_u64..1_u64) {
  }
  for @maximum (maximum in 18446744073709551614_u64..18446744073709551615_u64) {
  }
  let mutable_lower = lower;
  let mutable_upper = upper;
  for @mutated (at in mutable_lower..mutable_upper) {
    set mutable_lower = 0_u64;
    set mutable_upper = 0_u64;
    if leave {
      break @mutated;
    }
  }
  for @returning (at in 0_u64..1_u64) {
    if leave {
      return Ok<unit, Stop>(value: unit);
    }
  }
  for @propagating (at in 0_u64..1_u64) {
    let ignored = propagate maybe(fail: fail);
  }
  for @outer_counted (outer in 0_u64..1_u64) {
    for @inner_counted (inner in 0_u64..1_u64) {
      if leave {
        break @inner_counted;
      }
    }
  }
  loop @ordinary {
    for @breaking (at in 0_u64..1_u64) {
      if leave {
        break @ordinary;
      } else {
        break @breaking;
      }
    }
    break @ordinary;
  }
  return Ok<unit, Stop>(value: unit);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "mixed_edges");
    validate_derivations(&summary);
    assert_eq!(summary.counted_derivations.len(), 10);
    assert_eq!(
        summary
            .derivations
            .roots
            .iter()
            .filter(|root| matches!(root.kind, DerivationRootKind::CountedS11 { .. }))
            .count(),
        80
    );
    let paths: std::collections::HashSet<_> = summary
        .counted_derivations
        .iter()
        .map(|counted| counted.counted_node_path.clone())
        .collect();
    assert_eq!(paths.len(), 10, "every counted occurrence is retained once");
    let mutated = &summary.counted_derivations[4];
    for equality in [
        &mutated.lower_capture_eq_endpoint,
        &mutated.upper_capture_eq_endpoint,
    ] {
        assert!(matches!(
            retained_term(
                &summary,
                match equality.relation {
                    Relation::Equal { right, .. } => right,
                    _ => unreachable!(),
                }
            ),
            TermKind::Place(_, IntegerType::U64)
        ));
        assert_eq!(
            equality.forward.proof_point,
            CountedProofPoint::PreheaderSnapshot,
            "both mutable endpoint identities remain rooted at the once-only snapshot"
        );
    }
}

#[test]
fn counted_roots_cover_contradictory_preheaders_and_neutral_join_predecessors() {
    let source = br#"const count: u64 = 1_u64;

const values: Array<i32, count> =[0_i32];

fn contradictory(left: u64, right: u64, choose: Bool) -> result: unit pure {
  if choose {
    if left < left {
    } else {
      return unit;
    }
  } else if right < right {
  } else {
    return unit;
  }
  for @impossible (i in 0_u64..1_u64) {
  }
  return unit;
}

fn joined(x: u64) -> result: i32 pure {
  let upper = 1_u64;
  if x < 0_u64 {
    let impossible = x;
  }
  let total = 0_i32;
  for @items (i in 0_u64..upper) {
    let item = values[i];
    set total = total +wrap item;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let contradictory = entailment(source, "contradictory");
    validate_derivations(&contradictory);
    assert_eq!(contradictory.counted_derivations.len(), 1);
    for (_, atomic) in counted_atoms(&contradictory.counted_derivations[0])
        .into_iter()
        .take(6)
    {
        assert!(matches!(
            contradictory.derivations.nodes[atomic.parent.0 as usize],
            DerivationNode::MaterializedContradiction { .. }
        ));
        assert_eq!(atomic.proof_point, CountedProofPoint::PreheaderSnapshot);
    }
    for (_, atomic) in counted_atoms(&contradictory.counted_derivations[0])
        .into_iter()
        .skip(6)
    {
        assert!(matches!(
            contradictory.derivations.nodes[atomic.parent.0 as usize],
            DerivationNode::SourceBound { event, .. }
                if contradictory.derivations.events[event.0 as usize].kind == FlowEventKind::S11
        ));
        assert_eq!(atomic.proof_point, CountedProofPoint::BodyEntry);
    }
    assert_root_contains(
        &contradictory,
        contradictory.counted_derivations[0]
            .lower_capture_eq_endpoint
            .forward
            .parent,
        |node| matches!(node, DerivationNode::JoinContradiction { parents, .. } if parents.len() == 2),
        "the counted statement follows the binary all-contradictory join",
    );

    let joined = entailment(source, "joined");
    validate_derivations(&joined);
    assert_eq!(joined.counted_derivations.len(), 1);
    assert_eq!(joined.obligations.len(), 1);
    assert!(joined.obligations[0].discharged);
    assert_root_contains(
        &joined,
        obligation_root(&joined, 0),
        |node| {
            let DerivationNode::JoinBound { parents, .. } = node else {
                return false;
            };
            let contradictory = parents
                .iter()
                .filter(|parent| {
                    matches!(
                        joined.derivations.nodes[parent.parent.0 as usize],
                        DerivationNode::L0Contradiction { .. }
                            | DerivationNode::GoalContradiction { .. }
                            | DerivationNode::JoinContradiction { .. }
                            | DerivationNode::MaterializedContradiction { .. }
                    )
                })
                .count();
            parents.len() == 2 && contradictory == 1
        },
        "the counted snapshot consequence's join with its contradictory-neutral predecessor",
    );
}

#[test]
fn inconsistent_counted_root_metadata_fails_the_test_checker() {
    let source = br#"fn probe(upper: u64) -> result: unit pure {
  for @items (i in 0_u64..upper) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "probe");
    validate_derivations(&summary);
    assert_eq!(summary.counted_derivations.len(), 1);

    assert_derivation_metadata_edit_rejected(&summary, |edited| {
        let index = edited
            .derivations
            .roots
            .iter()
            .position(|root| matches!(root.kind, DerivationRootKind::CountedS11 { .. }))
            .expect("counted root");
        edited.derivations.roots.remove(index);
    });
    assert_derivation_metadata_edit_rejected(&summary, |edited| {
        let root = *edited
            .derivations
            .roots
            .iter()
            .find(|root| matches!(root.kind, DerivationRootKind::CountedS11 { .. }))
            .expect("counted root");
        edited.derivations.roots.push(root);
    });
    assert_derivation_metadata_edit_rejected(&summary, |edited| {
        edited.counted_derivations[0]
            .counted_node_path
            .components
            .push(999);
    });
    assert_derivation_metadata_edit_rejected(&summary, |edited| {
        let relation = &mut edited.counted_derivations[0]
            .lower_capture_eq_endpoint
            .forward
            .relation;
        let Relation::Bound { bound, .. } = relation else {
            unreachable!();
        };
        *bound = 1;
    });
    assert_derivation_metadata_edit_rejected(&summary, |edited| {
        edited.counted_derivations[0]
            .upper_capture_eq_endpoint
            .reverse
            .parent = DerivationId(u32::MAX);
    });
    assert_derivation_metadata_edit_rejected(&summary, |edited| {
        let parent = edited.counted_derivations[0]
            .lower_capture_eq_endpoint
            .forward
            .parent;
        let event = match edited.derivations.nodes[parent.0 as usize] {
            DerivationNode::MaterializedBound { event, .. }
            | DerivationNode::MaterializedContradiction { event, .. } => event,
            ref node => panic!("snapshot root has the wrong node: {node:?}"),
        };
        edited.derivations.events[event.0 as usize].kind = FlowEventKind::Join;
    });
    assert_derivation_metadata_edit_rejected(&summary, |edited| {
        let killed_preheader_parent = edited.counted_derivations[0]
            .binder_eq_lower_capture
            .reverse
            .parent;
        edited.counted_derivations[0]
            .lower_capture_le_binder
            .atomic
            .parent = killed_preheader_parent;
    });
}

#[test]
fn generic_counted_roots_are_deterministic_across_twenty_analyses() {
    let source = br#"fn ranges<const n: u64>(values: Slots<u8, n>) -> result: unit pure {
  let upper = values.len;
  for @first (i in 0_u64..upper) {
  }
  for @second (j in 1_u64..upper) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let small = slots_new::<u8, 2>();
  place_back(window: &small, value: 0_u8);
  place_back(window: &small, value: 0_u8);
  ranges::<2>(values: move small);
  let large = slots_new::<u8, 5>();
  place_back(window: &large, value: 0_u8);
  place_back(window: &large, value: 0_u8);
  place_back(window: &large, value: 0_u8);
  place_back(window: &large, value: 0_u8);
  place_back(window: &large, value: 0_u8);
  ranges::<5>(values: move large);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let normalized_instances = || {
        let instances = entailments(source, "ranges");
        assert_eq!(instances.len(), 2);
        let mut normalized = Vec::new();
        for summary in instances {
            validate_derivations(&summary);
            assert_eq!(summary.counted_derivations.len(), 2);
            assert!(
                summary
                    .inventory
                    .terms
                    .iter()
                    .all(|term| !matches!(term, TermKind::ConstParameter(..))),
                "concrete instances retain no symbolic const term"
            );
            // [MSR-1] the window place carries three measures, and [MSR-2]'s
            // table fixes a constant for exactly one of them on a
            // constant-capacity `Slots<T, n>`: `cap` is the type constant
            // `n`, which is the instance's own N and identifies it. A window
            // carries `len` and `head` as descriptor words, so they are not
            // constants of the type [WIN-1, MSR-2].
            let mut constants: Vec<_> = summary
                .inventory
                .measure_bounds
                .iter()
                .filter_map(|bound| match bound {
                    Some(MeasureBound::Constant(value)) => Some(*value),
                    Some(MeasureBound::Equal(_)) | None => None,
                })
                .collect();
            constants.sort_unstable();
            constants.dedup();
            assert_eq!(constants.len(), 1);
            normalized.push((constants[0], normalized_derivation_dump(&summary)));
        }
        normalized.sort_by_key(|(length, _)| *length);
        normalized
    };
    let expected = normalized_instances();
    assert_eq!(
        expected
            .iter()
            .map(|(length, _)| *length)
            .collect::<Vec<_>>(),
        vec![2, 5]
    );
    for run in 1..20 {
        assert_eq!(
            normalized_instances(),
            expected,
            "normalized concrete counted S11 ledgers changed on run {run}"
        );
    }
}

#[test]
fn a_break_free_zero_trip_counted_continuation_is_reachable_not_contradictory() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read() -> result: i32 pure {
  for @empty (i in 4_u64..4_u64) {
    let ignored = i;
  }
  return values[9_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![false],
        "the real false-header edge prevents the ordinary break-free-loop contradictory join"
    );
}

#[test]
fn a_counted_body_fact_does_not_escape_through_the_zero_trip_edge() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  for @maybe (n in 0_u64..1_u64) {
    if i < 4_u64 {
      let ignored = n;
    } else {
      return 0_i32;
    }
  }
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![false],
        "the structural false edge contributes the pre-body state to the continuation join"
    );
}

#[test]
fn a_nested_counted_loop_kill_can_reach_an_outer_ordinary_loop_head() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64, leave: Bool) -> result: i32 pure {
  if i < 4_u64 {
  } else {
    return 0_i32;
  }
  loop @outer {
    for @inner (n in 0_u64..1_u64) {
      set i = i +wrap 1_u64;
      let ignored = n;
    }
    let at_head = values[i];
    if leave {
      break @outer;
    }
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![false],
        "nested counted exhaustion reaches the outer continuation and then its backedge"
    );
}

// ---------------------------------------------------------------------
// [ENT-6] obligations and residual rendering
// ---------------------------------------------------------------------

#[test]
fn a_struct_field_base_renders_its_canonical_place_in_the_residual() {
    let source = br#"const count: u64 = 4_u64;

struct Holder {
  data: Array<u8, count>;
}

fn read(h: Holder, i: u64) -> result: u8 pure {
  return h.data[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outcomes = obligations(source, "read");
    assert_eq!(outcomes.len(), 1);
    assert!(!outcomes[0].discharged);
    assert_eq!(outcomes[0].residual.as_deref(), Some("i < h.data.len"));
}

#[test]
fn a_nested_index_offset_is_no_term_and_renders_its_canonical_bytes() {
    let source = br#"const count: u64 = 4_u64;

const lens: Array<u8, count> =[0_u8, 0_u8, 0_u8, 0_u8];

const order: Array<u64, count> =[0_u64, 0_u64, 0_u64, 0_u64];

fn read(j: u64) -> result: u8 pure {
  if j < 4_u64 {
    return lens[order[j]];
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outcomes = obligations(source, "read");
    assert_eq!(outcomes.len(), 2, "inner offset first, then the outer site");
    assert!(
        outcomes[0].discharged,
        "the inner index over order discharges"
    );
    assert!(
        !outcomes[1].discharged,
        "an index-bearing offset is no term [ENT-2], so the outer obligation is underivable"
    );
    assert_eq!(outcomes[1].residual.as_deref(), Some("order[j] < lens.len"));
}

#[test]
fn a_failed_inner_index_prevents_the_unreached_outer_bounds_obligation() {
    let source = br#"const count: u64 = 4_u64;

const lens: Array<u8, count> =[0_u8, 0_u8, 0_u8, 0_u8];

const order: Array<u64, count> =[0_u64, 0_u64, 0_u64, 0_u64];

fn read(j: u64) -> result: u8 pure {
  return lens[order[j]];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outcomes = obligations(source, "read");
    let [inner] = outcomes.as_slice() else {
        panic!("only the reached inner index may retain an obligation: {outcomes:?}");
    };
    assert_eq!(inner.family, ObligationFamily::Bounds);
    assert!(!inner.discharged);
    assert_eq!(inner.residual.as_deref(), Some("j < order.len"));
}

#[test]
fn a_failed_boolean_index_publishes_no_admitted_goal_origin() {
    let source = br#"fn remember(flags: Array<Bool, 1>) -> result: unit pure {
  let observed = flags[1_u64];
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "remember");
    let [index] = summary.obligations.as_slice() else {
        panic!(
            "the failed Boolean index must retain exactly its own obligation: {:?}",
            summary.obligations
        );
    };
    assert_eq!(index.family, ObligationFamily::Bounds);
    assert!(!index.discharged);
    assert!(
        summary.inventory.goals.iter().all(|goal| !matches!(
            &goal.expression,
            GoalExpression::Operation {
                row: GoalOperation::ArrayIndex { .. },
                ..
            }
        )),
        "an index whose own bounds obligation failed has no admitted value expression to publish"
    );
}

/// The range half's residual spells its measure through the `deref` step.
/// [REF-1]: "the storage it names is reached only through `deref` [TYPE-7]:
/// every place expression, subscript, field selection, payload step, and
/// measure read that goes through a reference variable `p` is written under
/// that step -- `deref(p)`, `deref(p).field`, `deref(part)[i]`,
/// `deref(part).len`", and [OP-15] gives the same one spelling: "`deref(part).len`
/// of a range reference is the one measure no declaration states [REF-4]".
/// A residual printing `order.len` names an expression the writer cannot
/// write.
#[test]
fn a_window_offset_renders_the_outer_subscript_and_a_failed_range_offset_stops_there() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<u8, count> =[0_u8, 0_u8, 0_u8, 0_u8];

fn from_window() -> result: u8 pure {
  let filled = array_filled::<u64, 4>(value: 0_u64);
  let b = slots_from_array::<u64, 4>(values: filled);
  return values[b[0_u64]];
}

fn from_range(order: &[u64]) -> result: u8 reads(order) {
  return values[deref(order)[0_u64]];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let window = obligations(source, "from_window")
        .into_iter()
        .filter(|outcome| outcome.family == ObligationFamily::Bounds)
        .collect::<Vec<_>>();
    assert_eq!(window.len(), 2, "inner offset first, then the outer site");
    assert!(
        window[0].discharged,
        "the declared construction length proves the inner offset [ENT-3.S12]"
    );
    assert_eq!(window[1].residual.as_deref(), Some("b[0_u64] < values.len"));

    let ranged = obligations(source, "from_range")
        .into_iter()
        .filter(|outcome| outcome.family == ObligationFamily::Bounds)
        .collect::<Vec<_>>();
    assert_eq!(
        ranged.len(),
        1,
        "the failed inner range index prevents the outer site from being reached"
    );
    assert_eq!(
        ranged[0].residual.as_deref(),
        Some("0_u64 < deref(order).len")
    );
}

#[test]
fn failed_partial_operations_do_not_create_parent_or_downstream_domain_authority() {
    let source = br#"const values: Array<u8, 1> =[0_u8];

fn add_after_index() -> result: u8 pure {
  let result = values[1_u64] + 1_u8;
  return result;
}

fn continue_after_failed_exact() -> result: u8 pure {
  let below_zero = 0_u8 - 1_u8;
  let recovered = below_zero + 1_u8;
  return recovered;
}

fn continue_after_failed_set() -> result: u8 pure {
  let current = 0_u8;
  set current = 0_u8 - 1_u8;
  let recovered = current + 1_u8;
  return recovered;
}

fn return_after_failed_set(value: u8) -> result: u8 pure contract {
  ensures result > value;
} {
  let current = 0_u8;
  set current = value + 1_u8;
  return current;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

    let indexed = obligations(source, "add_after_index");
    let [inner_index] = indexed.as_slice() else {
        panic!("only the reached inner index may retain an obligation: {indexed:?}");
    };
    assert_eq!(inner_index.family, ObligationFamily::Bounds);
    assert!(!inner_index.discharged);
    assert!(
        indexed
            .iter()
            .all(|outcome| outcome.family != ObligationFamily::IntegerDomain),
        "the outer exact addition has no admitted indexed operand and is not reached"
    );

    let exact = obligations(source, "continue_after_failed_exact");
    let [subtraction, downstream_addition] = exact.as_slice() else {
        panic!("both source-ordered exact sites must retain their own outcome: {exact:?}");
    };
    assert_eq!(subtraction.family, ObligationFamily::IntegerDomain);
    assert!(subtraction.refuted);
    assert!(matches!(
        &subtraction.canonical_goal,
        Some(GoalExpression::Operation {
            row: GoalOperation::Integer {
                operation: CheckedIntegerOperation::SubtractDefined,
                ..
            },
            ..
        })
    ));
    assert_eq!(downstream_addition.family, ObligationFamily::IntegerDomain);
    assert!(!downstream_addition.discharged);
    assert!(matches!(
        &downstream_addition.canonical_goal,
        Some(GoalExpression::Operation {
            row: GoalOperation::Integer {
                operation: CheckedIntegerOperation::AddDefined,
                ..
            },
            ..
        })
    ));

    let set = obligations(source, "continue_after_failed_set");
    let [set_value, after_set] = set.as_slice() else {
        panic!("the failed set value and later exact site retain outcomes: {set:?}");
    };
    assert_eq!(set_value.family, ObligationFamily::IntegerDomain);
    assert!(set_value.refuted);
    assert_eq!(after_set.family, ObligationFamily::IntegerDomain);
    assert!(!after_set.discharged);

    let returned = entailment(source, "return_after_failed_set");
    let [set_value] = returned.obligations.as_slice() else {
        panic!(
            "the failed set value must retain its own domain outcome: {:?}",
            returned.obligations
        );
    };
    assert_eq!(set_value.family, ObligationFamily::IntegerDomain);
    assert!(!set_value.discharged);
    let [postcondition] = returned.postconditions.as_slice() else {
        panic!(
            "the failed return still retains an unproved FN-9 result: {:?}",
            returned.postconditions
        );
    };
    assert_eq!(postcondition.exits.len(), 1);
    assert_eq!(
        postcondition.exits[0].disposition,
        PostconditionDisposition::Unproved
    );
    assert!(!postcondition.aggregate.discharged);
}

// ---------------------------------------------------------------------
// [ENT-3] S6 length facts
// ---------------------------------------------------------------------

#[test]
fn a_declared_construction_length_proves_a_constant_offset_and_an_unknown_one_does_not() {
    // [ENT-3.S6] carries no construction row in v0.60: a construction
    // function's length and capacity facts are the `ensures` of its [PRE-1]
    // record and reach the caller through [ENT-3.S12] like any other declared
    // relation. Its length is the const-generic n, so the proving half must
    // retain S12 publication; no runtime argument datum is needed for n.
    let source = br#"fn sized() -> result: u8 pure {
  let filled = array_filled::<u8, 4>(value: 0_u8);
  let b = slots_from_array::<u8, 4>(values: filled);
  return b[3_u64];
}

fn unknown(b: Slots<u8, 4>) -> result: u8 pure {
  return b[3_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let sized = entailment(source, "sized");
    validate_derivations(&sized);
    assert_eq!(
        sized
            .obligations
            .iter()
            .filter(|outcome| outcome.family == ObligationFamily::Bounds)
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "the declared b.len = 4 makes 3 < b.len derivable [ENT-3] S12"
    );
    let sized_bounds = sized
        .obligations
        .iter()
        .position(|outcome| outcome.family == ObligationFamily::Bounds)
        .expect("the subscript carries one bounds obligation");
    assert_root_contains(
        &sized,
        obligation_root(&sized, sized_bounds),
        |node| matches!(node, DerivationNode::PostconditionDirectResult { .. }),
        "the constructor's S12 result publication",
    );
    let unknown = obligations(source, "unknown");
    let unknown_bounds = unknown
        .iter()
        .find(|outcome| outcome.family == ObligationFamily::Bounds)
        .expect("the unknown-length subscript carries one bounds obligation");
    assert!(
        !unknown_bounds.discharged,
        "a capacity of four bounds nothing about the window's own length"
    );
    assert_eq!(unknown_bounds.residual.as_deref(), Some("3_u64 < b.len"));
}

#[test]
fn a_measure_binding_carries_the_length_into_a_branch() {
    // [ENT-3.S6] `let m = P.len;` establishes m = P.len, so a branch over m is
    // a branch over the length itself. The measure is a member read [OP-15]
    // and no longer a former call.
    let source = br#"fn read(b: Slots<u8, 4>, i: u64) -> result: u8 pure {
  let m = b.len;
  if i < m {
    return b[i];
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(discharge_flags(source, "read"), vec![true]);
}

#[test]
fn a_range_reference_carries_its_formed_length() {
    // [ENT-3.S6] `let part = &P[lo..hi];` establishes `deref(part).len = hi -
    // lo` over the captured endpoint values, which is the successor of the
    // retired whole-run `slice_of` former [REF-4].
    let source = br#"const count: u64 = 4_u64;

const values: Array<u8, count> =[0_u8, 0_u8, 0_u8, 0_u8];

fn read() -> result: u8 pure {
  let window = &values[0_u64..4_u64];
  return deref(window)[3_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        obligations(source, "read")
            .into_iter()
            .filter(|outcome| outcome.family == ObligationFamily::Bounds)
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "deref(window).len = 4 - 0 = 4 [ENT-3] S6"
    );
}

#[test]
fn window_bounds_survive_writes_that_only_kill_their_establishment_middle() {
    // [ENT-5, MSR-2]: an element write reaches the element's own storage and
    // not the window's descriptor, so it never kills the length fact. Writing
    // n kills facts that still mention n, but first projects the already true
    // 3 < b.len consequence whose two endpoints survive the write.
    let source = br#"fn kept(b: Slots<u8, 4>, n: u64) -> result: u8 pure contract {
  requires b.len == n;
} {
  if 3_u64 < n {
    set b[0_u64] = 1_u8;
    return b[3_u64];
  } else {
    return 0_u8;
  }
}

fn killed(b: Slots<u8, 4>, n: u64) -> result: u8 pure contract {
  requires b.len == n;
} {
  if 3_u64 < n {
    set n = 0_u64;
    return b[3_u64];
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let kept = entailment(source, "kept");
    validate_derivations(&kept);
    assert!(
        kept.obligations
            .last()
            .is_some_and(|outcome| outcome.discharged),
        "an element write leaves b.len = n alive"
    );
    let kept_root = obligation_root(&kept, kept.obligations.len() - 1);
    assert_root_has_event_kind(&kept, kept_root, FlowEventKind::S4);

    let killed = entailment(source, "killed");
    validate_derivations(&killed);
    assert!(
        killed
            .obligations
            .last()
            .is_some_and(|outcome| outcome.discharged),
        "writing n preserves the already established 3 < b.len consequence"
    );
    assert_root_contains(
        &killed,
        obligation_root(&killed, killed.obligations.len() - 1),
        |node| match node {
            DerivationNode::TransitiveBound {
                left,
                middle,
                right,
                bound: -1,
                ..
            } => {
                matches!(
                    (
                        retained_term(&killed, *left),
                        retained_term(&killed, *middle),
                        retained_term(&killed, *right),
                    ),
                    (
                        TermKind::Constant(3),
                        TermKind::Place(n, IntegerType::U64),
                        TermKind::Measure(CheckedMeasure::Length, window),
                    ) if n.root == PlaceRoot::Binding(BindingId(1))
                        && window.root == PlaceRoot::Binding(BindingId(0))
                )
            }
            _ => false,
        },
        "the exact 3 - n <= -1 plus n - b.len <= 0 projection",
    );
}

#[test]
fn consuming_the_window_projects_its_copied_length_before_the_root_dies() {
    // The support of b.len is b's root binding, so the measure term dies with
    // the move. Its already copied mathematical value m does not: projecting
    // m = b.len = 4 before the kill soundly preserves m < 8.
    let source = br#"const wide: u64 = 8_u64;

const other: Array<u8, wide> =[0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8];

fn eat(b: Slots<u8, 4>) -> result: unit pure {
  return unit;
}

fn kept() -> result: u8 pure {
  let filled = array_filled::<u8, 4>(value: 0_u8);
  let b = slots_from_array::<u8, 4>(values: filled);
  let m = b.len;
  let sample = other[m];
  eat(b: move b);
  return sample;
}

fn killed() -> result: u8 pure {
  let filled = array_filled::<u8, 4>(value: 0_u8);
  let b = slots_from_array::<u8, 4>(values: filled);
  let m = b.len;
  eat(b: move b);
  let sample = other[m];
  return sample;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let kept = entailment(source, "kept");
    validate_derivations(&kept);
    assert_eq!(
        kept.obligations
            .iter()
            .filter(|outcome| outcome.family == ObligationFamily::Bounds)
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "m = b.len = 4 < 8 while b is live"
    );
    let kept_bounds = kept
        .obligations
        .iter()
        .position(|outcome| outcome.family == ObligationFamily::Bounds)
        .expect("the subscript carries one bounds obligation");
    assert_root_has_event_kind(
        &kept,
        obligation_root(&kept, kept_bounds),
        FlowEventKind::S6,
    );

    let projected = entailment(source, "killed");
    validate_derivations(&projected);
    assert_eq!(
        projected
            .obligations
            .iter()
            .filter(|outcome| outcome.family == ObligationFamily::Bounds)
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "the consuming use projects the independent copied-length bound"
    );
    let projected_bounds = projected
        .obligations
        .iter()
        .position(|outcome| outcome.family == ObligationFamily::Bounds)
        .expect("the subscript carries one bounds obligation");
    assert_root_contains(
        &projected,
        obligation_root(&projected, projected_bounds),
        |node| matches!(node, DerivationNode::TransitiveBound { .. }),
        "the consumed length-root projection",
    );
}

#[test]
fn set_targets_carry_the_same_obligation_in_target_position() {
    let source = br#"const count: u64 = 4_u64;

fn write(values: Slots<u16, count>, i: u64) -> result: u16 pure contract {
  requires values.len == count;
} {
  if i < 4_u64 {
    set values[i] = 9_u16;
    return 1_u16;
  } else {
    set values[i] = 9_u16;
    return 0_u16;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outcomes = obligations(source, "write");
    assert_eq!(outcomes.len(), 2);
    assert!(
        outcomes[0].discharged,
        "target-position discharge is identical"
    );
    assert!(!outcomes[1].discharged);
    assert_eq!(outcomes[1].residual.as_deref(), Some("i < values.len"));
}

// ---------------------------------------------------------------------
// [ENT-3] S1 guard facts
// ---------------------------------------------------------------------

#[test]
fn a_true_guard_establishes_its_comparison_on_the_selected_edge() {
    let source = br#"const values: Array<i32, 4> =[0_i32, 0_i32, 0_i32, 0_i32];

fn clamp_three(value: u64) -> result: u64 pure {
  return imin(value, 3_u64);
}

fn direct(input: u64) -> result: i32 pure {
  let bounded = 0_u64;
  loop @select_bound {
    if bounded == input {
      break @select_bound;
    } else if bounded == 3_u64 {
      break @select_bound;
    } else {
      set bounded = bounded +wrap 1_u64;
    }
  }
  if bounded < 4_u64 {
    return values[bounded];
  }
  return 0_i32;
}

fn through_origin(input: u64) -> result: i32 pure {
  let bounded = 0_u64;
  loop @select_bound {
    if bounded == input {
      break @select_bound;
    } else if bounded == 3_u64 {
      break @select_bound;
    } else {
      set bounded = bounded +wrap 1_u64;
    }
  }
  let ok = bounded < 4_u64;
  if ok {
    return values[bounded];
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(accepted_discharge_flags(source, "direct"), vec![true]);
    assert_eq!(
        accepted_discharge_flags(source, "through_origin"),
        vec![true],
        "the guard reads comparison-origin shape (b) exactly as a direct comparison does"
    );
}

#[test]
fn a_true_band_guard_establishes_its_conjuncts_not_a_whole_tree_relation() {
    // Each conjunct is consumed by a distinct bounds obligation on the true
    // edge of the same Boolean guard.
    let source = br#"const left_values: Array<i32, 4> =[0_i32, 0_i32, 0_i32, 0_i32];

const right_values: Array<i32, 4> =[0_i32, 0_i32, 0_i32, 0_i32];

fn clamp_three(value: u64) -> result: u64 pure {
  return imin(value, 3_u64);
}

fn read(left_raw: u64, right_raw: u64) -> result: i32 pure {
  let left = 0_u64;
  loop @select_left {
    if left == left_raw {
      break @select_left;
    } else if left == 3_u64 {
      break @select_left;
    } else {
      set left = left +wrap 1_u64;
    }
  }
  let right = 0_u64;
  loop @select_right {
    if right == right_raw {
      break @select_right;
    } else if right == 3_u64 {
      break @select_right;
    } else {
      set right = right +wrap 1_u64;
    }
  }
  let left_inside = left < 4_u64;
  let right_inside = right < 4_u64;
  if band(left_inside, right_inside) {
    let first = left_values[left];
    let second = right_values[right];
    return first +wrap second;
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(accepted_discharge_flags(source, "read"), vec![true, true]);
}

// ---------------------------------------------------------------------
// [ENT-3] S5 copy and conversion equalities
// ---------------------------------------------------------------------

#[test]
fn a_literal_a_copy_and_a_total_conversion_carry_the_value_forward() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read() -> result: i32 pure {
  let k = 2_u64;
  let j = k;
  let narrow = 3_u16;
  let widened = cvt::<u16, u64>(narrow);
  let first = values[j];
  let second = values[widened];
  return first +wrap second;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true, true, true],
        "the total conversion discharges; j = k = 2 and widened = narrow = 3 are below values.len"
    );
    for ordinal in 1..3 {
        assert_root_has_event_kind(
            &summary,
            obligation_root(&summary, ordinal),
            FlowEventKind::S5,
        );
    }
}

#[test]
fn a_set_commit_from_a_term_publishes_its_post_commit_value() {
    // The RHS value is read before the write. After the target's stale facts
    // are killed, S5 publishes `start = back`; no runtime check is needed.
    let source = br#"fn tail_byte(data: &[u8]) -> result: u8 reads(data) {
  let n = deref(data).len;
  let have_room = n >= 8_u64;
  let start = 0_u64;
  let out = 0_u8;
  if have_room {
    let back = n -wrap 8_u64;
    set start = back;
    let byte = deref(data)[start];
    set out = byte;
  }
  return out;
}

fn main() -> status: std::process::ExitStatus pure {
  let input = array_filled::<u8, 4096>(value: 7_u8);
  let window = &input[0_u64..4096_u64];
  let byte = tail_byte(data: window);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "tail_byte");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true]
    );
    assert_root_has_event_kind(&summary, obligation_root(&summary, 0), FlowEventKind::S5);
}

#[test]
fn a_scope_exit_keeps_a_closed_consequence_that_does_not_name_the_local() {
    // `next` cannot survive its arm. The mathematical consequence
    // `out = i + 1`, and therefore `out < 4`, does not name `next` and must be
    // materialized before the arm-local binding is killed.
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  let out = 0_u64;
  if i < 3_u64 {
    let next = i +wrap 1_u64;
    set out = next;
  } else {
    return 0_i32;
  }
  return values[out];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true]
    );
    let root = obligation_root(&summary, 0);
    assert_root_has_event_kind(&summary, root, FlowEventKind::S5);
    assert_root_has_event_kind(&summary, root, FlowEventKind::Snapshot);
}

#[test]
fn a_set_commit_kills_the_old_target_fact_before_publishing_the_new_copy() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(replacement: u64) -> result: i32 pure {
  let offset = 0_u64;
  if offset < 4_u64 {
  } else {
    return 0_i32;
  }
  set offset = replacement;
  return values[offset];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![false],
        "offset = replacement cannot preserve the killed fact offset < 4"
    );
}

/// [ENT-3.S5, ENT-3.S7] A `set` evaluates its right-hand side under exactly
/// the value sources a `let` initializer uses, so a wrapping offset whose
/// range the closed state derives publishes its image through the commit
/// value just as the two-statement spelling does.
///
/// This test previously pinned the opposite: the same expression was accepted
/// through an intervening `let` binding and rejected when written directly,
/// because the commit published only the three-row copy image and an
/// arithmetic right-hand side had none. That asymmetry was a defect in the
/// commit's value image, not a language rule, so the test now states the
/// agreement it should always have had.
#[test]
fn a_wrapping_offset_commit_publishes_the_image_its_let_spelling_publishes() {
    let direct = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(replacement: u64) -> result: i32 pure {
  if replacement < 4_u64 {
    let offset = 0_u64;
    set offset = replacement +wrap 0_u64;
    return values[offset];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let through_let = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(replacement: u64) -> result: i32 pure {
  if replacement < 4_u64 {
    let offset = 0_u64;
    let shifted = replacement +wrap 0_u64;
    set offset = shifted;
    return values[offset];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(direct, "read"),
        vec![true],
        "the commit value carries the derived wrapping offset to the target"
    );
    assert_eq!(
        discharge_flags(through_let, "read"),
        discharge_flags(direct, "read"),
        "one value written two ways must have one verdict"
    );
}

/// [ENT-3.S5] A right-hand side outside every value source still publishes no
/// image. `k +wrap k` is no constant offset, so the commit value carries only
/// its type range and the subscript stays undischarged — in both spellings.
#[test]
fn a_set_right_hand_side_outside_every_value_source_publishes_no_image() {
    let direct = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(replacement: u64) -> result: i32 pure {
  if replacement < 4_u64 {
    let offset = 0_u64;
    set offset = replacement +wrap replacement;
    return values[offset];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let through_let = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(replacement: u64) -> result: i32 pure {
  if replacement < 4_u64 {
    let offset = 0_u64;
    let doubled = replacement +wrap replacement;
    set offset = doubled;
    return values[offset];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(direct, "read"),
        vec![false],
        "the compiler does not search for an unlisted operation image"
    );
    assert_eq!(
        discharge_flags(through_let, "read"),
        discharge_flags(direct, "read"),
        "one value written two ways must have one verdict"
    );
}

/// [ENT-3.S5, ENT-3.S7] A wrapping subtraction whose range the closed state
/// cannot derive publishes no image at a commit, exactly as at a `let`: the
/// commit value keeps only its own type range, so a following exact use is
/// still rejected. Accepting it would read `values[u64::MAX]` for a zero
/// operand, which is why the commit value must never carry the unwrapped
/// result of an unproved wrap.
#[test]
fn a_wrapping_subtraction_commit_publishes_no_post_write_image() {
    let direct = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(replacement: u64) -> result: i32 pure {
  if replacement < 4_u64 {
    let offset = 0_u64;
    set offset = replacement -wrap 1_u64;
    return values[offset];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let through_let = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(replacement: u64) -> result: i32 pure {
  if replacement < 4_u64 {
    let offset = 0_u64;
    let lowered = replacement -wrap 1_u64;
    set offset = lowered;
    return values[offset];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(direct, "read"),
        vec![false],
        "an unproved wrapping subtraction leaves the commit value unbounded"
    );
    assert_eq!(
        discharge_flags(through_let, "read"),
        discharge_flags(direct, "read"),
        "one value written two ways must have one verdict"
    );
}

#[test]
fn a_checked_narrowing_conversion_carries_its_input_equality_into_the_ok_arm() {
    // [OP-6, ENT-5] the checked conversion's private success context retains
    // the input value; delivery substitutes its payload into ordinary L0.
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(n: u64) -> result: i32 pure {
  if n < 4_u64 {
    match cvt.checked::<u64, u8>(n) {
      Ok(value: small) => {
        let widened = cvt::<u8, u64>(small);
        return values[widened];
      }
      Err(error: narrowed) => {
        return 0_i32;
      }
    }
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true, true],
        "the exact widening and the index both discharge from the captured small value"
    );
}

#[test]
fn conversion_domain_goals_keep_operand_support_and_loop_lifetimes() {
    let source = br#"fn ignored(value: f64) -> result: i32 pure {
  let allowed = cvt.defined::<f64, i32>(value);
  return cvt::<f64, i32>(value);
}

fn other_operand(value: f64, other: f64) -> result: i32 pure {
  if cvt.defined::<f64, i32>(value) {
    return cvt::<f64, i32>(other);
  }
  return 0_i32;
}

fn alias_write(value: f64) -> result: i32 pure {
  let allowed = cvt.defined::<f64, i32>(value);
  let aliased = &value;
  set deref(aliased) = 1.5_f64;
  if allowed {
    return cvt::<f64, i32>(value);
  }
  return 0_i32;
}

fn backedge(value: f64, stop: Bool) -> result: unit pure {
  let allowed = cvt.defined::<f64, i32>(value);
  loop @again {
    if allowed {
      let converted = cvt::<f64, i32>(value);
    }
    if stop {
      break @again;
    }
    set value = 1.5_f64;
  }
  return unit;
}

fn fresh(value: f64, stop: Bool) -> result: unit pure {
  loop @again {
    if cvt.defined::<f64, i32>(value) {
      let converted = cvt::<f64, i32>(value);
    }
    if stop {
      break @again;
    }
    set value = 1.5_f64;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for (name, discharged) in [
        ("ignored", false),
        ("other_operand", false),
        ("alias_write", false),
        ("backedge", false),
        ("fresh", true),
    ] {
        let summary = entailment(source, name);
        validate_derivations(&summary);
        assert_eq!(summary.obligations.len(), 1, "{name}");
        let outcome = &summary.obligations[0];
        assert_eq!(outcome.family, ObligationFamily::ConversionDomain);
        assert_eq!(outcome.discharged, discharged, "{name}");
        assert!(
            !outcome.contradictory,
            "a stale goal cannot invent contradiction"
        );
    }
}

// ---------------------------------------------------------------------
// [ENT-3] S7 constant-offset arithmetic
// ---------------------------------------------------------------------

#[test]
fn a_conversion_of_an_affine_value_retains_both_ordered_domain_parents() {
    let source = br#"fn sum(left: u32, right: u32) -> result: u8 pure contract {
  requires left <= 127_u32;
  requires right <= 128_u32;
} {
  let total = left + right;
  return cvt::<u32, u8>(total);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "sum");
    validate_derivations(&summary);
    assert_eq!(summary.obligations.len(), 2);
    let conversion = &summary.obligations[1];
    assert_eq!(conversion.family, ObligationFamily::ConversionDomain);
    assert!(conversion.discharged);
    assert_root_contains(
        &summary,
        conversion.derivation.unwrap(),
        |node| matches!(node, DerivationNode::ConversionDomain { parents, .. } if parents.len() == 2),
        "the ordered upper and lower conversion-domain parents",
    );
}

/// [ENT-3.S7] the `%` row's strict divisor relation proves the postcondition,
/// and the retained fact is the divisor relation itself.
#[test]
fn unsigned_remainder_publishes_its_strict_divisor_relation() {
    let source = br#"fn reduce(dividend: u64, divisor: u64) -> result: u64 pure contract {
  requires divisor != 0_u64;
  ensures result < divisor;
} {
  let remainder = dividend % divisor;
  return remainder;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "reduce");
    validate_derivations(&summary);
    let [postcondition] = summary.postconditions.as_slice() else {
        panic!("reduce must retain one postcondition proof");
    };
    assert!(postcondition.aggregate.discharged);
    assert_root_contains(
        &summary,
        postcondition
            .aggregate
            .derivation
            .expect("discharged aggregate root"),
        |node| {
            matches!(
                node,
                DerivationNode::OperationFact {
                    relation: Relation::Bound { bound: -1, .. },
                    ..
                }
            )
        },
        "the remainder's strict divisor relation",
    );
}

/// [ENT-3.S7] signed remainders by constant divisors publish their intervals,
/// and the exact sum's domain stands on them.
#[test]
fn signed_constant_remainders_publish_intervals_for_exact_arithmetic() {
    let source = br#"fn combine(left_seed: i32, right_seed: i32) -> result: i32 pure {
  let left = left_seed % 41_i32;
  let right = right_seed % -3_i32;
  return left + right;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "combine");
    validate_derivations(&summary);
    assert_eq!(summary.obligations.len(), 3);
    assert!(summary.obligations.iter().all(|outcome| outcome.discharged));
    assert_root_contains(
        &summary,
        obligation_root(&summary, 2),
        |node| {
            matches!(
                node,
                DerivationNode::OperationFact {
                    relation: Relation::Bound { right, bound: 40, .. },
                    ..
                } if *right == ZERO
            )
        },
        "the left remainder's upper bound 40",
    );
}

/// [ENT-3.S7, DIAG-2] a fact no required root reaches is not retained, and
/// a consumed fact stands on the closed bound its operand read.
#[test]
fn operation_facts_are_retained_where_a_root_reaches_them() {
    let source = br#"fn at_most(value: u32, limit: u32) -> result: unit pure contract {
  requires value <= limit;
} {
  return unit;
}

fn unused(left: u32, right: u32, count: u32) -> result: u32 pure {
  let masked = iand(left, right);
  let shifted = ishl.wrap(1_u32, count);
  let joined = ior(masked, shifted);
  return joined;
}

fn consumed(value: u32) -> result: unit pure {
  if value <= 1000_u32 {
    let shifted = ishr(value, 3_u32);
    at_most(value: shifted, limit: 125_u32);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let unused = accepted_entailment(source, "unused");
    validate_derivations(&unused);
    assert!(
        !unused
            .derivations
            .nodes
            .iter()
            .any(|node| matches!(node, DerivationNode::OperationFact { .. })),
        "an unconsumed S7 fact is pruned like every other source fact"
    );

    let consumed = accepted_entailment(source, "consumed");
    validate_derivations(&consumed);
    assert_root_contains(
        &consumed,
        call_root(&consumed, 0),
        |node| {
            matches!(
                node,
                DerivationNode::OperationFact {
                    relation: Relation::Bound { right, bound: 125, .. },
                    parents,
                    ..
                } if *right == ZERO && !parents.is_empty()
            )
        },
        "the shift's upper bound over its guarded operand",
    );
}

/// [ENT-3.S7, ENT-2] a measure operand is read as its term: the remainder by
/// a length relates the result to that length directly.
#[test]
fn a_measure_operand_is_read_as_its_term() {
    let source = br#"fn pick(source: &[u64], seed: u64) -> result: u64 reads(source) contract {
  define length = deref(source).len;
  requires length != 0_u64;
} {
  let slot = seed % deref(source).len;
  let value = deref(source)[slot];
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "pick");
    validate_derivations(&summary);
    assert!(summary.obligations.iter().all(|outcome| outcome.discharged));
    let bounds = summary
        .obligations
        .iter()
        .position(|outcome| outcome.family == ObligationFamily::Bounds)
        .expect("the subscript owns one bounds obligation");
    assert_root_contains(
        &summary,
        obligation_root(&summary, bounds),
        |node| {
            matches!(
                node,
                DerivationNode::OperationFact {
                    relation: Relation::Bound { right, bound: -1, .. },
                    ..
                } if *right != ZERO
            )
        },
        "the remainder's strict relation to the length term",
    );
}

/// [ENT-5, ENT-3.S7] a checked row's success payload carries the exact row's
/// offset relation into its `Ok` arm, through the payload transport.
#[test]
fn a_checked_row_payload_carries_the_exact_row_facts() {
    let source = br#"fn at_least(value: u64, floor: u64) -> result: unit pure contract {
  requires floor <= value;
} {
  return unit;
}

fn advance(start: u64, count: u64) -> result: u64 pure {
  match start +checked count {
    Ok(value: end) => {
      at_least(value: end, floor: start);
      return end;
    }
    Err(error: overflow) => {
      return start;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "advance");
    validate_derivations(&summary);
    let root = call_root(&summary, 0);
    assert_root_contains(
        &summary,
        root,
        |node| matches!(node, DerivationNode::ResultTransport { .. }),
        "the payload's transport to the Ok binder",
    );
    assert_root_contains(
        &summary,
        root,
        |node| matches!(node, DerivationNode::OperationFact { .. }),
        "the exact row's offset relation on the payload",
    );
}

/// [ENT-3.S7] a signed `iand` whose operands may both be negative has no
/// row fact, so a nonnegativity requirement on its result stays unproved.
#[test]
fn a_signed_bit_and_without_a_nonnegative_operand_has_no_bound() {
    let source = br#"fn nonnegative(value: i32) -> result: unit pure contract {
  requires 0_i32 <= value;
} {
  return unit;
}

fn signed(left: i32, right: i32) -> result: unit pure {
  let masked = iand(left, right);
  nonnegative(value: masked);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Fn8, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallRequirement(_))
    });
}

#[test]
fn postcondition_exit_and_aggregate_roots_match_retained_metadata() {
    let source = br#"fn identity(value: i32, choose: Bool) -> result: i32 pure contract {
  ensures result == value;
} {
  if choose {
    return value;
  } else {
    return value;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "identity");
    validate_derivations(&summary);
    let proof = summary
        .postconditions
        .first()
        .expect("identity retains its local proof");
    assert_eq!(proof.exits.len(), 2);
    assert!(proof.aggregate.discharged);
}

#[test]
fn caller_postcondition_sources_retain_the_originating_parents() {
    let b_first = br#"fn callee(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn caller(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let called = callee(value: value);
  return called;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(b_first, "caller");
    validate_derivations(&summary);
    let calls = summary
        .derivations
        .nodes
        .iter()
        .filter_map(|node| {
            let DerivationNode::PostconditionCall { detail } = node else {
                return None;
            };
            Some(&detail.parents)
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1);
    assert!(
        calls.iter().all(|parents| parents.is_empty()),
        "the exact callee summary needs no extra relation parent"
    );

    let u_fallback = br#"fn normalized(value: i32) -> result: i32 pure contract {
  requires value == 1_i32;
  ensures result == value;
} {
  return 1_i32;
}

fn caller() -> result: i32 pure contract {
  ensures result == 1_i32;
} {
  let called = normalized(value: 1_i32);
  return called;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let callee = entailment(u_fallback, "normalized");
    validate_derivations(&callee);
    let proof = callee
        .postconditions
        .first()
        .expect("normalized retains a postcondition proof");
    assert!(proof.aggregate.discharged);

    let summary = entailment(u_fallback, "caller");
    validate_derivations(&summary);
    let calls = summary
        .derivations
        .nodes
        .iter()
        .filter_map(|node| {
            let DerivationNode::PostconditionCall { detail } = node else {
                return None;
            };
            Some(&detail.parents)
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].len(), 1);
}

#[test]
fn success_selections_retain_one_conditional_call_context() {
    let source = br#"fn callee(value: i32) -> result: Result<i32, Overflow> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  return Ok<i32, Overflow>(value: value);
}

fn direct(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  match callee(value: value) {
    Ok(value: payload) => {
      return payload;
    }
    Err(error: problem) => {
      return value;
    }
  }
}

fn delivered(value: i32) -> result: i32 pure {
  let selected = match callee(value: value) {
    Ok(value: payload) => {
      give payload;
    }
    Err(error: problem) => {
      give value;
    }
  }
  return selected;
}

fn propagated(value: i32) -> result: Result<i32, Overflow> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  let selected = propagate callee(value: value);
  return Ok<i32, Overflow>(value: selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for function in ["direct", "delivered", "propagated"] {
        let summary = entailment(source, function);
        validate_derivations(&summary);
        let roots = summary
            .derivations
            .roots
            .iter()
            .filter(|root| {
                matches!(
                    root.kind,
                    DerivationRootKind::PostconditionConditional { .. }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            roots.len(),
            1,
            "{function} retains its conditional call clause"
        );
        let DerivationNode::PostconditionCall { detail } =
            &summary.derivations.nodes[roots[0].node.0 as usize]
        else {
            panic!("conditional evidence must retain its checked call");
        };
        assert!(detail.relation.terms().iter().any(|term| matches!(
            retained_term(&summary, *term),
            TermKind::ResultPayload(IntegerType::I32)
        )));
        assert!(
            summary.derivations.nodes.iter().any(|node| matches!(
                node, DerivationNode::ResultTransport { from, .. }
                    if matches!(retained_term(&summary, *from), TermKind::ResultPayload(_))
            )),
            "{function} uses a selected payload in its ordinary proof"
        );
        assert!(
            summary
                .derivations
                .nodes
                .iter()
                .all(|node| { !matches!(node, DerivationNode::PostconditionDirectResult { .. }) })
        );
    }
}

#[test]
fn direct_receivers_and_selected_payload_assignments_discharge_their_uses() {
    let source = br#"fn choose(ignored: i32, value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn selected(value: i32) -> result: Result<i32, Overflow> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  return Ok<i32, Overflow>(value: value);
}

fn guard(left: i32, right: i32) -> result: unit pure contract {
  requires left == right;
} {
  return unit;
}

fn same_binding(slot: i32, replacement: i32) -> result: unit pure {
  set slot = choose(ignored: slot, value: replacement);
  guard(left: slot, right: replacement);
  return unit;
}

fn matched(outer: i32, replacement: i32) -> result: unit pure {
  match selected(value: replacement) {
    Ok(value: payload) => {
      set outer = payload;
      guard(left: outer, right: replacement);
    }
    Err(error: problem) => {
    }
  }
  return unit;
}

fn valued(outer: i32, replacement: i32) -> result: i32 pure {
  let delivered = match selected(value: replacement) {
    Ok(value: payload) => {
      set outer = payload;
      guard(left: outer, right: replacement);
      give outer;
    }
    Err(error: problem) => {
      give outer;
    }
  }
  return delivered;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for (function, selected_route) in [("same_binding", false), ("matched", true), ("valued", true)]
    {
        let summary = entailment(source, function);
        validate_derivations(&summary);
        let count = summary
            .derivations
            .nodes
            .iter()
            .filter(|node| {
                if selected_route {
                    matches!(node, DerivationNode::PostconditionCall { .. })
                } else {
                    matches!(node, DerivationNode::PostconditionDirectReceiver { .. })
                }
            })
            .count();
        assert_eq!(count, 1, "{function} retains one source-context route");
    }
}

#[test]
fn a_proved_exact_offset_establishes_its_equality_unconditionally() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure {
  if i < 3_u64 {
    let next = i + 1_u64;
    return values[next];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    // The subscript's bounds obligation plus the exact site's one canonical
    // integer-domain obligation. Its discharge makes the S7 equality
    // unconditional.
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true, true],
        "the site's discharged overflow obligation is the proof [ENT-3] S7"
    );
    let bounds = summary
        .obligations
        .iter()
        .position(|outcome| outcome.family == ObligationFamily::Bounds)
        .expect("the subscript attaches one bounds obligation");
    assert_root_has_event_kind(
        &summary,
        obligation_root(&summary, bounds),
        FlowEventKind::S7,
    );
}

#[test]
fn a_wrapping_offset_establishes_only_where_the_range_is_already_proved() {
    // The wrap has no runtime check, so the equality holds only where the
    // closed state already proves the unwrapped result stays in range.
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn guarded(p: u64) -> result: i32 pure {
  if p < 4_u64 {
    if p >= 1_u64 {
      let s = p -wrap 1_u64;
      return values[s];
    } else {
      return 0_i32;
    }
  } else {
    return 0_i32;
  }
}

fn unguarded(p: u64) -> result: i32 pure {
  if p < 4_u64 {
    let s = p -wrap 1_u64;
    return values[s];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "guarded"),
        vec![true],
        "p >= 1 proves p - 1 does not underflow, so s = p - 1 <= 2"
    );
    assert_eq!(
        discharge_flags(source, "unguarded"),
        vec![false],
        "p may be 0, where the wrap reaches u64::MAX"
    );
}

#[test]
fn a_checked_offset_payload_keeps_its_interval_and_loses_its_relation_with_its_base() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn direct(i: u64) -> result: i32 pure {
  if i < 3_u64 {
    match i +checked 1_u64 {
      Ok(value: next) => {
        return values[next];
      }
      Err(error: overflowed) => {
        return 0_i32;
      }
    }
  } else {
    return 0_i32;
  }
}

fn through_binding(i: u64) -> result: i32 pure {
  if i < 3_u64 {
    let outcome = i +checked 1_u64;
    match outcome {
      Ok(value: next) => {
        return values[next];
      }
      Err(error: overflowed) => {
        return 0_i32;
      }
    }
  } else {
    return 0_i32;
  }
}

fn interval_survives(i: u64, j: u64) -> result: i32 pure {
  if i < 3_u64 {
    let outcome = i +checked 1_u64;
    set i = j;
    match outcome {
      Ok(value: next) => {
        return values[next];
      }
      Err(error: overflowed) => {
        return 0_i32;
      }
    }
  } else {
    return 0_i32;
  }
}

fn relation_lives(i: u64) -> result: u64 pure {
  match i +checked 1_u64 {
    Ok(value: next) => {
      return next - i;
    }
    Err(error: overflowed) => {
      return 0_u64;
    }
  }
}

fn relation_killed(i: u64, j: u64) -> result: u64 pure {
  let outcome = i +checked 1_u64;
  set i = j;
  match outcome {
    Ok(value: next) => {
      return next - i;
    }
    Err(error: overflowed) => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(discharge_flags(source, "direct"), vec![true]);
    assert_eq!(
        discharge_flags(source, "through_binding"),
        vec![true],
        "a bare IDENT naming the outcome carries the same conditional facts"
    );
    assert_eq!(
        discharge_flags(source, "interval_survives"),
        vec![true],
        "the payload's interval names only the payload, so a base write leaves it"
    );
    assert_eq!(
        discharge_flags(source, "relation_lives"),
        vec![true],
        "the offset relation to the base proves the difference's domain"
    );
    assert_eq!(
        discharge_flags(source, "relation_killed"),
        vec![false],
        "writing the base between the evaluation and the match kills the offset relation"
    );
}

// ---------------------------------------------------------------------
// [ENT-3] S9 const-run element ranges
// ---------------------------------------------------------------------

#[test]
fn a_const_array_element_carries_its_declared_value_range() {
    let source = br#"const count: u64 = 4_u64;

const inside: Array<u64, count> =[0_u64, 1_u64, 3_u64, 2_u64];

const outside: Array<u64, count> =[0_u64, 1_u64, 4_u64, 2_u64];

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn low(i: u64) -> result: i32 pure {
  if i < 4_u64 {
    let bound = inside[i];
    return values[bound];
  } else {
    return 0_i32;
  }
}

fn high(i: u64) -> result: i32 pure {
  if i < 4_u64 {
    let bound = outside[i];
    return values[bound];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let low = entailment(source, "low");
    validate_derivations(&low);
    assert_eq!(
        low.obligations.len(),
        2,
        "the element read carries its own obligation"
    );
    assert!(
        low.obligations[0].discharged,
        "the dominating guard reaches the const-table element read"
    );
    assert!(
        low.obligations[1].discharged,
        "every declared element is at most 3 < values.len"
    );
    assert_root_has_event_kind(&low, obligation_root(&low, 1), FlowEventKind::S9);
    let high = obligations(source, "high");
    assert!(
        !high[1].discharged,
        "a declared element of 4 reaches values.len"
    );
}

#[test]
fn a_projected_const_array_does_not_gain_a_bare_constant_element_range() {
    let source = br#"struct Indices {
  entries: Array<u64, 2>;
}

const table: Indices = Indices(entries:[0_u64, 1_u64]);

const values: Array<i32, 2> =[7_i32, 9_i32];

fn bound_read(i: u64) -> result: i32 pure {
  if i < 2_u64 {
    let index = table.entries[i];
    return values[index];
  } else {
    return 0_i32;
  }
}

fn direct_read(i: u64) -> result: i32 pure {
  if i < 2_u64 {
    return values[table.entries[i]];
  } else {
    return 0_i32;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let bound = obligations(source, "bound_read");
    assert_eq!(bound.len(), 2);
    assert!(
        bound[0].discharged,
        "the actual index guard proves the read"
    );
    assert!(
        !bound[1].discharged,
        "S9 does not read initializer ranges through a constant's field"
    );
    let direct = obligations(source, "direct_read");
    assert_eq!(direct.len(), 2);
    assert!(direct[0].discharged);
    assert!(
        !direct[1].discharged,
        "the nested subscript remains no term"
    );
    assert_eq!(
        direct[1].residual.as_deref(),
        Some("table.entries[i] < values.len")
    );
}

// ---------------------------------------------------------------------
// [ENT-3] S4 requires facts
// ---------------------------------------------------------------------

#[test]
fn a_requirement_establishes_its_substituted_relation_at_body_entry() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure contract {
  define ok = i < 4_u64;
  requires ok;
} {
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    validate_derivations(&summary);
    assert_eq!(
        summary
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "the requirement relation is available at body entry"
    );
    assert_root_has_event_kind(&summary, obligation_root(&summary, 0), FlowEventKind::S4);
}

#[test]
fn a_requires_chain_substitutes_repeatedly_and_reads_a_measure_member() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure contract {
  define n = values.len;
  define ok = i < n;
  requires ok;
} {
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true],
        "ok substitutes to the comparison, then n to the measure term itself"
    );
}

#[test]
fn every_occurrence_of_a_requires_local_substitutes() {
    // Both operands name the same clause local. Expanding only one would
    // leave a non-term operand and establish nothing; expanding both derives
    // values.len < values.len, a contradictory entry state [ENT-4].
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read() -> result: i32 pure contract {
  define n = values.len;
  define ok = n < n;
  requires ok;
} {
  return values[9_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outcomes = obligations(source, "read");
    assert_eq!(outcomes.len(), 1);
    assert!(
        outcomes[0].contradictory,
        "both occurrences expanded to the same length term"
    );
    assert!(outcomes[0].discharged);
}

#[test]
fn a_band_s4_goal_establishes_its_conjuncts_at_body_entry() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(i: u64) -> result: i32 pure contract {
  define low = i < 4_u64;
  define high = i >= 0_u64;
  define ok = band(low, high);
  requires ok;
} {
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        discharge_flags(source, "read"),
        vec![true],
        "S4 establishes the band's signed decomposition set at body entry"
    );
}

// ---------------------------------------------------------------------
// [PRE-1, FN-8, CALL-6] Ordinary declaration range contracts. v0.58
// deletes SYS-8's operand-family obligations; these preserve the endpoint
// witnesses through the same requires path as a WF function body.
// ---------------------------------------------------------------------

fn range_contract_source(contract: &str, body: &str) -> String {
    format!(
        "const endpoints: Array<u64, 2> =[0_u64, 0_u64];\n\nfn publish(factory: &std::io::HandleFactory, output: &std::io::OutputStream, source: &[u8], start: u64, end: u64) -> result: unit reads(source), writes(factory), writes(output){contract} {{\n{body}  return unit;\n}}\n"
    )
}

#[test]
fn a_failed_endpoint_expression_prevents_unreached_call_requirements() {
    let source = range_contract_source(
        "",
        "  let outcome = std::io::write_once(factory: factory, output: output, source: source, start: 0_u64, end: endpoints[2_u64]);\n",
    );
    let outcomes = obligations(source.as_bytes(), "publish");
    let [endpoint_index] = outcomes.as_slice() else {
        panic!("only the reached endpoint index may retain an obligation: {outcomes:?}");
    };
    assert_eq!(endpoint_index.family, ObligationFamily::Bounds);
    assert!(!endpoint_index.discharged);
    assert!(
        call_goals(source.as_bytes(), "publish").is_empty(),
        "CALL-6 starts after the endpoint expression succeeds"
    );
}

#[test]
fn one_ordinary_call_retains_two_independent_ordered_range_requirements() {
    let source = range_contract_source(
        "",
        "  let outcome = std::io::write_once(factory: factory, output: output, source: source, start: start, end: end);\n",
    );
    let outcomes = call_goals(source.as_bytes(), "publish");
    assert_eq!(outcomes.len(), 2);
    assert!(
        outcomes
            .iter()
            .all(|outcome| outcome.disposition != CallGoalDisposition::Discharged)
    );
    assert!(outcomes[0].requires_clause.components() < outcomes[1].requires_clause.components());
    assert_eq!(outcomes[0].node_path, outcomes[1].node_path);
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("an unproved ordinary call range must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn8);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::UndischargedCallRequirement(..)
        ));
    });
}

#[test]
fn ordinary_source_relations_discharge_both_signature_ranges() {
    let source = range_contract_source(
        " contract {\n  requires start <= end;\n  requires end <= deref(source).len;\n}",
        "  let outcome = std::io::write_once(factory: factory, output: output, source: source, start: start, end: end);\n",
    );
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("ordinary source relations discharge the signature requires: {outcome:?}");
        };
        let function = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "publish")
            .expect("publish");
        validate_derivations(&function.entailment);
        let ranges = &function.entailment.call_goals;
        assert_eq!(ranges.len(), 2);
        assert!(
            ranges
                .iter()
                .all(|range| range.disposition == CallGoalDisposition::Discharged)
        );
        for range in ranges {
            let root = range
                .derivation
                .expect("an accepted CALL-6 relation retains its derivation");
            let mut seen = vec![false; function.entailment.derivations.nodes.len()];
            let mut stack = vec![root];
            while let Some(node) = stack.pop() {
                let position = node.0 as usize;
                if seen[position] {
                    continue;
                }
                seen[position] = true;
                let retained = &function.entailment.derivations.nodes[position];
                assert!(
                    !matches!(retained, DerivationNode::AffineConsequence { .. }),
                    "ordinary source relations have priority over affine routes"
                );
                stack.extend(retained.parent_ids());
            }
        }
    });
}

#[test]
fn indexed_guards_discharge_structurally_identical_signature_ranges() {
    let source = range_contract_source(
        "",
        "  let capacity = deref(source).len;\n  if endpoints[0_u64] <= endpoints[1_u64] {\n    if endpoints[1_u64] <= capacity {\n      let outcome = std::io::write_once(factory: factory, output: output, source: source, start: endpoints[0_u64], end: endpoints[1_u64]);\n    }\n  }\n",
    );
    let ranges = call_goals(source.as_bytes(), "publish");
    assert_eq!(ranges.len(), 2);
    assert!(
        ranges
            .iter()
            .all(|range| range.disposition == CallGoalDisposition::Discharged),
        "the indexed guards and actual values must retain the same goals: {ranges:#?}"
    );
    let GoalExpression::Operation {
        arguments: first, ..
    } = &ranges[0].goal.root
    else {
        panic!("first comparison");
    };
    let GoalExpression::Operation {
        arguments: second, ..
    } = &ranges[1].goal.root
    else {
        panic!("second comparison");
    };
    assert!(matches!(
        first.as_slice(),
        [
            GoalExpression::Operation {
                row: GoalOperation::ArrayIndex { .. },
                ..
            },
            GoalExpression::Operation {
                row: GoalOperation::ArrayIndex { .. },
                ..
            }
        ]
    ));
    assert_eq!(
        first[1], second[0],
        "both requirements reuse the end value identity"
    );
}

#[test]
fn a_nonterm_endpoint_is_never_replaced_by_the_zero_term() {
    let source = range_contract_source(
        "",
        "  let outcome = std::io::write_once(factory: factory, output: output, source: source, start: 1_u64, end: endpoints[0_u64]);\n",
    );
    let ranges = call_goals(source.as_bytes(), "publish");
    assert_eq!(ranges.len(), 2);
    assert_eq!(
        ranges[0].disposition,
        CallGoalDisposition::Unproved,
        "the indexed end is unknown, not replaced by zero"
    );
}

// ---------------------------------------------------------------------
// [ENT-3.S12, CALL-6] Declaration postcondition endpoint facts
// ---------------------------------------------------------------------

#[test]
fn a_transfer_endpoint_is_bounded_by_end_and_not_beyond_it() {
    // The bound is `next <= end` against the operation's absolute endpoint,
    // so an endpoint equal to the table length proves nothing.
    let source = br#"const count: u64 = 4_u64;

const table: Array<u8, count> =[0_u8, 0_u8, 0_u8, 0_u8];

fn under(factory: &std::io::HandleFactory, output: &std::io::OutputStream, source: &[u8]) -> result: unit reads(source), writes(factory), writes(output) {
  let source_length = deref(source).len;
  let enough = 3_u64 <= source_length;
  if enough {
    match std::io::write_once(factory: factory, output: output, source: source, start: 0_u64, end: 3_u64) {
      Ok(value: next) => {
        let sample = table[next];
      }
      Err(error: problem) => {
      }
    }
  }
  return unit;
}

fn exact(factory: &std::io::HandleFactory, output: &std::io::OutputStream, source: &[u8]) -> result: unit reads(source), writes(factory), writes(output) {
  let source_length = deref(source).len;
  let enough = 4_u64 <= source_length;
  if enough {
    match std::io::write_once(factory: factory, output: output, source: source, start: 0_u64, end: 4_u64) {
      Ok(value: next) => {
        let sample = table[next];
      }
      Err(error: problem) => {
      }
    }
  }
  return unit;
}
"#;
    let under = entailment(source, "under");
    validate_derivations(&under);
    assert_eq!(
        under
            .obligations
            .iter()
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "next <= 3 < table.len discharges after both ordinary requires"
    );
    let indexed = under
        .obligations
        .iter()
        .position(|outcome| outcome.family == ObligationFamily::Bounds)
        .expect("the endpoint indexes the table once");
    assert!(
        under
            .derivations
            .nodes
            .iter()
            .any(|node| matches!(node, DerivationNode::PostconditionCall { .. })),
        "the endpoint bound comes from the ordinary supplied ensures"
    );
    assert!(under.obligations[indexed].discharged);
    assert_eq!(
        discharge_flags(source, "exact"),
        vec![false],
        "next <= 4 admits next = table.len"
    );
}

#[test]
fn a_transfer_endpoint_bound_enters_the_observing_arm_only() {
    // `Ok(value: next)` observes the endpoint bound; the error arm's own u64
    // payload is an unrelated required size and gains nothing [ENT-3.S12].
    let source = br#"const table: Array<u8, 4> =[0_u8, 0_u8, 0_u8, 0_u8];

fn main(text: &std::text::HostString, destination: &[u8]) -> result: unit reads(text), writes(destination) contract {
  requires 3_u64 <= deref(destination).len;
} {
  match std::text::host_copy_bytes(value: text, destination: destination, start: 0_u64, end: 3_u64) {
    Ok(value: copied) => {
      let good = table[copied];
    }
    Err(error: problem) => {
      match problem {
        CopyTooSmall(required: needed) => {
          let bad = table[needed];
        }
      }
    }
  }
  return unit;
}
"#;
    assert_eq!(
        obligations(source, "main")
            .into_iter()
            .filter(|outcome| outcome.family == ObligationFamily::Bounds)
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true, false],
        "only the success arm's binder carries the transfer endpoint bound"
    );
}

#[test]
fn a_host_copy_utf8_success_endpoint_is_bounded_by_end() {
    // The UTF-8 copy producer carries the same ordinary ensures success-endpoint bound as
    // the byte-preserving copy producer: copied <= 3 < table.len.
    let source = br#"const table: Array<u8, 4> =[0_u8, 0_u8, 0_u8, 0_u8];

fn main(text: &std::text::HostString, destination: &[u8]) -> result: unit reads(text), writes(destination) contract {
  requires 3_u64 <= deref(destination).len;
} {
  match std::text::host_copy_utf8(value: text, destination: destination, start: 0_u64, end: 3_u64) {
    Ok(value: copied) => {
      let good = table[copied];
    }
    Err(error: problem) => {
    }
  }
  return unit;
}
"#;
    assert_eq!(
        obligations(source, "main")
            .into_iter()
            .filter(|outcome| outcome.family == ObligationFamily::Bounds)
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "host_copy_utf8's Ok payload is at most the end actual"
    );
}

#[test]
fn a_named_boundary_outcome_uses_the_same_numeric_evidence_as_source_calls() {
    // PRE-1 calls and source calls retain the same conditional context.
    // A known endpoint proves the fixed-table bound; an unconstrained old
    // limit does not become a table bound after writing a new limit.
    let source = br#"const count: u64 = 4_u64;

const table: Array<u8, count> =[0_u8, 0_u8, 0_u8, 0_u8];

fn deferred(factory: &std::io::HandleFactory, output: &std::io::OutputStream, source: &[u8], limit: u64) -> result: unit reads(source), writes(factory), writes(output) contract {
  define capacity = deref(source).len;
  requires 3_u64 <= capacity;
} {
  let outcome = std::io::write_once(factory: factory, output: output, source: source, start: 0_u64, end: 3_u64);
  match outcome {
    Ok(value: written) => {
      let sample = table[written];
    }
    Err(error: problem) => {
    }
  }
  return unit;
}

fn killed(factory: &std::io::HandleFactory, output: &std::io::OutputStream, source: &[u8], limit: u64) -> result: unit reads(source), writes(factory), writes(output) contract {
  define capacity = deref(source).len;
  requires limit <= capacity;
} {
  let outcome = std::io::write_once(factory: factory, output: output, source: source, start: 0_u64, end: limit);
  set limit = 9_u64;
  match outcome {
    Ok(value: written) => {
      let sample = table[written];
    }
    Err(error: problem) => {
    }
  }
  return unit;
}
"#;
    assert_eq!(
        obligations(source, "deferred")
            .into_iter()
            .filter(|outcome| outcome.family == ObligationFamily::Bounds)
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "the named outcome retains its proved endpoint"
    );
    assert_eq!(
        obligations(source, "killed")
            .into_iter()
            .filter(|outcome| outcome.family == ObligationFamily::Bounds)
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![false],
        "a new limit cannot strengthen the old call datum"
    );
}

#[test]
fn a_read_at_endpoint_is_observed_on_its_own_outcome_variant() {
    // PRE-1 read_at uses Result and an ordinary selected ensures.
    let source = br#"const table: Array<u8, 4> =[0_u8, 0_u8, 0_u8, 0_u8];

fn main(factory: &std::io::HandleFactory, file: &std::fs::ReadFile, destination: &[u8]) -> result: unit writes(factory), writes(file), writes(destination) contract {
  requires 3_u64 <= deref(destination).len;
} {
  match std::fs::read_at(factory: factory, file: file, destination: destination, file_offset: 0_u64, start: 0_u64, end: 3_u64) {
    Ok(value: next) => {
      let sample = table[next];
    }
    Err(error: problem) => {
    }
  }
  return unit;
}
"#;
    assert_eq!(
        obligations(source, "main")
            .into_iter()
            .filter(|outcome| outcome.family == ObligationFamily::Bounds)
            .map(|outcome| outcome.discharged)
            .collect::<Vec<_>>(),
        vec![true],
        "the Ok endpoint is at most the end actual"
    );
}

// ---------------------------------------------------------------------
// [ENT-3] originating call and loop facts
// ---------------------------------------------------------------------

#[test]
fn a_zero_argument_call_goal_is_discharged_in_its_originating_context() {
    let source = br#"fn need() -> result: unit pure contract {
  define first = 0_u64 < 1_u64;
  define second = 1_u64 < 2_u64;
  define complete = band(first, second);
  requires complete;
} {
  return unit;
}

fn caller() -> result: unit pure {
  need();
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("zero-argument call must be accepted: {outcome:?}");
        };
        let caller = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "caller")
            .expect("caller function");
        assert_eq!(caller.entailment.call_goals.len(), 1);
        assert_eq!(caller.entailment.call_goals[0].argument_count, 0);
        assert_eq!(
            caller.entailment.call_goals[0].disposition,
            CallGoalDisposition::Discharged
        );
    });
}

#[test]
fn a_counted_loop_body_bound_reaches_only_its_dominated_obligation() {
    let source = br#"const values: Array<i32, 4> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read(leave: Bool) -> result: i32 pure {
  for (index in 0_u64..4_u64) {
    let value = values[index];
    if leave {
      return value;
    }
  }
  return values[0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the counted-loop fact must discharge its body read: {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "read")
            .expect("read function");
        validate_derivations(&function.entailment);
        assert_eq!(function.entailment.obligations.len(), 2);
        assert!(function.entailment.obligations[0].discharged);
        assert!(function.entailment.obligations[1].discharged);
    });
}

// ---------------------------------------------------------------------
// [OP-4] discharge-or-reject through the ordinary acceptance path
// ---------------------------------------------------------------------

#[test]
fn an_undischarged_subscript_is_an_op4_rejection_with_the_exact_residual() {
    let source = br#"fn read(values: Slots<i32, 4>, i: u64) -> result: i32 pure {
  return values[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    // Both terms are parameters no event of `read` writes, so the repair
    // offers the requirement that holds at entry and reaches the access
    // [DIAG-1]; its complete sentence is pinned in `driver::pinned_repairs`.
    assert_rule_kind(source, SemanticRule::Op4, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedBoundsObligation {
                residual,
                disposition: StaticObligationDisposition::Unproved,
                mechanical_fix,
            } if residual == "i < values.len"
                && mechanical_fix.starts_with(
                    "add `requires i < values.len;` to the `contract` of `read`"
                )
        )
    });
}

#[test]
fn a_closed_opposite_bound_marks_the_subscript_obligation_refuted() {
    let source = br#"const values: Array<i32, 2> =[0_i32, 0_i32];

fn read() -> result: i32 pure {
  return values[2_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "read");
    let [bounds] = summary.obligations.as_slice() else {
        panic!("the one subscript must retain exactly one OP-4 obligation");
    };
    assert_eq!(bounds.family, ObligationFamily::Bounds);
    assert!(!bounds.discharged);
    assert!(
        bounds.refuted,
        "the implicit values.len = 2 relation proves the negation of 2 < values.len"
    );
}

#[test]
fn a_discharged_program_accepts_and_retains_its_derivations() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read() -> result: i32 pure {
  return values[2_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("a discharged subscript must accept, got {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "read")
            .expect("read must exist");
        assert_eq!(function.entailment.obligations.len(), 1);
        assert!(function.entailment.obligations[0].discharged);
        validate_derivations(&function.entailment);
    });
}

#[test]
fn counted_flow_publishes_one_exact_originating_outcome_shape() {
    let source = br#"const count: u64 = 4_u64;

const values: Array<i32, count> =[0_i32, 0_i32, 0_i32, 0_i32];

fn read() -> result: i32 pure {
  let total = 0_i32;
  for @items (i in 0_u64..4_u64) {
    let value = values[i];
    set total = total +wrap value;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the counted-flow fixture must accept: {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "read")
            .expect("read must exist");
        validate_derivations(&function.entailment);
        assert_eq!(function.entailment.counted_derivations.len(), 1);
        assert_eq!(
            function
                .entailment
                .derivations
                .roots
                .iter()
                .filter(|root| matches!(root.kind, DerivationRootKind::CountedS11 { .. }))
                .count(),
            8
        );
        let [outcome] = function.entailment.obligations.as_slice() else {
            panic!("one counted subscript obligation must be retained");
        };
        assert!(!outcome.node_path.components().is_empty());
        assert!(outcome.discharged);
        assert_eq!(outcome.residual, None);
        assert!(function.entailment.call_goals.is_empty());
    });
}

#[test]
fn counted_sha256_discharges_all_nine_indices_from_counted_facts() {
    let source = include_bytes!("../../../../tests/programs/sha256_abc.wf");
    let summary = entailment(source, "sha256_abc_word_zero");
    validate_derivations(&summary);
    assert_eq!(summary.obligations.len(), 9);
    assert!(summary.obligations.iter().all(|outcome| outcome.discharged));
    assert_eq!(summary.counted_derivations.len(), 3);
    assert_eq!(
        summary.counted_derivations.len() * 5,
        15,
        "the three SHA-256 ranges retain all five semantic S11 relations"
    );
    assert_eq!(
        summary
            .derivations
            .roots
            .iter()
            .filter(|root| matches!(root.kind, DerivationRootKind::CountedS11 { .. }))
            .count(),
        24,
        "the three SHA-256 ranges retain all eight directed atomic roots"
    );
    assert_eq!(
        summary
            .derivations
            .roots
            .iter()
            .filter(|root| matches!(root.kind, DerivationRootKind::BoundsObligation(_)))
            .count(),
        9,
        "all nine existing accepted bounds obligations retain exact roots"
    );
}

#[test]
fn real_sources_retain_complete_proof_roots_without_counted_false_positives() {
    // Each bundle is its sources' logical names and bytes, so a check below
    // can select a bundle by the source it compiles rather than by position.
    let bundles: [&[(&str, &[u8])]; 3] = [
        &[(
            "utf8parse.wf",
            include_bytes!("../../../../tests/programs/utf8parse.wf"),
        )],
        &[
            (
                "raw_deflate.wf",
                include_bytes!("../../../../tests/programs/raw_deflate.wf"),
            ),
            (
                "raw_deflate_dynamic.wf",
                include_bytes!("../../../../tests/programs/raw_deflate_dynamic.wf"),
            ),
            (
                "raw_deflate_dynamic_decode.wf",
                include_bytes!("../../../../tests/programs/raw_deflate_dynamic_decode.wf"),
            ),
            (
                "raw_deflate_boundary.wf",
                include_bytes!("../../../../tests/programs/raw_deflate_boundary.wf"),
            ),
        ],
        &[(
            "wfgrep.wf",
            include_bytes!("../../../../tests/programs/wfgrep.wf"),
        )],
    ];
    // Bundle 0 is `utf8parse.wf`, bundle 1 the raw DEFLATE chain, bundle 2
    // `wfgrep.wf`; the expectation is keyed on the bundle because B7c4b-1 gave
    // exactly one of the three mains a counted loop of its own — utf8parse's
    // output run is taken from a bump extent and filled by a counted `for`
    // where it was a `buffer_new` with an initial value.
    let mut raw_deflate_routes_checked = 0;
    let mut wfgrep_routes_checked = 0;
    for (bundle, sources) in bundles.into_iter().enumerate() {
        let inputs = sources
            .iter()
            .map(|(name, bytes)| SourceInput::new(name, bytes))
            .collect::<Vec<_>>();
        let compiles_raw_deflate = sources.iter().any(|(name, _)| *name == "raw_deflate.wf");
        let compiles_wfgrep = sources.iter().any(|(name, _)| *name == "wfgrep.wf");
        super::with_semantics_inputs(&inputs, |outcome| {
            let SemanticOutcome::Complete(program) = outcome else {
                panic!("real source bundle must remain accepted: {outcome:?}");
            };
            for function in &program.data.functions {
                validate_derivations(&function.entailment);
                let expected_counted = match (bundle, function.name.as_str()) {
                    (_, "parse") => 1,
                    (0, "main") => 1,
                    (_, "append_slice") => 1,
                    (_, "scan_line") => 1,
                    (_, "shift_input_tail") => 1,
                    // B7c4b's migration of the raw DEFLATE chain: the Huffman
                    // table reserves its three inline runs by counted loops,
                    // `decode_dynamic` its three length runs, and the boundary
                    // driver's `exercise` its four extent-resident runs.
                    // v0.58's ordinary Inputs wrapper keeps main separate
                    // from that unchanged four-loop operation chain.
                    (1, "build_huffman_table") => 5,
                    (1, "decode_dynamic") => 3,
                    // RFC 1951's fixed distance symbol is reconstructed from
                    // five wire bits by one counted bit-reversal loop.
                    (1, "decode_fixed") => 1,
                    (1, "exercise") => 4,
                    (1, "main") => 0,
                    // `wfgrep.wf`'s fill helper, which carries the zero fill
                    // its runs took from `buffer_new` before B7c4b. Its word
                    // helper retired when the walk's fixed visit order became
                    // a growable offset store.
                    (2, "zeroed_bytes") => 1,
                    _ => 0,
                };
                assert_eq!(
                    function.entailment.counted_derivations.len(),
                    expected_counted,
                    "the source keeps its exact counted induction groups in {}",
                    function.name,
                );
            }
            // Both route checks are selected by the source the bundle
            // compiles rather than by the presence of a function name: a name
            // probe skipped wfgrep's assertions without failing when
            // `report_failure` was renamed `assemble_failure`. A missing
            // anchor function fails inside each check, a reordered bundle
            // list cannot re-target either, and the counts below require
            // each to run exactly once.
            if compiles_raw_deflate {
                assert_real_read_bits_routes(&program.data);
                assert_real_raw_append_routes(&program.data);
                raw_deflate_routes_checked += 1;
            }
            if compiles_wfgrep {
                assert_real_wfgrep_routes(&program.data);
                wfgrep_routes_checked += 1;
            }
        });
    }
    assert_eq!(
        raw_deflate_routes_checked, 1,
        "exactly one bundle compiles raw_deflate.wf and runs its route assertions"
    );
    assert_eq!(
        wfgrep_routes_checked, 1,
        "exactly one bundle compiles wfgrep.wf and runs its route assertions"
    );
}

fn assert_real_read_bits_routes(program: &CheckedProgramData) {
    #[derive(Clone, Copy)]
    enum MaskActual {
        Literal(u64),
        Binding(BindingId),
    }

    struct ReadCall {
        caller: usize,
        path: NodePath,
        mask: MaskActual,
    }

    let read_bits = program
        .functions
        .iter()
        .find(|function| function.name == "read_bits")
        .expect("raw DEFLATE's bit reader, read_bits, anchors these routes")
        .id;
    let mut calls = Vec::new();
    for (caller, function) in program.functions.iter().enumerate() {
        let mut found = Vec::new();
        // An ordinary fn_sig has no source statements or source call sites.
        // Its contract derivations are checked by the enclosing traversal.
        collect_direct_calls(
            function.body.as_deref().unwrap_or(&[]),
            read_bits,
            &mut found,
        );
        for (path, arguments) in found {
            assert_eq!(arguments.len(), 4);
            let mask = match &arguments[3] {
                CheckedExpression::Constant(CheckedValue::Integer {
                    ty: IntegerType::U64,
                    bits,
                }) => MaskActual::Literal(*bits),
                CheckedExpression::Binding {
                    binding,
                    ty: super::super::model::CheckedType::Integer(IntegerType::U64),
                    consume_root: false,
                    ..
                } => MaskActual::Binding(*binding),
                actual => panic!("read_bits mask must be an exact u64 atom: {actual:?}"),
            };
            calls.push(ReadCall {
                caller,
                path: path.clone(),
                mask,
            });
        }
    }
    calls.sort_by(|left, right| left.path.components().cmp(right.path.components()));
    let expected_masks = [
        Some(1),
        Some(31),
        Some(1),
        Some(3),
        None,
        None,
        Some(1),
        Some(7),
        Some(31),
        Some(31),
        Some(15),
        Some(3),
        Some(7),
        Some(127),
    ];
    assert_eq!(calls.len(), expected_masks.len());

    for (ordinal, (call, expected)) in calls.iter().zip(expected_masks).enumerate() {
        match (call.mask, expected) {
            (MaskActual::Literal(actual), Some(expected)) => {
                assert_eq!(actual, expected, "read_bits row {ordinal}");
            }
            (MaskActual::Binding(_), None) => {}
            _ => panic!("read_bits row {ordinal} has the wrong mask class"),
        }

        let summary = &program.functions[call.caller].entailment;
        let direct = summary
            .derivations
            .roots
            .iter()
            .filter_map(|root| {
                if !matches!(
                    root.kind,
                    DerivationRootKind::PostconditionConditional { .. }
                ) {
                    return None;
                }
                let DerivationNode::PostconditionCall { detail } =
                    &summary.derivations.nodes[root.node.0 as usize]
                else {
                    return None;
                };
                (detail.call == call.path).then_some(&detail.relation)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            direct.len(),
            1,
            "read_bits row {ordinal} conditional call clause"
        );
        for relation in direct {
            assert!(
                relation.terms().into_iter().any(|term| match call.mask {
                    MaskActual::Literal(mask) => {
                        retained_term(summary, term) == &TermKind::Constant(i128::from(mask))
                    }
                    // [MSR-3] an `own` operand read at a caller denotes that
                    // call's call datum, which the same call established
                    // equal to the actual's pre-transfer term. A binding
                    // actual is mutable, so the relation names the datum and
                    // the actual's own place term is retained beside it; a
                    // literal actual is already immutable with empty support
                    // and the datum is that constant itself.
                    MaskActual::Binding(binding) => {
                        matches!(
                            retained_term(summary, term),
                            TermKind::CallDatum { measure: None, .. }
                        ) && summary.inventory.terms.iter().any(|kind| {
                            matches!(
                                kind,
                                TermKind::Place(place, IntegerType::U64)
                                    if place.root == PlaceRoot::Binding(binding)
                                        && place.path.is_empty()
                            )
                        })
                    }
                }),
                "read_bits row {ordinal} relation must retain its exact mask actual"
            );
        }
        assert!(summary.derivations.nodes.iter().all(|node| {
            let DerivationNode::PostconditionDirectReceiver { parent, .. } = node else {
                return true;
            };
            !root_contains(summary, *parent, |ancestor| {
                matches!(
                    ancestor,
                    DerivationNode::PostconditionCall { detail }
                        if detail.call == call.path
                )
            })
        }));
    }

    // PRE-2 supplies ordinary signatures: write_once and read_at each publish
    // their two endpoint clauses through the same conditional call route.
    // The fourteen read_bits calls retain their original clause inventory.
    for (caller_name, callee_name) in [("publish_all", "write_once"), ("exercise", "read_at")] {
        let caller = program
            .functions
            .iter()
            .find(|function| function.name == caller_name)
            .expect("boundary caller");
        let callee = program
            .functions
            .iter()
            .find(|function| function.name == callee_name)
            .expect("ordinary endpoint signature");
        let mut sites = Vec::new();
        collect_direct_calls(
            caller.body.as_deref().expect("WF body"),
            callee.id,
            &mut sites,
        );
        let [(path, _)] = sites.as_slice() else {
            panic!("{caller_name} has exactly one {callee_name} call");
        };
        assert_eq!(
            caller
                .entailment
                .derivations
                .roots
                .iter()
                .filter(|root| matches!(
                    root.kind,
                    DerivationRootKind::PostconditionConditional { .. }
                ))
                .filter(|root| matches!(
                    &caller.entailment.derivations.nodes[root.node.0 as usize],
                    DerivationNode::PostconditionCall { detail } if detail.call == **path
                ))
                .count(),
            2,
            "{callee_name} retains both endpoint clauses at its source call"
        );
    }

    assert_eq!(
        program
            .functions
            .iter()
            .flat_map(|function| &function.entailment.derivations.roots)
            .filter(|root| matches!(
                root.kind,
                DerivationRootKind::PostconditionConditional { .. }
            ))
            .count(),
        18
    );
    // Result transport retires required roots for a direct match's first assignment.
    // An unused assignment no longer forces a special retained proof. Source
    // call identities and exact masks remain checked above; focused tests
    // check ordinary assignments by consuming their facts in obligations.
    // Check every source call, rather than retaining totals from the retired
    // owned-view API. PRE-1 gives slots_new two result clauses (len and cap),
    // and place_back one exit-state clause. The two source helpers each
    // declare result <= capacity. Reference formation has no call contract.
    // The site counts below come from the maintained WF sources; the clause
    // counts come from those declarations, independently of the proof DAG.
    let mut expected_results = 0;
    let mut expected_states = 0;
    for (caller_name, callee_name, sites_expected, clauses, is_result) in [
        ("build_huffman_table", "slots_new", 3, 2, true),
        ("decode_dynamic", "slots_new", 3, 2, true),
        ("assemble_reason", "append_slice", 8, 1, true),
        ("exercise", "slots_new", 4, 2, true),
        ("exercise", "assemble_reason", 7, 1, true),
        ("build_huffman_table", "place_back", 3, 1, false),
        ("decode_dynamic", "place_back", 3, 1, false),
        ("exercise", "place_back", 4, 1, false),
    ] {
        let caller = program
            .functions
            .iter()
            .find(|function| function.name == caller_name)
            .expect("source caller");
        let mut sites = Vec::new();
        // Type/const arguments select distinct ordinary function instances.
        // Count source sites across every instance of the named declaration.
        for callee in program
            .functions
            .iter()
            .filter(|function| function.name == callee_name)
        {
            collect_direct_calls(
                caller.body.as_deref().expect("WF body"),
                callee.id,
                &mut sites,
            );
        }
        assert_eq!(sites.len(), sites_expected, "{caller_name}: {callee_name}");
        let derivations = &caller.entailment.derivations;
        for (path, _) in sites {
            let publications = derivations
                .roots
                .iter()
                .filter_map(|root| {
                    let call_node = match (is_result, root.kind) {
                        (true, DerivationRootKind::PostconditionDirectResult { .. }) => {
                            let DerivationNode::PostconditionDirectResult { parent, .. } =
                                &derivations.nodes[root.node.0 as usize]
                            else {
                                panic!("direct-result root must retain its publication route");
                            };
                            &derivations.nodes[parent.0 as usize]
                        }
                        (false, DerivationRootKind::PostconditionState { .. }) => {
                            &derivations.nodes[root.node.0 as usize]
                        }
                        _ => return None,
                    };
                    let DerivationNode::PostconditionCall { detail } = call_node else {
                        panic!("publication must name its source call");
                    };
                    (detail.call == *path).then_some(())
                })
                .count();
            assert_eq!(
                publications, clauses,
                "{caller_name}: every {callee_name} call publishes each declared clause"
            );
        }
        if is_result {
            expected_results += sites_expected * clauses;
        } else {
            expected_states += sites_expected * clauses;
        }
    }
    for (is_result, expected) in [(true, expected_results), (false, expected_states)] {
        let actual = program
            .functions
            .iter()
            .flat_map(|function| &function.entailment.derivations.roots)
            .filter(|root| {
                matches!(
                    (is_result, root.kind),
                    (true, DerivationRootKind::PostconditionDirectResult { .. })
                        | (false, DerivationRootKind::PostconditionState { .. })
                )
            })
            .count();
        assert_eq!(
            actual, expected,
            "no publication escapes the source inventory"
        );
    }
    assert!(program.functions.iter().all(|function| {
        function.entailment.derivations.roots.iter().all(|root| {
            !matches!(
                root.kind,
                DerivationRootKind::PostconditionGive { .. }
                    | DerivationRootKind::PostconditionDeliveryJoin { .. }
            )
        })
    }));
}

fn assert_real_raw_append_routes(program: &CheckedProgramData) {
    // The eight routes are `assemble_reason`'s eight direct-receiver
    // `set length = append_slice(filled: length, ...)` sites in
    // `raw_deflate_boundary.wf`, the only such form in the chain.
    let assemble = program
        .functions
        .iter()
        .find(|function| function.name == "assemble_reason")
        .expect("raw DEFLATE's diagnostic assembly, assemble_reason, anchors these routes");
    assert_eq!(
        assemble
            .entailment
            .derivations
            .roots
            .iter()
            .filter(|root| matches!(
                root.kind,
                DerivationRootKind::PostconditionDirectReceiver { .. }
            ))
            .count(),
        8,
        "every receiver route is one of assemble_reason's append sites",
    );
    assert_eq!(
        program
            .functions
            .iter()
            .flat_map(|function| &function.entailment.derivations.roots)
            .filter(|root| matches!(
                root.kind,
                DerivationRootKind::PostconditionDirectReceiver { .. }
            ))
            .count(),
        8,
    );
}

/// The `append_slice` receiver routes and the A10-A16 delivery chain the
/// searching `wfgrep` retains.
///
/// One route per source `append_slice` call site, and the count is derived
/// from source, never from the module: every site has the direct-receiver
/// form `set x = append_slice(filled: x, ...)`. `assemble_failure` appends the
/// prefix and then, after the A10 clamp, the separator and ten reasons —
/// twelve sites. `exercise` has the three startup-message sites plus the
/// defensive root-length route introduced when `append_slice` gained its
/// static input contract — four more sites.
fn assert_real_wfgrep_routes(program: &CheckedProgramData) {
    let proof_nodes: u64 = program
        .functions
        .iter()
        .map(|function| u64::from(function.entailment.derivations.metrics.unique_nodes))
        .sum();
    let proof_edges: u64 = program
        .functions
        .iter()
        .map(|function| u64::from(function.entailment.derivations.metrics.parent_edges))
        .sum();
    // A wall-clock assertion would vary with the host and with the Rust
    // profile.  These two deterministic sizes guard the cost shape instead,
    // with a small margin for ordinary proof evolution.  A return to the
    // former million-node shape fails here on every machine.  The history
    // below records how the retained size moved; the last paragraph states
    // the current measurement and ceilings.
    //
    // When this check was written the source retained 27,460 nodes and
    // 53,423 parent edges.
    //
    // The counts rose from 23,394 and 45,669 when a `set` to a fragment place
    // gained the commit-value image [ENT-3.S5]: each such statement now
    // carries one more term through its pre-kill closure, exactly as the
    // equivalent `let` of the same right-hand side already did.  The shape is
    // unchanged.
    //
    // They rose again at v0.44, to 36,760 and 71,153, when [ENT-3.S13] gave
    // each `own` operand of a published relation its call datum: one equality
    // per such operand at the call, and one more term through that call's
    // pre-kill closure. The rise is linear in calls to functions that publish
    // a relation over an `own` operand, and an operand whose pre-transfer term
    // is already immutable mints nothing [MSR-3], which is why a literal
    // actual costs nothing at all. The ceilings keep the same proportional
    // margin they had, and a return to the former million-node shape still
    // fails here on every machine.
    //
    // They rose again at v0.45, to 46,998 and 92,148, when [MSR-1] gave every
    // measured place four measure terms instead of one length term. Each
    // measure carries the [ENT-2] implicit type bounds and the [MSR-2]
    // standing fact its table cell fixes, so the rise is linear in measured
    // places and is the cost of the facts themselves rather than of a wider
    // search: no derivation family and no candidate set grew. The ceilings
    // keep the same proportional margin, and the million-node shape still
    // fails here.
    //
    // These assertions then stopped running: they were selected by the name
    // `report_failure`, which b7bccc3f5 renamed `assemble_failure`, and no
    // later change re-measured them. Re-measured at 803f37668 (main before the
    // wfgrep directory-walk fix), wfgrep retained 3,796 nodes and 6,392 edges,
    // about a twelfth of the v0.45 figures; the proof-engine changes in
    // between were not attributed one by one, but the 52,000 and 100,000
    // ceilings sat more than twelve times above the measurement and would
    // have missed any regression short of that.
    // The directory-walk fix raised the program to 4,040 nodes and 6,692
    // edges: `assemble_failure` +114/+144 for its four new reason appends,
    // `walk` +99/+141 for entry collection and the output-refusal paths, the
    // new helpers `search_root`, `publish_oversized`, `widen_window`,
    // `push_byte`, `push_word` and `keep_entry` +72/+61, the two `grow`
    // instances +4/+0, while the retired `zeroed_words` -39/-32, `exercise`
    // -6/-13 and `search_file` -0/-1. `append_slice` is unchanged at
    // 2,600/5,124 and is still most of the total. The ceilings are tightened
    // to 4,500 and 7,400, about the ten-percent margin the earlier ceilings
    // kept above their measurement.
    assert!(
        proof_nodes <= 4_500,
        "wfgrep retained {proof_nodes} proof nodes; expected at most 4,500"
    );
    assert!(
        proof_edges <= 7_400,
        "wfgrep retained {proof_edges} proof edges; expected at most 7,400"
    );
    let shift = program
        .functions
        .iter()
        .find(|function| function.name == "shift_input_tail")
        .expect("shift_input_tail function");
    let [invariant] = shift.entailment.loop_invariants.as_slice() else {
        panic!("shift_input_tail keeps its one source-written induction relation");
    };
    assert_eq!(invariant.name, "destination_precedes_source");
    assert!(
        invariant.proof.base,
        "the originating context proves the invariant before iteration zero"
    );
    assert_eq!(
        invariant.proof.step,
        Some(true),
        "the originating context proves the invariant from an arbitrary loop header to its next backedge"
    );
    assert_eq!(
        shift.entailment.obligations.len(),
        3,
        "the invariant-backed copy retains its source read, destination write, and exact increment obligations"
    );
    assert!(
        shift
            .entailment
            .obligations
            .iter()
            .all(|outcome| outcome.discharged)
    );
    assert_eq!(
        program
            .functions
            .iter()
            .flat_map(|function| &function.entailment.derivations.roots)
            .filter(|root| matches!(
                root.kind,
                DerivationRootKind::PostconditionDirectReceiver { .. }
            ))
            .count(),
        16,
        "wfgrep has exactly sixteen append_slice receiver routes",
    );

    let report = program
        .functions
        .iter()
        .find(|function| function.name == "assemble_failure")
        .expect("wfgrep's diagnostic assembly, assemble_failure, anchors these routes");
    let mut delivery_receivers = report
        .entailment
        .derivations
        .nodes
        .iter()
        .filter_map(|node| match node {
            DerivationNode::PostconditionDeliveryJoin { detail } => Some(detail.receiver),
            _ => None,
        })
        .collect::<Vec<_>>();
    // A10 survives the searching rewrite because the diagnostic still wants
    // the shape: `assemble_failure` assembles `wfgrep: PATH: reason` in one
    // reusable range and hands the assembled length back for its caller to
    // publish with one host write, so it has to clamp that length against the
    // range's capacity, and a `value_if` over the fits test is how that clamp
    // is written. Publishing the three pieces separately would drop the
    // clamp — and the delivery route with it — at the price of three host
    // writes for one diagnostic and no guarantee that the pieces stay
    // adjacent in a shared sink.
    assert!(
        !delivery_receivers.is_empty(),
        "A10 must deliver one receiver"
    );
    let bounded_length = delivery_receivers[0];
    assert!(
        delivery_receivers
            .drain(..)
            .all(|receiver| receiver == bounded_length),
        "A10 is the only value_if delivery in assemble_failure",
    );
    let mut bounded_routes = report
        .entailment
        .derivations
        .nodes
        .iter()
        .filter_map(|node| {
            let DerivationNode::PostconditionDirectReceiver {
                statement,
                binding,
                parent,
                ..
            } = node
            else {
                return None;
            };
            (*binding == bounded_length).then_some((statement.clone(), *parent))
        })
        .collect::<Vec<_>>();
    bounded_routes.sort_by(|left, right| left.0.components().cmp(right.0.components()));
    assert_eq!(
        bounded_routes.len(),
        11,
        "the separator and ten following reason appends use bounded_length",
    );
    for (_, parent) in &bounded_routes {
        assert!(
            root_contains(&report.entailment, *parent, |node| matches!(
                node,
                DerivationNode::PostconditionDeliveryJoin { detail }
                    if detail.receiver == bounded_length
            )),
            "each receiver chain after the clamp must descend from A10",
        );
    }

    // The clamped length is what the caller publishes: `assemble_failure`
    // makes no host write of its own, and its one return hands back the
    // binding A10 delivered, which its `result <= capacity` postcondition
    // then carries to each caller's `publish_all` requirement.
    let publish = program
        .functions
        .iter()
        .find(|function| function.name == "publish_all")
        .expect("publish_all function")
        .id;
    let body = report.body.as_deref().expect("WF body");
    let mut publish_calls = Vec::new();
    collect_direct_calls(body, publish, &mut publish_calls);
    assert!(
        publish_calls.is_empty(),
        "assemble_failure hands its length back instead of publishing"
    );
    let returns = body
        .iter()
        .filter_map(|statement| match statement {
            CheckedStatement::Return { value, .. } => Some(value),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        matches!(
            returns.as_slice(),
            [CheckedExpression::Binding {
                binding,
                consume_root: false,
                ..
            }] if *binding == bounded_length
        ),
        "assemble_failure returns the clamped length once"
    );
}

#[test]
fn counted_range_reads_a_box_content_endpoint_as_an_s11_term() {
    // [TYPE-9] a `Box`'s content is its field `inner`, so the endpoint is
    // written `holder.value.inner`; the resolved path is still the field step
    // followed by the content step [REF-1].
    let source = br#"struct Holder {
  value: Box<u64>;
}

fn probe(holder: Holder) -> result: unit pure {
  for @items (i in holder.value.inner..1_u64) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "probe");
    validate_derivations(&summary);
    assert_eq!(summary.counted_derivations.len(), 1);
    let Relation::Equal { right, .. } = summary.counted_derivations[0]
        .lower_capture_eq_endpoint
        .relation
    else {
        panic!("the lower capture identity must be an equality");
    };
    let TermKind::Place(endpoint, IntegerType::U64) = retained_term(&summary, right) else {
        panic!("holder.value.inner must retain its exact resolved-path endpoint identity");
    };
    assert_eq!(endpoint.path, vec![PlaceStep::Field(0), PlaceStep::Deref]);
}

#[test]
fn counted_range_kills_a_borrowed_projected_endpoint_after_a_write() {
    let source = br#"struct Limit {
  upper: u64;
}

fn need(index: u64, upper: u64) -> result: unit pure contract {
  requires index < upper;
} {
  return unit;
}

fn probe(limit: Limit) -> result: unit pure {
  let holder = &limit;
  for @items (i in 0_u64..deref(holder).upper) {
    set deref(holder).upper = 0_u64;
    need(index: i, upper: deref(holder).upper);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("the written endpoint must be reread and refute the call: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn8);
        let SemanticIssueKind::UndischargedCallRequirement(detail) = issue.kind() else {
            panic!("expected FN-8 detail, got {:?}", issue.kind());
        };
        assert_eq!(detail.disposition, CallRequirementDisposition::Unproved);
    });
}

#[test]
fn counted_range_preserves_multiple_box_content_steps_in_one_endpoint_term() {
    let source = br#"fn probe(holder: Box<Box<u64>>) -> result: unit pure {
  for @items (i in holder.inner.inner..1_u64) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "probe");
    validate_derivations(&summary);
    assert_eq!(summary.counted_derivations.len(), 1);
    let Relation::Equal { right, .. } = summary.counted_derivations[0]
        .lower_capture_eq_endpoint
        .relation
    else {
        panic!("the lower capture identity must be an equality");
    };
    let TermKind::Place(endpoint, IntegerType::U64) = retained_term(&summary, right) else {
        panic!("the nested box endpoint must keep both content steps");
    };
    assert_eq!(endpoint.path, vec![PlaceStep::Deref, PlaceStep::Deref]);
}

#[test]
fn counted_range_keeps_nested_box_steps_without_a_reference_wrapper_step() {
    let source = br#"fn probe(holder: &Box<Box<u64>>) -> result: unit reads(holder) {
  for @items (i in deref(holder).inner.inner..1_u64) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "probe");
    let Relation::Equal { right, .. } = summary.counted_derivations[0]
        .lower_capture_eq_endpoint
        .relation
    else {
        panic!("the lower capture identity must be an equality");
    };
    let TermKind::Place(endpoint, IntegerType::U64) = retained_term(&summary, right) else {
        panic!("the referenced nested endpoint must keep both Box content steps");
    };
    assert_eq!(endpoint.path, vec![PlaceStep::Deref, PlaceStep::Deref]);
}

#[test]
fn counted_range_does_not_treat_a_read_only_box_content_read_as_a_consume() {
    let source = br#"fn probe(holder: Box<u64>) -> result: unit pure {
  for @items (i in holder.inner..1_u64) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "probe");
    let Relation::Equal { right, .. } = summary.counted_derivations[0]
        .lower_capture_eq_endpoint
        .relation
    else {
        panic!("the lower capture identity must be an equality");
    };
    let TermKind::Place(endpoint, IntegerType::U64) = retained_term(&summary, right) else {
        panic!("the read-only box endpoint must keep its content step");
    };
    assert_eq!(endpoint.path, vec![PlaceStep::Deref]);
}

#[test]
fn counted_range_does_not_duplicate_the_content_step_of_a_let_bound_owning_box() {
    let source = br#"fn probe() -> result: unit pure {
  let holder = box_new::<u64>(value: 0_u64);
  for @items (i in holder.inner..1_u64) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "probe");
    let Relation::Equal { right, .. } = summary.counted_derivations[0]
        .lower_capture_eq_endpoint
        .relation
    else {
        panic!("the lower capture identity must be an equality");
    };
    let TermKind::Place(endpoint, IntegerType::U64) = retained_term(&summary, right) else {
        panic!("the owning box endpoint must keep its content step");
    };
    assert_eq!(endpoint.path, vec![PlaceStep::Deref]);
}

// ---------------------------------------------------------------------
// [ENT-2..ENT-5, FN-8] exact signed goals and ordinary calls
// ---------------------------------------------------------------------

#[test]
fn integer_domain_normalization_discharges_requires_and_ordinary_calls() {
    let source = br#"fn shift_once(value: u8) -> result: u8 pure contract {
  requires ishl.defined(value, 1_u32);
} {
  let shifted = ishl(value, 1_u32);
  return shifted;
}

fn caller(value: u8) -> result: u8 pure {
  return shift_once(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the fixed count must discharge requires and exact shift: {outcome:?}");
        };
        let shift = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "shift_once")
            .expect("shift_once function");
        assert!(
            shift
                .entailment
                .obligations
                .iter()
                .filter(|obligation| obligation.family == ObligationFamily::IntegerDomain)
                .all(|obligation| obligation.discharged),
        );
    });
    let summary = entailment(source, "caller");
    validate_derivations(&summary);
    assert_eq!(summary.call_goals.len(), 1);
    assert_eq!(
        summary.call_goals[0].disposition,
        CallGoalDisposition::Discharged,
    );
    assert_eq!(
        summary.call_goals[0].evidence,
        vec![CallGoalEvidence::NormalizationPositive],
    );
    let root = summary.call_goals[0]
        .derivation
        .expect("a discharged domain call retains its proof");
    assert!(root_contains(&summary, root, |node| matches!(
        node,
        DerivationNode::GoalNormalization {
            sign: GoalSign::Positive,
            ..
        }
    )));
}

#[test]
fn impossible_integer_domain_true_edge_closes_to_contradiction() {
    let source = br#"fn impossible_branch(value: u8) -> result: u8 pure {
  if ishl.defined(value, 8_u32) {
    let shifted = ishl(value, 8_u32);
    return shifted;
  } else {
    return value;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the impossible true edge must be contradictory: {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "impossible_branch")
            .expect("impossible_branch function");
        let obligations = function
            .entailment
            .obligations
            .iter()
            .filter(|obligation| obligation.family == ObligationFamily::IntegerDomain)
            .collect::<Vec<_>>();
        assert_eq!(obligations.len(), 1);
        assert!(obligations[0].discharged);
        assert!(obligations[0].contradictory);
        validate_derivations(&function.entailment);
        assert!(
            function
                .entailment
                .derivations
                .nodes
                .iter()
                .any(|node| { matches!(node, DerivationNode::GoalContradiction { .. }) })
        );
    });
}

#[test]
fn impossible_integer_domain_false_edge_closes_to_contradiction() {
    let source = br#"fn impossible_branch(value: u8) -> result: u8 pure {
  if ishl.defined(value, 1_u32) {
    return value;
  } else {
    let shifted = ishl(value, 1_u32);
    return shifted;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the impossible false edge must be contradictory: {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "impossible_branch")
            .expect("impossible_branch function");
        let obligations = function
            .entailment
            .obligations
            .iter()
            .filter(|obligation| obligation.family == ObligationFamily::IntegerDomain)
            .collect::<Vec<_>>();
        assert_eq!(obligations.len(), 1);
        assert!(obligations[0].discharged);
        assert!(obligations[0].contradictory);
        validate_derivations(&function.entailment);
        assert!(
            function
                .entailment
                .derivations
                .nodes
                .iter()
                .any(|node| { matches!(node, DerivationNode::GoalContradiction { .. }) })
        );
    });
}

#[test]
fn whole_goal_sources_and_boolean_introduction_discharge_exact_parents() {
    let source = br#"fn guarded(value: u64) -> result: unit pure contract {
  define positive = value > 0_u64;
  define small = value < 10_u64;
  define complete = band(positive, small);
  requires complete;
} {
  return unit;
}

fn from_branch(value: u64) -> result: unit pure {
  let positive = value > 0_u64;
  let small = value < 10_u64;
  let complete = band(positive, small);
  if complete {
    guarded(value: value);
  } else {
    return unit;
  }
  return unit;
}

fn from_requirement(value: u64) -> result: unit pure contract {
  define positive = value > 0_u64;
  define small = value < 10_u64;
  define complete = band(positive, small);
  requires complete;
} {
  guarded(value: value);
  return unit;
}

fn from_children(value: u64) -> result: unit pure {
  let positive = value > 0_u64;
  let small = value < 10_u64;
  if positive {
    if small {
      guarded(value: value);
    } else {
      return unit;
    }
  } else {
    return unit;
  }
  return unit;
}

fn from_false(value: u64) -> result: unit pure {
  let positive = value > 0_u64;
  let small = value < 10_u64;
  let complete = band(positive, small);
  if complete {
    return unit;
  } else {
    guarded(value: value);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

    for (function, evidence) in [
        ("from_branch", vec![CallGoalEvidence::OpaquePositive]),
        ("from_requirement", vec![CallGoalEvidence::OpaquePositive]),
        (
            "from_children",
            vec![CallGoalEvidence::BooleanIntroductionPositive],
        ),
    ] {
        let outcomes = call_goals(source, function);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].disposition, CallGoalDisposition::Discharged);
        assert_eq!(outcomes[0].evidence, evidence);
    }

    let negative = call_goals(source, "from_false");
    assert_eq!(negative[0].disposition, CallGoalDisposition::Refuted);
    assert_eq!(negative[0].evidence, vec![CallGoalEvidence::OpaqueNegative]);
}

#[test]
fn an_exact_comparison_call_retains_every_positive_derivation_ground() {
    let source = br#"fn below(value: u64) -> result: unit pure contract {
  requires value < 10_u64;
} {
  return unit;
}

fn exact(value: u64) -> result: unit pure {
  let small = value < 10_u64;
  if small {
    below(value: value);
  } else {
    return unit;
  }
  return unit;
}

fn projected(value: u64) -> result: unit pure {
  let at_most_nine = value <= 9_u64;
  if at_most_nine {
    below(value: value);
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let exact = entailment(source, "exact");
    validate_derivations(&exact);
    assert_eq!(
        exact.call_goals[0].disposition,
        CallGoalDisposition::Discharged
    );
    assert_eq!(
        exact.call_goals[0].evidence,
        vec![
            CallGoalEvidence::OpaquePositive,
            CallGoalEvidence::ExactL0Projection,
        ]
    );
    assert!(matches!(
        exact.derivations.nodes[call_root(&exact, 0).0 as usize],
        DerivationNode::SourceGoal { .. }
    ));
    let projected = entailment(source, "projected");
    validate_derivations(&projected);
    assert_eq!(
        projected.call_goals[0].disposition,
        CallGoalDisposition::Discharged
    );
    assert_eq!(
        projected.call_goals[0].evidence,
        vec![CallGoalEvidence::ExactL0Projection]
    );
    assert!(matches!(
        projected.derivations.nodes[call_root(&projected, 0).0 as usize],
        DerivationNode::GoalProjection { .. }
    ));
}

#[test]
fn joined_whole_goals_require_the_same_sign_on_every_reachable_input() {
    let source = br#"fn guarded(value: u64) -> result: unit pure contract {
  define positive = value > 0_u64;
  define small = value < 10_u64;
  define complete = band(positive, small);
  requires complete;
} {
  return unit;
}

fn both(value: u64, choose: Bool) -> result: unit pure {
  let positive = value > 0_u64;
  let small = value < 10_u64;
  let complete = band(positive, small);
  if choose {
    if complete {
    } else {
      return unit;
    }
  } else if complete {
  } else {
    return unit;
  }
  guarded(value: value);
  return unit;
}

fn one(value: u64, choose: Bool) -> result: unit pure {
  let positive = value > 0_u64;
  let small = value < 10_u64;
  let complete = band(positive, small);
  if choose {
    if complete {
    } else {
      return unit;
    }
  }
  guarded(value: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let both = entailment(source, "both");
    validate_derivations(&both);
    assert_eq!(
        both.call_goals[0].disposition,
        CallGoalDisposition::Discharged
    );
    assert_eq!(
        both.call_goals[0].evidence,
        vec![CallGoalEvidence::OpaquePositive]
    );
    assert!(matches!(
        both.derivations.nodes[call_root(&both, 0).0 as usize],
        DerivationNode::JoinGoal { ref parents, .. } if parents.len() == 2
    ));
    let one = entailment(source, "one");
    validate_derivations(&one);
    assert_eq!(one.call_goals[0].disposition, CallGoalDisposition::Unproved);
}

#[test]
fn a_computed_bool_truth_survives_an_origin_write_but_its_expansion_does_not() {
    let source = br#"fn need_true(value: Bool) -> result: unit pure contract {
  requires value;
} {
  return unit;
}

fn below(value: u64) -> result: unit pure contract {
  requires value < 10_u64;
} {
  return unit;
}

fn probe(value: u64) -> result: unit pure {
  let small = value < 10_u64;
  if small {
    set value = 20_u64;
    need_true(value: small);
    below(value: value);
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outcomes = call_goals(source, "probe");
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].disposition, CallGoalDisposition::Discharged);
    assert_eq!(outcomes[0].evidence, vec![CallGoalEvidence::OpaquePositive]);
    assert_eq!(
        outcomes[1].disposition,
        CallGoalDisposition::Refuted,
        "the set commit kills the old comparison origin, then S5 establishes value = 20"
    );
}

#[test]
fn a_copy_referent_read_through_an_affine_box_is_an_exact_goal_origin() {
    let source = br#"fn observe(value: &Box<i32>) -> result: unit reads(value) contract {
  define positive = deref(value).inner > 0_i32;
  define small = deref(value).inner < 10_i32;
  define complete = band(positive, small);
  requires complete;
} {
  let seen = deref(value).inner;
  return unit;
}

fn caller() -> result: unit pure {
  let owner = box_new::<i32>(value: 5_i32);
  let positive = owner.inner > 0_i32;
  let small = owner.inner < 10_i32;
  let complete = band(positive, small);
  if complete {
    observe(value: &owner);
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "caller");
    validate_derivations(&summary);
    assert_eq!(summary.call_goals.len(), 1);
    assert_eq!(
        summary.call_goals[0].disposition,
        CallGoalDisposition::Discharged
    );
    assert_eq!(
        summary.call_goals[0].evidence,
        vec![CallGoalEvidence::OpaquePositive]
    );
    assert_root_has_event_kind(&summary, call_root(&summary, 0), FlowEventKind::S1);
}

#[test]
fn setting_an_intermediate_bool_binding_stops_later_origin_expansion() {
    let source = br#"fn guarded(value: u64) -> result: unit pure contract {
  requires value > 0_u64;
} {
  return unit;
}

fn caller(value: u64) -> result: unit pure {
  let positive = value > 0_u64;
  let aliased = positive;
  set positive = False();
  if aliased {
    guarded(value: value);
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outcomes = call_goals(source, "caller");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].disposition, CallGoalDisposition::Unproved);
    assert!(outcomes[0].evidence.is_empty());
}

#[test]
fn resolved_writes_stop_future_expansion_of_the_written_origin_binding() {
    let source = br#"fn need(first: Bool, second: Bool) -> result: unit pure contract {
  define complete = band(first, second);
  requires complete;
} {
  return unit;
}

fn mutate(value: &Bool) -> result: unit writes(value) {
  set deref(value) = False();
  return unit;
}

fn through_holder(first: Bool, second: Bool) -> result: unit pure {
  let source = band(first, second);
  let holder = &source;
  set deref(holder) = False();
  let aliased = source;
  if aliased {
    need(first: first, second: second);
  } else {
    return unit;
  }
  return unit;
}

fn through_call(first: Bool, second: Bool) -> result: unit pure {
  let source = band(first, second);
  mutate(value: &source);
  let aliased = source;
  if aliased {
    need(first: first, second: second);
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for function in ["through_holder", "through_call"] {
        let outcomes = call_goals(source, function);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].disposition, CallGoalDisposition::Unproved);
        assert!(outcomes[0].evidence.is_empty());
    }
}

#[test]
fn combined_contradiction_survives_ordinary_set_kills_of_goal_and_l0_support() {
    let source = br#"fn guarded(value: u64) -> result: unit pure contract {
  define positive = value > 0_u64;
  define small = value < 10_u64;
  define complete = band(positive, small);
  requires complete;
} {
  return unit;
}

fn signed(value: u64) -> result: unit pure {
  let positive = value > 0_u64;
  let small = value < 10_u64;
  let complete = band(positive, small);
  if complete {
    if complete {
      return unit;
    } else {
      set value = value +wrap 1_u64;
      guarded(value: value);
    }
  } else {
    return unit;
  }
  return unit;
}

fn l0(value: u64) -> result: unit pure {
  if value < 5_u64 {
    if value >= 5_u64 {
      set value = value +wrap 1_u64;
      guarded(value: value);
    } else {
      return unit;
    }
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for (function, is_goal) in [("signed", true), ("l0", false)] {
        let summary = accepted_entailment(source, function);
        validate_derivations(&summary);
        assert_eq!(summary.call_goals.len(), 1);
        assert_eq!(
            summary.call_goals[0].disposition,
            CallGoalDisposition::Discharged
        );
        assert_eq!(
            summary.call_goals[0].evidence,
            vec![CallGoalEvidence::AllDerivable]
        );
        let root = &summary.derivations.nodes[call_root(&summary, 0).0 as usize];
        assert!(
            if is_goal {
                matches!(root, DerivationNode::GoalContradiction { .. })
            } else {
                matches!(root, DerivationNode::L0Contradiction { .. })
            },
            "{function} must retain its exact contradiction class: {root:#?}"
        );
    }
}

#[test]
fn contradiction_survives_effectful_prepared_call_writes_before_fn8() {
    let source = br#"fn rewrite(out: &i32) -> result: i32 writes(out) contract {
  ensures result == 0_i32;
} {
  set deref(out) = 0_i32;
  return 0_i32;
}

fn need_negative(value: i32) -> result: unit pure contract {
  requires value < 0_i32;
} {
  return unit;
}

fn caller(slot: i32) -> result: unit pure {
  if slot < 5_i32 {
    if slot >= 5_i32 {
      let rewritten = rewrite(out: &slot);
      need_negative(value: slot);
    } else {
      return unit;
    }
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "caller");
    validate_derivations(&summary);
    assert_eq!(summary.call_goals.len(), 1);
    assert_eq!(
        summary.call_goals[0].disposition,
        CallGoalDisposition::Discharged
    );
    assert_eq!(
        summary.call_goals[0].evidence,
        vec![CallGoalEvidence::AllDerivable],
        "the prepared call's writes row must not erase a contradiction established before its kill"
    );
}

#[test]
fn contradiction_survives_direct_receiver_set_target_kill_before_fn8() {
    let source = br#"fn choose(ignored: i32, value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn need_negative(value: i32) -> result: unit pure contract {
  requires value < 0_i32;
} {
  return unit;
}

fn caller(slot: i32, replacement: i32) -> result: unit pure {
  if slot < 5_i32 {
    if slot >= 5_i32 {
      set slot = choose(ignored: slot, value: replacement);
      need_negative(value: slot);
    } else {
      return unit;
    }
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = accepted_entailment(source, "caller");
    validate_derivations(&summary);
    assert_eq!(summary.call_goals.len(), 1);
    assert_eq!(
        summary.call_goals[0].disposition,
        CallGoalDisposition::Discharged
    );
    assert_eq!(
        summary.call_goals[0].evidence,
        vec![CallGoalEvidence::AllDerivable],
        "the direct receiver target event must preserve contradiction before killing the old receiver facts"
    );
}

#[test]
fn a_discharged_whole_goal_is_accepted_on_the_ordinary_path() {
    let source = br#"fn guarded(value: u64) -> result: unit pure contract {
  define positive = value > 0_u64;
  define small = value < 10_u64;
  define complete = band(positive, small);
  requires complete;
} {
  return unit;
}

fn caller(value: u64) -> result: unit pure {
  let positive = value > 0_u64;
  let small = value < 10_u64;
  let complete = band(positive, small);
  if complete {
    guarded(value: value);
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the established whole goal must accept its call: {outcome:?}");
        };
        let caller = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "caller")
            .expect("caller function");
        assert_eq!(caller.entailment.call_goals.len(), 1);
        assert_eq!(
            caller.entailment.call_goals[0].disposition,
            CallGoalDisposition::Discharged
        );
    });
}

#[test]
fn fn8_call_rejection_carries_the_complete_deterministic_payload() {
    let source = br#"fn guarded(value: u64) -> result: unit pure contract {
  define positive = value > 0_u64;
  define small = value < 10_u64;
  define complete = band(positive, small);
  requires complete;
} {
  return unit;
}

fn caller(value: u64) -> result: unit pure {
  guarded(value: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an unproved complete goal must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn8);
        let SemanticIssueKind::UndischargedCallRequirement(detail) = issue.kind() else {
            panic!(
                "FN-8 must carry the ordinary-call payload: {:?}",
                issue.kind()
            );
        };
        assert_eq!(detail.concrete_callee, "guarded");
        assert!(!detail.requires_clause.path().components().is_empty());
        // The alpha-expanded requirement in the terms the caller wrote, with
        // the caller's own binder in it. The structural dump this replaced
        // opened `Boolean(And)<types=[], consts=[]>(Integer { operation:
        // Greater, .. }(Place { root: BindingId(0), .. }))`, and four rounds of
        // readers could not find any of that in their own program. [OP-4] and
        // Ordinary call requirements also print their complete residual this way.
        assert_eq!(
            detail.instantiated_goal,
            "band(value > 0_u64, value < 10_u64)"
        );
        assert_eq!(detail.disposition, CallRequirementDisposition::Unproved);
        // `value` is a parameter of `caller` its body never writes, and the
        // goal is no single comparison a clause could copy, so the repair
        // asks for the relation as a requirement of `caller` [DIAG-1].
        assert!(
            detail
                .mechanical_fix
                .starts_with("state the relation over the parameters of `caller` as a `requires`"),
            "{}",
            detail.mechanical_fix
        );
        let crate::SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        assert_eq!(&source[start..end], b"guarded(value: value)");
    });
}

/// [FN-8, FN-2] the payload names the callee instance as a call writes it,
/// with its type and const arguments, never by the internal symbol that
/// keys its lowering.
#[test]
fn a_call_requirement_names_the_generic_instance_as_written() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/blk0-neg-full-array-freeze-requires-fullness.wf"
    );
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the partial freeze must reject at FN-8: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn8);
        let SemanticIssueKind::UndischargedCallRequirement(detail) = issue.kind() else {
            panic!("expected FN-8 payload, got {:?}", issue.kind());
        };
        assert_eq!(detail.concrete_callee, "slots_into_array::<u64, 2>");
    });
}

#[test]
fn actual_obligations_precede_fn8_and_admitted_index_goals_use_the_source_fix() {
    let admitted_actual = br#"const values: Array<u8, 2> =[3_u8, 3_u8];

fn positive(value: u8) -> result: unit pure contract {
  requires value < 10_u8;
} {
  return unit;
}

fn caller() -> result: unit pure {
  positive(value: values[0_u64]);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(admitted_actual, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the unproved indexed goal must reject at FN-8: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn8);
        let SemanticIssueKind::UndischargedCallRequirement(detail) = issue.kind() else {
            panic!("expected FN-8 payload, got {:?}", issue.kind());
        };
        assert_eq!(detail.disposition, CallRequirementDisposition::Unproved);
        assert_eq!(detail.instantiated_goal, "values[0_u64] < 10_u8");
        // The admitted element read is part of the goal's identity, so a
        // condition naming the same expression establishes it [ENT-3].
        assert!(
            detail
                .mechanical_fix
                .contains("guard the call with `if values[0_u64] < 10_u8`"),
            "{}",
            detail.mechanical_fix
        );
    });
    let admitted = entailment(admitted_actual, "caller");
    validate_derivations(&admitted);
    assert_eq!(admitted.obligations.len(), 1);
    assert!(admitted.obligations[0].discharged);
    let actual_root = obligation_root(&admitted, 0);
    // The written `0_u64` index is the zero term itself, so the concrete
    // actual's proof is its run-length bound against Z with no constant
    // fold in between.
    assert_root_contains(
        &admitted,
        actual_root,
        |node| {
            matches!(
                node,
                DerivationNode::ImplicitBound {
                    kind: ImplicitBoundKind::StandingMeasure,
                    ..
                }
            )
        },
        "the concrete run actual's implicit length bound",
    );
    assert_eq!(admitted.call_goals.len(), 1);
    assert_eq!(
        admitted.call_goals[0].disposition,
        CallGoalDisposition::Unproved
    );
    assert!(admitted.call_goals[0].derivation.is_none());

    let failed_actual = br#"const values: Array<u8, 2> =[3_u8, 3_u8];

fn positive(value: u8) -> result: unit pure contract {
  requires value < 10_u8;
} {
  return unit;
}

fn caller() -> result: unit pure {
  positive(value: values[9_u64]);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(failed_actual, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the actual's own OP-4 failure must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op4);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::UndischargedBoundsObligation { .. }
        ));
    });
    let failed = entailment(failed_actual, "caller");
    validate_derivations(&failed);
    assert_eq!(failed.obligations.len(), 1);
    assert!(!failed.obligations[0].discharged);
    assert!(failed.obligations[0].derivation.is_none());
    assert!(
        failed.call_goals.is_empty(),
        "FN-8 judgment begins only after every actual obligation succeeds"
    );
}

#[test]
fn a_call_is_judged_before_its_callee_write_and_that_write_kills_the_second_call() {
    let source = br#"fn update(value: &u64) -> result: unit writes(value) contract {
  requires deref(value) < 10_u64;
} {
  let old = deref(value);
  set deref(value) = old;
  return unit;
}

fn caller(value: u64) -> result: unit pure {
  let small = value < 10_u64;
  if small {
    update(value: &value);
    update(value: &value);
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "caller");
    validate_derivations(&summary);
    assert_eq!(summary.call_goals.len(), 2);
    assert_eq!(
        summary.call_goals[0].disposition,
        CallGoalDisposition::Discharged
    );
    assert_eq!(
        summary.call_goals[0].evidence,
        vec![
            CallGoalEvidence::OpaquePositive,
            CallGoalEvidence::ExactL0Projection,
        ]
    );
    assert_root_has_event_kind(&summary, call_root(&summary, 0), FlowEventKind::S1);
    assert_eq!(
        summary.call_goals[1].disposition,
        CallGoalDisposition::Unproved
    );
    assert!(summary.call_goals[1].derivation.is_none());
}

#[test]
fn writing_back_an_independent_copy_preserves_the_call_precondition() {
    let source = br#"fn observe(value: &u64) -> result: unit reads(value) contract {
  requires deref(value) < 10_u64;
} {
  let seen = deref(value);
  return unit;
}

fn update(value: &u64) -> result: unit writes(value) contract {
  requires deref(value) < 10_u64;
} {
  observe(value: value);
  let old = deref(value);
  set deref(value) = old;
  observe(value: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let summary = entailment(source, "update");
    validate_derivations(&summary);
    let outcomes = &summary.call_goals;
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].disposition, CallGoalDisposition::Discharged);
    assert_eq!(
        outcomes[0].evidence,
        vec![
            CallGoalEvidence::OpaquePositive,
            CallGoalEvidence::ExactL0Projection,
        ]
    );
    assert_eq!(outcomes[1].disposition, CallGoalDisposition::Discharged);
    assert_eq!(
        outcomes[1].evidence,
        vec![CallGoalEvidence::ExactL0Projection]
    );
    assert_root_contains(
        &summary,
        call_root(&summary, 1),
        |node| match node {
            DerivationNode::TransitiveBound {
                left,
                middle,
                right,
                bound: -1,
                ..
            } => {
                matches!(
                    (
                        retained_term(&summary, *left),
                        retained_term(&summary, *middle),
                        retained_term(&summary, *right),
                    ),
                    (
                        TermKind::Place(old, IntegerType::U64),
                        TermKind::Place(value, IntegerType::U64),
                        TermKind::Constant(10),
                    ) if old.root == PlaceRoot::Binding(BindingId(1))
                        && value.root == PlaceRoot::Binding(BindingId(0))
                )
            }
            _ => false,
        },
        "the exact old - deref(value) <= 0 plus deref(value) - 10 <= -1 projection",
    );
}

#[test]
fn an_element_write_keeps_a_whole_goal_supported_only_by_length() {
    let source = br#"fn sized(values: Slots<u8, 2>) -> result: unit pure contract {
  define size = values.len;
  define exact = size == 2_u64;
  define complete = band(exact, exact);
  requires complete;
} {
  return unit;
}

fn caller(values: Slots<u8, 2>) -> result: unit pure {
  let size = values.len;
  let exact = size == 2_u64;
  let complete = band(exact, exact);
  if complete {
    set values[0_u64] = 9_u8;
    sized(values: move values);
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outcomes = call_goals(source, "caller");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].disposition, CallGoalDisposition::Discharged);
    assert_eq!(outcomes[0].evidence, vec![CallGoalEvidence::OpaquePositive]);
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "length-only support survives the element commit: {outcome:?}"
        );
    });
}

#[test]
fn a_run_measure_participates_only_in_body_origin_expansion() {
    let source = br#"const values: Array<u8, 4> =[0_u8, 0_u8, 0_u8, 0_u8];

fn need_true(value: Bool) -> result: unit pure contract {
  requires value;
} {
  return unit;
}

fn probe() -> result: unit pure {
  let first_size = values.len;
  let first_exact = first_size == 4_u64;
  let first = band(first_exact, first_exact);
  if first {
    let second_size = values.len;
    let second_exact = second_size == 4_u64;
    let second = band(second_exact, second_exact);
    if second {
      return unit;
    } else {
      let impossible = False();
      need_true(value: impossible);
    }
  } else {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outcomes = call_goals(source, "probe");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].disposition, CallGoalDisposition::Discharged);
    assert_eq!(outcomes[0].evidence, vec![CallGoalEvidence::AllDerivable]);
}

#[test]
fn s4_is_independent_of_forward_and_mutually_recursive_traversal_order() {
    let source = br#"fn first(value: u64) -> result: unit pure contract {
  requires value < 10_u64;
} {
  second(value: value);
  return unit;
}

fn second(value: u64) -> result: unit pure contract {
  requires value < 10_u64;
} {
  first(value: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for function in ["first", "second"] {
        let summary = entailment(source, function);
        validate_derivations(&summary);
        assert_eq!(summary.call_goals.len(), 1);
        assert_eq!(
            summary.call_goals[0].disposition,
            CallGoalDisposition::Discharged
        );
        assert_eq!(
            summary.call_goals[0].evidence,
            vec![
                CallGoalEvidence::OpaquePositive,
                CallGoalEvidence::ExactL0Projection,
            ]
        );
        assert_root_has_event_kind(&summary, call_root(&summary, 0), FlowEventKind::S4);
    }
}

#[test]
fn a_forward_concrete_generic_call_uses_its_substituted_goal() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn caller(value: i32) -> result: unit pure {
  let positive = value > 0_i32;
  if positive {
    let result = guarded::<i32>(value: value);
  } else {
    return unit;
  }
  return unit;
}

fn guarded<T: Int>(value: T) -> result: T pure contract {
  requires value > 0_T;
} {
  return value;
}
"#;
    let summary = entailment(source, "caller");
    validate_derivations(&summary);
    assert_eq!(summary.call_goals.len(), 1);
    assert_eq!(
        summary.call_goals[0].disposition,
        CallGoalDisposition::Discharged
    );
    assert_eq!(
        summary.call_goals[0].evidence,
        vec![
            CallGoalEvidence::OpaquePositive,
            CallGoalEvidence::ExactL0Projection,
        ],
        "concrete generic goal: {:#?}",
        summary.call_goals[0].goal
    );
    assert_root_has_event_kind(&summary, call_root(&summary, 0), FlowEventKind::S1);
    let concrete_goal = format!("{:#?}", summary.call_goals[0].goal);
    assert!(concrete_goal.contains("I32"));
    assert!(!concrete_goal.contains("GenericInt"));
}

#[test]
fn concrete_const_instances_keep_function_local_derivation_inventories() {
    let source = br#"fn first<const n: u64>(values: Slots<u8, n>) -> result: u8 pure contract {
  requires values.len == n;
} {
  return values[0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  let small = slots_new::<u8, 2>();
  place_back(window: &small, value: 7_u8);
  place_back(window: &small, value: 7_u8);
  let small_first = first::<2>(values: move small);
  let large = slots_new::<u8, 5>();
  place_back(window: &large, value: 9_u8);
  place_back(window: &large, value: 9_u8);
  place_back(window: &large, value: 9_u8);
  place_back(window: &large, value: 9_u8);
  place_back(window: &large, value: 9_u8);
  let large_first = first::<5>(values: move large);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("both concrete const instances must check: {outcome:?}");
        };
        let instances: Vec<_> = checked
            .data
            .functions
            .iter()
            .filter(|function| function.name == "first")
            .collect();
        assert_eq!(instances.len(), 2);
        let mut concrete_lengths = Vec::new();
        for instance in instances {
            let summary = &instance.entailment;
            validate_derivations(summary);
            assert_eq!(summary.obligations.len(), 1);
            assert!(summary.obligations[0].discharged);
            let root = obligation_root(summary, 0);
            // The written `0_u64` index is the zero term itself, so each
            // instance proves its own subscript from its own length bound
            // against Z. Where that bound comes from depends on the shape: an
            // `Array<T, N>` carries a standing `len = N` off its type, while a
            // `Slots<T, n>` window keeps its length in a descriptor word, so
            // the bound here is the declared requirement instantiated at the
            // instance's own `n` [WIN-1, MSR-1]. It is still each instance's
            // own — the requirement mentions `n`, and the two instances
            // instantiate it at two constants — which is what this test asks.
            assert_root_contains(
                summary,
                root,
                |node| matches!(node, DerivationNode::SourceBound { .. }),
                "the concrete const instance's own declared length bound",
            );
            // [MSR-1] the window place carries all three measures, and exactly
            // one cell of [MSR-2]'s table fixes a constant: `cap` is the type
            // constant `n`, so it is the instance's own N, while `len` and
            // `head` are descriptor words of the value rather than facts of
            // the type [WIN-1].
            let mut constants: Vec<_> = summary
                .inventory
                .measure_bounds
                .iter()
                .filter_map(|bound| match bound {
                    Some(MeasureBound::Constant(value)) => Some(*value),
                    Some(MeasureBound::Equal(_)) | None => None,
                })
                .collect();
            constants.sort_unstable();
            assert_eq!(constants.len(), 1);
            concrete_lengths.push(constants[0]);
        }
        concrete_lengths.sort_unstable();
        assert_eq!(concrete_lengths, vec![2, 5]);
    });
}

/// The companion boundary: re-closing a state that lost facts must not
/// resurrect an *established* one. A window's `len` is a descriptor word and
/// carries no [ENT-2] constant of its own [MSR-2], so once the commit kills
/// the checked-branch bound on `offset` the only surviving relation is
/// `offset = start` over an unbounded argument, and [OP-4] still rejects the
/// subscript.
#[test]
fn a_write_still_kills_an_established_bound_on_its_target() {
    let source = br#"fn get(b: Slots<u8, 4>, start: u64) -> result: u8 pure {
  let spare = b.len;
  let offset = 0_u64;
  let inside = offset < spare;
  if inside {
  } else {
    return 0_u8;
  }
  set offset = start;
  return b[offset];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    // `offset` is a local the body writes, so no requirement names it and
    // the repair offers the proof and guard routes [DIAG-1].
    assert_rule_kind(source, SemanticRule::Op4, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedBoundsObligation {
                residual,
                disposition: StaticObligationDisposition::Unproved,
                mechanical_fix,
            } if residual == "offset < b.len"
                && mechanical_fix.starts_with("`offset < b.len` is not proved here")
                && mechanical_fix.contains("guard the access with `if offset < b.len`")
        )
    });
}
