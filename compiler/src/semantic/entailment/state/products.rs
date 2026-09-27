//! Typed private encoding of the retained values defined by the parent.

use super::*;
use crate::semantic::products::{Reader, Record, Writer, record_enum, record_struct, record_tuple};

impl Record for DerivationLedger {
    fn write(&self, writer: &mut Writer) {
        let Self {
            events,
            nodes,
            roots,
            depths,
            postcondition_call_ancestry,
            interned: _,
            metrics,
        } = self;
        events.write(writer);
        nodes.write(writer);
        roots.write(writer);
        depths.write(writer);
        postcondition_call_ancestry.write(writer);
        metrics.write(writer);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Some(Self {
            events: Record::read(reader)?,
            nodes: Record::read(reader)?,
            roots: Record::read(reader)?,
            depths: Record::read(reader)?,
            postcondition_call_ancestry: Record::read(reader)?,
            // A retained completed query is read-only. Finalization already
            // discards its construction accelerator.
            interned: InternIndex::default(),
            metrics: Record::read(reader)?,
        })
    }
}

record_enum!(Relation {
    0 => Bound { left, right, bound },
    1 => Equal { left, right, difference },
    2 => Distinct { left, right, difference },
});

record_tuple!(GoalId, 0);

record_enum!(GoalSign {
    0 => Positive,
    1 => Negative,
});

record_struct!(GoalNormalizationLiteral { component, negated });

record_struct!(GoalNormalization {
    components,
    positive_clauses,
    negative_clauses
});

record_tuple!(DerivationId, 0);

record_tuple!(FlowEventId, 0);

record_enum!(FlowEventKind {
    0 => S1,
    1 => S4,
    2 => S5,
    3 => S6,
    4 => S7,
    5 => S9,
    6 => S11,
    7 => S13,
    8 => Entry,
    9 => Join,
    10 => Snapshot,
    11 => PostconditionEntryImageInvalidation,
    12 => PostconditionCallConsume,
    13 => PostconditionCallWrite,
    14 => PostconditionReceiverWrite,
    15 => PostconditionGive,
    16 => PostconditionDeliveryJoin,
});

record_struct!(FlowEvent { kind, node_path });

record_struct!(RetainedGoal {
    expression,
    projection,
    normalization
});

record_struct!(DerivationInventory {
    terms,
    measure_bounds,
    goals
});

record_enum!(ImplicitBoundKind {
    0 => Reflexive,
    1 => Constant,
    2 => TypeMinimum,
    3 => TypeMaximum,
    4 => StandingMeasure,
    5 => MeasureOrdering,
});

record_struct!(JoinParent { ordinal, parent });

record_struct!(PostconditionCallSubstitution {
    operand,
    formal,
    term,
    transfer_holders,
    datum,
    exit_state
});

record_enum!(DerivationNode {
    0 => RequirementAffineImage { goal, sign, parent },
    1 => UnsignedDivisionProduct { product, division, domain },
    2 => SourceBound { relation, left, right, bound, event },
    3 => SourceDistinct { left, right, event },
    4 => SourceGoal { goal, sign, event },
    5 => OperationFact { relation, event, parents },
    6 => BooleanLiteral { goal, sign },
    7 => ImplicitBound { left, right, bound, kind },
    8 => TransitiveBound { left, middle, right, bound, first, second },
    9 => StrengthenedBound { left, right, bound, weak, distinct },
    10 => SubsumedBound { left, right, held, requested, parent },
    11 => Equality { left, right, forward, reverse },
    12 => DisequalityFromStrictBound { left, right, parent },
    13 => GoalProjection { goal, sign, relation, parent },
    14 => L0Contradiction { term, parent },
    15 => GoalContradiction { goal, positive, negative },
    16 => IntegerDomain { goal, parents },
    17 => ConversionDomain { goal, parents },
    18 => AffineConsequence { relation, premises, parents },
    19 => GoalNormalization { goal, sign, clause, parents },
    20 => GoalAffineConsequence { goal, sign, parent },
    21 => RangeSeparation { detail },
    22 => IndexSeparation { detail },
    23 => BooleanIntroduction { goal, sign, parents },
    24 => JoinBound { left, right, bound, event, parents },
    25 => JoinDistinct { left, right, event, parents },
    26 => JoinGoal { goal, sign, event, parents },
    27 => JoinContradiction { event, parents },
    28 => MaterializedBound { left, right, bound, event, parent },
    29 => MaterializedDistinct { left, right, event, parent },
    30 => MaterializedGoal { goal, sign, event, parent },
    31 => MaterializedContradiction { event, parent },
    32 => PostconditionExit { statement, relation_ordinal, relation, parent },
    33 => PostconditionAggregate { block, relation_ordinal, parents },
    34 => SignatureContract { block, relation_ordinal },
    35 => PostconditionCall { detail },
    36 => ContractCall { call, query, parents },
    37 => PostconditionDirectResult { statement, binding, relation, parent },
    38 => ResultTransport { statement, from, to, relation, parent },
    39 => ResultErr { statement },
    40 => PostconditionDirectReceiver { statement, binding, receiver_formal, relation, target_event, parent },
    41 => PostconditionGive { statement, carrier, receiver, relation, event, parent },
    42 => PostconditionDeliveryJoin { detail },
});

record_struct!(SourceLoopInvariantRef {
    loop_id,
    source_ordinal
});

record_enum!(SourceAffineFactRef {
    0 => LoopInvariant(f0),
    1 => SourceProof { source_ordinal },
    2 => JoinedSourceProof { join_ordinal },
});

record_struct!(AffinePremiseUse { source, factor });

record_struct!(PostconditionCallDetail {
    call,
    relation,
    summary,
    substitutions,
    transfer_events,
    parents
});

record_struct!(PostconditionDeliveryJoinDetail {
    statement,
    receiver,
    relation,
    event,
    parents
});

record_enum!(RangeSeparationOrdering {
    0 => LeftBeforeRight,
    1 => RightBeforeLeft,
    2 => LeftEmpty,
    3 => RightEmpty,
});

record_struct!(RangeSeparationDetail {
    left,
    right,
    ordering,
    parent
});

record_struct!(IndexSeparationDetail {
    left,
    right,
    parent,
    affine_target,
    affine_images,
    substitution
});

record_struct!(IndexCaptureSubstitution {
    source_left,
    source_right,
    left_identity,
    right_identity
});

record_struct!(DerivationMetrics {
    bounds_roots,
    opaque_goal_roots,
    projected_goal_roots,
    contradiction_roots,
    unique_nodes,
    parent_edges,
    maximum_depth,
    retained_bytes
});

record_enum!(DerivationRootKind {
    0 => BodyEntryContradiction,
    1 => BoundsObligation(f0),
    2 => AllocationUpperBound(f0),
    3 => RangePartition { obligation, partition, base },
    4 => IntegerDomainObligation(f0),
    5 => ConversionDomainObligation(f0),
    6 => CallGoal(f0),
    7 => CallContract(f0),
    8 => ContractGoal(f0),
    9 => PermissionSeparation { query, occurrence },
    10 => UnsignedDivisionProduct(f0),
    11 => RequirementAffineImage { requirement, member },
    12 => CountedS11 { occurrence, atom },
    13 => PostconditionExit { relation_ordinal, occurrence },
    14 => PostconditionAggregate { relation_ordinal },
    15 => PostconditionState { occurrence },
    16 => PostconditionConditional { occurrence },
    17 => PostconditionDirectResult { occurrence },
    18 => PostconditionDirectReceiver { occurrence },
    19 => PostconditionGive { occurrence },
    20 => PostconditionDeliveryJoin { occurrence },
});

record_enum!(CountedRootAtom {
    0 => LowerCaptureToEndpoint,
    1 => LowerEndpointToCapture,
    2 => UpperCaptureToEndpoint,
    3 => UpperEndpointToCapture,
    4 => BinderToLowerCapture,
    5 => LowerCaptureToBinder,
    6 => LowerCaptureLeBinder,
    7 => BinderLtUpperCapture,
});

record_struct!(DerivationRoot { kind, node });
