//! Typed private encoding of the retained values defined by the parent.

use super::*;
use crate::semantic::products::{record_enum, record_struct};

record_enum!(CallTransport {
    0 => ReadOnlyReference,
    1 => Value,
    2 => ViewedRange,
    3 => Conservative,
});

record_enum!(ObligationFamily {
    0 => Bounds,
    1 => IntegerDomain,
    2 => ConversionDomain,
    3 => AllocationFit,
    4 => RangeFormation,
    5 => CallSeparation(f0),
    6 => ExchangeSeparation(f0),
    7 => ReferencePreservation(f0),
});

record_struct!(ProvedAffineIndexMap {
    loop_id,
    coefficient,
    constant
});

record_struct!(ProvedRangePartition {
    loop_id,
    range,
    stride,
    base,
    stride_nonnegative,
    base_nonnegative
});

record_struct!(ObligationOutcome {
    node_path,
    family,
    conjunct,
    canonical_goal,
    components,
    discharged,
    refuted,
    contradictory,
    residual,
    overlap_targets,
    derivation,
    allocation_length_upper_bound,
    allocation_length_upper_bound_derivation,
    affine_index_maps,
    range_partitions,
    written_before
});

record_struct!(BoundsRequest {
    left,
    right,
    bound,
    distinct
});

record_struct!(OverflowConjuncts {
    upper,
    lower,
    ground,
    ground_result
});

record_struct!(GroundResult {
    negative,
    magnitude
});

record_enum!(CountedProofPoint {
    0 => PreheaderSnapshot,
    1 => BodyEntry,
});

record_struct!(CountedAtomicDerivation {
    relation,
    proof_point,
    parent
});

record_struct!(CountedEqualityDerivation {
    relation,
    forward,
    reverse
});

record_struct!(CountedBoundDerivation { relation, atomic });

record_struct!(CountedDerivationSet {
    counted_node_path,
    lower_capture_eq_endpoint,
    upper_capture_eq_endpoint,
    binder_eq_lower_capture,
    lower_capture_le_binder,
    binder_lt_upper_capture
});

record_struct!(LoopInvariantProof {
    base,
    step,
    base_refuted,
    step_refuted
});

record_struct!(LoopInvariantOutcome {
    node_path,
    loop_id,
    source_ordinal,
    name,
    base_target,
    backedge_target,
    proof
});

record_enum!(SourceProofCertificateFailure {
    0 => RepeatedUse { first, repeated },
    1 => UseCapacity { maximum, actual },
    2 => ArithmeticOverflow,
    3 => FormationCapacity,
    4 => InvalidFactor { use_index },
    5 => NonlinearResidual,
});

record_struct!(SourceProofCheck {
    premises,
    first_unproved_premise,
    combination,
    target_failure,
    source_failure,
    source_failure_use_index,
    certificate_failure,
    certificate_failure_use_index,
    residual_failure,
    redundant,
    target_refuted
});

record_struct!(SourceProofOutcome {
    node_path,
    use_node_paths,
    source_ordinal,
    name,
    certificate_written,
    check
});

record_struct!(JoinedSourceProofProvenance { predecessors });

record_enum!(PostconditionDisposition {
    0 => Discharged,
    1 => Refuted,
    2 => Unproved,
});

record_struct!(PostconditionEntryImage {
    parameter,
    projections,
    measure
});

record_struct!(PostconditionEntryImageOutcome {
    datum,
    invalidation
});

record_struct!(PostconditionExit {
    statement,
    relation,
    residual,
    entry_images,
    disposition,
    derivation
});

record_struct!(PostconditionAggregate {
    discharged,
    derivation
});

record_struct!(FunctionPostconditionProof {
    block,
    selector,
    relation_ordinal,
    summary,
    exits,
    aggregate
});

record_struct!(VerifiedPostconditionSummary {
    function,
    block,
    relation_ordinal,
    component
});

record_enum!(RelationProvenance {
    0 => Verified(f0),
    1 => FormalBoundary { query, actual, premises },
});

record_struct!(VerifiedPostconditionSummaryRef { summary });

record_struct!(PostconditionComponent {
    ordinal,
    functions,
    outgoing,
    summaries
});

record_struct!(ConcreteCallOccurrence {
    caller,
    node_path,
    callee
});

record_struct!(PostconditionSchedule {
    components,
    function_components,
    calls
});

record_enum!(CallGoalDisposition {
    0 => Discharged,
    1 => Refuted,
    2 => Unproved,
});

record_enum!(CallGoalEvidence {
    0 => AllDerivable,
    1 => OpaquePositive,
    2 => ExactL0Projection,
    3 => NormalizationPositive,
    4 => BooleanIntroductionPositive,
    5 => AffinePositive,
    6 => OpaqueNegative,
    7 => NegatedL0Projection,
    8 => NormalizationNegative,
    9 => BooleanIntroductionNegative,
});

record_struct!(CallGoalOutcome {
    node_path,
    callee,
    requires_clause,
    goal,
    rendered_goal,
    argument_count,
    disposition,
    evidence,
    derivation,
    written_before
});

record_struct!(ContractGoalOutcome {
    goal,
    disposition,
    evidence,
    derivation
});

record_struct!(BooleanGoalDecomposition {
    parent,
    sign,
    members
});

record_struct!(FunctionEntailment {
    body_disposition,
    obligations,
    call_goals,
    contract_goals,
    counted_derivations,
    loop_invariants,
    source_proofs,
    joined_source_proofs,
    postconditions,
    boolean_decompositions,
    permission_separations,
    answers,
    unrecorded,
    derivations,
    inventory
});
