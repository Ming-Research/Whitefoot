//! [ENT-4] fact states and the least-fixed-point difference-bound closure.
//!
//! A state carries the *flow* of [ENT-3]: the live established facts (plus
//! facts a join admitted, since [ENT-5] closes each joined state first). The
//! closed state at a point is computed on demand as the least set containing
//! the live and implicit facts, closed under transitivity, disequality
//! strengthening, and subsumption; every derivability answer equals that
//! least-closure answer.

use std::collections::{HashMap, HashSet};
use std::mem::{size_of, size_of_val};
use std::rc::Rc;

use super::super::goal::{GoalExpression, GoalOperation, GoalProjection};
use super::super::model::{
    BindingId, CheckedBooleanOperation, CheckedLoopId, CheckedMeasure, CheckedValue, IntegerType,
};
use super::super::places::{CapturedRange, CapturedValue};
use super::VerifiedPostconditionSummaryRef;
use super::affine::{AffineForm, AffineInequality};
use super::term::{MeasureBound, TermId, TermKind, TermTable, ZERO, type_range};
use crate::NodePath;

/// One normalized source relation over interned terms.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Relation {
    /// `left - right <= bound`.
    Bound {
        left: TermId,
        right: TermId,
        bound: i128,
    },
    /// `left - right = difference`, the bound pair in both directions.
    ///
    /// The displacement is the one a clause side writes [FN-9] and a kernel
    /// row declares [BLK-0]: `len_of(rest) + 1_u64 == len_of(vector)` is
    /// `len_of(rest) - len_of(vector) = -1`, which [ENT-4] holds as the two
    /// ordinary bounds `<= -1` and `>= -1` rather than as a new fact class.
    Equal {
        left: TermId,
        right: TermId,
        difference: i128,
    },
    /// `left - right != difference`, one disequality.
    ///
    /// Reversing the endpoints negates the displacement [ENT-2].
    Distinct {
        left: TermId,
        right: TermId,
        difference: i128,
    },
}

/// Dense identity of one finite typed expression in a concrete function's
/// [ENT-2] goal universe. Only Bool-typed members may carry signed facts;
/// non-Bool members are retained solely as ordinary-let origin expansions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GoalId(pub(crate) u32);

/// The two exact opaque facts [ENT-2] admits for one complete goal.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum GoalSign {
    Positive,
    Negative,
}

/// One literal in a fixed goal-normalization clause.
///
/// The component index preserves [ENT-6]'s normative order; `negated`
/// selects the exact L0 negation of that component for a negative-goal proof.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct GoalNormalizationLiteral {
    component: u32,
    negated: bool,
}

/// The complete fixed [ENT-4] normalization of one goal.
///
/// Clauses are a small deterministic DNF. `None` components retain an
/// operand outside L0's term vocabulary: a clause using one is unavailable,
/// while another complete clause may still prove the goal or its negation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GoalNormalization {
    components: Vec<Option<Relation>>,
    positive_clauses: Vec<Vec<GoalNormalizationLiteral>>,
    negative_clauses: Vec<Vec<GoalNormalizationLiteral>>,
}

impl GoalNormalization {
    /// A sufficient positive conjunction. Its failure says nothing about the
    /// domain's complement, so it supplies no negative clause.
    pub(crate) fn sufficient_conjunction(components: Vec<Option<Relation>>) -> Self {
        let mut normalization = Self::conjunction(components);
        normalization.negative_clauses.clear();
        normalization
    }

    /// A fixed conjunction and its exact De Morgan negation.
    pub(crate) fn conjunction(components: Vec<Option<Relation>>) -> Self {
        let count = u32::try_from(components.len())
            .expect("goal-normalization component count exceeds u32");
        let positive_clauses = vec![
            (0..count)
                .map(|component| GoalNormalizationLiteral {
                    component,
                    negated: false,
                })
                .collect(),
        ];
        let negative_clauses = (0..count)
            .map(|component| {
                vec![GoalNormalizationLiteral {
                    component,
                    negated: true,
                }]
            })
            .collect();
        Self {
            components,
            positive_clauses,
            negative_clauses,
        }
    }

    /// Signed division/remainder: `c0 && (c1 || c2)`, with its exact
    /// negation `!c0 || (!c1 && !c2)`.
    pub(crate) fn signed_division(components: Vec<Option<Relation>>) -> Self {
        debug_assert_eq!(components.len(), 3);
        let literal = |component, negated| GoalNormalizationLiteral { component, negated };
        Self {
            components,
            positive_clauses: vec![
                vec![literal(0, false), literal(1, false)],
                vec![literal(0, false), literal(2, false)],
            ],
            negative_clauses: vec![
                vec![literal(0, true)],
                vec![literal(1, true), literal(2, true)],
            ],
        }
    }

    fn clauses(&self, sign: GoalSign) -> &[Vec<GoalNormalizationLiteral>] {
        match sign {
            GoalSign::Positive => &self.positive_clauses,
            GoalSign::Negative => &self.negative_clauses,
        }
    }

    /// Whether one fixed clause is exactly the supplied single L0 relation.
    /// An occurrence-local component must not be represented as a
    /// normalization of a globally interned goal whose corresponding
    /// component is unavailable.
    pub(crate) fn clause_is_single_relation(
        &self,
        sign: GoalSign,
        clause: u32,
        expected: &Relation,
    ) -> bool {
        let Some([literal]) = usize::try_from(clause)
            .ok()
            .and_then(|clause| self.clauses(sign).get(clause))
            .map(Vec::as_slice)
        else {
            return false;
        };
        let Some(mut relation) = usize::try_from(literal.component)
            .ok()
            .and_then(|component| self.components.get(component))
            .cloned()
            .flatten()
        else {
            return false;
        };
        if literal.negated {
            relation = relation.negated();
        }
        relation == *expected
    }

    /// Exact selected clause relations, exposed only to the independent
    /// retained-derivation test oracle.
    #[cfg(test)]
    pub(crate) fn clause_relations(&self, sign: GoalSign, clause: u32) -> Option<Vec<Relation>> {
        self.clauses(sign)
            .get(usize::try_from(clause).ok()?)?
            .iter()
            .map(|literal| {
                let relation = self
                    .components
                    .get(usize::try_from(literal.component).ok()?)?
                    .clone()?;
                Some(if literal.negated {
                    relation.negated()
                } else {
                    relation
                })
            })
            .collect()
    }
}

/// Dense function-local identity of one retained ENT-4 derivation node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct DerivationId(pub(crate) u32);

/// Dense function-local identity of one proof-producing flow event.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct FlowEventId(pub(crate) u32);

/// Proof-producing phase of one event in the existing ENT flow.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FlowEventKind {
    /// An admitted ordinary-let definition, not a signed fact source.
    LetOrigin,
    S1,
    S4,
    S5,
    S6,
    S7,
    S9,
    S11,
    /// [ENT-3.S13] one declared relation instantiated at its call.
    S13,
    /// [ENT-3.S16] a proved header or local invariant's exact L0 conclusion.
    S16,
    /// [MSR-3] one entry datum minted at body entry, per parameter measure a
    /// declared relation names.
    Entry,
    Join,
    Snapshot,
    PostconditionEntryImageInvalidation,
    PostconditionCallConsume,
    PostconditionCallWrite,
    PostconditionReceiverWrite,
    PostconditionGive,
    PostconditionDeliveryJoin,
}

/// One deterministic flow event. A source path is retained exactly when the
/// checked tree already carries one; synthetic joins and loop edges need only
/// their dense event identity and predecessor ordinal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FlowEvent {
    pub(crate) kind: FlowEventKind,
    pub(crate) node_path: Option<NodePath>,
}

/// Canonical retained identity for one dense goal ID after analysis.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RetainedGoal {
    pub(crate) expression: GoalExpression,
    pub(crate) projection: Option<Relation>,
    pub(crate) normalization: Option<GoalNormalization>,
}

/// The exact canonical identities referenced by retained dense term and goal
/// IDs. The analyzer moves these inventories into the checked function; it
/// does not duplicate them or invent a portable encoding.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DerivationInventory {
    pub(crate) terms: Vec<TermKind>,
    pub(crate) measure_bounds: Vec<Option<MeasureBound>>,
    pub(crate) goals: Vec<RetainedGoal>,
}

/// Why an implicit bound exists independently of writer facts.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ImplicitBoundKind {
    Reflexive,
    Constant,
    TypeMinimum,
    TypeMaximum,
    /// One [MSR-2] standing fact: a measure whose table cell fixes its
    /// value, or a measure the table equates to another measure of the
    /// same place. It has empty support and no event kills it.
    StandingMeasure,
    /// [MSR-2]'s standing ordering between two measures of one place.
    MeasureOrdering,
}

/// One reaching predecessor named by a join derivation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct JoinParent {
    pub(crate) ordinal: u32,
    pub(crate) parent: DerivationId,
}

/// One caller-side image substituted for a callee postcondition formal.
/// The holder chain is retained because the checked goal image already names
/// the ultimate referent, while M must still observe any holder consumed by
/// the call transfer before S12 publishes the relation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PostconditionCallSubstitution {
    pub(crate) operand: u32,
    pub(crate) formal: u32,
    pub(crate) term: TermId,
    /// Exact checked holder chain used only by M at this call transfer. The
    /// published term's later support is reconstructed from `term` itself;
    /// an ultimate-referent image must not keep a temporary borrow holder
    /// alive after establishment.
    pub(crate) transfer_holders: Vec<BindingId>,
    /// Whether this operand was substituted by that call's call datum
    /// [MSR-3]. A datum contains no place, so no event at or after the call
    /// kills it and M's survival test does not apply; the formal is still
    /// recorded, because [FN-9]'s narrow receiver routes are stated over
    /// which formal an actual supplies and not over what the operand denotes.
    pub(crate) datum: bool,
    /// This operand is read after the call's own effects; later kills still apply.
    pub(crate) exit_state: bool,
}

/// The closed set of proof steps emitted by the existing entailment flow.
/// Parent IDs always precede their child in the arena.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum DerivationNode {
    LetOrigin {
        binding: BindingId,
        value: GoalId,
        event: FlowEventId,
    },
    OriginTransport {
        from: GoalId,
        goal: GoalId,
        sign: GoalSign,
        parent: DerivationId,
        origins: Box<[DerivationId]>,
    },
    OriginEquality {
        detail: Box<OriginEquality>,
    },
    /// The ordinary comparison projection of a transported entering source.
    OriginProjection {
        goal: GoalId,
        sign: GoalSign,
        relation: Relation,
        parent: DerivationId,
    },
    /// [ENT-5] introduction of one written relation over joined values.
    /// Each contributor proves its own complete instance; this is not an
    /// equality between mutually exclusive predecessor atoms.
    TransportedHeaderRelation {
        detail: Box<TransportedHeaderRelation>,
    },
    /// The fixed affine projection of an established S4 ordering leaf.
    RequirementAffineImage {
        goal: GoalId,
        sign: GoalSign,
        parent: DerivationId,
    },
    /// The fixed S7 quotient-product consequence. Both operations have
    /// already discharged their domains; the product source identifies the
    /// checked operand images matched against the retained division.
    UnsignedDivisionProduct {
        product: NodePath,
        division: DerivationId,
        domain: DerivationId,
    },
    SourceBound {
        relation: Relation,
        left: TermId,
        right: TermId,
        bound: i128,
        event: FlowEventId,
    },
    SourceDistinct {
        left: TermId,
        right: TermId,
        difference: i128,
        event: FlowEventId,
    },
    SourceGoal {
        goal: GoalId,
        sign: GoalSign,
        event: FlowEventId,
    },
    /// One [ENT-3.S7] result bound, order relation or offset relation. Its
    /// parents are the closed operand bounds its table row read, or the
    /// discharged domain whose interval-product measurement it states.
    OperationFact {
        relation: Relation,
        event: FlowEventId,
        parents: Box<[DerivationId]>,
    },
    /// The canonical truth sign of a Bool literal in the finite goal
    /// universe. This is an ordinary ENT-4 ground, not a source event.
    BooleanLiteral {
        goal: GoalId,
        sign: GoalSign,
    },
    ImplicitBound {
        left: TermId,
        right: TermId,
        bound: i128,
        kind: ImplicitBoundKind,
    },
    TransitiveBound {
        left: TermId,
        middle: TermId,
        right: TermId,
        bound: i128,
        first: DerivationId,
        second: DerivationId,
    },
    StrengthenedBound {
        left: TermId,
        right: TermId,
        bound: i128,
        weak: DerivationId,
        distinct: DerivationId,
    },
    SubsumedBound {
        left: TermId,
        right: TermId,
        held: i128,
        requested: i128,
        parent: DerivationId,
    },
    Equality {
        left: TermId,
        right: TermId,
        forward: DerivationId,
        reverse: DerivationId,
    },
    DisequalityFromStrictBound {
        left: TermId,
        right: TermId,
        difference: i128,
        parent: DerivationId,
    },
    GoalProjection {
        goal: GoalId,
        sign: GoalSign,
        relation: Relation,
        parent: DerivationId,
    },
    L0Contradiction {
        term: TermId,
        parent: DerivationId,
    },
    GoalContradiction {
        goal: GoalId,
        positive: DerivationId,
        negative: DerivationId,
    },
    /// One successful [ENT-6] integer-domain judgment. `parents` is either
    /// the contradiction/direct-goal proof or the fixed normalization's
    /// component proofs in ordinal order.
    IntegerDomain {
        goal: Option<GoalId>,
        parents: Vec<DerivationId>,
    },
    /// An OP-6 domain established from the evaluated operand's upper and
    /// lower bounds, in that order. The same rule answers FN-8 queries.
    ConversionDomain {
        goal: GoalId,
        parents: Vec<DerivationId>,
    },
    /// One fixed affine consequence used by an integer-domain, bounds,
    /// callable-boundary, or postcondition judgment. `premises` records every
    /// source fact and its positive integer factor in the deterministic
    /// residual reduction; parents are the L0 facts used to close the final
    /// residual. This is ordinary compiler analysis metadata retained for
    /// diagnostics.
    AffineConsequence {
        /// Exact L0 conclusion when this affine step feeds an ordinary bound
        /// or goal projection. General integer-domain and postcondition
        /// targets need no L0 spelling and retain `None`. The uncommon
        /// conclusion is held out of line so it does not widen every entry in
        /// the multi-million-node derivation arena.
        relation: Option<Box<Relation>>,
        premises: Box<[AffinePremiseUse]>,
        parents: Vec<DerivationId>,
    },
    /// One signed goal derived from a fixed normalization clause.
    /// The retained goal inventory makes the clause and every ordered parent
    /// relation independently verifiable.
    GoalNormalization {
        goal: GoalId,
        sign: GoalSign,
        clause: u32,
        parents: Vec<DerivationId>,
    },
    /// One exact signed goal concluded by an affine proof when the goal has
    /// no L0 projection or complete fixed normalization clause. The affine
    /// target is fixed by the owning checker; this node only connects that
    /// accepted route to the canonical goal retained for diagnostics.
    GoalAffineConsequence {
        goal: GoalId,
        sign: GoalSign,
        parent: DerivationId,
    },
    /// One exact [OWN-7] conclusion wrapped around the affine proof of the
    /// selected ordering. Both EFF-5 and PAR-1 retain this same evidence.
    RangeSeparation {
        detail: Box<RangeSeparationDetail>,
    },
    /// One exact EFF-5 indexed-position conclusion and the fixed proof that
    /// established it for these immutable capture occurrences.
    IndexSeparation {
        detail: Box<IndexSeparationDetail>,
    },
    /// One finite truth-table introduction for an already-interned Boolean
    /// parent (`band`, `bor`, or `bnot`).
    BooleanIntroduction {
        goal: GoalId,
        sign: GoalSign,
        parents: Vec<DerivationId>,
    },
    JoinBound {
        left: TermId,
        right: TermId,
        bound: i128,
        event: FlowEventId,
        parents: Vec<JoinParent>,
    },
    JoinDistinct {
        left: TermId,
        right: TermId,
        difference: i128,
        event: FlowEventId,
        parents: Vec<JoinParent>,
    },
    JoinGoal {
        goal: GoalId,
        sign: GoalSign,
        event: FlowEventId,
        parents: Vec<JoinParent>,
    },
    JoinContradiction {
        event: FlowEventId,
        parents: Vec<JoinParent>,
    },
    MaterializedBound {
        left: TermId,
        right: TermId,
        bound: i128,
        event: FlowEventId,
        parent: DerivationId,
    },
    MaterializedDistinct {
        left: TermId,
        right: TermId,
        difference: i128,
        event: FlowEventId,
        parent: DerivationId,
    },
    MaterializedGoal {
        goal: GoalId,
        sign: GoalSign,
        event: FlowEventId,
        parent: DerivationId,
    },
    MaterializedContradiction {
        event: FlowEventId,
        parent: DerivationId,
    },
    PostconditionExit {
        statement: NodePath,
        relation_ordinal: u32,
        relation: Box<Relation>,
        parent: DerivationId,
    },
    PostconditionAggregate {
        block: NodePath,
        relation_ordinal: u32,
        parents: Vec<DerivationId>,
    },
    /// A signature's declared contract: a function-formal premise [FN-4]
    /// or a prelude declaration [PRE-1]. Definitions instead discharge every
    /// selected return under [FN-9].
    SignatureContract {
        block: NodePath,
        relation_ordinal: u32,
    },
    /// Caller-local S12 evidence for one instantiated authorized relation,
    /// held out of line by [`PostconditionCallDetail`].
    PostconditionCall {
        detail: Box<PostconditionCallDetail>,
    },
    /// Caller-local evidence that proving the formal requirements authorizes
    /// execution under one exact accepted FN-4 implication. `query` names an
    /// external checked-program record; its isolated proof DAG is never
    /// imported into this ledger.
    ContractCall {
        call: NodePath,
        query: super::super::model::ContractQueryId,
        parents: Vec<DerivationId>,
    },
    PostconditionDirectResult {
        statement: NodePath,
        binding: BindingId,
        relation: Box<Relation>,
        parent: DerivationId,
    },
    /// Forward substitution into or out of an isolated Ok-payload context.
    ResultTransport {
        statement: NodePath,
        from: TermId,
        to: TermId,
        relation: Box<Relation>,
        parent: DerivationId,
    },
    /// An Err constructor makes its conditional Ok context unreachable.
    ResultErr {
        statement: NodePath,
    },
    PostconditionDirectReceiver {
        statement: NodePath,
        binding: BindingId,
        receiver_formal: u32,
        relation: Box<Relation>,
        target_event: FlowEventId,
        parent: DerivationId,
    },
    /// One eligible value-initializer edge after forward carrier-to-receiver
    /// substitution and the edge's ordinary kills. `carrier` is the term the
    /// receiver replaced [ENT-5]: a bare atom, or the give's evaluated value,
    /// which a literal or named-const carrier and a bare atom's carrier
    /// equality `v = d` are stated over.
    PostconditionGive {
        statement: NodePath,
        carrier: TermId,
        receiver: BindingId,
        relation: Box<Relation>,
        event: FlowEventId,
        parent: DerivationId,
    },
    /// The ordinary weakest-bound L0 join of every reaching delivery image,
    /// held out of line by [`PostconditionDeliveryJoinDetail`].
    PostconditionDeliveryJoin {
        detail: Box<PostconditionDeliveryJoinDetail>,
    },
}

/// Stable function-local reference to one source-written invariant in its
/// directly enclosing counted range.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SourceLoopInvariantRef {
    pub(crate) loop_id: CheckedLoopId,
    pub(crate) source_ordinal: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AffineRelationInstance {
    /// Written leaves in source order, including constants and repeated uses.
    pub(crate) operands: Vec<AffineForm>,
    pub(crate) components: Vec<AffineInequality>,
    pub(crate) sides: Vec<(Option<TermId>, Option<TermId>)>,
    pub(crate) formation: Vec<DerivationId>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct HeaderRelationInput {
    pub(crate) site: NodePath,
    pub(crate) instance: Option<AffineRelationInstance>,
    pub(crate) components: Vec<DerivationId>,
    pub(crate) contradiction: Option<DerivationId>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TransportedHeaderRelation {
    pub(crate) template: SourceLoopInvariantRef,
    pub(crate) inputs: Vec<HeaderRelationInput>,
    pub(crate) output: AffineRelationInstance,
    pub(crate) component: u8,
}

impl TransportedHeaderRelation {
    fn parents(&self) -> impl Iterator<Item = DerivationId> + '_ {
        self.inputs
            .iter()
            .flat_map(|input| {
                input
                    .contradiction
                    .iter()
                    .chain(input.components.iter())
                    .copied()
                    .chain(
                        input
                            .instance
                            .iter()
                            .flat_map(|instance| instance.formation.iter().copied()),
                    )
            })
            .chain(self.output.formation.iter().copied())
    }
}

/// Stable function-local identity of one already-checked affine source fact.
/// Later proof consumers retain which admitted source statement supplied
/// their affine premise.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SourceAffineFactRef {
    LoopInvariant(SourceLoopInvariantRef),
    SourceProof {
        source_ordinal: u32,
    },
    /// Diagnostic-only identity for one canonical source-proof inequality
    /// established independently on every predecessor of a structural join.
    JoinedSourceProof {
        join_ordinal: u32,
    },
}

/// One source-affine premise selected by the fixed automatic residual rule.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AffinePremiseUse {
    pub(crate) source: SourceAffineFactRef,
    pub(crate) factor: i128,
}

/// The retained content of one [`DerivationNode::PostconditionCall`].
///
/// `parents` are the exact caller-local proofs that establish the call's
/// actual-operation domains and requirements in the current source context.
///
/// It is held behind a pointer because [`DerivationNode`] lives in one flat
/// arena, so the widest variant sets the width of every entry. This is by far
/// the widest, and the arena is overwhelmingly [ENT-4] transitivity and join
/// steps: on `tests/programs/wfgrep.wf` 349 of 2.3 M nodes are S12 call
/// evidence, and inlining them cost 128 bytes on each of the other 2.3 M.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PostconditionCallDetail {
    pub(crate) call: NodePath,
    pub(crate) relation: Relation,
    pub(crate) summary: VerifiedPostconditionSummaryRef,
    /// Instantiated ENT-2 terms and their exact caller holder support, in
    /// relation operand order.
    pub(crate) substitutions: Vec<PostconditionCallSubstitution>,
    pub(crate) transfer_events: Vec<FlowEventId>,
    pub(crate) parents: Vec<DerivationId>,
}

/// The retained content of one [`DerivationNode::PostconditionDeliveryJoin`],
/// held out of line for the same reason as [`PostconditionCallDetail`].
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PostconditionDeliveryJoinDetail {
    pub(crate) statement: NodePath,
    pub(crate) receiver: BindingId,
    pub(crate) relation: Relation,
    pub(crate) event: FlowEventId,
    pub(crate) parents: Vec<JoinParent>,
}

/// Which fixed [OWN-7] ordering discharged one pair of captured ranges.
/// The declaration order is the proof entry's deterministic probe order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum RangeSeparationOrdering {
    LeftBeforeRight,
    RightBeforeLeft,
    LeftEmpty,
    RightEmpty,
}

/// The exact conclusion of one successful range-separation proof.
///
/// Range endpoints may have affine images that are not L0 terms, so the
/// targetless affine parent cannot state this conclusion by itself. The
/// uncommon payload stays out of line to keep the derivation arena compact.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RangeSeparationDetail {
    pub(crate) left: CapturedRange,
    pub(crate) right: CapturedRange,
    pub(crate) ordering: RangeSeparationOrdering,
    pub(crate) parent: DerivationId,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct IndexSeparationDetail {
    pub(crate) left: CapturedValue,
    pub(crate) right: CapturedValue,
    pub(crate) parent: DerivationId,
    pub(crate) affine_target: Option<Box<AffineInequality>>,
    pub(crate) affine_images: Option<Box<(AffineForm, AffineForm)>>,
    pub(crate) substitution: Option<Box<IndexCaptureSubstitution>>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct IndexCaptureSubstitution {
    pub(crate) source_left: TermId,
    pub(crate) source_right: TermId,
    pub(crate) left_identity: DerivationId,
    pub(crate) right_identity: DerivationId,
}

impl DerivationNode {
    fn for_each_parent(&self, mut visit: impl FnMut(DerivationId)) {
        match self {
            Self::OriginTransport {
                parent, origins, ..
            } => {
                visit(*parent);
                origins.iter().copied().for_each(visit);
            }
            Self::OriginEquality { detail } => detail.origins.iter().copied().for_each(visit),
            Self::OriginProjection { parent, .. } => visit(*parent),
            Self::LetOrigin { .. } => {}
            Self::TransportedHeaderRelation { detail } => detail.parents().for_each(visit),
            Self::UnsignedDivisionProduct {
                division, domain, ..
            } => {
                visit(*division);
                visit(*domain);
            }
            Self::OperationFact { parents, .. } => {
                for parent in parents {
                    visit(*parent);
                }
            }
            Self::TransitiveBound { first, second, .. } => {
                visit(*first);
                visit(*second);
            }
            Self::StrengthenedBound { weak, distinct, .. } => {
                visit(*weak);
                visit(*distinct);
            }
            Self::SubsumedBound { parent, .. }
            | Self::RequirementAffineImage { parent, .. }
            | Self::DisequalityFromStrictBound { parent, .. }
            | Self::GoalProjection { parent, .. }
            | Self::GoalAffineConsequence { parent, .. }
            | Self::L0Contradiction { parent, .. }
            | Self::MaterializedBound { parent, .. }
            | Self::MaterializedDistinct { parent, .. }
            | Self::MaterializedGoal { parent, .. }
            | Self::MaterializedContradiction { parent, .. }
            | Self::PostconditionExit { parent, .. }
            | Self::PostconditionDirectResult { parent, .. }
            | Self::PostconditionDirectReceiver { parent, .. }
            | Self::ResultTransport { parent, .. }
            | Self::PostconditionGive { parent, .. } => visit(*parent),
            Self::Equality {
                forward, reverse, ..
            } => {
                visit(*forward);
                visit(*reverse);
            }
            Self::GoalContradiction {
                positive, negative, ..
            } => {
                visit(*positive);
                visit(*negative);
            }
            Self::JoinBound { parents, .. }
            | Self::JoinDistinct { parents, .. }
            | Self::JoinGoal { parents, .. }
            | Self::JoinContradiction { parents, .. } => {
                for parent in parents {
                    visit(parent.parent);
                }
            }
            Self::PostconditionDeliveryJoin { detail } => {
                for parent in &detail.parents {
                    visit(parent.parent);
                }
            }
            Self::PostconditionCall { detail } => {
                for parent in &detail.parents {
                    visit(*parent);
                }
            }
            Self::RangeSeparation { detail } => visit(detail.parent),
            Self::IndexSeparation { detail } => {
                visit(detail.parent);
                if let Some(substitution) = &detail.substitution {
                    visit(substitution.left_identity);
                    visit(substitution.right_identity);
                }
            }
            Self::ContractCall { parents, .. } => {
                for parent in parents {
                    visit(*parent);
                }
            }
            Self::PostconditionAggregate { parents, .. }
            | Self::IntegerDomain { parents, .. }
            | Self::ConversionDomain { parents, .. }
            | Self::AffineConsequence { parents, .. }
            | Self::GoalNormalization { parents, .. }
            | Self::BooleanIntroduction { parents, .. } => {
                for parent in parents {
                    visit(*parent);
                }
            }
            Self::SourceBound { .. }
            | Self::SourceDistinct { .. }
            | Self::SourceGoal { .. }
            | Self::BooleanLiteral { .. }
            | Self::SignatureContract { .. }
            | Self::ResultErr { .. }
            | Self::ImplicitBound { .. } => {}
        }
    }

    /// Every retained node reference.
    fn for_each_retained_reference(&self, mut visit: impl FnMut(DerivationId)) {
        self.for_each_parent(&mut visit);
    }

    #[cfg(test)]
    pub(crate) fn parent_ids(&self) -> Vec<DerivationId> {
        let mut parents = Vec::with_capacity(self.parent_count());
        self.for_each_retained_reference(|parent| parents.push(parent));
        parents
    }

    fn parent_count(&self) -> usize {
        match self {
            Self::OriginTransport { origins, .. } => 1 + origins.len(),
            Self::OriginEquality { detail } => detail.origins.len(),
            Self::OriginProjection { .. } => 1,
            Self::LetOrigin { .. } => 0,
            Self::TransportedHeaderRelation { detail } => detail.parents().count(),
            Self::UnsignedDivisionProduct { .. }
            | Self::TransitiveBound { .. }
            | Self::StrengthenedBound { .. }
            | Self::Equality { .. }
            | Self::GoalContradiction { .. } => 2,
            Self::SubsumedBound { .. }
            | Self::RequirementAffineImage { .. }
            | Self::DisequalityFromStrictBound { .. }
            | Self::GoalProjection { .. }
            | Self::GoalAffineConsequence { .. }
            | Self::L0Contradiction { .. }
            | Self::MaterializedBound { .. }
            | Self::MaterializedDistinct { .. }
            | Self::MaterializedGoal { .. }
            | Self::MaterializedContradiction { .. }
            | Self::PostconditionExit { .. }
            | Self::PostconditionDirectResult { .. }
            | Self::PostconditionDirectReceiver { .. }
            | Self::ResultTransport { .. }
            | Self::PostconditionGive { .. } => 1,
            Self::JoinBound { parents, .. }
            | Self::JoinDistinct { parents, .. }
            | Self::JoinGoal { parents, .. }
            | Self::JoinContradiction { parents, .. } => parents.len(),
            Self::PostconditionDeliveryJoin { detail } => detail.parents.len(),
            Self::PostconditionAggregate { parents, .. } => parents.len(),
            Self::IntegerDomain { parents, .. }
            | Self::ConversionDomain { parents, .. }
            | Self::AffineConsequence { parents, .. }
            | Self::GoalNormalization { parents, .. }
            | Self::BooleanIntroduction { parents, .. } => parents.len(),
            Self::PostconditionCall { detail } => detail.parents.len(),
            Self::RangeSeparation { .. } => 1,
            Self::IndexSeparation { detail } => {
                if detail.substitution.is_some() {
                    3
                } else {
                    1
                }
            }
            Self::ContractCall { parents, .. } => parents.len(),
            Self::OperationFact { parents, .. } => parents.len(),
            Self::SourceBound { .. }
            | Self::SourceDistinct { .. }
            | Self::SourceGoal { .. }
            | Self::BooleanLiteral { .. }
            | Self::SignatureContract { .. }
            | Self::ResultErr { .. }
            | Self::ImplicitBound { .. } => 0,
        }
    }

    fn maximum_parent_depth(&self, depths: &[u32]) -> Option<u32> {
        let mut maximum = None;
        self.for_each_retained_reference(|parent| {
            let depth = depths[parent.0 as usize];
            maximum = Some(maximum.map_or(depth, |current: u32| current.max(depth)));
        });
        maximum
    }

    fn rank(&self) -> u8 {
        match self {
            Self::LetOrigin { .. } => 47,
            Self::OriginTransport { .. } => 48,
            Self::OriginEquality { .. } => 49,
            Self::OriginProjection { .. } => 50,
            Self::TransportedHeaderRelation { .. } => 46,
            Self::ResultTransport { .. } => 42,
            Self::ResultErr { .. } => 43,
            Self::ConversionDomain { .. } => 44,
            Self::UnsignedDivisionProduct { .. } => 36,
            Self::SourceBound { .. } => 0,
            Self::SourceDistinct { .. } => 1,
            Self::SourceGoal { .. } => 2,
            Self::BooleanLiteral { .. } => 3,
            Self::ImplicitBound { .. } => 4,
            Self::TransitiveBound { .. } => 5,
            Self::StrengthenedBound { .. } => 6,
            Self::SubsumedBound { .. } => 7,
            Self::Equality { .. } => 8,
            Self::DisequalityFromStrictBound { .. } => 9,
            Self::GoalProjection { .. } => 10,
            Self::GoalNormalization { .. } => 11,
            Self::BooleanIntroduction { .. } => 12,
            Self::L0Contradiction { .. } => 13,
            Self::GoalContradiction { .. } => 14,
            Self::JoinBound { .. } => 15,
            Self::JoinDistinct { .. } => 16,
            Self::JoinGoal { .. } => 17,
            Self::JoinContradiction { .. } => 18,
            Self::MaterializedBound { .. } => 19,
            Self::MaterializedDistinct { .. } => 20,
            Self::MaterializedGoal { .. } => 21,
            Self::MaterializedContradiction { .. } => 22,
            Self::PostconditionExit { .. } => 23,
            Self::PostconditionAggregate { .. } => 24,
            Self::SignatureContract { .. } => 35,
            Self::PostconditionCall { .. } => 25,
            Self::PostconditionDirectResult { .. } => 26,
            Self::PostconditionDirectReceiver { .. } => 28,
            Self::PostconditionGive { .. } => 30,
            Self::PostconditionDeliveryJoin { .. } => 31,
            Self::IntegerDomain { .. } => 32,
            Self::AffineConsequence { .. } => 33,
            Self::GoalAffineConsequence { .. } => 34,
            Self::RequirementAffineImage { .. } => 37,
            Self::ContractCall { .. } => 38,
            Self::RangeSeparation { .. } => 39,
            Self::IndexSeparation { .. } => 40,
            Self::OperationFact { .. } => 45,
        }
    }
}

/// Root class counts are kept explicit so task 0056 can measure the frozen
/// corpus without parsing debug output or inventing a persistent format.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DerivationMetrics {
    pub(crate) bounds_roots: u32,
    pub(crate) opaque_goal_roots: u32,
    pub(crate) projected_goal_roots: u32,
    pub(crate) contradiction_roots: u32,
    pub(crate) unique_nodes: u32,
    pub(crate) parent_edges: u32,
    pub(crate) maximum_depth: u32,
    pub(crate) retained_bytes: usize,
}

/// Which mandatory checked-program query owns a retained root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DerivationRootKind {
    HeaderRelation {
        occurrence: u32,
    },
    LoopInduction {
        occurrence: u32,
    },
    BodyEntryContradiction,
    BoundsObligation(u32),
    RangePartition {
        obligation: u32,
        partition: u32,
        base: bool,
    },
    IntegerDomainObligation(u32),
    ConversionDomainObligation(u32),
    CallGoal(u32),
    CallContract(u32),
    /// One declaration-only [FN-4] compatibility query. Its ledger and dense
    /// identity namespace belong only to the retained contract query.
    ContractGoal(u32),
    /// One successful visit of an optional pair-scoped [PAR-1] range query.
    PermissionSeparation {
        query: u32,
        occurrence: u32,
    },
    UnsignedDivisionProduct(u32),
    RequirementAffineImage {
        requirement: u32,
        member: u32,
    },
    CountedS11 {
        occurrence: u32,
        atom: CountedRootAtom,
    },
    PostconditionExit {
        relation_ordinal: u32,
        occurrence: u32,
    },
    PostconditionAggregate {
        relation_ordinal: u32,
    },
    PostconditionState {
        occurrence: u32,
    },
    PostconditionConditional {
        occurrence: u32,
    },
    PostconditionDirectResult {
        occurrence: u32,
    },
    PostconditionDirectReceiver {
        occurrence: u32,
    },
    PostconditionGive {
        occurrence: u32,
    },
    PostconditionDeliveryJoin {
        occurrence: u32,
    },
}

pub(crate) struct FinishRemap {
    pub(crate) nodes: Vec<Option<DerivationId>>,
    pub(crate) events: Vec<Option<FlowEventId>>,
}

/// The fixed eight directed atomic bounds in one normative S11 group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CountedRootAtom {
    LowerCaptureToEndpoint,
    LowerEndpointToCapture,
    UpperCaptureToEndpoint,
    UpperEndpointToCapture,
    BinderToLowerCapture,
    LowerCaptureToBinder,
    LowerCaptureLeBinder,
    BinderLtUpperCapture,
}

/// One mandatory query root into the function-local arena.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DerivationRoot {
    pub(crate) kind: DerivationRootKind,
    pub(crate) node: DerivationId,
}

/// Private, lifetime-bound derivation storage for one concrete checked
/// function. It is intentionally neither serializable nor independently
/// verifiable; the checked program remains the only authority.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DerivationLedger {
    pub(crate) events: Vec<FlowEvent>,
    pub(crate) nodes: Vec<DerivationNode>,
    pub(crate) roots: Vec<DerivationRoot>,
    depths: Vec<u32>,
    /// Parallel to `nodes`: whether a live value dependency reaches an
    /// S12 call relation without crossing an absorbing contradiction.
    /// Parent IDs precede children, so this is computed
    /// once at interning rather than rediscovered by every kill/join query.
    postcondition_call_ancestry: Vec<bool>,
    /// Parallel to `nodes`: whether the derivation rests on [ENT-2] implicit
    /// bounds alone through closure rules, so its conclusion holds at every
    /// program point and no kill or join boundary can remove it.
    implicit_only: Vec<bool>,
    interned: InternIndex,
    pub(crate) metrics: DerivationMetrics,
}

/// Content-addressed accelerator for [`DerivationLedger::intern`]. The index
/// is ordinary live compiler state: cloning a ledger clones the index, while
/// finalization discards it because no later semantic query may intern nodes.
#[derive(Clone, Debug, Default)]
struct InternIndex {
    entries: WordHashMap<u64, DerivationId>,
}

/// Hash builder for [`InternIndex`] and the relation maps of a fact state.
///
/// The [ENT-4] closure interns one candidate proof step for every accepted
/// matrix cell, which made hashing whole [`DerivationNode`] values with the
/// default `SipHash` the largest single remaining cost of checking
/// `tests/programs/wfgrep.wf`; rebuilding the term-pair relation maps of every
/// closed state was the next. The intern index is never iterated. The other
/// maps' iteration order was already random per process under `SipHash`, so
/// deterministic compiler output could not depend on it and cannot depend on
/// this fixed function either.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WordHashBuilder;

/// A relation map keyed by small dense identities.
pub(crate) type WordHashMap<K, V> = HashMap<K, V, WordHashBuilder>;
/// A relation set keyed by small dense identities.
pub(crate) type WordHashSet<K> = HashSet<K, WordHashBuilder>;

impl std::hash::BuildHasher for WordHashBuilder {
    type Hasher = WordHasher;

    fn build_hasher(&self) -> Self::Hasher {
        WordHasher::default()
    }
}

/// Deterministic multiply-rotate word hasher, seeded by the fractional bits of
/// the golden ratio.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WordHasher {
    state: u64,
}

impl WordHasher {
    const SEED: u64 = 0x517c_c1b7_2722_0a95;

    fn mix(&mut self, word: u64) {
        self.state = (self.state.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }
}

impl std::hash::Hasher for WordHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (words, tail) = bytes.as_chunks::<8>();
        for word in words {
            self.mix(u64::from_le_bytes(*word));
        }
        if !tail.is_empty() {
            let mut word = [0_u8; 8];
            word[..tail.len()].copy_from_slice(tail);
            self.mix(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.mix(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.mix(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.mix(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.mix(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.mix(value as u64);
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

impl PartialEq for InternIndex {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for InternIndex {}

impl DerivationLedger {
    pub(crate) fn event(
        &mut self,
        kind: FlowEventKind,
        node_path: Option<NodePath>,
    ) -> FlowEventId {
        let id = FlowEventId(
            u32::try_from(self.events.len())
                .expect("ENT flow event inventory exceeds the u32 identity space"),
        );
        self.events.push(FlowEvent { kind, node_path });
        id
    }

    /// Whether one proof is already the materialization of exactly this bound.
    ///
    /// A snapshot re-materializes every bound the state carries, and most of
    /// them are the bounds the previous snapshot materialized, unchanged. A
    /// second wrapper over such a proof records nothing the first does not:
    /// it is the same fact, with the same value, made independently live at
    /// an earlier point, and the earlier point is the honest one. Reusing it
    /// is what keeps a body of many measured commits from interning one node
    /// per bound per kill.
    pub(crate) fn materializes_bound(
        &self,
        proof: DerivationId,
        left: TermId,
        right: TermId,
        bound: i128,
    ) -> bool {
        matches!(
            self.nodes.get(proof.0 as usize),
            Some(DerivationNode::MaterializedBound {
                left: recorded_left,
                right: recorded_right,
                bound: recorded_bound,
                ..
            }) if *recorded_left == left && *recorded_right == right && *recorded_bound == bound
        )
    }

    pub(crate) fn intern(&mut self, node: DerivationNode) -> DerivationId {
        super::work::intern(&node);
        let key = match self.probe_intern(&node) {
            Ok(id) => return id,
            Err(key) => key,
        };
        let id = DerivationId(
            u32::try_from(self.nodes.len())
                .expect("ENT derivation inventory exceeds the u32 identity space"),
        );
        let mut parents_precede = true;
        node.for_each_parent(|parent| {
            parents_precede &= parent.0 < id.0;
        });
        node.for_each_retained_reference(|parent| {
            parents_precede &= parent.0 < id.0;
        });
        assert!(
            parents_precede,
            "ENT derivation parents must precede their child"
        );
        let depth = node
            .maximum_parent_depth(&self.depths)
            .map_or(0, |maximum| maximum.saturating_add(1));
        let postcondition_call_ancestry =
            self.node_has_postcondition_dependency(&node, &self.postcondition_call_ancestry);
        let implicit_only = node_is_implicit_only(&node, &self.implicit_only);
        self.nodes.push(node);
        self.depths.push(depth);
        self.postcondition_call_ancestry
            .push(postcondition_call_ancestry);
        self.implicit_only.push(implicit_only);
        self.interned.entries.insert(key, id);
        id
    }

    fn node_has_postcondition_dependency(
        &self,
        node: &DerivationNode,
        dependencies: &[bool],
    ) -> bool {
        let mut depends = matches!(node, DerivationNode::PostconditionCall { .. });
        if !depends {
            node.for_each_parent(|parent| {
                // An absorbing contradictory predecessor is neutral at a
                // join under ENT-5. Its proof is retained, but its former
                // value support cannot make a surviving path's conclusion
                // removable by a later S12 holder kill.
                if !matches!(
                    self.nodes[parent.0 as usize],
                    DerivationNode::L0Contradiction { .. }
                        | DerivationNode::GoalContradiction { .. }
                        | DerivationNode::JoinContradiction { .. }
                        | DerivationNode::MaterializedContradiction { .. }
                ) {
                    depends |= dependencies[parent.0 as usize];
                }
            });
        }
        depends
    }

    /// The key one interned identity is filed under.
    fn intern_key(node: &DerivationNode) -> u64 {
        use std::hash::BuildHasher;
        WordHashBuilder.hash_one(node)
    }

    /// The identity already filed for this exact node, or the free key it
    /// would be filed under.
    ///
    /// The walk starts at the node's own key and steps by one while the key it
    /// reaches is taken by a different node, so a hash shared by two nodes
    /// separates them by one step instead of losing one of them. Nothing is
    /// ever removed from the index — it is only rebuilt or cleared whole — so
    /// a free key always ends the walk.
    fn probe_intern(&self, node: &DerivationNode) -> Result<DerivationId, u64> {
        let mut key = Self::intern_key(node);
        while let Some(&id) = self.interned.entries.get(&key) {
            let index = id.0 as usize;
            if self.nodes[index] == *node {
                return Ok(id);
            }
            key = key.wrapping_add(1);
        }
        Err(key)
    }

    pub(crate) fn depth(&self, id: DerivationId) -> u32 {
        self.depths[id.0 as usize]
    }

    #[cfg(test)]
    pub(crate) fn node_event(&self, id: DerivationId) -> Option<FlowEventId> {
        node_event(&self.nodes[id.0 as usize])
    }

    /// Whether this proof consumes a still-removable S12 value relation.
    /// Contradiction parents are absorbing grounds rather than live value
    /// support, and A0 references are execution premises; neither propagates
    /// the candidate-removal dependency. All parents remain in the ledger.
    pub(crate) fn depends_on_postcondition_call(&self, id: DerivationId) -> bool {
        self.postcondition_call_ancestry[id.0 as usize]
    }

    /// Whether this proof rests on implicit bounds alone, so its conclusion
    /// holds at every program point.
    pub(crate) fn implicit_only(&self, id: DerivationId) -> bool {
        self.implicit_only[id.0 as usize]
    }

    /// Whether a closed L0 proof depends on one established relation rather
    /// than only on the universal implicit type/reflexive inventory. Delivery
    /// transports writer-visible facts and their closure; it does not create
    /// roots merely because every fresh integer term has implicit bounds.
    pub(crate) fn depends_on_explicit_relation(
        &self,
        id: DerivationId,
        memo: &mut HashMap<DerivationId, bool>,
    ) -> bool {
        if let Some(depends) = memo.get(&id) {
            return *depends;
        }
        let node = &self.nodes[id.0 as usize];
        let mut depends = matches!(
            node,
            DerivationNode::SourceBound { .. }
                | DerivationNode::SourceDistinct { .. }
                | DerivationNode::OperationFact { .. }
                | DerivationNode::PostconditionCall { .. }
                | DerivationNode::PostconditionDirectResult { .. }
                | DerivationNode::PostconditionDirectReceiver { .. }
                | DerivationNode::PostconditionGive { .. }
                | DerivationNode::PostconditionDeliveryJoin { .. }
        );
        depends |= matches!(
            node,
            DerivationNode::AffineConsequence {
                premises,
                ..
            } if !premises.is_empty()
        );
        if !depends
            && !matches!(
                node,
                DerivationNode::ImplicitBound { .. } | DerivationNode::SourceGoal { .. }
            )
        {
            node.for_each_parent(|parent| {
                depends |= self.depends_on_explicit_relation(parent, memo);
            });
        }
        memo.insert(id, depends);
        depends
    }

    fn better(&self, candidate: DerivationId, current: DerivationId) -> bool {
        let candidate_depth = self.depth(candidate);
        let current_depth = self.depth(current);
        candidate_depth < current_depth
            || (candidate_depth == current_depth
                && compare_node_ties(
                    &self.nodes[candidate.0 as usize],
                    &self.nodes[current.0 as usize],
                )
                .is_lt())
    }

    fn candidate_better(&self, candidate: &DerivationNode, current: DerivationId) -> bool {
        let candidate_depth = candidate
            .maximum_parent_depth(&self.depths)
            .map_or(0, |maximum| maximum.saturating_add(1));
        let current_depth = self.depth(current);
        candidate_depth < current_depth
            || (candidate_depth == current_depth
                && compare_node_ties(candidate, &self.nodes[current.0 as usize]).is_lt())
    }

    /// Releases the working storage of a settled analysis.
    ///
    /// The arena is built by pushing, so it ends a run with spare capacity and
    /// an interning index that no later semantic query uses. Finalization
    /// releases both without reconstructing them.
    ///
    /// `roots` is deliberately not released. It is the one vector that
    /// survives `finish_with_event_roots` in place — that pass rewrites each
    /// root's identity rather than rebuilding the vector — so its capacity
    /// does reach `measure`, and shrinking it here would move the
    /// `metrics.retained_bytes` a settled function records. That field is
    /// written only by `measure` and read only by this crate's own tests: no
    /// diagnostic, ledger line or emitted artifact carries it, so the cost
    /// would be an unpublished number that disagrees with the same analysis
    /// unsettled, rather than a wrong compilation. Leaving `roots` alone keeps
    /// `settle` invisible in the one place it could have shown, and costs
    /// nothing — a handful of entries per function beside millions of nodes.
    /// `settling_a_ledger_does_not_move_the_finished_byte_metric` guards it.
    pub(crate) fn settle(&mut self) {
        self.events.shrink_to_fit();
        self.nodes.shrink_to_fit();
        self.depths.shrink_to_fit();
        self.postcondition_call_ancestry.shrink_to_fit();
        self.implicit_only.shrink_to_fit();
        self.interned.entries = HashMap::default();
    }

    pub(crate) fn add_root(&mut self, kind: DerivationRootKind, node: DerivationId) {
        self.roots.push(DerivationRoot { kind, node });
    }

    #[cfg(test)]
    pub(crate) fn finish(&mut self) -> Vec<Option<DerivationId>> {
        self.finish_with_event_roots(&[]).nodes
    }

    pub(crate) fn finish_with_event_roots(&mut self, event_roots: &[FlowEventId]) -> FinishRemap {
        let old_len = self.nodes.len();
        let mut keep = vec![false; old_len];
        let mut stack: Vec<DerivationId> = self.roots.iter().map(|root| root.node).collect();
        while let Some(id) = stack.pop() {
            let index = id.0 as usize;
            if keep[index] {
                continue;
            }
            keep[index] = true;
            self.nodes[index].for_each_retained_reference(|parent| stack.push(parent));
        }
        let mut remap = vec![None; old_len];
        let retained = keep.iter().filter(|kept| **kept).count();
        let mut nodes = Vec::with_capacity(retained);
        for (index, node) in self.nodes.iter().enumerate() {
            if keep[index] {
                let id = DerivationId(
                    u32::try_from(nodes.len())
                        .expect("retained ENT derivations exceed the u32 identity space"),
                );
                remap[index] = Some(id);
                nodes.push(node.clone());
            }
        }
        for node in &mut nodes {
            remap_node(node, &remap);
        }
        for root in &mut self.roots {
            root.node = remap[root.node.0 as usize].expect("root retained");
        }
        self.nodes = nodes;
        let mut depths = Vec::with_capacity(self.nodes.len());
        let mut postcondition_call_ancestry = Vec::with_capacity(self.nodes.len());
        let mut implicit_only = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let depth = node
                .maximum_parent_depth(&depths)
                .map_or(0, |maximum| maximum.saturating_add(1));
            depths.push(depth);
            postcondition_call_ancestry
                .push(self.node_has_postcondition_dependency(node, &postcondition_call_ancestry));
            implicit_only.push(node_is_implicit_only(node, &implicit_only));
        }
        self.depths = depths;
        self.postcondition_call_ancestry = postcondition_call_ancestry;
        self.implicit_only = implicit_only;
        self.interned.entries.clear();
        self.interned.entries.shrink_to_fit();
        let event_remap = self.prune_events(event_roots);
        self.validate_integrity();
        self.metrics = self.measure();
        FinishRemap {
            nodes: remap,
            events: event_remap,
        }
    }

    fn validate_integrity(&self) {
        assert_eq!(self.postcondition_call_ancestry.len(), self.nodes.len());
        assert_eq!(self.implicit_only.len(), self.nodes.len());
        for (index, node) in self.nodes.iter().enumerate() {
            node.for_each_parent(|parent| {
                assert!(parent.0 < index as u32);
            });
            if let DerivationNode::PostconditionCall { detail } = node {
                for parent in &detail.parents {
                    assert!(parent.0 < index as u32);
                }
            }
        }
    }

    fn prune_events(&mut self, event_roots: &[FlowEventId]) -> Vec<Option<FlowEventId>> {
        let mut keep = vec![false; self.events.len()];
        for node in &self.nodes {
            for_each_node_event(node, |event| {
                keep[event.0 as usize] = true;
            });
        }
        for event in event_roots {
            keep[event.0 as usize] = true;
        }
        let mut remap = vec![None; self.events.len()];
        let mut events = Vec::with_capacity(keep.iter().filter(|kept| **kept).count());
        for (index, event) in self.events.iter().enumerate() {
            if keep[index] {
                let id = FlowEventId(
                    u32::try_from(events.len())
                        .expect("retained ENT events exceed the u32 identity space"),
                );
                remap[index] = Some(id);
                events.push(event.clone());
            }
        }
        for node in &mut self.nodes {
            remap_node_events(node, &remap);
        }
        self.events = events;
        remap
    }

    fn measure(&self) -> DerivationMetrics {
        let mut metrics = DerivationMetrics::default();
        for root in &self.roots {
            match &self.nodes[root.node.0 as usize] {
                DerivationNode::SourceGoal { .. }
                | DerivationNode::JoinGoal { .. }
                | DerivationNode::MaterializedGoal { .. } => metrics.opaque_goal_roots += 1,
                DerivationNode::GoalProjection { .. }
                | DerivationNode::GoalAffineConsequence { .. } => {
                    metrics.projected_goal_roots += 1;
                }
                DerivationNode::L0Contradiction { .. }
                | DerivationNode::GoalContradiction { .. }
                | DerivationNode::JoinContradiction { .. }
                | DerivationNode::MaterializedContradiction { .. } => {
                    metrics.contradiction_roots += 1;
                }
                _ => metrics.bounds_roots += 1,
            }
        }
        metrics.unique_nodes = u32::try_from(self.nodes.len())
            .expect("retained ENT derivations exceed the u32 metric space");
        let parent_edges: usize = self.nodes.iter().map(DerivationNode::parent_count).sum();
        metrics.parent_edges = u32::try_from(parent_edges)
            .expect("retained ENT parent edges exceed the u32 metric space");
        metrics.maximum_depth = self.depths.iter().copied().max().unwrap_or(0);
        metrics.retained_bytes = self.nodes.capacity() * size_of::<DerivationNode>()
            + self.events.capacity() * size_of::<FlowEvent>()
            + self.roots.capacity() * size_of::<DerivationRoot>()
            + self.depths.capacity() * size_of::<u32>()
            + self.postcondition_call_ancestry.capacity() * size_of::<bool>()
            + self.implicit_only.capacity() * size_of::<bool>()
            + self
                .nodes
                .iter()
                .map(|node| match node {
                    DerivationNode::OriginTransport { origins, .. } => {
                        size_of_val(origins.as_ref())
                    }
                    DerivationNode::OriginEquality { detail } => {
                        size_of::<OriginEquality>() + size_of_val(detail.origins.as_ref())
                    }
                    DerivationNode::JoinBound { parents, .. }
                    | DerivationNode::JoinDistinct { parents, .. }
                    | DerivationNode::JoinGoal { parents, .. }
                    | DerivationNode::JoinContradiction { parents, .. } => {
                        parents.capacity() * size_of::<JoinParent>()
                    }
                    DerivationNode::ResultTransport { .. } => size_of::<Relation>(),
                    DerivationNode::PostconditionAggregate { parents, .. } => {
                        parents.capacity() * size_of::<DerivationId>()
                    }
                    DerivationNode::IntegerDomain { parents, .. }
                    | DerivationNode::ConversionDomain { parents, .. }
                    | DerivationNode::GoalNormalization { parents, .. } => {
                        parents.capacity() * size_of::<DerivationId>()
                    }
                    DerivationNode::AffineConsequence {
                        relation,
                        premises,
                        parents,
                    } => {
                        parents.capacity() * size_of::<DerivationId>()
                            + premises.len() * size_of::<AffinePremiseUse>()
                            + relation.as_ref().map_or(0, |_| size_of::<Relation>())
                    }
                    DerivationNode::PostconditionDeliveryJoin { detail } => {
                        size_of::<PostconditionDeliveryJoinDetail>()
                            + detail.parents.capacity() * size_of::<JoinParent>()
                    }
                    DerivationNode::PostconditionCall { detail } => {
                        size_of::<PostconditionCallDetail>()
                            + detail.substitutions.capacity()
                                * size_of::<PostconditionCallSubstitution>()
                            + detail
                                .substitutions
                                .iter()
                                .map(|substitution| {
                                    substitution.transfer_holders.capacity()
                                        * size_of::<BindingId>()
                                })
                                .sum::<usize>()
                            + detail.transfer_events.capacity() * size_of::<FlowEventId>()
                            + detail.parents.capacity() * size_of::<DerivationId>()
                    }
                    DerivationNode::ContractCall { parents, .. } => {
                        parents.capacity() * size_of::<DerivationId>()
                    }
                    DerivationNode::RangeSeparation { .. } => size_of::<RangeSeparationDetail>(),
                    DerivationNode::IndexSeparation { detail } => {
                        size_of::<IndexSeparationDetail>()
                            + detail
                                .substitution
                                .as_ref()
                                .map_or(0, |_| size_of::<IndexCaptureSubstitution>())
                            + detail.affine_target.as_ref().map_or(0, |target| {
                                size_of::<AffineInequality>() + size_of_val(target.terms())
                            })
                            + detail.affine_images.as_ref().map_or(0, |images| {
                                size_of::<(AffineForm, AffineForm)>()
                                    + size_of_val(images.0.terms())
                                    + size_of_val(images.1.terms())
                            })
                    }
                    _ => 0,
                })
                .sum::<usize>()
            + self
                .nodes
                .iter()
                .filter_map(|node| match node {
                    DerivationNode::PostconditionExit { statement, .. }
                    | DerivationNode::ResultTransport { statement, .. }
                    | DerivationNode::ResultErr { statement } => Some(statement),
                    DerivationNode::PostconditionAggregate { block, .. }
                    | DerivationNode::SignatureContract { block, .. } => Some(block),
                    DerivationNode::PostconditionCall { detail } => Some(&detail.call),
                    DerivationNode::ContractCall { call, .. } => Some(call),
                    DerivationNode::PostconditionDirectResult { statement, .. }
                    | DerivationNode::PostconditionDirectReceiver { statement, .. }
                    | DerivationNode::PostconditionGive { statement, .. } => Some(statement),
                    DerivationNode::PostconditionDeliveryJoin { detail } => Some(&detail.statement),
                    _ => None,
                })
                .map(|path| path.components.capacity() * size_of::<u32>())
                .sum::<usize>()
            + self
                .events
                .iter()
                .filter_map(|event| event.node_path.as_ref())
                .map(|path| path.components.capacity() * size_of::<u32>())
                .sum::<usize>();
        metrics
    }
}

/// Whether a node is an [ENT-2] implicit bound, or an [ENT-4] closure rule
/// whose every parent is one of these, given the flags of earlier nodes.
fn node_is_implicit_only(node: &DerivationNode, flags: &[bool]) -> bool {
    match node {
        DerivationNode::ImplicitBound { .. } => true,
        DerivationNode::TransitiveBound { .. }
        | DerivationNode::StrengthenedBound { .. }
        | DerivationNode::SubsumedBound { .. }
        | DerivationNode::DisequalityFromStrictBound { .. } => {
            let mut implicit = true;
            node.for_each_parent(|parent| implicit &= flags[parent.0 as usize]);
            implicit
        }
        _ => false,
    }
}

fn compare_node_ties(left: &DerivationNode, right: &DerivationNode) -> std::cmp::Ordering {
    let rank = left.rank().cmp(&right.rank());
    if !rank.is_eq() {
        return rank;
    }
    if let (
        DerivationNode::RangeSeparation { detail: left },
        DerivationNode::RangeSeparation { detail: right },
    ) = (left, right)
    {
        return left.cmp(right);
    }
    if let (
        DerivationNode::IndexSeparation { detail: left },
        DerivationNode::IndexSeparation { detail: right },
    ) = (left, right)
    {
        return left.cmp(right);
    }
    let mut index = 0;
    loop {
        match (tie_component(left, index), tie_component(right, index)) {
            (Some(left), Some(right)) => {
                let ordering = left.cmp(&right);
                if !ordering.is_eq() {
                    return ordering;
                }
            }
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
        }
        index += 1;
    }
}

fn tie_component(node: &DerivationNode, index: usize) -> Option<u32> {
    match node {
        DerivationNode::LetOrigin {
            binding,
            value,
            event,
        } => [binding.0, value.0, event.0].get(index).copied(),
        DerivationNode::OriginTransport {
            from,
            goal,
            sign,
            parent,
            origins,
        } => [
            from.0,
            goal.0,
            u32::from(*sign == GoalSign::Negative),
            parent.0,
        ]
        .get(index)
        .copied()
        .or_else(|| origins.get(index.checked_sub(4)?).map(|id| id.0)),
        DerivationNode::OriginEquality { detail } => [detail.left.0, detail.right.0, detail.goal.0]
            .get(index)
            .copied()
            .or_else(|| detail.origins.get(index.checked_sub(3)?).map(|id| id.0)),
        DerivationNode::OriginProjection {
            goal, sign, parent, ..
        } => [goal.0, u32::from(*sign == GoalSign::Negative), parent.0]
            .get(index)
            .copied(),
        DerivationNode::TransportedHeaderRelation { detail } => [
            detail.template.loop_id.0,
            detail.template.source_ordinal,
            u32::from(detail.component),
        ]
        .get(index)
        .copied()
        .or_else(|| {
            detail
                .parents()
                .nth(index.checked_sub(3)?)
                .map(|parent| parent.0)
        }),
        DerivationNode::SourceBound { event, .. }
        | DerivationNode::SourceDistinct { event, .. }
        | DerivationNode::SourceGoal { event, .. } => (index == 0).then_some(event.0),
        DerivationNode::OperationFact { event, parents, .. } => (index == 0)
            .then_some(event.0)
            .or_else(|| parents.get(index.checked_sub(1)?).map(|parent| parent.0)),
        DerivationNode::BooleanLiteral { goal, sign } => [
            goal.0,
            match sign {
                GoalSign::Positive => 0,
                GoalSign::Negative => 1,
            },
        ]
        .get(index)
        .copied(),
        DerivationNode::ImplicitBound { kind, .. } => (index == 0).then_some(match kind {
            ImplicitBoundKind::Reflexive => 0,
            ImplicitBoundKind::Constant => 1,
            ImplicitBoundKind::TypeMinimum => 2,
            ImplicitBoundKind::TypeMaximum => 3,
            ImplicitBoundKind::StandingMeasure => 4,
            ImplicitBoundKind::MeasureOrdering => 5,
        }),
        DerivationNode::UnsignedDivisionProduct {
            product,
            division,
            domain,
        } => {
            if index < 2 {
                [division.0, domain.0].get(index).copied()
            } else {
                product.components().get(index - 2).copied()
            }
        }
        DerivationNode::TransitiveBound { first, second, .. } => {
            [first.0, second.0].get(index).copied()
        }
        DerivationNode::StrengthenedBound { weak, distinct, .. } => {
            [weak.0, distinct.0].get(index).copied()
        }
        DerivationNode::Equality {
            forward, reverse, ..
        } => [forward.0, reverse.0].get(index).copied(),
        DerivationNode::GoalContradiction {
            positive, negative, ..
        } => [positive.0, negative.0].get(index).copied(),
        DerivationNode::IntegerDomain { goal, parents } => {
            goal.map(|goal| goal.0).filter(|_| index == 0).or_else(|| {
                parents
                    .get(index - usize::from(goal.is_some()))
                    .map(|parent| parent.0)
            })
        }
        DerivationNode::ConversionDomain { goal, parents } => (index == 0)
            .then_some(goal.0)
            .or_else(|| parents.get(index.checked_sub(1)?).map(|parent| parent.0)),
        DerivationNode::AffineConsequence {
            premises, parents, ..
        } => {
            const WORDS_PER_PREMISE: usize = 7;
            if index == 0 {
                return u32::try_from(premises.len()).ok();
            }
            let premise_word = index - 1;
            if let Some(premise) = premises.get(premise_word / WORDS_PER_PREMISE) {
                let (tag, first, second) = match premise.source {
                    SourceAffineFactRef::LoopInvariant(source) => {
                        (1, source.loop_id.0, source.source_ordinal)
                    }
                    SourceAffineFactRef::SourceProof { source_ordinal } => (2, source_ordinal, 0),
                    SourceAffineFactRef::JoinedSourceProof { join_ordinal } => (3, join_ordinal, 0),
                };
                let factor = premise.factor as u128;
                return [
                    tag,
                    first,
                    second,
                    (factor >> 96) as u32,
                    (factor >> 64) as u32,
                    (factor >> 32) as u32,
                    factor as u32,
                ]
                .get(premise_word % WORDS_PER_PREMISE)
                .copied();
            }
            parents
                .get(premise_word.checked_sub(premises.len() * WORDS_PER_PREMISE)?)
                .map(|parent| parent.0)
        }
        DerivationNode::GoalNormalization {
            goal,
            sign,
            clause,
            parents,
        } => [
            goal.0,
            match sign {
                GoalSign::Positive => 0,
                GoalSign::Negative => 1,
            },
            *clause,
        ]
        .get(index)
        .copied()
        .or_else(|| parents.get(index.checked_sub(3)?).map(|parent| parent.0)),
        DerivationNode::GoalAffineConsequence { goal, sign, parent }
        | DerivationNode::RequirementAffineImage { goal, sign, parent } => [
            goal.0,
            match sign {
                GoalSign::Positive => 0,
                GoalSign::Negative => 1,
            },
            parent.0,
        ]
        .get(index)
        .copied(),
        DerivationNode::RangeSeparation { detail } => (index == 0).then_some(detail.parent.0),
        DerivationNode::IndexSeparation { detail } => (index == 0).then_some(detail.parent.0),
        DerivationNode::BooleanIntroduction {
            goal,
            sign,
            parents,
        } => [
            goal.0,
            match sign {
                GoalSign::Positive => 0,
                GoalSign::Negative => 1,
            },
        ]
        .get(index)
        .copied()
        .or_else(|| parents.get(index.checked_sub(2)?).map(|parent| parent.0)),
        DerivationNode::SubsumedBound { parent, .. }
        | DerivationNode::DisequalityFromStrictBound { parent, .. }
        | DerivationNode::GoalProjection { parent, .. }
        | DerivationNode::L0Contradiction { parent, .. } => (index == 0).then_some(parent.0),
        DerivationNode::JoinBound { event, parents, .. }
        | DerivationNode::JoinDistinct { event, parents, .. }
        | DerivationNode::JoinGoal { event, parents, .. }
        | DerivationNode::JoinContradiction { event, parents, .. } => {
            if index == 0 {
                Some(event.0)
            } else {
                let parent_index = (index - 1) / 2;
                let parent = parents.get(parent_index)?;
                if (index - 1).is_multiple_of(2) {
                    Some(parent.ordinal)
                } else {
                    Some(parent.parent.0)
                }
            }
        }
        DerivationNode::MaterializedBound { event, parent, .. }
        | DerivationNode::MaterializedDistinct { event, parent, .. }
        | DerivationNode::MaterializedGoal { event, parent, .. }
        | DerivationNode::MaterializedContradiction { event, parent, .. } => {
            [parent.0, event.0].get(index).copied()
        }
        DerivationNode::PostconditionExit { parent, .. } => (index == 0).then_some(parent.0),
        DerivationNode::PostconditionAggregate { parents, .. } => {
            parents.get(index).map(|parent| parent.0)
        }
        DerivationNode::SignatureContract {
            relation_ordinal, ..
        } => (index == 0).then_some(*relation_ordinal),
        DerivationNode::PostconditionCall { detail } => {
            let PostconditionCallDetail {
                summary,
                parents,
                transfer_events,
                ..
            } = detail.as_ref();
            let fixed = summary.summary.identity();
            fixed
                .get(index)
                .copied()
                .or_else(|| {
                    let index = index.checked_sub(fixed.len())?;
                    parents.get(index).map(|parent| parent.0)
                })
                .or_else(|| {
                    let index = index.checked_sub(fixed.len() + parents.len())?;
                    transfer_events.get(index).map(|event| event.0)
                })
        }
        DerivationNode::ContractCall { query, parents, .. } => {
            if index == 0 {
                Some(query.0)
            } else {
                parents.get(index - 1).map(|parent| parent.0)
            }
        }
        DerivationNode::ResultTransport {
            from, to, parent, ..
        } => [from.0, to.0, parent.0].get(index).copied(),
        DerivationNode::ResultErr { .. } => None,
        DerivationNode::PostconditionDirectResult {
            binding, parent, ..
        } => [binding.0, parent.0].get(index).copied(),
        DerivationNode::PostconditionDirectReceiver {
            binding,
            receiver_formal,
            target_event,
            parent,
            ..
        } => [binding.0, *receiver_formal, target_event.0, parent.0]
            .get(index)
            .copied(),
        DerivationNode::PostconditionGive {
            carrier,
            receiver,
            event,
            parent,
            ..
        } => [carrier.0, receiver.0, event.0, parent.0]
            .get(index)
            .copied(),
        DerivationNode::PostconditionDeliveryJoin { detail } => {
            [Some(detail.receiver.0), Some(detail.event.0)]
                .get(index)
                .copied()
                .flatten()
                .or_else(|| {
                    let index = index.checked_sub(2)?;
                    let parent = detail.parents.get(index / 2)?;
                    if index.is_multiple_of(2) {
                        Some(parent.ordinal)
                    } else {
                        Some(parent.parent.0)
                    }
                })
        }
    }
}

fn node_event(node: &DerivationNode) -> Option<FlowEventId> {
    match node {
        DerivationNode::LetOrigin { event, .. } => Some(*event),
        DerivationNode::SourceBound { event, .. }
        | DerivationNode::SourceDistinct { event, .. }
        | DerivationNode::SourceGoal { event, .. }
        | DerivationNode::JoinBound { event, .. }
        | DerivationNode::JoinDistinct { event, .. }
        | DerivationNode::JoinGoal { event, .. }
        | DerivationNode::JoinContradiction { event, .. }
        | DerivationNode::MaterializedBound { event, .. }
        | DerivationNode::MaterializedDistinct { event, .. }
        | DerivationNode::MaterializedGoal { event, .. }
        | DerivationNode::MaterializedContradiction { event, .. }
        | DerivationNode::PostconditionDirectReceiver {
            target_event: event,
            ..
        }
        | DerivationNode::PostconditionGive { event, .. }
        | DerivationNode::OperationFact { event, .. } => Some(*event),
        DerivationNode::PostconditionDeliveryJoin { detail } => Some(detail.event),
        _ => None,
    }
}

fn node_event_mut(node: &mut DerivationNode) -> Option<&mut FlowEventId> {
    match node {
        DerivationNode::LetOrigin { event, .. } => Some(event),
        DerivationNode::SourceBound { event, .. }
        | DerivationNode::SourceDistinct { event, .. }
        | DerivationNode::SourceGoal { event, .. }
        | DerivationNode::JoinBound { event, .. }
        | DerivationNode::JoinDistinct { event, .. }
        | DerivationNode::JoinGoal { event, .. }
        | DerivationNode::JoinContradiction { event, .. }
        | DerivationNode::MaterializedBound { event, .. }
        | DerivationNode::MaterializedDistinct { event, .. }
        | DerivationNode::MaterializedGoal { event, .. }
        | DerivationNode::MaterializedContradiction { event, .. }
        | DerivationNode::PostconditionDirectReceiver {
            target_event: event,
            ..
        }
        | DerivationNode::PostconditionGive { event, .. }
        | DerivationNode::OperationFact { event, .. } => Some(event),
        DerivationNode::PostconditionDeliveryJoin { detail } => Some(&mut detail.event),
        _ => None,
    }
}

fn for_each_node_event(node: &DerivationNode, mut visit: impl FnMut(FlowEventId)) {
    if let Some(event) = node_event(node) {
        visit(event);
    }
    if let DerivationNode::PostconditionCall { detail } = node {
        for event in &detail.transfer_events {
            visit(*event);
        }
    }
}

fn remap_node_events(node: &mut DerivationNode, remap: &[Option<FlowEventId>]) {
    if let Some(event) = node_event_mut(node) {
        *event = remap[event.0 as usize].expect("retained node event retained");
    }
    if let DerivationNode::PostconditionCall { detail } = node {
        for event in &mut detail.transfer_events {
            *event = remap[event.0 as usize].expect("retained S12 transfer event retained");
        }
    }
}

fn remap_id(id: &mut DerivationId, remap: &[Option<DerivationId>]) {
    *id = remap[id.0 as usize].expect("retained node parent retained");
}

fn remap_node(node: &mut DerivationNode, remap: &[Option<DerivationId>]) {
    match node {
        DerivationNode::OriginTransport {
            parent, origins, ..
        } => {
            remap_id(parent, remap);
            for origin in origins {
                remap_id(origin, remap);
            }
        }
        DerivationNode::OriginEquality { detail } => {
            for origin in &mut detail.origins {
                remap_id(origin, remap);
            }
        }
        DerivationNode::OriginProjection { parent, .. } => remap_id(parent, remap),
        DerivationNode::LetOrigin { .. } => {}
        DerivationNode::TransportedHeaderRelation { detail } => {
            for input in &mut detail.inputs {
                for parent in input
                    .contradiction
                    .iter_mut()
                    .chain(input.components.iter_mut())
                {
                    remap_id(parent, remap);
                }
                if let Some(instance) = &mut input.instance {
                    for parent in &mut instance.formation {
                        remap_id(parent, remap);
                    }
                }
            }
            for parent in &mut detail.output.formation {
                remap_id(parent, remap);
            }
        }
        DerivationNode::UnsignedDivisionProduct {
            division, domain, ..
        } => {
            remap_id(division, remap);
            remap_id(domain, remap);
        }
        DerivationNode::OperationFact { parents, .. } => {
            for parent in parents.iter_mut() {
                remap_id(parent, remap);
            }
        }
        DerivationNode::TransitiveBound { first, second, .. } => {
            remap_id(first, remap);
            remap_id(second, remap);
        }
        DerivationNode::StrengthenedBound { weak, distinct, .. } => {
            remap_id(weak, remap);
            remap_id(distinct, remap);
        }
        DerivationNode::Equality {
            forward, reverse, ..
        } => {
            remap_id(forward, remap);
            remap_id(reverse, remap);
        }
        DerivationNode::GoalContradiction {
            positive, negative, ..
        } => {
            remap_id(positive, remap);
            remap_id(negative, remap);
        }
        DerivationNode::IntegerDomain { parents, .. }
        | DerivationNode::ConversionDomain { parents, .. }
        | DerivationNode::AffineConsequence { parents, .. }
        | DerivationNode::GoalNormalization { parents, .. }
        | DerivationNode::BooleanIntroduction { parents, .. } => {
            for parent in parents {
                remap_id(parent, remap);
            }
        }
        DerivationNode::JoinBound { parents, .. }
        | DerivationNode::JoinDistinct { parents, .. }
        | DerivationNode::JoinGoal { parents, .. }
        | DerivationNode::JoinContradiction { parents, .. } => {
            for parent in parents {
                remap_id(&mut parent.parent, remap);
            }
        }
        DerivationNode::SubsumedBound { parent, .. }
        | DerivationNode::DisequalityFromStrictBound { parent, .. }
        | DerivationNode::GoalProjection { parent, .. }
        | DerivationNode::GoalAffineConsequence { parent, .. }
        | DerivationNode::RequirementAffineImage { parent, .. }
        | DerivationNode::L0Contradiction { parent, .. }
        | DerivationNode::MaterializedBound { parent, .. }
        | DerivationNode::MaterializedDistinct { parent, .. }
        | DerivationNode::MaterializedGoal { parent, .. }
        | DerivationNode::MaterializedContradiction { parent, .. }
        | DerivationNode::PostconditionExit { parent, .. }
        | DerivationNode::PostconditionDirectResult { parent, .. }
        | DerivationNode::PostconditionDirectReceiver { parent, .. }
        | DerivationNode::ResultTransport { parent, .. }
        | DerivationNode::PostconditionGive { parent, .. } => remap_id(parent, remap),
        DerivationNode::PostconditionDeliveryJoin { detail } => {
            for parent in &mut detail.parents {
                remap_id(&mut parent.parent, remap);
            }
        }
        DerivationNode::PostconditionAggregate { parents, .. } => {
            for parent in parents {
                remap_id(parent, remap);
            }
        }
        DerivationNode::PostconditionCall { detail } => {
            for parent in &mut detail.parents {
                remap_id(parent, remap);
            }
        }
        DerivationNode::ContractCall { parents, .. } => {
            for parent in parents {
                remap_id(parent, remap);
            }
        }
        DerivationNode::RangeSeparation { detail } => {
            remap_id(&mut detail.parent, remap);
        }
        DerivationNode::IndexSeparation { detail } => {
            remap_id(&mut detail.parent, remap);
            if let Some(substitution) = &mut detail.substitution {
                remap_id(&mut substitution.left_identity, remap);
                remap_id(&mut substitution.right_identity, remap);
            }
        }
        DerivationNode::SourceBound { .. }
        | DerivationNode::SourceDistinct { .. }
        | DerivationNode::SourceGoal { .. }
        | DerivationNode::BooleanLiteral { .. }
        | DerivationNode::SignatureContract { .. }
        | DerivationNode::ResultErr { .. }
        | DerivationNode::ImplicitBound { .. } => {}
    }
}

/// One place read by a complete goal. `length` records ENT-5's fixed-length
/// boundary: an element write does not invalidate a `len_of(P)` observation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GoalSupport {
    pub(crate) root: BindingId,
    pub(crate) projections: Vec<GoalProjection>,
    /// Which [MSR-1] measure of the place this support belongs to, when the
    /// node is a measure node; `None` is the ordinary place node. Every
    /// measure of one place has the same support, P's descriptor storage
    /// [MSR-2], so this selects the node class rather than the storage.
    pub(crate) measure: Option<CheckedMeasure>,
}

/// A live ordinary-let definition and its retained introduction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GoalOrigin {
    pub(crate) goal: GoalId,
    pub(crate) proof: DerivationId,
}

/// A query-local equality justified by live ordinary-let definitions.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct OriginEquality {
    pub(crate) left: GoalId,
    pub(crate) right: GoalId,
    pub(crate) goal: GoalId,
    pub(crate) relation: Relation,
    pub(crate) origins: Box<[DerivationId]>,
}

/// Derived data attached to one exact typed expression.
#[derive(Clone, Debug)]
struct GoalRecord {
    expression: GoalExpression,
    projection: Option<Relation>,
    normalization: Option<GoalNormalization>,
    support: Vec<GoalSupport>,
}

/// Per-function interning table for [ENT-2]'s finite goal universe.
#[derive(Default)]
pub(crate) struct GoalTable {
    ids: HashMap<GoalExpression, GoalId>,
    records: Vec<GoalRecord>,
    revision: usize,
}

impl GoalTable {
    pub(crate) fn intern(
        &mut self,
        expression: GoalExpression,
        projection: Option<Relation>,
        normalization: Option<GoalNormalization>,
        support: Vec<GoalSupport>,
    ) -> GoalId {
        if let Some(id) = self.ids.get(&expression).copied() {
            let record = &mut self.records[id.0 as usize];
            if (record.projection.is_none() && projection.is_some())
                || (record.normalization.is_none() && normalization.is_some())
            {
                self.revision = self
                    .revision
                    .checked_add(1)
                    .expect("goal revision fits usize");
            }
            debug_assert_eq!(record.support, support);
            if record.projection.is_none() {
                record.projection = projection;
            } else {
                debug_assert_eq!(record.projection, projection);
            }
            if record.normalization.is_none() {
                record.normalization = normalization;
            } else {
                debug_assert_eq!(record.normalization, normalization);
            }
            return id;
        }
        let id = GoalId(
            u32::try_from(self.records.len())
                .expect("ENT goal inventory exceeds the u32 identity space"),
        );
        self.ids.insert(expression.clone(), id);
        self.records.push(GoalRecord {
            expression,
            projection,
            normalization,
            support,
        });
        self.revision = self
            .revision
            .checked_add(1)
            .expect("goal revision fits usize");
        id
    }

    /// Includes metadata supplied to an existing goal, not only new identities.
    pub(crate) fn revision(&self) -> usize {
        self.revision
    }

    pub(crate) fn expression(&self, id: GoalId) -> &GoalExpression {
        &self.records[id.0 as usize].expression
    }

    pub(crate) fn id(&self, expression: &GoalExpression) -> Option<GoalId> {
        self.ids.get(expression).copied()
    }

    pub(crate) fn projection(&self, id: GoalId) -> Option<&Relation> {
        self.records[id.0 as usize].projection.as_ref()
    }

    pub(crate) fn normalization(&self, id: GoalId) -> Option<&GoalNormalization> {
        self.records[id.0 as usize].normalization.as_ref()
    }

    pub(crate) fn support(&self, id: GoalId) -> &[GoalSupport] {
        &self.records[id.0 as usize].support
    }

    pub(crate) fn ids(&self) -> impl Iterator<Item = GoalId> + '_ {
        (0..self.records.len()).map(|index| {
            GoalId(u32::try_from(index).expect("ENT goal inventory exceeds the u32 identity space"))
        })
    }

    pub(crate) fn into_inventory(self) -> Vec<RetainedGoal> {
        self.records
            .into_iter()
            .map(|record| RetainedGoal {
                expression: record.expression,
                projection: record.projection,
                normalization: record.normalization,
            })
            .collect()
    }
}

impl Relation {
    /// [ENT-3] S1 negation using the stored bound arithmetic convention.
    pub(crate) fn negated(&self) -> Self {
        match self {
            Self::Bound { left, right, bound } => Self::Bound {
                left: *right,
                right: *left,
                bound: bound.saturating_neg().saturating_sub(1),
            },
            Self::Equal {
                left,
                right,
                difference,
            } => Self::Distinct {
                left: *left,
                right: *right,
                difference: *difference,
            },
            Self::Distinct {
                left,
                right,
                difference,
            } => Self::Equal {
                left: *left,
                right: *right,
                difference: *difference,
            },
        }
    }

    /// Every term occurring in the relation, for kill support tests.
    pub(crate) fn terms(&self) -> [TermId; 2] {
        match self {
            Self::Bound { left, right, .. }
            | Self::Equal { left, right, .. }
            | Self::Distinct { left, right, .. } => [*left, *right],
        }
    }
}

/// One live fact state on the structural flow [ENT-3].
/// How much of a fact state's bound matrix is already its own [ENT-4] closure.
#[derive(Clone, Debug, Default)]
enum ClosureRecord {
    /// Nothing is known; the next closure starts from every live fact.
    #[default]
    Unknown,
    /// `bounds` and `distinct` are the complete closure over the first
    /// `terms` registered terms. S11 snapshots, pre-kill materialization and
    /// ENT-5 joins produce such states.
    Closed { terms: u32 },
    /// The closure over the first `terms` terms, except for the listed cells
    /// and every cell in a listed term's row or column. The remaining cells
    /// are closed among themselves: every transitive, strengthening and
    /// disequality consequence of two of them is already present and no
    /// weaker, and their terms' implicit bounds are already reflected.
    ///
    /// A fresh term has lost every explicit fact: it was killed or is newly
    /// registered. A weakened cell lost its selected proof candidate and now
    /// holds a weaker surviving selection, or none, so it may be weaker than
    /// the closure of the facts that remain.
    Core {
        terms: u32,
        fresh_terms: Vec<TermId>,
        fresh_cells: Vec<(TermId, TermId)>,
        weakened_cells: Vec<(TermId, TermId)>,
    },
}

impl ClosureRecord {
    fn closed(term_count: usize) -> Self {
        Self::Closed {
            terms: u32::try_from(term_count)
                .expect("ENT term inventory exceeds the u32 identity space"),
        }
    }

    fn is_closed_over(&self, term_count: usize) -> bool {
        matches!(self, Self::Closed { terms } if *terms as usize == term_count)
    }

    /// Turns a closed record into an empty core; returns whether the record
    /// is now a core that can take fresh marks.
    fn ensure_core(&mut self) -> bool {
        if let Self::Closed { terms } = *self {
            *self = Self::Core {
                terms,
                fresh_terms: Vec::new(),
                fresh_cells: Vec::new(),
                weakened_cells: Vec::new(),
            };
        }
        matches!(self, Self::Core { .. })
    }

    fn mark_fresh_cell(&mut self, cell: (TermId, TermId)) {
        if self.ensure_core()
            && let Self::Core { fresh_cells, .. } = self
        {
            fresh_cells.push(cell);
        }
    }

    fn mark_fresh_term(&mut self, term: TermId) {
        if self.ensure_core()
            && let Self::Core { fresh_terms, .. } = self
        {
            fresh_terms.push(term);
        }
    }

    fn mark_weakened_cell(&mut self, cell: (TermId, TermId)) {
        if self.ensure_core()
            && let Self::Core { weakened_cells, .. } = self
        {
            weakened_cells.push(cell);
        }
    }
}

/// The independently live proofs of one relation. Nearly every relation has
/// exactly one, held inline so cloning a fact state copies it without an
/// allocation; an empty list selects nothing.
#[derive(Clone, Debug)]
enum Candidates<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> Default for Candidates<T> {
    fn default() -> Self {
        Self::Many(Vec::new())
    }
}

impl<T: Copy + PartialEq> Candidates<T> {
    fn as_slice(&self) -> &[T] {
        match self {
            Self::One(value) => std::slice::from_ref(value),
            Self::Many(values) => values,
        }
    }

    fn contains(&self, value: &T) -> bool {
        self.as_slice().contains(value)
    }

    fn iter(&self) -> std::slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    fn len(&self) -> usize {
        self.as_slice().len()
    }

    fn push(&mut self, value: T) {
        match self {
            Self::Many(values) if values.is_empty() => *self = Self::One(value),
            Self::Many(values) => values.push(value),
            Self::One(first) => *self = Self::Many(vec![*first, value]),
        }
    }

    fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        match self {
            Self::One(value) => {
                if !keep(value) {
                    *self = Self::Many(Vec::new());
                }
            }
            Self::Many(values) => values.retain(keep),
        }
    }
}

impl<'a, T: Copy + PartialEq> IntoIterator for &'a Candidates<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The selected difference bounds of a fact state, dense over the terms that
/// have a cell.
///
/// Each such term has a slot, and slots follow term order, so row-major cells
/// are in sorted `(left, right)` order. The stride leaves room for more slots,
/// so a cell naming a term registered after the others, the usual new term,
/// rarely re-lays the store out. A term whose cells are all gone keeps its
/// slot until the store is next copied or re-laid out, which drops it.
/// A relation's selected candidate is its cell; any further independently live
/// candidates of the same pair are kept, in order, in `extra`.
#[derive(Debug, Default)]
pub(crate) struct BoundStore {
    /// Each term's slot by term identity, or [`NO_SLOT`].
    slots: Vec<u32>,
    /// Each slot's term, ascending.
    terms: Vec<TermId>,
    stride: usize,
    bounds: Vec<i128>,
    proofs: Vec<DerivationId>,
    present: Vec<bool>,
    live: usize,
    extra: WordHashMap<(TermId, TermId), Vec<(i128, DerivationId)>>,
}

impl Clone for BoundStore {
    /// The copy holds slots only for the terms that still have a cell.
    fn clone(&self) -> Self {
        let mut copy = Self {
            extra: self.extra.clone(),
            ..Self::default()
        };
        copy.lay_out_from(self, self.live_terms());
        copy
    }
}

impl BoundStore {
    fn slot(&self, term: TermId) -> Option<usize> {
        self.slots
            .get(term.0 as usize)
            .filter(|slot| **slot != NO_SLOT)
            .map(|slot| *slot as usize)
    }

    fn index(&self, left: TermId, right: TermId) -> Option<usize> {
        Some(self.slot(left)? * self.stride + self.slot(right)?)
    }

    /// The selected bound and proof of `left - right`, if any.
    pub(crate) fn get(&self, left: TermId, right: TermId) -> Option<(i128, DerivationId)> {
        let index = self.index(left, right)?;
        self.present[index].then(|| (self.bounds[index], self.proofs[index]))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Every selected bound in sorted `(left, right)` order.
    pub(crate) fn cells(&self) -> impl Iterator<Item = (TermId, TermId, i128, DerivationId)> + '_ {
        let count = self.terms.len();
        (0..count).flat_map(move |row| {
            let first = row * self.stride;
            (0..count).filter_map(move |column| {
                let index = first + column;
                self.present[index].then(|| {
                    (
                        self.terms[row],
                        self.terms[column],
                        self.bounds[index],
                        self.proofs[index],
                    )
                })
            })
        })
    }

    /// The terms with at least one cell, ascending.
    fn live_terms(&self) -> Vec<TermId> {
        let count = self.terms.len();
        let mut live = vec![false; count];
        for row in 0..count {
            for column in 0..count {
                if self.present[row * self.stride + column] {
                    live[row] = true;
                    live[column] = true;
                }
            }
        }
        self.terms
            .iter()
            .zip(live)
            .filter_map(|(term, live)| live.then_some(*term))
            .collect()
    }

    /// Lays this store out with a slot for each of `terms`, ascending, and
    /// copies `source`'s cells among them.
    fn lay_out_from(&mut self, source: &Self, terms: Vec<TermId>) {
        let stride = (terms.len() + terms.len() / 2).max(16);
        let count = stride
            .checked_mul(stride)
            .expect("ENT bound store exceeds the address space");
        let mut bounds = vec![0; count];
        let mut proofs = vec![DerivationId(0); count];
        let mut present = vec![false; count];
        let width = terms.last().map_or(0, |term| term.0 as usize + 1);
        let mut slots = vec![NO_SLOT; width];
        for (slot, term) in terms.iter().enumerate() {
            slots[term.0 as usize] = u32::try_from(slot).expect("slot fits the u32 identity");
        }
        let sources = terms
            .iter()
            .map(|term| source.slot(*term))
            .collect::<Vec<_>>();
        let mut live = 0;
        for (row, source_row) in sources.iter().enumerate() {
            let Some(source_row) = source_row else {
                continue;
            };
            for (column, source_column) in sources.iter().enumerate() {
                let Some(source_column) = source_column else {
                    continue;
                };
                let from = source_row * source.stride + source_column;
                if source.present[from] {
                    let to = row * stride + column;
                    bounds[to] = source.bounds[from];
                    proofs[to] = source.proofs[from];
                    present[to] = true;
                    live += 1;
                }
            }
        }
        self.slots = slots;
        self.terms = terms;
        self.stride = stride;
        self.bounds = bounds;
        self.proofs = proofs;
        self.present = present;
        self.live = live;
    }

    /// Gives both terms of a pair a slot: appended when the new ones follow
    /// every slotted term and the stride has room, otherwise by re-laying the
    /// store out over its live terms and these two.
    fn slots_for(&mut self, left: TermId, right: TermId) {
        let mut missing = [left, right]
            .into_iter()
            .filter(|term| self.slot(*term).is_none())
            .collect::<Vec<_>>();
        missing.sort_unstable();
        missing.dedup();
        let Some(first) = missing.first() else {
            return;
        };
        if self.terms.last().is_none_or(|last| last < first)
            && self.terms.len() + missing.len() <= self.stride
        {
            for term in missing {
                let index = term.0 as usize;
                if index >= self.slots.len() {
                    self.slots.resize(index + 1, NO_SLOT);
                }
                self.slots[index] =
                    u32::try_from(self.terms.len()).expect("slot fits the u32 identity");
                self.terms.push(term);
            }
            return;
        }
        let mut terms = self.live_terms();
        for term in [left, right] {
            if let Err(position) = terms.binary_search(&term) {
                terms.insert(position, term);
            }
        }
        let mut source = std::mem::take(self);
        self.extra = std::mem::take(&mut source.extra);
        self.lay_out_from(&source, terms);
    }

    /// Every independently live candidate of one pair, the selected one first.
    fn candidates(&self, pair: (TermId, TermId)) -> Vec<(i128, DerivationId)> {
        let mut candidates = self.get(pair.0, pair.1).into_iter().collect::<Vec<_>>();
        if let Some(extra) = self.extra.get(&pair) {
            candidates.extend(extra.iter().copied());
        }
        candidates
    }

    fn contains_candidate(&self, pair: (TermId, TermId), candidate: (i128, DerivationId)) -> bool {
        self.get(pair.0, pair.1) == Some(candidate)
            || self
                .extra
                .get(&pair)
                .is_some_and(|extra| extra.contains(&candidate))
    }

    /// Adds one candidate. The selection is the least candidate by bound and
    /// then proof, so only the new candidate and the current selection
    /// compete; the loser is kept among the other candidates.
    fn add_candidate(
        &mut self,
        pair: (TermId, TermId),
        candidate: (i128, DerivationId),
        ledger: &DerivationLedger,
    ) {
        let Some(selected) = self.get(pair.0, pair.1) else {
            self.store_single(pair.0, pair.1, candidate.0, candidate.1);
            return;
        };
        if self.contains_candidate(pair, candidate) {
            return;
        }
        let preferred = candidate.0 < selected.0
            || (candidate.0 == selected.0 && ledger.better(candidate.1, selected.1));
        let displaced = if preferred {
            let index = self
                .index(pair.0, pair.1)
                .expect("a selected pair is in range");
            self.bounds[index] = candidate.0;
            self.proofs[index] = candidate.1;
            selected
        } else {
            candidate
        };
        self.extra.entry(pair).or_default().push(displaced);
    }

    /// The smallest bound among a pair's candidates whose proof passes `test`.
    pub(crate) fn candidate_minimum(
        &self,
        pair: (TermId, TermId),
        mut test: impl FnMut(DerivationId) -> bool,
    ) -> Option<i128> {
        let selected = self
            .get(pair.0, pair.1)
            .filter(|(_, proof)| test(*proof))
            .map(|(bound, _)| bound);
        let extra = self.extra.get(&pair).and_then(|extra| {
            extra
                .iter()
                .filter(|(_, proof)| test(*proof))
                .map(|(bound, _)| *bound)
                .min()
        });
        selected.into_iter().chain(extra).min()
    }

    /// Keeps a pair's candidates that pass `keep`, in place. A surviving
    /// selection stays selected: no other candidate was preferred to it.
    /// Otherwise the best surviving candidate, in candidate order, is selected.
    fn retain_candidates(
        &mut self,
        pair: (TermId, TermId),
        mut keep: impl FnMut((i128, DerivationId)) -> bool,
        ledger: &DerivationLedger,
    ) {
        let selected_kept = self.get(pair.0, pair.1).is_some_and(&mut keep);
        let mut extra = self.extra.remove(&pair).unwrap_or_default();
        extra.retain(|candidate| keep(*candidate));
        if !selected_kept {
            let best = extra
                .iter()
                .copied()
                .enumerate()
                .reduce(|current, candidate| {
                    if candidate.1.0 < current.1.0
                        || (candidate.1.0 == current.1.0
                            && ledger.better(candidate.1.1, current.1.1))
                    {
                        candidate
                    } else {
                        current
                    }
                });
            match best {
                Some((position, (bound, proof))) => {
                    extra.remove(position);
                    let index = self
                        .index(pair.0, pair.1)
                        .expect("a selected pair is in range");
                    self.bounds[index] = bound;
                    self.proofs[index] = proof;
                }
                None => {
                    self.clear(pair);
                    return;
                }
            }
        }
        if !extra.is_empty() {
            self.extra.insert(pair, extra);
        }
    }

    /// Selects a pair's only candidate.
    fn store_single(&mut self, left: TermId, right: TermId, bound: i128, proof: DerivationId) {
        self.slots_for(left, right);
        let index = self.index(left, right).expect("both terms have slots");
        if !self.present[index] {
            self.live += 1;
        }
        self.bounds[index] = bound;
        self.proofs[index] = proof;
        self.present[index] = true;
        // A fresh store has no further candidates; skipping the probe keeps
        // building a materialized closure free of one hash lookup per cell.
        if !self.extra.is_empty() {
            self.extra.remove(&(left, right));
        }
    }

    fn clear(&mut self, pair: (TermId, TermId)) {
        if let Some(index) = self.index(pair.0, pair.1)
            && self.present[index]
        {
            self.present[index] = false;
            self.live -= 1;
        }
        self.extra.remove(&pair);
    }
}

/// A remembered closure of one fact state.
#[derive(Clone)]
struct ClosedView {
    key: ClosedViewKey,
    closed: Rc<ClosedState>,
}

impl std::fmt::Debug for ClosedView {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClosedView")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

/// The tables a closure read, by identity and revision. Derivation ledgers
/// only grow while an analysis runs, so proofs a view names stay valid in the
/// same ledger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ClosedViewKey {
    terms: usize,
    term_revision: usize,
    term_count: usize,
    measure_replacements: usize,
    goals: usize,
    goal_revision: usize,
    ledger: usize,
}

/// The remembered closed view of a state with no closure record, kept while
/// the state only gains relations: the view is the closure of the state's
/// facts when it was taken, so the closure now is that view with the gained
/// bound cells and any later term's implicit bounds inserted as edges.
#[derive(Clone)]
struct ViewSeed {
    /// The term table and derivation ledger the view read, by identity, and
    /// the term table's standing-measure replacements then. Terms registered
    /// since are fresh; a replaced standing fact could weaken an implicit
    /// bound the view already used, so it ends the seed.
    terms: usize,
    ledger: usize,
    measure_replacements: usize,
    closed: Rc<ClosedState>,
    /// Cells whose selected bound has become strictly smaller since the
    /// view was taken.
    fresh_cells: Vec<(TermId, TermId)>,
}

impl std::fmt::Debug for ViewSeed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ViewSeed")
            .field("fresh_cells", &self.fresh_cells)
            .finish_non_exhaustive()
    }
}

/// One bound cell `left - right <= bound` an [ENT-5] kill removed, with its
/// proof and which of its two terms the kill reached.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct KilledCell {
    pub(crate) left: TermId,
    pub(crate) right: TermId,
    pub(crate) bound: i128,
    pub(crate) proof: DerivationId,
    pub(crate) left_killed: bool,
    pub(crate) right_killed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct FactState {
    /// What part of the bound matrix is already closed. Only a state closed
    /// over the current term universe may answer a query without closing
    /// again; a closed core lets the next closure start from its fresh part.
    closure: ClosureRecord,
    /// The same record for the ordinary layer: the selection each pair would
    /// have without any proof that depends on a postcondition call. Removing
    /// every such candidate leaves exactly that layer, so the removal can
    /// take this record instead of rederiving the weakened cells.
    ordinary_closure: ClosureRecord,
    /// The closed view most recently taken of exactly this content. A clone
    /// copies the cell and shares the view, so either copy's later change
    /// clears only its own. Every method that changes a relation, a signed goal or the
    /// contradiction clears it; its key names the term, goal and derivation
    /// tables and their revisions, so a registered term or goal also misses.
    closed_view: std::cell::RefCell<Option<ClosedView>>,
    /// While the closure record is unknown and the state has only gained
    /// bound relations since a closed view was taken, that view, as the
    /// starting point of the next closure. Any other change ends it.
    view_seed: Option<ViewSeed>,
    /// Whether some bound or disequality candidate may depend on a
    /// postcondition call. A state without one has nothing for a
    /// postcondition-candidate removal to remove.
    postcondition_candidates: bool,
    /// The loop rule's empty join: the contradictory all-derivable state, in
    /// which every relation is derivable and every fact is present. Z has
    /// empty support, so `Z - Z <= -1` never dies and the flag is absorbing
    /// under kills [ENT-4, ENT-5].
    pub(crate) all_derivable: bool,
    /// Exact reason the state is all-derivable. Every transition that sets
    /// the flag sets this handle at the same time.
    pub(crate) contradiction: Option<DerivationId>,
    /// Live difference bounds `left - right <= bound`, smallest bound kept.
    pub(crate) bounds: Rc<BoundStore>,
    /// Live disequalities, stored with canonical endpoints and displacement.
    pub(crate) distinct: Rc<WordHashSet<DistinctKey>>,
    pub(crate) distinct_proofs: Rc<WordHashMap<DistinctKey, DerivationId>>,
    /// Independently live disequality proofs, parallel to `bound_candidates`.
    distinct_candidates: Rc<WordHashMap<DistinctKey, Candidates<DerivationId>>>,
    /// [ENT-3] comparison origins (b): `own Bool` bindings that hold their
    /// initializer comparison here — no [ENT-5] event has reached the binding
    /// or the comparison's operands on any path from the initializer.
    pub(crate) origins: HashMap<BindingId, Relation>,
    /// Live exact signed whole-goal facts [ENT-2..ENT-4].
    pub(crate) opaque: WordHashSet<(GoalId, GoalSign)>,
    pub(crate) opaque_proofs: WordHashMap<(GoalId, GoalSign), DerivationId>,
    /// Still-valid admitted ordinary-let definitions and their introductions.
    pub(crate) goal_origins: HashMap<BindingId, GoalOrigin>,
    /// Bool value initializers with two or more distinct live source goals.
    /// Such a receiver has no unique source-goal expansion.
    pub(crate) ambiguous_goal_origins: HashSet<BindingId>,
}

impl Default for FactState {
    fn default() -> Self {
        Self::new()
    }
}

impl FactState {
    pub(crate) fn new() -> Self {
        Self {
            closure: ClosureRecord::Unknown,
            ordinary_closure: ClosureRecord::Unknown,
            closed_view: std::cell::RefCell::new(None),
            view_seed: None,
            postcondition_candidates: false,
            all_derivable: false,
            contradiction: None,
            bounds: Rc::default(),
            distinct: Rc::default(),
            distinct_proofs: Rc::default(),
            distinct_candidates: Rc::default(),
            origins: HashMap::default(),
            opaque: HashSet::default(),
            opaque_proofs: HashMap::default(),
            goal_origins: HashMap::default(),
            ambiguous_goal_origins: HashSet::default(),
        }
    }

    /// Makes the state the absorbing contradiction proved by `contradiction`.
    pub(crate) fn promote_to_contradiction(&mut self, contradiction: Option<DerivationId>) {
        self.forget_closed_view();
        self.all_derivable = true;
        self.contradiction = contradiction;
    }

    /// Drops the remembered closed view and any seed: the change about to be
    /// made can remove or weaken a relation, or add a disequality, which a
    /// seed does not carry forward.
    fn forget_closed_view(&mut self) {
        self.closed_view.take();
        self.view_seed = None;
    }

    /// Drops the remembered closed view, keeping it as the seed of the next
    /// closure while the closure record is unknown; `fresh` is a bound cell
    /// about to become strictly smaller.
    fn keep_view_as_seed(&mut self, fresh: Option<(TermId, TermId)>) {
        let view = self.closed_view.take();
        if !matches!(self.closure, ClosureRecord::Unknown) {
            self.view_seed = None;
            return;
        }
        if let Some(view) = view
            && !view.closed.all_derivable
        {
            self.view_seed = Some(ViewSeed {
                terms: view.key.terms,
                ledger: view.key.ledger,
                measure_replacements: view.key.measure_replacements,
                closed: view.closed,
                fresh_cells: Vec::new(),
            });
        }
        if let (Some(seed), Some(cell)) = (&mut self.view_seed, fresh) {
            seed.fresh_cells.push(cell);
        }
    }

    pub(crate) fn contradictory(contradiction: DerivationId) -> Self {
        Self {
            all_derivable: true,
            contradiction: Some(contradiction),
            ..Self::new()
        }
    }

    /// Establishes one normalized source bound and returns that exact source
    /// parent even when a stronger fact was already live. S11 uses this at a
    /// true body entry so the retained root names the executed header proof
    /// point instead of whichever equivalent bound won fact-state
    /// canonicalization.
    pub(crate) fn establish_bound_with_proof(
        &mut self,
        left: TermId,
        right: TermId,
        bound: i128,
        ledger: &mut DerivationLedger,
        event: FlowEventId,
    ) -> DerivationId {
        let relation = Relation::Bound { left, right, bound };
        let proof = ledger.intern(DerivationNode::SourceBound {
            relation,
            left,
            right,
            bound,
            event,
        });
        if !self.all_derivable {
            self.add_bound(left, right, bound, proof, ledger);
        }
        proof
    }

    /// Returns the live parent proving this directed bound without rerunning
    /// closure. Immediately after S11 materializes its preheader snapshot,
    /// this is either the exact materialized bound or the materialized
    /// contradiction that proves every requested relation.
    pub(crate) fn bound_parent(
        &self,
        left: TermId,
        right: TermId,
        requested: i128,
    ) -> Option<DerivationId> {
        if self.all_derivable {
            return self.contradiction;
        }
        self.bounds
            .get(left, right)
            .filter(|(held, _)| *held <= requested)
            .map(|(_, proof)| proof)
    }

    pub(crate) fn establish(
        &mut self,
        relation: &Relation,
        ledger: &mut DerivationLedger,
        event: FlowEventId,
    ) {
        if self.all_derivable {
            return;
        }
        match relation {
            Relation::Bound { left, right, bound } => {
                self.establish_bound_with_proof(*left, *right, *bound, ledger, event);
            }
            Relation::Equal {
                left,
                right,
                difference,
            } => {
                let forward = ledger.intern(DerivationNode::SourceBound {
                    relation: relation.clone(),
                    left: *left,
                    right: *right,
                    bound: *difference,
                    event,
                });
                self.add_bound(*left, *right, *difference, forward, ledger);
                let reverse = ledger.intern(DerivationNode::SourceBound {
                    relation: relation.clone(),
                    left: *right,
                    right: *left,
                    bound: difference.saturating_neg(),
                    event,
                });
                self.add_bound(*right, *left, difference.saturating_neg(), reverse, ledger);
            }
            Relation::Distinct {
                left,
                right,
                difference,
            } => {
                self.establish_distinct_with_proof(*left, *right, *difference, ledger, event);
            }
        }
    }

    /// Installs an already-validated caller-local S12 proof as an ordinary
    /// live L0 fact. The specialized route node is itself the proof of every
    /// normalized component; no second source event or proof authority is
    /// manufactured here.
    pub(crate) fn establish_from_proof(
        &mut self,
        relation: &Relation,
        proof: DerivationId,
        ledger: &DerivationLedger,
    ) {
        if self.all_derivable {
            return;
        }
        match relation {
            Relation::Bound { left, right, bound } => {
                self.add_bound(*left, *right, *bound, proof, ledger);
            }
            Relation::Equal {
                left,
                right,
                difference,
            } => {
                self.add_bound(*left, *right, *difference, proof, ledger);
                self.add_bound(*right, *left, difference.saturating_neg(), proof, ledger);
            }
            Relation::Distinct {
                left,
                right,
                difference,
            } => {
                let key = distinct_key(*left, *right, *difference);
                self.add_distinct_candidate(key, proof, ledger);
            }
        }
    }

    /// The numeric part of a materialized snapshot, retaining its completed
    /// closure and shared stores. Opaque goals and writer-origin metadata do
    /// not travel with a Result. Their numeric consequences have already been
    /// materialized by the caller, so removing that metadata does not make
    /// the retained numeric core incomplete.
    pub(crate) fn numeric_snapshot(&self) -> Self {
        Self {
            closure: self.closure.clone(),
            ordinary_closure: self.ordinary_closure.clone(),
            postcondition_candidates: self.postcondition_candidates,
            all_derivable: self.all_derivable,
            contradiction: self.contradiction,
            bounds: Rc::clone(&self.bounds),
            distinct: Rc::clone(&self.distinct),
            distinct_proofs: Rc::clone(&self.distinct_proofs),
            distinct_candidates: Rc::clone(&self.distinct_candidates),
            ..Self::new()
        }
    }

    /// Size of the recorded numeric core, before its fresh and weakened
    /// cells are closed again. This selects a reuse opportunity, not a fact
    /// or a bound on the amount of closure work that remains.
    pub(crate) fn numeric_core_terms(&self) -> u32 {
        match self.closure {
            ClosureRecord::Unknown => 0,
            ClosureRecord::Closed { terms } | ClosureRecord::Core { terms, .. } => terms,
        }
    }

    /// Deterministic normalized live L0 facts and their canonical proofs.
    /// This excludes opaque goals and origin metadata.
    pub(crate) fn live_l0_relations(&self) -> Vec<(Relation, DerivationId)> {
        if self.all_derivable {
            return Vec::new();
        }
        let mut relations = self
            .bounds
            .cells()
            .map(|(left, right, bound, proof)| (Relation::Bound { left, right, bound }, proof))
            .collect::<Vec<_>>();
        let mut distinct = self.distinct.iter().copied().collect::<Vec<_>>();
        distinct.sort_unstable();
        relations.extend(distinct.into_iter().map(|(left, right, difference)| {
            (
                Relation::Distinct {
                    left,
                    right,
                    difference,
                },
                self.distinct_proofs[&(left, right, difference)],
            )
        }));
        relations
    }

    /// Every independently live numeric candidate, including the ordinary
    /// fallback behind a stronger call-dependent bound. Value transport must
    /// preserve both when their later support kills differ.
    pub(crate) fn l0_candidates(&self) -> Vec<(Relation, DerivationId)> {
        if self.all_derivable {
            return Vec::new();
        }
        let mut relations = Vec::new();
        for (left, right, _, _) in self.bounds.cells() {
            for (bound, parent) in self.bounds.candidates((left, right)) {
                relations.push((Relation::Bound { left, right, bound }, parent));
            }
        }
        let mut pairs = self.distinct_candidates.keys().copied().collect::<Vec<_>>();
        pairs.sort_unstable();
        for (left, right, difference) in pairs {
            for parent in &self.distinct_candidates[&(left, right, difference)] {
                relations.push((
                    Relation::Distinct {
                        left,
                        right,
                        difference,
                    },
                    *parent,
                ));
            }
        }
        relations
    }

    pub(crate) fn establish_goal(
        &mut self,
        goal: GoalId,
        sign: GoalSign,
        ledger: &mut DerivationLedger,
        event: FlowEventId,
    ) {
        let _ = self.establish_goal_with_proof(goal, sign, ledger, event);
    }

    pub(crate) fn establish_goal_with_proof(
        &mut self,
        goal: GoalId,
        sign: GoalSign,
        ledger: &mut DerivationLedger,
        event: FlowEventId,
    ) -> DerivationId {
        let fact = (goal, sign);
        // A signed goal decides no bound or disequality, so the closed view
        // stays a seed of the next closure.
        self.keep_view_as_seed(None);
        let proof = ledger.intern(DerivationNode::SourceGoal { goal, sign, event });
        if !self.all_derivable
            && (self.opaque.insert(fact) || ledger.better(proof, self.opaque_proofs[&fact]))
        {
            self.opaque_proofs.insert(fact, proof);
        }
        proof
    }

    /// A query-local derived sign. It is not another source establishment.
    pub(crate) fn establish_derived_goal(
        &mut self,
        goal: GoalId,
        sign: GoalSign,
        proof: DerivationId,
    ) {
        self.keep_view_as_seed(None);
        if !self.all_derivable && self.opaque.insert((goal, sign)) {
            self.opaque_proofs.insert((goal, sign), proof);
        }
    }

    pub(crate) fn establish_distinct_with_proof(
        &mut self,
        left: TermId,
        right: TermId,
        difference: i128,
        ledger: &mut DerivationLedger,
        event: FlowEventId,
    ) -> DerivationId {
        let pair = distinct_key(left, right, difference);
        let proof = ledger.intern(DerivationNode::SourceDistinct {
            left: pair.0,
            right: pair.1,
            difference: pair.2,
            event,
        });
        if !self.all_derivable {
            self.add_distinct_candidate(pair, proof, ledger);
        }
        proof
    }

    fn selected_relations_depend_on_postcondition_call(&self, ledger: &DerivationLedger) -> bool {
        self.bounds
            .cells()
            .map(|(_, _, _, proof)| proof)
            .chain(self.distinct_proofs.values().copied())
            .any(|proof| ledger.depends_on_postcondition_call(proof))
    }

    fn add_bound(
        &mut self,
        left: TermId,
        right: TermId,
        bound: i128,
        proof: DerivationId,
        ledger: &DerivationLedger,
    ) {
        let pair = (left, right);
        if self.bounds.contains_candidate(pair, (bound, proof)) {
            return;
        }
        // A new proof candidate must remain available to later support kills,
        // but only a stronger numeric bound changes this layer's closure.
        // Result transport routinely imports already-known ordinary bounds.
        let stronger = self
            .bounds
            .get(left, right)
            .is_none_or(|(old, _)| bound < old);
        self.keep_view_as_seed(stronger.then_some(pair));
        if stronger {
            self.closure.mark_fresh_cell(pair);
        }
        if ledger.depends_on_postcondition_call(proof) {
            self.postcondition_candidates = true;
        } else if self
            .bounds
            .candidate_minimum(pair, |parent| !ledger.depends_on_postcondition_call(parent))
            .is_none_or(|old| bound < old)
        {
            self.ordinary_closure.mark_fresh_cell(pair);
        }
        Rc::make_mut(&mut self.bounds).add_candidate((left, right), (bound, proof), ledger);
    }

    fn add_distinct_candidate(
        &mut self,
        pair: DistinctKey,
        proof: DerivationId,
        ledger: &DerivationLedger,
    ) {
        if self
            .distinct_candidates
            .get(&pair)
            .is_some_and(|candidates| candidates.contains(&proof))
        {
            return;
        }
        self.forget_closed_view();
        if !self.distinct.contains(&pair) {
            self.closure.mark_fresh_cell((pair.0, pair.1));
            self.closure.mark_fresh_cell((pair.1, pair.0));
        }
        if ledger.depends_on_postcondition_call(proof) {
            self.postcondition_candidates = true;
        } else if !self
            .distinct_candidates
            .get(&pair)
            .is_some_and(|candidates| {
                candidates
                    .iter()
                    .any(|parent| !ledger.depends_on_postcondition_call(*parent))
            })
        {
            self.ordinary_closure.mark_fresh_cell((pair.0, pair.1));
            self.ordinary_closure.mark_fresh_cell((pair.1, pair.0));
        }
        let candidates = Rc::make_mut(&mut self.distinct_candidates)
            .entry(pair)
            .or_default();
        if !candidates.contains(&proof) {
            candidates.push(proof);
        }
        self.select_distinct_candidate(pair, ledger);
    }

    fn select_distinct_candidate(&mut self, pair: DistinctKey, ledger: &DerivationLedger) {
        let selected = self.distinct_candidates.get(&pair).and_then(|candidates| {
            candidates.iter().copied().reduce(|current, candidate| {
                if ledger.better(candidate, current) {
                    candidate
                } else {
                    current
                }
            })
        });
        if let Some(proof) = selected {
            Rc::make_mut(&mut self.distinct).insert(pair);
            Rc::make_mut(&mut self.distinct_proofs).insert(pair, proof);
        } else {
            Rc::make_mut(&mut self.distinct).remove(&pair);
            Rc::make_mut(&mut self.distinct_proofs).remove(&pair);
            Rc::make_mut(&mut self.distinct_candidates).remove(&pair);
        }
    }

    /// Removes every live fact and origin with a support member the kill
    /// predicate reaches. Flow callers materialize the complete closure before
    /// invoking this endpoint filter, so survivor-only consequences remain
    /// independently live and closure never resurrects a killed fact.
    ///
    /// The projection is not closure-preserving even so. The [ENT-2] implicit
    /// facts of a killed term are a function of the term table and the place's
    /// type alone, so they hold again the instant the kill is done and can
    /// carry survivors to conclusions the projected map no longer lists. The
    /// state therefore stops being a witness of its own closure here. Removing
    /// whole rows and columns keeps the surviving cells closed among
    /// themselves, so a closed state keeps that core and marks the killed
    /// terms fresh for the next closure to rebuild.
    pub(crate) fn kill(&mut self, mut predicate: impl FnMut(TermId) -> bool) -> Vec<KilledCell> {
        if self.all_derivable {
            return Vec::new();
        }
        self.forget_closed_view();
        // The predicate depends only on the term, and matrix-sized key scans
        // would otherwise call it twice per cell.
        let mut verdicts: Vec<Option<bool>> = Vec::new();
        let mut killed = |term: TermId| {
            let index = term.0 as usize;
            if index >= verdicts.len() {
                verdicts.resize(index + 1, None);
            }
            *verdicts[index].get_or_insert_with(|| predicate(term))
        };
        let mut removed = Vec::new();
        for (left, right, bound, proof) in self.bounds.cells() {
            let left_killed = killed(left);
            let right_killed = killed(right);
            if left_killed || right_killed {
                removed.push(KilledCell {
                    left,
                    right,
                    bound,
                    proof,
                    left_killed,
                    right_killed,
                });
            }
        }
        let dead: Vec<(TermId, TermId)> =
            removed.iter().map(|cell| (cell.left, cell.right)).collect();
        let mut dead_terms = dead
            .iter()
            .flat_map(|(left, right)| [*left, *right])
            .chain(
                self.distinct
                    .iter()
                    .flat_map(|(left, right, _)| [*left, *right]),
            )
            .filter(|term| killed(*term))
            .collect::<Vec<_>>();
        dead_terms.sort_unstable();
        dead_terms.dedup();
        for term in dead_terms {
            self.closure.mark_fresh_term(term);
            self.ordinary_closure.mark_fresh_term(term);
        }
        if !dead.is_empty() {
            let bounds = Rc::make_mut(&mut self.bounds);
            for pair in dead {
                bounds.clear(pair);
            }
        }
        // Shared relation maps are copied only when this kill changes them.
        if self
            .distinct_candidates
            .keys()
            .chain(self.distinct.iter())
            .any(|(left, right, _)| killed(*left) || killed(*right))
        {
            Rc::make_mut(&mut self.distinct)
                .retain(|(left, right, _)| !killed(*left) && !killed(*right));
            Rc::make_mut(&mut self.distinct_proofs)
                .retain(|(left, right, _), _| !killed(*left) && !killed(*right));
            Rc::make_mut(&mut self.distinct_candidates)
                .retain(|(left, right, _), _| !killed(*left) && !killed(*right));
        }
        self.origins.retain(|_, relation| {
            let [left, right] = relation.terms();
            !killed(left) && !killed(right)
        });
        removed
    }

    /// [DIAG-1] puts back bound cells a kill removed, each with the proof it
    /// had, so a failed judgment can ask whether those cells alone would
    /// have discharged it. Only a diagnostic copy of a state is restored; no
    /// judgment that decides acceptance reads one.
    pub(crate) fn restore_killed(&mut self, cells: &[KilledCell], ledger: &DerivationLedger) {
        for cell in cells {
            self.add_bound(cell.left, cell.right, cell.bound, cell.proof, ledger);
        }
    }

    /// Removes only proof candidates invalidated by an S12-private holder
    /// event, then deterministically exposes the best surviving ordinary or
    /// S12 proof. Ordinary ENT-5 term kills continue to delete whole pairs.
    pub(crate) fn kill_proof_candidates(
        &mut self,
        ledger: &DerivationLedger,
        mut killed: impl FnMut(TermId, TermId, DerivationId) -> bool,
    ) -> bool {
        if self.all_derivable {
            return false;
        }
        self.forget_closed_view();
        let mut changed = false;
        let mut weakened = Vec::new();
        // Each pair's selection depends only on its own candidates, and the
        // weakened record is sorted before use, so no iteration order is
        // observable here.
        // Only pairs with a removed candidate are touched, so shared relation
        // maps are copied only when a candidate actually dies.
        let mut bound_pairs = self
            .bounds
            .cells()
            .filter(|(left, right, _, proof)| killed(*left, *right, *proof))
            .map(|(left, right, _, _)| (left, right))
            .collect::<Vec<_>>();
        bound_pairs.extend(
            self.bounds
                .extra
                .iter()
                .filter(|(pair, extra)| {
                    extra
                        .iter()
                        .any(|(_, proof)| killed(pair.0, pair.1, *proof))
                })
                .map(|(pair, _)| *pair),
        );
        bound_pairs.sort_unstable();
        bound_pairs.dedup();
        let ordinary = |proof: DerivationId| !ledger.depends_on_postcondition_call(proof);
        let mut ordinary_weakened = Vec::new();
        for pair in bound_pairs {
            let ordinary_before = self.bounds.candidate_minimum(pair, ordinary);
            let held = self.bounds.get(pair.0, pair.1).map(|(bound, _)| bound);
            changed = true;
            Rc::make_mut(&mut self.bounds).retain_candidates(
                pair,
                |(_, proof)| !killed(pair.0, pair.1, proof),
                ledger,
            );
            if self.bounds.candidate_minimum(pair, ordinary) != ordinary_before {
                ordinary_weakened.push(pair);
            }
            if self.bounds.get(pair.0, pair.1).map(|(bound, _)| bound) != held {
                weakened.push(pair);
            }
        }
        let distinct_pairs = self
            .distinct_candidates
            .iter()
            .filter(|(pair, candidates)| {
                candidates
                    .iter()
                    .any(|proof| killed(pair.0, pair.1, *proof))
            })
            .map(|(pair, _)| *pair)
            .collect::<Vec<_>>();
        for pair in distinct_pairs {
            let candidates = Rc::make_mut(&mut self.distinct_candidates)
                .get_mut(&pair)
                .expect("candidate key came from the same map");
            let ordinary = |candidates: &Candidates<DerivationId>| {
                candidates
                    .iter()
                    .any(|proof| !ledger.depends_on_postcondition_call(*proof))
            };
            let ordinary_before = ordinary(candidates);
            let before = candidates.len();
            candidates.retain(|proof| !killed(pair.0, pair.1, *proof));
            if ordinary(candidates) != ordinary_before {
                ordinary_weakened.push((pair.0, pair.1));
            }
            if candidates.len() != before {
                changed = true;
                self.select_distinct_candidate(pair, ledger);
                if !self.distinct.contains(&pair) {
                    weakened.push((pair.0, pair.1));
                }
            }
        }
        // Candidate removal is not an endpoint projection. A surviving
        // ordinary and S12 candidate can rederive a relation whose one
        // retained materialized proof was just removed. A selection that only
        // changed its proof keeps every bound; a weaker or missing one is
        // recorded so the next closure can rederive the cell from its
        // neighbours. A removed disequality weakens both orientations.
        for (left, right) in weakened {
            self.closure.mark_weakened_cell((left, right));
            self.closure.mark_weakened_cell((right, left));
        }
        for (left, right) in ordinary_weakened {
            self.ordinary_closure.mark_weakened_cell((left, right));
            self.ordinary_closure.mark_weakened_cell((right, left));
        }
        changed
    }

    /// Whether a removal of postcondition-dependent candidates can change
    /// this state.
    pub(crate) fn may_hold_postcondition_candidates(&self) -> bool {
        self.postcondition_candidates
    }

    pub(crate) fn retain_non_postcondition_candidates(
        &mut self,
        ledger: &DerivationLedger,
    ) -> bool {
        let changed = self.postcondition_candidates
            && self.kill_proof_candidates(ledger, |_, _, proof| {
                ledger.depends_on_postcondition_call(proof)
            });
        self.postcondition_candidates = false;
        // Every remaining selection is now its ordinary selection.
        self.closure = self.ordinary_closure.clone();
        self.view_seed = None;
        changed
    }

    /// Adds `other`'s candidates where this state's selection depends on a
    /// postcondition call. Any other selection is derivable without such
    /// calls and so already equals the ordinary view `other` holds.
    pub(crate) fn merge_relation_candidates_from(
        &mut self,
        other: &Self,
        ledger: &DerivationLedger,
    ) {
        self.merge_fallback_candidates(other, true, ledger);
    }

    /// [`Self::merge_relation_candidates_from`] for a pair this state holds,
    /// or also for one it does not when `absent` is set. A join leaves out a
    /// pair whose term it reads through zero, which an ordinary candidate
    /// could only weaken.
    fn merge_fallback_candidates(&mut self, other: &Self, absent: bool, ledger: &DerivationLedger) {
        let needs_fallback = |proof: Option<&DerivationId>| match proof {
            Some(proof) => ledger.depends_on_postcondition_call(*proof),
            None => absent,
        };
        let bound_pairs = other
            .bounds
            .cells()
            .map(|(left, right, _, _)| (left, right))
            .filter(|pair| {
                needs_fallback(
                    self.bounds
                        .get(pair.0, pair.1)
                        .map(|(_, proof)| proof)
                        .as_ref(),
                )
            })
            .collect::<Vec<_>>();
        for pair in bound_pairs {
            for (bound, proof) in other.bounds.candidates(pair) {
                self.add_bound(pair.0, pair.1, bound, proof, ledger);
            }
        }
        let mut distinct_pairs = other
            .distinct_candidates
            .keys()
            .copied()
            .filter(|pair| needs_fallback(self.distinct_proofs.get(pair)))
            .collect::<Vec<_>>();
        distinct_pairs.sort_unstable();
        for pair in distinct_pairs {
            for proof in &other.distinct_candidates[&pair] {
                self.add_distinct_candidate(pair, *proof, ledger);
            }
        }
    }

    /// Removes signed facts and ordinary-let origin expansions whose exact
    /// goal support is invalidated by one ENT-5 event. The L0 matrix and its
    /// closure record are unchanged: goal contradictions are recomputed by
    /// every closure.
    pub(crate) fn kill_goals(&mut self, mut killed: impl FnMut(GoalId) -> bool) {
        if self.all_derivable {
            return;
        }
        self.keep_view_as_seed(None);
        self.opaque.retain(|(goal, _)| !killed(*goal));
        self.opaque_proofs.retain(|(goal, _), _| !killed(*goal));
        self.goal_origins.retain(|_, origin| !killed(origin.goal));
    }
}

/// One offset disequality, with endpoint reversal negating its offset.
/// Stored arithmetic follows the saturating i128 convention of difference
/// bounds, including reversal at MIN. This is not an unbounded integer
/// representation; the source-folding limit is tracked in status board item
/// lm-bl-i128-constants.
pub(crate) type DistinctKey = (TermId, TermId, i128);

pub(crate) fn distinct_key(left: TermId, right: TermId, difference: i128) -> DistinctKey {
    if left <= right {
        (left, right, difference)
    } else {
        (right, left, difference.saturating_neg())
    }
}

/// Concrete constant operands fold through Z [ENT-2]. A zero-offset pair
/// containing a retained constant term would bypass ENT-5's nonzero-candidate
/// boundary, so it must not be materialized as a zero-offset disequality.
pub(crate) fn zero_distinct_candidate(terms: &TermTable, left: TermId, right: TermId) -> bool {
    left != right && terms.constant_part(left).0 == left && terms.constant_part(right).0 == right
}

/// The arithmetic convention used by complete difference-bound closure.
/// Difference bounds are weakest when their constant is greater;
/// at Whitefoot's finite integer term ranges, saturation preserves a
/// representable conservative bound and, unlike wrapping, cannot cross signs.
fn compose_transitive_bounds(first: i128, second: i128) -> i128 {
    first.saturating_add(second)
}

/// The closed fact state at one point: the [ENT-4] least fixed point over the
/// live facts and the implicit facts of every registered term.
#[derive(Clone)]
pub(crate) struct ClosedState {
    all_derivable: bool,
    contradiction: Option<DerivationId>,
    /// Closed bounds and their proofs, indexed by the dense term identities
    /// registered when the closure was taken. Row-major order is the sorted
    /// `(left, right)` order every deterministic consumer iterates in.
    matrix: DenseClosureBounds,
    /// The bounds through zero of each term the closure computed no row for,
    /// by term identity: such a term's closed row and column are zero's,
    /// shifted by these bounds, except toward another member of its dormant
    /// implicit component [ENT-2, ENT-4].
    passive: PassiveReading,
    distinct: WordHashSet<DistinctKey>,
    distinct_proofs: WordHashMap<DistinctKey, DerivationId>,
    opaque: WordHashSet<(GoalId, GoalSign)>,
    opaque_proofs: WordHashMap<(GoalId, GoalSign), DerivationId>,
}

/// The closed bounds through zero of a term the closure computed no row for:
/// `term - Z <= upper` and `Z - term <= lower`.
#[derive(Clone, Copy, Debug)]
struct PassiveBounds {
    upper: i128,
    lower: i128,
    source: PassiveSource,
}

/// Where a passive term's bounds come from: its own implicit bounds, or the
/// closure of the dormant implicit component it belongs to.
#[derive(Clone, Copy, Debug)]
enum PassiveSource {
    Implicit {
        upper: ImplicitBoundKind,
        lower: ImplicitBoundKind,
    },
    /// The component's index in [`PassiveReading::clusters`] and the term's
    /// position among its members.
    Cluster { cluster: u32, position: u32 },
}

/// The closure of one dormant component over its members and zero, at
/// position 0. Each cell keeps how its bound was reached, from which its
/// proof is rebuilt on demand.
#[derive(Clone, Debug)]
struct DormantCluster {
    members: Vec<TermId>,
    cells: Vec<Option<(i128, ClusterHop)>>,
}

#[derive(Clone, Copy, Debug)]
enum ClusterHop {
    Implicit(ImplicitBoundKind),
    Through(u32),
}

impl DormantCluster {
    fn cell(&self, from: usize, to: usize) -> Option<(i128, ClusterHop)> {
        self.cells[from * self.members.len() + to]
    }

    fn proof(&self, from: usize, to: usize, ledger: &mut DerivationLedger) -> (i128, DerivationId) {
        let (bound, hop) = self
            .cell(from, to)
            .expect("a dormant cluster cell on a closed path");
        let (left, right) = (self.members[from], self.members[to]);
        let proof = match hop {
            ClusterHop::Implicit(kind) => ledger.intern(DerivationNode::ImplicitBound {
                left,
                right,
                bound,
                kind,
            }),
            ClusterHop::Through(middle) => {
                let middle = middle as usize;
                let (_, first) = self.proof(from, middle, ledger);
                let (_, second) = self.proof(middle, to, ledger);
                ledger.intern(DerivationNode::TransitiveBound {
                    left,
                    middle: self.members[middle],
                    right,
                    bound,
                    first,
                    second,
                })
            }
        };
        (bound, proof)
    }
}

/// How a closed view reads the terms it computed no row for.
#[derive(Clone, Debug, Default)]
struct PassiveReading {
    bounds: Vec<Option<PassiveBounds>>,
    clusters: Vec<Rc<DormantCluster>>,
}

type ZeroBounds = (
    Option<(i128, ImplicitBoundKind)>,
    Option<(i128, ImplicitBoundKind)>,
);

/// What the term table alone decides about [ENT-2] implicit bounds, kept up
/// to date from the table's change log: each term's bounds through zero, the
/// components of the implicit edges between nonzero terms that are tighter
/// than the path through zero (see [`closure_middle_terms`]), and the closure
/// of each component of more than one term.
///
/// A term registered later or given a standing measure fact later can only
/// add an edge or tighten a bound, so the components only merge; an edge a
/// later bound makes dominated keeps its merge, which leaves a component
/// larger than needed and every reading still exact. A replaced standing
/// fact can weaken a bound and rebuilds the structure.
#[derive(Debug, Default)]
pub(crate) struct ImplicitStructure {
    consumed: usize,
    measure_replacements: usize,
    zero: Vec<ZeroBounds>,
    parent: Vec<u32>,
    /// The members of each component of more than one term, ascending, by
    /// representative.
    members: WordHashMap<u32, Vec<TermId>>,
    clusters: WordHashMap<u32, Rc<DormantCluster>>,
}

impl ImplicitStructure {
    fn find(&mut self, term: TermId) -> u32 {
        let mut root = term.0;
        while self.parent[root as usize] != root {
            root = self.parent[root as usize];
        }
        let mut current = term.0;
        while self.parent[current as usize] != root {
            let next = self.parent[current as usize];
            self.parent[current as usize] = root;
            current = next;
        }
        root
    }

    fn union(&mut self, left: TermId, right: TermId) {
        let (left, right) = (self.find(left), self.find(right));
        if left == right {
            return;
        }
        let (root, child) = (left.min(right), left.max(right));
        self.parent[child as usize] = root;
        let mut members = self
            .members
            .remove(&root)
            .unwrap_or_else(|| vec![TermId(root)]);
        members.extend(
            self.members
                .remove(&child)
                .unwrap_or_else(|| vec![TermId(child)]),
        );
        members.sort_unstable();
        self.members.insert(root, members);
    }

    fn upper(&self, term: TermId) -> Option<i128> {
        self.zero[term.0 as usize].0.map(|(bound, _)| bound)
    }

    fn lower(&self, term: TermId) -> Option<i128> {
        self.zero[term.0 as usize].1.map(|(bound, _)| bound)
    }

    /// Whether `left - right <= bound` between two nonzero terms is tighter
    /// than the path through zero.
    fn tighter_than_zero(&self, left: TermId, right: TermId, bound: i128) -> bool {
        self.upper(left)
            .zip(self.lower(right))
            .is_none_or(|(upper, lower)| bound < compose_transitive_bounds(upper, lower))
    }
}

/// The term table's implicit structure, first brought up to date.
fn implicit_structure(terms: &TermTable) -> std::cell::RefMut<'_, ImplicitStructure> {
    let mut structure = terms.implicit_cache().borrow_mut();
    if structure.measure_replacements != terms.measure_replacements() {
        *structure = ImplicitStructure {
            measure_replacements: terms.measure_replacements(),
            ..ImplicitStructure::default()
        };
    }
    let log = terms.implicit_log();
    if structure.consumed == log.len() {
        return structure;
    }
    let count = terms.ids().count();
    structure.zero.resize(count, (None, None));
    let known = structure.parent.len();
    structure.parent.extend(
        (known..count).map(|term| u32::try_from(term).expect("term index fits the u32 identity")),
    );
    let changed = log[structure.consumed..].to_vec();
    structure.consumed = log.len();
    for term in &changed {
        structure.zero[term.0 as usize] = implicit_zero_bounds(terms, *term);
    }
    let mut touched = changed.clone();
    for term in changed {
        // A capacity term emits the orderings toward its length and head,
        // so a newly registered length or head reaches it through there.
        let capacity = terms.sibling_measure(term, CheckedMeasure::Capacity);
        for emitter in std::iter::once(term).chain(capacity) {
            let mut edges = Vec::new();
            for_each_implicit_bound(terms, emitter, |left, right, bound, _| {
                if left != right && left != ZERO && right != ZERO {
                    edges.push((left, right, bound));
                }
            });
            for (left, right, bound) in edges {
                if structure.tighter_than_zero(left, right, bound) {
                    structure.union(left, right);
                    touched.extend([left, right]);
                }
            }
        }
    }
    for term in touched {
        let root = structure.find(term);
        structure.clusters.remove(&root);
    }
    structure
}

/// The reading of every term outside `universe` other than zero, and of no
/// term inside it or excluded from the closure. A term alone in its implicit
/// component reads its own implicit bounds; the members of a larger dormant
/// component read that component's closure, which no path through any other
/// term can improve, since every edge leaving the component is a dominated
/// implicit edge or reaches zero.
fn passive_bounds(
    terms: &TermTable,
    universe: &ActiveMiddles,
    excluded: Option<TermId>,
) -> PassiveReading {
    let passive = |id: TermId| id != ZERO && !universe.contains(id) && Some(id) != excluded;
    let mut structure = implicit_structure(terms);
    let mut reading = PassiveReading::default();
    let mut cluster_of = HashMap::<u32, u32>::default();
    reading.bounds = terms
        .ids()
        .map(|id| {
            if !passive(id) {
                return None;
            }
            let component = structure.find(id);
            let Some(members) = structure.members.get(&component) else {
                let ((upper, upper_kind), (lower, lower_kind)) = (
                    structure.zero[id.0 as usize].0?,
                    structure.zero[id.0 as usize].1?,
                );
                return Some(PassiveBounds {
                    upper,
                    lower,
                    source: PassiveSource::Implicit {
                        upper: upper_kind,
                        lower: lower_kind,
                    },
                });
            };
            let cluster = match cluster_of.get(&component) {
                Some(cluster) => *cluster,
                None => {
                    let closure = if excluded.is_some_and(|excluded| members.contains(&excluded)) {
                        let kept = members
                            .iter()
                            .copied()
                            .filter(|member| Some(*member) != excluded)
                            .collect::<Vec<_>>();
                        Rc::new(dormant_cluster(terms, &kept))
                    } else if let Some(closure) = structure.clusters.get(&component) {
                        closure.clone()
                    } else {
                        let closure = Rc::new(dormant_cluster(terms, members));
                        structure.clusters.insert(component, closure.clone());
                        closure
                    };
                    reading.clusters.push(closure);
                    let cluster =
                        u32::try_from(reading.clusters.len() - 1).expect("cluster count fits u32");
                    cluster_of.insert(component, cluster);
                    cluster
                }
            };
            let closure = &reading.clusters[cluster as usize];
            let position = closure
                .members
                .iter()
                .position(|member| *member == id)
                .expect("a passive term is a member of its cluster");
            let (upper, _) = closure.cell(position, 0)?;
            let (lower, _) = closure.cell(0, position)?;
            Some(PassiveBounds {
                upper,
                lower,
                source: PassiveSource::Cluster {
                    cluster,
                    position: u32::try_from(position).expect("cluster size fits u32"),
                },
            })
        })
        .collect();
    reading
}

/// The closure of the implicit edges among `members` and zero.
fn dormant_cluster(terms: &TermTable, members: &[TermId]) -> DormantCluster {
    let members = std::iter::once(ZERO)
        .chain(members.iter().copied())
        .collect::<Vec<_>>();
    let width = members.len();
    let position = |term: TermId| members.iter().position(|member| *member == term);
    let mut cells = vec![None; width * width];
    for member in &members {
        for_each_implicit_bound(terms, *member, |left, right, bound, kind| {
            let (Some(from), Some(to)) = (position(left), position(right)) else {
                return;
            };
            let cell: &mut Option<(i128, ClusterHop)> = &mut cells[from * width + to];
            if cell.is_none_or(|(held, _)| bound < held) {
                *cell = Some((bound, ClusterHop::Implicit(kind)));
            }
        });
    }
    for middle in 0..width {
        for from in 0..width {
            let Some((first, _)) = cells[from * width + middle] else {
                continue;
            };
            for to in 0..width {
                let Some((second, _)) = cells[middle * width + to] else {
                    continue;
                };
                let via = compose_transitive_bounds(first, second);
                let cell = &mut cells[from * width + to];
                if cell.is_none_or(|(held, _)| via < held) {
                    *cell = Some((
                        via,
                        ClusterHop::Through(u32::try_from(middle).expect("cluster size fits u32")),
                    ));
                }
            }
        }
    }
    DormantCluster { members, cells }
}

/// A term's tightest implicit bounds through zero, `term - Z <= upper` and
/// `Z - term <= lower`, each with the kind that gives it.
fn implicit_zero_bounds(terms: &TermTable, id: TermId) -> ZeroBounds {
    let mut upper: Option<(i128, ImplicitBoundKind)> = None;
    let mut lower: Option<(i128, ImplicitBoundKind)> = None;
    for_each_implicit_bound(terms, id, |left, right, bound, kind| {
        let slot = if (left, right) == (id, ZERO) {
            &mut upper
        } else if (left, right) == (ZERO, id) {
            &mut lower
        } else {
            return;
        };
        if slot.is_none_or(|(held, _)| bound < held) {
            *slot = Some((bound, kind));
        }
    });
    (upper, lower)
}

impl ClosedState {
    fn passive(&self, term: TermId) -> Option<PassiveBounds> {
        self.passive.bounds.get(term.0 as usize).copied().flatten()
    }

    /// The two members' cluster and positions when both belong to the same
    /// dormant component.
    fn same_cluster(
        &self,
        from: PassiveBounds,
        to: PassiveBounds,
    ) -> Option<(&DormantCluster, usize, usize)> {
        match (from.source, to.source) {
            (
                PassiveSource::Cluster {
                    cluster,
                    position: from,
                },
                PassiveSource::Cluster {
                    cluster: other,
                    position: to,
                },
            ) if cluster == other => Some((
                &self.passive.clusters[cluster as usize],
                from as usize,
                to as usize,
            )),
            _ => None,
        }
    }

    /// The closed bound `left - right`, read from the matrix or, for a term
    /// the closure computed no row for, through zero.
    fn value(&self, left: TermId, right: TermId) -> Option<i128> {
        if let Some((bound, _)) = self.matrix.lookup(left, right) {
            return Some(bound);
        }
        match (self.passive(left), self.passive(right)) {
            (Some(_), _) | (_, Some(_)) if left == right => Some(0),
            (Some(from), Some(to)) => match self.same_cluster(from, to) {
                Some((cluster, from, to)) => cluster.cell(from, to).map(|(bound, _)| bound),
                None => Some(compose_transitive_bounds(from.upper, to.lower)),
            },
            (Some(from), None) => self
                .matrix
                .lookup(ZERO, right)
                .map(|(bound, _)| compose_transitive_bounds(from.upper, bound))
                .or_else(|| (right == ZERO).then_some(from.upper)),
            (None, Some(to)) => self
                .matrix
                .lookup(left, ZERO)
                .map(|(bound, _)| compose_transitive_bounds(bound, to.lower))
                .or_else(|| (left == ZERO).then_some(to.lower)),
            (None, None) => None,
        }
    }

    /// A passive term's bound toward zero, `term - Z` when `upward`, else
    /// `Z - term`, with its proof.
    fn passive_zero_cell(
        &self,
        term: TermId,
        bounds: PassiveBounds,
        upward: bool,
        ledger: &mut DerivationLedger,
    ) -> (i128, DerivationId) {
        match bounds.source {
            PassiveSource::Implicit { upper, lower } => {
                let (left, right, bound, kind) = if upward {
                    (term, ZERO, bounds.upper, upper)
                } else {
                    (ZERO, term, bounds.lower, lower)
                };
                (
                    bound,
                    ledger.intern(DerivationNode::ImplicitBound {
                        left,
                        right,
                        bound,
                        kind,
                    }),
                )
            }
            PassiveSource::Cluster { cluster, position } => {
                let cluster = &self.passive.clusters[cluster as usize];
                let position = position as usize;
                if upward {
                    cluster.proof(position, 0, ledger)
                } else {
                    cluster.proof(0, position, ledger)
                }
            }
        }
    }

    /// The closed bound `left - right` with its proof; a bound read through
    /// zero for a term without a row is proved by transitivity through zero
    /// from that term's implicit bound, and one between two members of a
    /// dormant component by that component's closure.
    fn cell(
        &self,
        left: TermId,
        right: TermId,
        ledger: &mut DerivationLedger,
    ) -> Option<(i128, DerivationId)> {
        if let Some(cell) = self.matrix.lookup(left, right) {
            return Some(cell);
        }
        let (first, second) = match (self.passive(left), self.passive(right)) {
            (Some(_), _) | (_, Some(_)) if left == right => {
                return Some((
                    0,
                    ledger.intern(DerivationNode::ImplicitBound {
                        left,
                        right,
                        bound: 0,
                        kind: ImplicitBoundKind::Reflexive,
                    }),
                ));
            }
            (Some(from), _) if right == ZERO => {
                return Some(self.passive_zero_cell(left, from, true, ledger));
            }
            (_, Some(to)) if left == ZERO => {
                return Some(self.passive_zero_cell(right, to, false, ledger));
            }
            (Some(from), Some(to)) => {
                if let Some((cluster, from, to)) = self.same_cluster(from, to) {
                    return cluster
                        .cell(from, to)
                        .is_some()
                        .then(|| cluster.proof(from, to, ledger));
                }
                (
                    self.passive_zero_cell(left, from, true, ledger),
                    self.passive_zero_cell(right, to, false, ledger),
                )
            }
            (Some(from), None) => (
                self.passive_zero_cell(left, from, true, ledger),
                self.matrix.lookup(ZERO, right)?,
            ),
            (None, Some(to)) => (
                self.matrix.lookup(left, ZERO)?,
                self.passive_zero_cell(right, to, false, ledger),
            ),
            (None, None) => return None,
        };
        let bound = compose_transitive_bounds(first.0, second.0);
        Some((
            bound,
            ledger.intern(DerivationNode::TransitiveBound {
                left,
                middle: ZERO,
                right,
                bound,
                first: first.1,
                second: second.1,
            }),
        ))
    }

    /// Whether the disequality of an ordered pair is held or derivable from
    /// either strict bound.
    fn holds_distinct(&self, pair: DistinctKey) -> bool {
        self.derives(&Relation::Distinct {
            left: pair.0,
            right: pair.1,
            difference: pair.2,
        })
    }

    /// The proof of a held or derivable disequality of an ordered pair.
    fn distinct_proof(
        &self,
        pair: DistinctKey,
        ledger: &mut DerivationLedger,
    ) -> Option<DerivationId> {
        if let Some(proof) = self.distinct_proofs.get(&pair) {
            return Some(*proof);
        }
        self.relation_proof(
            &Relation::Distinct {
                left: pair.0,
                right: pair.1,
                difference: pair.2,
            },
            ledger,
        )
    }

    fn selected_relations_depend_on_postcondition_call(&self, ledger: &DerivationLedger) -> bool {
        self.matrix
            .cells()
            .map(|(_, _, _, proof)| proof)
            .chain(self.distinct_proofs.values().copied())
            .any(|proof| ledger.depends_on_postcondition_call(proof))
    }

    /// `left - right <= bound` is derivable.
    pub(crate) fn derives_bound(&self, left: TermId, right: TermId, bound: i128) -> bool {
        if self.all_derivable {
            return true;
        }
        self.value(left, right).is_some_and(|held| held <= bound)
    }

    /// Strongest closed difference bound for interval projection.  Callers
    /// must handle a contradictory state separately because it has no single
    /// meaningful numeric interval.
    pub(crate) fn tight_bound(&self, left: TermId, right: TermId) -> Option<i128> {
        (!self.all_derivable)
            .then(|| self.value(left, right))
            .flatten()
    }

    /// A state is contradictory when `t - t <= -1` is derivable for any term;
    /// there every relation is derivable and every obligation is discharged.
    pub(crate) const fn contradictory(&self) -> bool {
        self.all_derivable
    }

    pub(crate) fn contradiction_proof(&self) -> Option<DerivationId> {
        self.contradiction
    }

    /// The finite normalized L0 inventory used by bounded `value_if`
    /// delivery. Opaque signed goals are deliberately absent.
    pub(crate) fn delivery_relations(&self) -> Vec<(Relation, DerivationId)> {
        if self.all_derivable {
            return Vec::new();
        }
        // Row-major cells are already in `(left, right)` order.
        let mut relations = self
            .matrix
            .cells()
            .map(|(left, right, bound, proof)| (Relation::Bound { left, right, bound }, proof))
            .collect::<Vec<_>>();
        let mut distinct = self.distinct.iter().copied().collect::<Vec<_>>();
        distinct.sort_unstable();
        relations.extend(distinct.into_iter().map(|(left, right, difference)| {
            (
                Relation::Distinct {
                    left,
                    right,
                    difference,
                },
                self.distinct_proofs[&(left, right, difference)],
            )
        }));
        relations
    }

    /// [ENT-4] exact derivability of one normalized relation: a bound by the
    /// held smaller-or-equal constant, an equality by both zero bounds, a
    /// disequality by presence or by either strict bound.
    pub(crate) fn derives(&self, relation: &Relation) -> bool {
        if self.all_derivable {
            return true;
        }
        match relation {
            Relation::Bound { left, right, bound } => self.derives_bound(*left, *right, *bound),
            Relation::Equal {
                left,
                right,
                difference,
            } => {
                self.derives_bound(*left, *right, *difference)
                    && self.derives_bound(*right, *left, difference.saturating_neg())
            }
            Relation::Distinct {
                left,
                right,
                difference,
            } => {
                self.distinct
                    .contains(&distinct_key(*left, *right, *difference))
                    || self.derives_bound(*left, *right, difference.saturating_sub(1))
                    || self.derives_bound(
                        *right,
                        *left,
                        difference.saturating_neg().saturating_sub(1),
                    )
            }
        }
    }

    fn derives_normalization(&self, normalization: &GoalNormalization, sign: GoalSign) -> bool {
        normalization.clauses(sign).iter().any(|clause| {
            clause.iter().all(|literal| {
                let Some(relation) = normalization
                    .components
                    .get(usize::try_from(literal.component).expect("component index fits usize"))
                    .and_then(Option::as_ref)
                else {
                    return false;
                };
                if literal.negated {
                    self.derives(&relation.negated())
                } else {
                    self.derives(relation)
                }
            })
        })
    }

    pub(crate) fn derives_normalized_goal(
        &self,
        goal: GoalId,
        sign: GoalSign,
        goals: &GoalTable,
    ) -> bool {
        goals
            .normalization(goal)
            .is_some_and(|normalization| self.derives_normalization(normalization, sign))
    }

    /// Exact signed-goal derivability: a retained opaque sign, its one
    /// comparison-root projection, its family's fixed normalization, a Bool
    /// literal, or finite truth-table introduction for an interned parent.
    pub(crate) fn derives_goal(&self, goal: GoalId, sign: GoalSign, goals: &GoalTable) -> bool {
        self.derives_goal_inner(goal, sign, goals, &mut HashSet::default())
    }

    fn derives_goal_inner(
        &self,
        goal: GoalId,
        sign: GoalSign,
        goals: &GoalTable,
        visiting: &mut WordHashSet<(GoalId, GoalSign)>,
    ) -> bool {
        if self.all_derivable || self.opaque.contains(&(goal, sign)) {
            return true;
        }
        let projected = goals.projection(goal).is_some_and(|relation| match sign {
            GoalSign::Positive => self.derives(relation),
            GoalSign::Negative => self.derives(&relation.negated()),
        });
        if projected || self.derives_normalized_goal(goal, sign, goals) {
            return true;
        }
        if literal_goal_sign(goals.expression(goal)).is_some_and(|truth| truth == sign) {
            return true;
        }
        if !visiting.insert((goal, sign)) {
            return false;
        }
        let result = match goals.expression(goal) {
            GoalExpression::Operation {
                row: GoalOperation::Boolean(operation),
                arguments,
                ..
            } => {
                let child =
                    |argument: &GoalExpression,
                     child_sign: GoalSign,
                     visiting: &mut WordHashSet<(GoalId, GoalSign)>| {
                        goals.id(argument).is_some_and(|child| {
                            self.derives_goal_inner(child, child_sign, goals, visiting)
                        })
                    };
                match (operation, sign) {
                    (CheckedBooleanOperation::And, GoalSign::Positive) => arguments
                        .iter()
                        .all(|argument| child(argument, GoalSign::Positive, visiting)),
                    (CheckedBooleanOperation::And, GoalSign::Negative) => arguments
                        .iter()
                        .any(|argument| child(argument, GoalSign::Negative, visiting)),
                    (CheckedBooleanOperation::Or, GoalSign::Positive) => arguments
                        .iter()
                        .any(|argument| child(argument, GoalSign::Positive, visiting)),
                    (CheckedBooleanOperation::Or, GoalSign::Negative) => arguments
                        .iter()
                        .all(|argument| child(argument, GoalSign::Negative, visiting)),
                    (CheckedBooleanOperation::Not, GoalSign::Positive) => arguments
                        .first()
                        .is_some_and(|argument| child(argument, GoalSign::Negative, visiting)),
                    (CheckedBooleanOperation::Not, GoalSign::Negative) => arguments
                        .first()
                        .is_some_and(|argument| child(argument, GoalSign::Positive, visiting)),
                    (CheckedBooleanOperation::ExclusiveOr, _) => false,
                }
            }
            GoalExpression::Datum(_) | GoalExpression::Operation { .. } => false,
        };
        visiting.remove(&(goal, sign));
        result
    }

    pub(crate) fn holds_opaque(&self, goal: GoalId, sign: GoalSign) -> bool {
        !self.all_derivable && self.opaque.contains(&(goal, sign))
    }

    pub(crate) fn opaque_proof(&self, goal: GoalId, sign: GoalSign) -> Option<DerivationId> {
        (!self.all_derivable)
            .then(|| self.opaque_proofs.get(&(goal, sign)).copied())
            .flatten()
    }

    pub(crate) fn bound_proof(
        &self,
        left: TermId,
        right: TermId,
        requested: i128,
        ledger: &mut DerivationLedger,
    ) -> Option<DerivationId> {
        if self.all_derivable {
            return self.contradiction;
        }
        if self.value(left, right)? > requested {
            return None;
        }
        let (held, parent) = self.cell(left, right, ledger)?;
        if held > requested {
            return None;
        }
        if held == requested {
            Some(parent)
        } else {
            Some(ledger.intern(DerivationNode::SubsumedBound {
                left,
                right,
                held,
                requested,
                parent,
            }))
        }
    }

    pub(crate) fn relation_proof(
        &self,
        relation: &Relation,
        ledger: &mut DerivationLedger,
    ) -> Option<DerivationId> {
        if self.all_derivable {
            return self.contradiction;
        }
        match relation {
            Relation::Bound { left, right, bound } => {
                self.bound_proof(*left, *right, *bound, ledger)
            }
            Relation::Equal {
                left,
                right,
                difference,
            } => {
                let forward = self.bound_proof(*left, *right, *difference, ledger)?;
                let reverse = self.bound_proof(*right, *left, difference.saturating_neg(), ledger)?;
                Some(ledger.intern(DerivationNode::Equality {
                    left: *left,
                    right: *right,
                    forward,
                    reverse,
                }))
            }
            Relation::Distinct {
                left,
                right,
                difference,
            } => {
                let pair = distinct_key(*left, *right, *difference);
                let mut best = self.distinct_proofs.get(&pair).copied();
                for (from, to, gap) in [
                    (*left, *right, difference.saturating_sub(1)),
                    (*right, *left, difference.saturating_neg().saturating_sub(1)),
                ] {
                    if let Some(parent) = self.bound_proof(from, to, gap, ledger) {
                        let candidate = ledger.intern(DerivationNode::DisequalityFromStrictBound {
                            left: pair.0,
                            right: pair.1,
                            difference: pair.2,
                            parent,
                        });
                        if best.is_none_or(|current| ledger.better(candidate, current)) {
                            best = Some(candidate);
                        }
                    }
                }
                best
            }
        }
    }

    pub(crate) fn goal_projection_proof(
        &self,
        goal: GoalId,
        sign: GoalSign,
        goals: &GoalTable,
        ledger: &mut DerivationLedger,
    ) -> Option<DerivationId> {
        if self.all_derivable {
            return self.contradiction;
        }
        let mut relation = goals.projection(goal)?.clone();
        if sign == GoalSign::Negative {
            relation = relation.negated();
        }
        let parent = self.relation_proof(&relation, ledger)?;
        Some(ledger.intern(DerivationNode::GoalProjection {
            goal,
            sign,
            relation,
            parent,
        }))
    }

    pub(crate) fn normalization_proof(
        &self,
        goal: GoalId,
        sign: GoalSign,
        goals: &GoalTable,
        ledger: &mut DerivationLedger,
    ) -> Option<DerivationId> {
        if self.all_derivable {
            return self.contradiction;
        }
        let normalization = goals.normalization(goal)?;
        for (clause, literals) in normalization.clauses(sign).iter().enumerate() {
            let parents = literals
                .iter()
                .map(|literal| {
                    let relation = normalization
                        .components
                        .get(usize::try_from(literal.component).ok()?)?
                        .clone()?;
                    self.relation_proof(
                        &if literal.negated {
                            relation.negated()
                        } else {
                            relation
                        },
                        ledger,
                    )
                })
                .collect::<Option<Vec<_>>>();
            if let Some(parents) = parents {
                return Some(ledger.intern(DerivationNode::GoalNormalization {
                    goal,
                    sign,
                    clause:
                        u32::try_from(clause).expect("goal-normalization clause count exceeds u32"),
                    parents,
                }));
            }
        }
        None
    }

    pub(crate) fn goal_proof(
        &self,
        goal: GoalId,
        sign: GoalSign,
        goals: &GoalTable,
        ledger: &mut DerivationLedger,
    ) -> Option<DerivationId> {
        self.goal_proof_inner(goal, sign, goals, ledger, &mut HashSet::default())
    }

    fn goal_proof_inner(
        &self,
        goal: GoalId,
        sign: GoalSign,
        goals: &GoalTable,
        ledger: &mut DerivationLedger,
        visiting: &mut WordHashSet<(GoalId, GoalSign)>,
    ) -> Option<DerivationId> {
        if self.all_derivable {
            return self.contradiction;
        }
        let direct = self
            .opaque_proof(goal, sign)
            .or_else(|| self.goal_projection_proof(goal, sign, goals, ledger))
            .or_else(|| self.normalization_proof(goal, sign, goals, ledger));
        if direct.is_some() {
            return direct;
        }
        if literal_goal_sign(goals.expression(goal)).is_some_and(|truth| truth == sign) {
            return Some(ledger.intern(DerivationNode::BooleanLiteral { goal, sign }));
        }
        if !visiting.insert((goal, sign)) {
            return None;
        }
        let proof = match goals.expression(goal) {
            GoalExpression::Operation {
                row: GoalOperation::Boolean(operation),
                arguments,
                ..
            } => {
                let child_proof =
                    |argument: &GoalExpression,
                     child_sign: GoalSign,
                     visiting: &mut WordHashSet<(GoalId, GoalSign)>,
                     ledger: &mut DerivationLedger| {
                        let child = goals.id(argument)?;
                        self.goal_proof_inner(child, child_sign, goals, ledger, visiting)
                    };
                let all = |child_sign: GoalSign,
                           visiting: &mut WordHashSet<(GoalId, GoalSign)>,
                           ledger: &mut DerivationLedger| {
                    arguments
                        .iter()
                        .map(|argument| child_proof(argument, child_sign, visiting, ledger))
                        .collect::<Option<Vec<_>>>()
                };
                let any = |child_sign: GoalSign,
                           visiting: &mut WordHashSet<(GoalId, GoalSign)>,
                           ledger: &mut DerivationLedger| {
                    let mut best = None;
                    for argument in arguments {
                        let Some(candidate) = child_proof(argument, child_sign, visiting, ledger)
                        else {
                            continue;
                        };
                        if best.is_none_or(|current| ledger.better(candidate, current)) {
                            best = Some(candidate);
                        }
                    }
                    best.map(|parent| vec![parent])
                };
                let parents = match (operation, sign) {
                    (CheckedBooleanOperation::And, GoalSign::Positive) => {
                        all(GoalSign::Positive, visiting, ledger)
                    }
                    (CheckedBooleanOperation::And, GoalSign::Negative) => {
                        any(GoalSign::Negative, visiting, ledger)
                    }
                    (CheckedBooleanOperation::Or, GoalSign::Positive) => {
                        any(GoalSign::Positive, visiting, ledger)
                    }
                    (CheckedBooleanOperation::Or, GoalSign::Negative) => {
                        all(GoalSign::Negative, visiting, ledger)
                    }
                    (CheckedBooleanOperation::Not, GoalSign::Positive) => arguments
                        .first()
                        .and_then(|argument| {
                            child_proof(argument, GoalSign::Negative, visiting, ledger)
                        })
                        .map(|parent| vec![parent]),
                    (CheckedBooleanOperation::Not, GoalSign::Negative) => arguments
                        .first()
                        .and_then(|argument| {
                            child_proof(argument, GoalSign::Positive, visiting, ledger)
                        })
                        .map(|parent| vec![parent]),
                    (CheckedBooleanOperation::ExclusiveOr, _) => None,
                };
                parents.map(|parents| {
                    ledger.intern(DerivationNode::BooleanIntroduction {
                        goal,
                        sign,
                        parents,
                    })
                })
            }
            GoalExpression::Datum(_) | GoalExpression::Operation { .. } => None,
        };
        visiting.remove(&(goal, sign));
        proof
    }
}

fn literal_goal_sign(expression: &GoalExpression) -> Option<GoalSign> {
    let GoalExpression::Datum(super::super::goal::GoalDatum::Literal(CheckedValue::Bool(value))) =
        expression
    else {
        return None;
    };
    Some(if *value {
        GoalSign::Positive
    } else {
        GoalSign::Negative
    })
}

/// Computes the [ENT-4] closure of `state` over the registered terms.
pub(crate) fn close(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
) -> Rc<ClosedState> {
    let key = ClosedViewKey {
        terms: std::ptr::from_ref(terms) as usize,
        term_revision: terms.revision(),
        term_count: terms.ids().count(),
        measure_replacements: terms.measure_replacements(),
        goals: std::ptr::from_ref(goals) as usize,
        goal_revision: goals.revision(),
        ledger: std::ptr::from_ref(ledger) as usize,
    };
    if let Some(view) = state.closed_view.borrow().as_ref()
        && view.key == key
    {
        #[cfg(test)]
        tests::record_route(tests::ClosureRoute::Remembered);
        #[cfg(test)]
        if tests::verifying_seeded_closures() {
            tests::assert_seeded_closure_matches_complete(
                state,
                terms,
                goals,
                ledger,
                &view.closed,
            );
        }
        super::work::closure_cache_hit();
        return Rc::clone(&view.closed);
    }
    let closed = Rc::new(close_with_excluded_term(state, terms, goals, ledger, None));
    *state.closed_view.borrow_mut() = Some(ClosedView {
        key,
        closed: Rc::clone(&closed),
    });
    closed
}

/// The tightest implicit bound on one ordered pair of Z and a term [ENT-4],
/// with its kind, when the term carries one.
pub(crate) fn implicit_bound_between(
    terms: &TermTable,
    pair: (TermId, TermId),
) -> Option<(i128, ImplicitBoundKind)> {
    let term = if pair.0 == ZERO { pair.1 } else { pair.0 };
    let mut tightest: Option<(i128, ImplicitBoundKind)> = None;
    for_each_implicit_bound(terms, term, |left, right, bound, kind| {
        if (left, right) == pair && tightest.is_none_or(|(held, _)| bound < held) {
            tightest = Some((bound, kind));
        }
    });
    tightest
}

/// Emits every [ENT-2] implicit bound carried by one term: the reflexive
/// bound, the fragment-type range, the constant fold through Z, and the
/// `len_of(P) = N` equality of an `array<T, N>` place.
///
/// Implicit facts are a function of the term table and the place's type
/// alone. They hold at every program point, so this is the single rule table
/// every closure entry point re-emits; no [ENT-5] kill and no join can remove
/// one, and a state that lost the materialized copy of one regains it here.
fn for_each_implicit_bound(
    terms: &TermTable,
    id: TermId,
    mut emit: impl FnMut(TermId, TermId, i128, ImplicitBoundKind),
) {
    emit(id, id, 0, ImplicitBoundKind::Reflexive);
    match terms.kind(id) {
        TermKind::Zero => {}
        TermKind::Constant(value) => {
            emit(id, ZERO, *value, ImplicitBoundKind::Constant);
            emit(ZERO, id, -value, ImplicitBoundKind::Constant);
        }
        TermKind::Place(_, ty) | TermKind::ConstParameter(_, ty) => {
            let (minimum, maximum) = type_range(*ty);
            emit(id, ZERO, maximum, ImplicitBoundKind::TypeMaximum);
            emit(ZERO, id, -minimum, ImplicitBoundKind::TypeMinimum);
        }
        // [MSR-2] every measure term carries the standing facts of its
        // place. The `u64` range already gives `Z <= m`; what the measure
        // table adds is the value a fixed cell has and the ordering between
        // two measures of one place. Each has empty support and no event
        // kills it, which is exactly what an implicit bound is.
        // [MSR-3] an entry or placement datum is one measure's value, of
        // fragment type u64, or one fragment-integer place's value, of that
        // place's type, with empty support. Its standing orderings reach it
        // through the equality this datum is established with; what it
        // carries of its own is the type range.
        TermKind::EntryDatum { ty, .. } | TermKind::MeasureDatum { ty, .. } => {
            let (minimum, maximum) = type_range(*ty);
            emit(id, ZERO, maximum, ImplicitBoundKind::TypeMaximum);
            emit(ZERO, id, -minimum, ImplicitBoundKind::TypeMinimum);
        }
        TermKind::Measure(measure, _) | TermKind::TargetMeasure { measure, .. } => {
            let (minimum, maximum) = type_range(IntegerType::U64);
            emit(id, ZERO, maximum, ImplicitBoundKind::TypeMaximum);
            emit(ZERO, id, -minimum, ImplicitBoundKind::TypeMinimum);
            match terms.measure_bound(id) {
                Some(MeasureBound::Constant(value)) => {
                    emit(id, ZERO, value, ImplicitBoundKind::StandingMeasure);
                    emit(ZERO, id, -value, ImplicitBoundKind::StandingMeasure);
                }
                Some(MeasureBound::Equal(other)) => {
                    emit(id, other, 0, ImplicitBoundKind::StandingMeasure);
                    emit(other, id, 0, ImplicitBoundKind::StandingMeasure);
                }
                None => {}
            }
            // `P.len <= P.cap` and `P.head <= P.cap`, emitted from the
            // capacity term so each ordering is emitted exactly once. x1's
            // [MSR-1] table gives the two `Array` rows no `cap` cell, so an
            // `Array` place registers no capacity term and neither ordering
            // is emitted for it.
            if *measure == CheckedMeasure::Capacity {
                for bounded in [CheckedMeasure::Length, CheckedMeasure::Head] {
                    if let Some(other) = terms.sibling_measure(id, bounded) {
                        emit(other, id, 0, ImplicitBoundKind::MeasureOrdering);
                    }
                }
            }
        }
        TermKind::CountedCapture { .. } | TermKind::IndexCapture { .. } => {
            let (minimum, maximum) = type_range(IntegerType::U64);
            emit(id, ZERO, maximum, ImplicitBoundKind::TypeMaximum);
            emit(ZERO, id, -minimum, ImplicitBoundKind::TypeMinimum);
        }
        TermKind::ResultPayload { ty, .. }
        | TermKind::CommitValue { ty, .. }
        | TermKind::CallDatum { ty, .. } => {
            let (minimum, maximum) = type_range(*ty);
            emit(id, ZERO, maximum, ImplicitBoundKind::TypeMaximum);
            emit(ZERO, id, -minimum, ImplicitBoundKind::TypeMinimum);
        }
    }
}

/// Puts one [ENT-2] implicit bound back into an already closed view.
///
/// A stronger stored relation stays as it is, together with its proof: the
/// implicit bound is an axiom about the term, not a competing derivation of
/// what the state already knows.
fn restore_implicit_bound(
    closed: &mut ClosedState,
    left: TermId,
    right: TermId,
    bound: i128,
    kind: ImplicitBoundKind,
    ledger: &mut DerivationLedger,
) {
    // A bound toward a term read through zero is already that reading.
    if !closed.matrix.holds(left)
        || !closed.matrix.holds(right)
        || closed
            .matrix
            .lookup(left, right)
            .is_some_and(|(current, _)| current <= bound)
    {
        return;
    }
    let proof = ledger.intern(DerivationNode::ImplicitBound {
        left,
        right,
        bound,
        kind,
    });
    closed.matrix.set(left, right, bound, proof, 0);
}

/// Exact contradiction query without constructing a derivation DAG.
///
/// Kill handling needs to know whether contradiction must become absorbing,
/// but an accepted path almost always answers no. This computes the same finite ENT-4 value
/// closure in a dense matrix, including disequality strengthening and finite
/// goal introduction; callers construct the ordinary proof closure only for
/// the exceptional contradictory result.
pub(crate) fn contradiction_without_proofs(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
) -> bool {
    if state.all_derivable {
        super::work::probe(0, 0);
        return true;
    }
    let universe = closure_universe(state, terms, goals, None);
    if let Some(EdgeClosure {
        dense, distinct, ..
    }) = insert_fresh_edges(state, terms, &universe, &mut NoProofs)
    {
        #[cfg(test)]
        tests::record_route(tests::ClosureRoute::InsertionWithoutProofs);
        super::work::probe(dense.dimension, dense.bounds.len());
        let contradictory = terms
            .ids()
            .any(|id| dense.get(id, id).is_some_and(|(bound, _)| bound < 0))
            || goal_contradiction_without_proofs(state, dense, distinct, goals);
        #[cfg(test)]
        if tests::verifying_seeded_closures() {
            let mut unseeded = state.clone();
            unseeded.closure = ClosureRecord::Unknown;
            assert_eq!(
                contradictory,
                contradiction_without_proofs(&unseeded, terms, goals),
                "the proof-free edge insertion disagrees with the complete probe"
            );
        }
        return contradictory;
    }
    let contradictory = complete_contradiction_probe(state, terms, goals, &universe);
    // The probe over the closure's terms is compared with the probe over
    // every registered term, the reference the universe argument elides.
    #[cfg(test)]
    if tests::verifying_seeded_closures() && !tests::closing_every_term() {
        let every = ActiveMiddles(vec![true; terms.ids().count()]);
        assert_eq!(
            contradictory,
            complete_contradiction_probe(state, terms, goals, &every),
            "the probe over the closure's terms disagrees with the probe over every term"
        );
    }
    contradictory
}

/// The complete proof-free fixed point over the terms of `universe`: every
/// stored cell and implicit bound among them, closed by transitivity and
/// disequality strengthening [ENT-4], decides a negative cycle or a goal held
/// in both signs. A term outside the universe takes part in no path tighter
/// than its reading through zero (see [`closure_middle_terms`]), so a
/// negative cycle through it is one through zero of no greater bound among
/// the universe's terms, and a goal names only universe terms.
fn complete_contradiction_probe(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    universe: &ActiveMiddles,
) -> bool {
    let term_count = terms.ids().count();
    let ids = terms
        .ids()
        .filter(|id| universe.contains(*id))
        .collect::<Vec<_>>();
    let dimension = ids.len();
    super::work::probe(dimension, dimension.saturating_mul(dimension));
    let mut slots = vec![NO_SLOT; term_count];
    for (slot, id) in ids.iter().enumerate() {
        slots[id.0 as usize] = u32::try_from(slot).expect("slot fits the u32 identity");
    }
    let slot = |term: TermId| {
        let slot = slots[term.0 as usize];
        (slot != NO_SLOT).then_some(slot as usize)
    };
    let cells = dimension
        .checked_mul(dimension)
        .expect("ENT contradiction matrix exceeds the address space");
    let mut bounds = vec![None; cells];
    let insert = |bounds: &mut [Option<i128>], left: TermId, right: TermId, bound: i128| {
        let (Some(left), Some(right)) = (slot(left), slot(right)) else {
            return;
        };
        let cell = &mut bounds[left * dimension + right];
        if cell.is_none_or(|current| bound < current) {
            *cell = Some(bound);
        }
    };
    for (left, right, bound, _) in state.bounds.cells() {
        insert(&mut bounds, left, right, bound);
    }
    for id in &ids {
        for_each_implicit_bound(terms, *id, |left, right, bound, _| {
            insert(&mut bounds, left, right, bound);
        });
    }

    let mut distinct = (*state.distinct).clone();
    distinct.retain(|(left, right, _)| slot(*left).is_some() && slot(*right).is_some());
    loop {
        // Floyd-Warshall over the exact same saturating difference bounds.
        // One pass closes the current edge set; a second is needed only when
        // a newly derived disequality strengthens a weak bound below.
        for middle in 0..dimension {
            let middle_row = middle * dimension;
            for left in 0..dimension {
                let left_row = left * dimension;
                let Some(first) = bounds[left_row + middle] else {
                    continue;
                };
                for right in 0..dimension {
                    let Some(second) = bounds[middle_row + right] else {
                        continue;
                    };
                    let via = compose_transitive_bounds(first, second);
                    let cell = &mut bounds[left_row + right];
                    if cell.is_none_or(|current| via < current) {
                        *cell = Some(via);
                    }
                }
            }
        }
        if (0..dimension).any(|id| bounds[id * dimension + id].is_some_and(|bound| bound < 0)) {
            return true;
        }

        for left in 0..dimension {
            for right in (left + 1)..dimension {
                if !zero_distinct_candidate(terms, ids[left], ids[right]) {
                    continue;
                }
                let forward = bounds[left * dimension + right];
                let reverse = bounds[right * dimension + left];
                if forward.is_some_and(|bound| bound <= -1)
                    || reverse.is_some_and(|bound| bound <= -1)
                {
                    distinct.insert((ids[left], ids[right], 0));
                }
            }
        }
        let mut strengthened = false;
        for &(left, right, difference) in &distinct {
            for (from, to, offset) in [
                (left, right, difference),
                (right, left, difference.saturating_neg()),
            ] {
                let (Some(from), Some(to)) = (slot(from), slot(to)) else {
                    continue;
                };
                let cell = &mut bounds[from * dimension + to];
                if *cell == Some(offset) && offset.saturating_sub(1) < offset {
                    *cell = Some(offset.saturating_sub(1));
                    strengthened = true;
                }
            }
        }
        if !strengthened {
            break;
        }
    }

    // Reuse the ordinary goal truth table over proof-free relation cells.
    // `derives_goal` consults no proof identity.
    let mut matrix = DenseClosureBounds::with_terms(term_count, ids.clone());
    for left in 0..dimension {
        for right in 0..dimension {
            if let Some(bound) = bounds[left * dimension + right] {
                matrix.set(ids[left], ids[right], bound, DerivationId(0), 0);
            }
        }
    }
    goal_contradiction_without_proofs(state, matrix, distinct, goals)
}

/// Whether both signs of one goal are derivable over closed proof-free
/// relation maps. `derives_goal` consults no proof identity.
fn goal_contradiction_without_proofs(
    state: &FactState,
    matrix: DenseClosureBounds,
    distinct: WordHashSet<DistinctKey>,
    goals: &GoalTable,
) -> bool {
    let closed = ClosedState {
        all_derivable: false,
        contradiction: None,
        matrix,
        passive: PassiveReading::default(),
        distinct,
        distinct_proofs: HashMap::default(),
        opaque: state.opaque.clone(),
        opaque_proofs: HashMap::default(),
    };
    goals.ids().any(|goal| {
        closed.derives_goal(goal, GoalSign::Positive, goals)
            && closed.derives_goal(goal, GoalSign::Negative, goals)
    })
}

/// The ordinary closure with one fresh, not-yet-in-scope receiver withheld
/// from the implicit term universe. Every `value_if` edge uses this same
/// narrow boundary, including edges visited after an earlier edge interned
/// the receiver's stable identity.
pub(crate) fn close_excluding_term(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
    excluded: TermId,
) -> ClosedState {
    close_with_excluded_term(state, terms, goals, ledger, Some(excluded))
}

fn close_with_excluded_term(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
    excluded: Option<TermId>,
) -> ClosedState {
    let closed = close_with_row_pruning::<true, false>(state, terms, goals, ledger, excluded);
    super::work::closure(closed.matrix.dimension, closed.matrix.live);
    closed
}

/// One closure implementation; tests instantiate the unpruned traversal and
/// the reference product loop to compare their complete facts and selected
/// derivations with the ordinary pruned, contiguous traversal.
fn close_with_row_pruning<const PRUNE_ROWS: bool, const REFERENCE_PRODUCT: bool>(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
    excluded: Option<TermId>,
) -> ClosedState {
    if state.all_derivable {
        return ClosedState {
            all_derivable: true,
            contradiction: state.contradiction,
            matrix: DenseClosureBounds::new(0),
            passive: PassiveReading::default(),
            distinct: HashSet::default(),
            distinct_proofs: HashMap::default(),
            opaque: HashSet::default(),
            opaque_proofs: HashMap::default(),
        };
    }
    let term_count = terms.ids().count();
    let universe = closure_universe(state, terms, goals, Some(ledger));
    if excluded.is_none()
        && !REFERENCE_PRODUCT
        && let Some(closed) = close_by_edge_insertion(state, terms, goals, &universe, ledger)
    {
        #[cfg(test)]
        if tests::verifying_seeded_closures() {
            tests::assert_seeded_closure_matches_complete(state, terms, goals, ledger, &closed);
        }
        return closed;
    }
    if excluded.is_none()
        && !REFERENCE_PRODUCT
        && let Some(closed) = close_from_view_seed(state, terms, goals, &universe, ledger)
    {
        #[cfg(test)]
        if tests::verifying_seeded_closures() {
            tests::assert_seeded_closure_matches_complete(state, terms, goals, ledger, &closed);
        }
        return closed;
    }
    if excluded.is_none() && state.closure.is_closed_over(term_count) {
        #[cfg(test)]
        tests::record_route(tests::ClosureRoute::Closed);
        let mut closed = ClosedState {
            all_derivable: false,
            contradiction: None,
            matrix: DenseClosureBounds::values_from_store(term_count, &state.bounds, &universe),
            passive: passive_bounds(terms, &universe, None),
            distinct: (*state.distinct).clone(),
            distinct_proofs: (*state.distinct_proofs).clone(),
            opaque: state.opaque.clone(),
            opaque_proofs: state.opaque_proofs.clone(),
        };
        // The marker says the stored relations are already the least closure
        // over this term universe, so the fixed point would add nothing. The
        // [ENT-2] implicit bounds are re-emitted regardless: they hold at
        // every program point by term kind, never by surviving in a map.
        for id in terms.ids().filter(|id| universe.contains(*id)) {
            for_each_implicit_bound(terms, id, |left, right, bound, kind| {
                restore_implicit_bound(&mut closed, left, right, bound, kind, ledger);
            });
        }
        let closed = close_goal_contradictions(closed, goals, ledger);
        #[cfg(test)]
        if tests::verifying_seeded_closures() {
            tests::assert_seeded_closure_matches_complete(state, terms, goals, ledger, &closed);
        }
        return closed;
    }
    let mut distinct = (*state.distinct).clone();
    let mut distinct_proofs = (*state.distinct_proofs).clone();
    // The dense matrix is the only live bound index while the fixed point
    // runs; every rule reads it and nothing reads the maps. Rebuilding the
    // maps once from the settled matrix keeps their exact content while
    // dropping one hashed pair insert per accepted candidate, of which
    // `tests/programs/wfgrep.wf` accepts eighteen million.
    let mut dense_bounds =
        DenseClosureBounds::from_store(term_count, &state.bounds, &universe, ledger);
    // A closed core seeds the fixed point: its cells start stale, so the first
    // round visits only triples through a fresh cell, and a candidate must
    // strictly lower a bound. Equal-bound candidates would replace the core's
    // retained proofs throughout the matrix for no change in any bound.
    let seeded = excluded.is_none() && dense_bounds.seed_from(&state.closure);
    #[cfg(test)]
    tests::record_route(if seeded {
        tests::ClosureRoute::Seeded
    } else {
        tests::ClosureRoute::Unseeded
    });
    let ids = terms
        .ids()
        .filter(|id| Some(*id) != excluded && universe.contains(*id))
        .collect::<Vec<_>>();
    let active_middles = closure_middle_terms(state, terms, goals, &ids, Some(ledger));
    {
        let mut add = |left: TermId,
                       right: TermId,
                       bound: i128,
                       kind: ImplicitBoundKind,
                       ledger: &mut DerivationLedger| {
            // A bound toward a term read through zero is already that
            // reading.
            if !dense_bounds.holds(left) || !dense_bounds.holds(right) {
                return;
            }
            let node = DerivationNode::ImplicitBound {
                left,
                right,
                bound,
                kind,
            };
            insert_closed_candidate(
                &mut dense_bounds,
                ClosedBoundCandidate {
                    left,
                    right,
                    bound,
                    node,
                },
                seeded,
                ledger,
            );
        };
        // Implicit facts [ENT-2]. They are a function of the term table and
        // the place's type alone, so every closure re-emits all of them.
        for id in ids.iter().copied() {
            for_each_implicit_bound(terms, id, |left, right, bound, kind| {
                add(left, right, bound, kind, ledger);
            });
        }
    }
    // Least fixed point of transitivity (1), disequality strengthening (2),
    // and subsumption (3). The rules are monotone over finitely many ordered
    // pairs; strengthening uses only the finite established exclusions.
    loop {
        dense_bounds.begin_round();
        let mut changed = false;
        for middle in ids.iter().filter(|id| active_middles.contains(**id)) {
            // Preserve the former dense TermId order while skipping absent
            // matrix cells.  Transitivity cannot add a new incoming or
            // outgoing key for `middle` while processing this middle: the
            // only candidate with `left == middle` or `right == middle`
            // already requires that same key plus the reflexive diagonal.
            // Proofs for existing keys may improve, so fetch their current
            // values inside the product rather than snapshotting them here.
            let incoming = ids
                .iter()
                .copied()
                .filter(|left| dense_bounds.get(*left, *middle).is_some())
                .collect::<Vec<_>>();
            let outgoing = ids
                .iter()
                .copied()
                .filter(|right| dense_bounds.get(*middle, *right).is_some())
                .collect::<Vec<_>>();
            // Semi-naive round: a triple whose two premise cells both still
            // hold the values they held when this same traversal last offered
            // it builds the identical candidate node, and the conclusion cell
            // has only improved since, so `insert_closed_candidate` would
            // reject it without touching the ledger. Skipping such a triple
            // removes work and nothing else; the surviving triples keep their
            // former order, so the accepted sequence is unchanged. When no
            // incident cell of `middle` is fresh, none of its triples can
            // become fresh either, because the block changes nothing.
            if !incoming
                .iter()
                .any(|left| dense_bounds.fresh(*left, *middle))
                && !outgoing
                    .iter()
                    .any(|right| dense_bounds.fresh(*middle, *right))
            {
                continue;
            }
            changed |= if REFERENCE_PRODUCT {
                reference_middle_products::<PRUNE_ROWS>(
                    &mut dense_bounds,
                    *middle,
                    &incoming,
                    &outgoing,
                    seeded,
                    ledger,
                )
            } else if seeded {
                middle_products::<PRUNE_ROWS, true>(
                    &mut dense_bounds,
                    *middle,
                    &incoming,
                    &outgoing,
                    ledger,
                )
            } else {
                middle_products::<PRUNE_ROWS, false>(
                    &mut dense_bounds,
                    *middle,
                    &incoming,
                    &outgoing,
                    ledger,
                )
            };
        }
        // Materialize zero-offset disequalities from strict bounds for
        // ENT-5's finite candidate rule. Nonzero candidates come only from
        // established facts; ENT-4 derives other offsets on demand.
        for left in &ids {
            for right in &ids {
                if !zero_distinct_candidate(terms, *left, *right)
                    || dense_bounds
                        .get(*left, *right)
                        .is_none_or(|(bound, _)| bound > -1)
                {
                    continue;
                }
                let pair = distinct_key(*left, *right, 0);
                let (_, parent) = dense_bounds
                    .get(*left, *right)
                    .expect("strict bound checked above");
                let node = DerivationNode::DisequalityFromStrictBound {
                    left: pair.0,
                    right: pair.1,
                    difference: pair.2,
                    parent,
                };
                let accepted = distinct_proofs
                    .get(&pair)
                    .is_none_or(|current| !seeded && ledger.candidate_better(&node, *current));
                if accepted {
                    let proof = ledger.intern(node);
                    distinct.insert(pair);
                    distinct_proofs.insert(pair, proof);
                    changed = true;
                }
            }
        }
        let mut distinct_pairs: Vec<_> = distinct.iter().copied().collect();
        distinct_pairs.sort_unstable();
        for (left, right, difference) in distinct_pairs {
            for (from, to, offset) in [
                (left, right, difference),
                (right, left, difference.saturating_neg()),
            ] {
                if let Some((held, weak)) = dense_bounds.get(from, to)
                    && held == offset
                    && offset.saturating_sub(1) < offset
                {
                    let node = DerivationNode::StrengthenedBound {
                        left: from,
                        right: to,
                        bound: offset.saturating_sub(1),
                        weak,
                        distinct: distinct_proofs[&(left, right, difference)],
                    };
                    changed |= insert_closed_candidate(
                        &mut dense_bounds,
                        ClosedBoundCandidate {
                            left: from,
                            right: to,
                            bound: offset.saturating_sub(1),
                            node,
                        },
                        seeded,
                        ledger,
                    );
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut contradiction = None;
    for id in &ids {
        if let Some((bound, parent)) = dense_bounds.get(*id, *id) {
            if bound >= 0 {
                continue;
            }
            let candidate = ledger.intern(DerivationNode::L0Contradiction { term: *id, parent });
            if contradiction.is_none_or(|current| ledger.better(candidate, current)) {
                contradiction = Some(candidate);
            }
        }
    }
    let closed = ClosedState {
        all_derivable: contradiction.is_some(),
        contradiction,
        matrix: dense_bounds,
        passive: passive_bounds(terms, &universe, excluded),
        distinct,
        distinct_proofs,
        opaque: state.opaque.clone(),
        opaque_proofs: state.opaque_proofs.clone(),
    };
    let closed = close_goal_contradictions(closed, goals, ledger);
    #[cfg(test)]
    if seeded && tests::verifying_seeded_closures() {
        tests::assert_seeded_closure_matches_complete(state, terms, goals, ledger, &closed);
    }
    closed
}

/// Closes a state whose record is a closed core with only unprocessed edges:
/// fresh cells, the implicit bounds of terms that carry no other fact, and
/// weakened cells. Returns `None` for an unknown record and for a closed state
/// that needs no work.
///
/// A weakened cell is first rederived through every middle term until no
/// weakened cell improves. Every other cell is unchanged and still holds an
/// independently live proof, so no weakening can make it stronger than the
/// closure of the remaining facts; and a repaired cell satisfies every
/// triangle through it. The repaired cells then enter as edges, which settles
/// their disequality and strengthening consequences.
///
/// Each edge `a - b <= w` is inserted once: every `i - b` first improves
/// through `i - a`, then every row whose `i - b` is now at most `i - a + w`
/// recomposes `i - j` through `b - j`. A triple `i - m`, `m - j` is composed
/// when its later-set premise is set, and a triple of two core cells was
/// already closed, so the matrix is closed when no edge remains. A row whose
/// `i - b` is strictly below `i - a + w` owes nothing to this edge; a row that
/// merely equals it is recomposed as well as an improved one, because an
/// unprocessed fresh cell can already have been used at its raw value. An improved bound that becomes strict adds its disequality;
/// a zero bound over a disequality is strengthened as a further edge.
/// Only strictly smaller bounds replace a cell, so core proofs are retained.
fn insert_fresh_edges<P: ClosureProofs>(
    state: &FactState,
    terms: &TermTable,
    universe: &ActiveMiddles,
    ledger: &mut P,
) -> Option<EdgeClosure> {
    let term_count = terms.ids().count();
    let missing_row = terms
        .ids()
        .any(|id| id != ZERO && universe.contains(id) && state.bounds.get(id, ZERO).is_none());
    type Cells<'a> = &'a [(TermId, TermId)];
    let (core_terms, fresh_terms, fresh_cells, weakened_cells): (usize, &[TermId], Cells, Cells) =
        match &state.closure {
            ClosureRecord::Closed { terms } if (*terms as usize) < term_count => {
                (*terms as usize, &[], &[], &[])
            }
            // A term that has become part of the universe since, by a signed
            // goal that names it, has no row yet; its implicit edges are
            // inserted as an absent cell's are.
            ClosureRecord::Closed { terms: core } if missing_row => (*core as usize, &[], &[], &[]),
            ClosureRecord::Core {
                terms,
                fresh_terms,
                fresh_cells,
                weakened_cells,
            } => (*terms as usize, fresh_terms, fresh_cells, weakened_cells),
            _ => return None,
        };
    // Repairing a weakened cell scans one row and column per pass. When a
    // removal weakens more cells than there are terms, the seeded fixed
    // point, which revisits only the weakened endpoints' rows and columns, is
    // the cheaper route to the same bounds.
    if weakened_cells.len() > term_count {
        #[cfg(test)]
        tests::record_route(tests::ClosureRoute::LargeFallback);
        return None;
    }
    let width = term_count;
    let mut dense = DenseClosureBounds::values_from_store(width, &state.bounds, universe);
    let distinct = (*state.distinct).clone();
    let distinct_proofs = (*state.distinct_proofs).clone();

    let mut fresh = vec![false; width];
    for term in fresh_terms {
        if let Some(slot) = fresh.get_mut(term.0 as usize) {
            *slot = true;
        }
    }
    for slot in fresh.iter_mut().skip(core_terms) {
        *slot = true;
    }
    // Pending edges, first the implicit bounds touching a fresh term (whose
    // other facts are gone) or stronger than their cell — a measure's
    // standing bound can be registered after the core closed — then every
    // fresh cell at its current value.
    let mut pending = pending_implicit_edges(terms, universe, &fresh, &dense, ledger);
    let mut weakened = weakened_cells.to_vec();
    weakened.sort_unstable();
    weakened.dedup();
    #[cfg(test)]
    if !weakened.is_empty() {
        tests::record_route(tests::ClosureRoute::Repair);
    }
    // Each pass extends every repaired path by at least one more weakened
    // cell, so a satisfiable state settles within one pass per weakened cell.
    // A repair after that is a negative cycle through raw cells, which would
    // lower the cells forever: the seeded fixed point over the weakened
    // endpoints decides that state instead.
    let mut passes = 0;
    loop {
        if passes > weakened.len() {
            #[cfg(test)]
            tests::record_route(tests::ClosureRoute::RepairFallback);
            return None;
        }
        passes += 1;
        let mut repaired = false;
        let slots = dense.dimension;
        for &(left, right) in &weakened {
            let (Some(row), Some(column)) = (dense.slot(left), dense.slot(right)) else {
                continue;
            };
            if row == column {
                continue;
            }
            for middle in 0..slots {
                if middle == row || middle == column {
                    continue;
                }
                let (first, second) = (row * slots + middle, middle * slots + column);
                if dense.stamps[first] == 0 || dense.stamps[second] == 0 {
                    continue;
                }
                let via = compose_transitive_bounds(dense.bounds[first], dense.bounds[second]);
                let target = row * slots + column;
                if dense.stamps[target] != 0 && via >= dense.bounds[target] {
                    continue;
                }
                let node = ledger.intern(DerivationNode::TransitiveBound {
                    left,
                    middle: dense.terms[middle],
                    right,
                    bound: via,
                    first: dense.proofs[first],
                    second: dense.proofs[second],
                });
                dense.set(left, right, via, node, ledger.depth(node));
                repaired = true;
            }
        }
        if !repaired {
            break;
        }
    }
    let mut cells = fresh_cells.to_vec();
    cells.extend(weakened);
    cells.sort_unstable();
    cells.dedup();
    for (left, right) in cells {
        if let Some((bound, proof)) = dense.get(left, right) {
            pending.push_back((left, right, bound, proof));
        }
        if let Some((bound, weak)) = dense.get(left, right)
            && bound.saturating_sub(1) < bound
            && let Some(parent) = distinct_proofs
                .get(&distinct_key(left, right, bound))
                .copied()
        {
            let proof = ledger.intern(DerivationNode::StrengthenedBound {
                left,
                right,
                bound: bound.saturating_sub(1),
                weak,
                distinct: parent,
            });
            pending.push_back((left, right, bound.saturating_sub(1), proof));
        }
    }

    Some(insert_pending_edges(
        dense,
        distinct,
        distinct_proofs,
        pending,
        terms,
        ledger,
    ))
}

/// The pending edge list's first part: every implicit bound touching a
/// fresh term, whose other facts are gone, or stronger than its cell — a
/// measure's standing bound can be registered after the core closed.
fn pending_implicit_edges<P: ClosureProofs>(
    terms: &TermTable,
    universe: &ActiveMiddles,
    fresh: &[bool],
    dense: &DenseClosureBounds,
    ledger: &mut P,
) -> PendingEdges {
    let mut pending = PendingEdges::new();
    for id in terms.ids().filter(|id| universe.contains(*id)) {
        for_each_implicit_bound(terms, id, |left, right, bound, kind| {
            if fresh[left.0 as usize]
                || fresh[right.0 as usize]
                || dense.get(left, right).is_none_or(|(held, _)| bound < held)
            {
                let proof = ledger.intern(DerivationNode::ImplicitBound {
                    left,
                    right,
                    bound,
                    kind,
                });
                pending.push_back((left, right, bound, proof));
            }
        });
    }
    pending
}

/// Edges `left - right <= bound` waiting to enter a closed matrix, in order.
type PendingEdges = std::collections::VecDeque<(TermId, TermId, i128, DerivationId)>;

/// Inserts each pending edge into a matrix that is closed apart from those
/// edges, as [`insert_fresh_edges`] describes, until none remains.
fn insert_pending_edges<P: ClosureProofs>(
    mut dense: DenseClosureBounds,
    mut distinct: WordHashSet<DistinctKey>,
    mut distinct_proofs: WordHashMap<DistinctKey, DerivationId>,
    mut pending: PendingEdges,
    terms: &TermTable,
    ledger: &mut P,
) -> EdgeClosure {
    let width = dense.dimension;
    // A cell improved by insertion: record its disequality or strengthening
    // consequence, exactly the (2) rule and the strict-bound disequality the
    // fixed point applies.
    let settle = |dense: &DenseClosureBounds,
                  distinct: &mut WordHashSet<DistinctKey>,
                  distinct_proofs: &mut WordHashMap<DistinctKey, DerivationId>,
                  pending: &mut PendingEdges,
                  ledger: &mut P,
                  left: TermId,
                  right: TermId| {
        if left == right {
            return;
        }
        let Some((bound, proof)) = dense.get(left, right) else {
            return;
        };
        let pair = distinct_key(left, right, 0);
        if bound <= -1 && zero_distinct_candidate(terms, left, right) && !distinct.contains(&pair) {
            let node = ledger.intern(DerivationNode::DisequalityFromStrictBound {
                left: pair.0,
                right: pair.1,
                difference: pair.2,
                parent: proof,
            });
            distinct.insert(pair);
            distinct_proofs.insert(pair, node);
            if let Some((0, weak)) = dense.get(right, left) {
                let strengthened = ledger.intern(DerivationNode::StrengthenedBound {
                    left: right,
                    right: left,
                    bound: -1,
                    weak,
                    distinct: node,
                });
                pending.push_back((right, left, -1, strengthened));
            }
        }
        if bound.saturating_sub(1) < bound
            && let Some(parent) = distinct_proofs
                .get(&distinct_key(left, right, bound))
                .copied()
        {
            let strengthened = ledger.intern(DerivationNode::StrengthenedBound {
                left,
                right,
                bound: bound.saturating_sub(1),
                weak: proof,
                distinct: parent,
            });
            pending.push_back((left, right, bound.saturating_sub(1), strengthened));
        }
    };

    let mut tight_rows = Vec::new();
    let mut improving_columns = Vec::new();
    while let Some((a, b, weight, proof)) = pending.pop_front() {
        // An edge with a term read through zero is never tighter than that
        // reading (see [`closure_middle_terms`]).
        let (Some(a_slot), Some(b_slot)) = (dense.slot(a), dense.slot(b)) else {
            continue;
        };
        let edge_cell = a_slot * width + b_slot;
        if dense.stamps[edge_cell] != 0 && dense.bounds[edge_cell] < weight {
            continue;
        }
        if dense.stamps[edge_cell] == 0 || weight < dense.bounds[edge_cell] {
            dense.set(a, b, weight, proof, ledger.depth(proof));
        }
        // A fresh cell enters at its current value without being set, so its
        // own disequality or strengthening consequence is settled here.
        settle(
            &dense,
            &mut distinct,
            &mut distinct_proofs,
            &mut pending,
            ledger,
            a,
            b,
        );
        let (weight, proof) = (dense.bounds[edge_cell], dense.proofs[edge_cell]);
        tight_rows.clear();
        tight_rows.push(a);
        // Column pass: i - a <= x and a - b <= w give i - b <= x + w.
        for i in 0..width {
            let into_a = i * width + a_slot;
            if i == a_slot || dense.stamps[into_a] == 0 {
                continue;
            }
            let via = compose_transitive_bounds(dense.bounds[into_a], weight);
            let target = i * width + b_slot;
            if dense.stamps[target] != 0 && via >= dense.bounds[target] {
                if via == dense.bounds[target] {
                    tight_rows.push(dense.terms[i]);
                }
                continue;
            }
            let left = dense.terms[i];
            let node = ledger.intern(DerivationNode::TransitiveBound {
                left,
                middle: a,
                right: b,
                bound: via,
                first: dense.proofs[into_a],
                second: proof,
            });
            dense.set(left, b, via, node, ledger.depth(node));
            settle(
                &dense,
                &mut distinct,
                &mut distinct_proofs,
                &mut pending,
                ledger,
                left,
                b,
            );
            tight_rows.push(left);
        }
        // Row pass: i - b <= y and b - j <= z give i - j <= y + z. A tight row
        // has i - b equal to i - a + w, so a column where w + (b - j) exceeds
        // a - j is already dominated by i - a and a - j, a triangle composed
        // when its later-set premise was set; only the other columns are
        // scanned. A fresh term's implicit edge thus fills one column.
        let b_row = b_slot * width;
        let a_row = a_slot * width;
        improving_columns.clear();
        for j in 0..width {
            let out_of_b = b_row + j;
            if j == b_slot || dense.stamps[out_of_b] == 0 {
                continue;
            }
            let through_b = compose_transitive_bounds(weight, dense.bounds[out_of_b]);
            if dense.stamps[a_row + j] == 0 || through_b <= dense.bounds[a_row + j] {
                improving_columns.push(j);
            }
        }
        for &left in &tight_rows {
            let left_row = dense.slots[left.0 as usize] as usize * width;
            let into_b = left_row + b_slot;
            if dense.stamps[into_b] == 0 {
                continue;
            }
            let (first, first_proof) = (dense.bounds[into_b], dense.proofs[into_b]);
            for &j in &improving_columns {
                let out_of_b = b_row + j;
                let via = compose_transitive_bounds(first, dense.bounds[out_of_b]);
                let target = left_row + j;
                if dense.stamps[target] != 0 && via >= dense.bounds[target] {
                    continue;
                }
                let right = dense.terms[j];
                let node = ledger.intern(DerivationNode::TransitiveBound {
                    left,
                    middle: b,
                    right,
                    bound: via,
                    first: first_proof,
                    second: dense.proofs[out_of_b],
                });
                dense.set(left, right, via, node, ledger.depth(node));
                settle(
                    &dense,
                    &mut distinct,
                    &mut distinct_proofs,
                    &mut pending,
                    ledger,
                    left,
                    right,
                );
            }
        }
    }

    EdgeClosure {
        dense,
        distinct,
        distinct_proofs,
    }
}

/// The settled matrix and disequalities of [`insert_fresh_edges`].
struct EdgeClosure {
    dense: DenseClosureBounds,
    distinct: WordHashSet<DistinctKey>,
    distinct_proofs: WordHashMap<DistinctKey, DerivationId>,
}

/// Where an edge-insertion closure files the proofs of the facts it derives.
trait ClosureProofs {
    fn intern(&mut self, node: DerivationNode) -> DerivationId;
    fn depth(&self, proof: DerivationId) -> u32;
}

impl ClosureProofs for DerivationLedger {
    fn intern(&mut self, node: DerivationNode) -> DerivationId {
        DerivationLedger::intern(self, node)
    }

    fn depth(&self, proof: DerivationId) -> u32 {
        DerivationLedger::depth(self, proof)
    }
}

/// A proof-free closure: derived facts carry a placeholder identity that no
/// caller reads, so the same insertion answers value questions alone.
struct NoProofs;

impl ClosureProofs for NoProofs {
    fn intern(&mut self, _: DerivationNode) -> DerivationId {
        DerivationId(0)
    }

    fn depth(&self, _: DerivationId) -> u32 {
        0
    }
}

/// [`insert_fresh_edges`] with proofs, completed as a closed state.
fn close_by_edge_insertion(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    universe: &ActiveMiddles,
    ledger: &mut DerivationLedger,
) -> Option<ClosedState> {
    let EdgeClosure {
        dense,
        distinct,
        distinct_proofs,
    } = insert_fresh_edges(state, terms, universe, ledger)?;
    #[cfg(test)]
    tests::record_route(tests::ClosureRoute::InsertionWithProofs);
    let mut contradiction = None;
    for id in terms.ids() {
        if let Some((bound, parent)) = dense.get(id, id) {
            if bound >= 0 {
                continue;
            }
            let candidate = ledger.intern(DerivationNode::L0Contradiction { term: id, parent });
            if contradiction.is_none_or(|current| ledger.better(candidate, current)) {
                contradiction = Some(candidate);
            }
        }
    }
    let closed = ClosedState {
        all_derivable: contradiction.is_some(),
        contradiction,
        matrix: dense,
        passive: passive_bounds(terms, universe, None),
        distinct,
        distinct_proofs,
        opaque: state.opaque.clone(),
        opaque_proofs: state.opaque_proofs.clone(),
    };
    Some(close_goal_contradictions(closed, goals, ledger))
}

/// Closes a state with no closure record from its view seed: the remembered
/// closed view, widened to the current term universe, receives the cells
/// that became strictly smaller since and every later term's implicit
/// bounds as edges. Returns `None` without a seed this ledger and term table
/// can continue.
///
/// The view is the complete closure of the facts the state then held, and
/// since then the state has only gained bound candidates, so inserting the
/// gained cells closes the current facts exactly as [`insert_fresh_edges`]
/// closes a recorded core. Only strictly smaller bounds replace a cell, so
/// the view's proofs are retained, as a seeded closure retains a core's.
fn close_from_view_seed(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    universe: &ActiveMiddles,
    ledger: &mut DerivationLedger,
) -> Option<ClosedState> {
    let seed = state.view_seed.as_ref()?;
    if !matches!(state.closure, ClosureRecord::Unknown)
        || seed.terms != std::ptr::from_ref(terms) as usize
        || seed.ledger != std::ptr::from_ref(ledger) as usize
        || seed.measure_replacements != terms.measure_replacements()
    {
        return None;
    }
    #[cfg(test)]
    tests::record_route(tests::ClosureRoute::ViewSeed);
    let width = terms.ids().count();
    let view = &seed.closed;
    let core_terms = view.matrix.term_count;
    // The term table only grows, so the view covers a prefix of its terms.
    debug_assert!(core_terms <= width);
    // A term the gained relations made passive leaves its cells behind: they
    // are read through zero.
    let mut dense = DenseClosureBounds::over(width, universe);
    let (slots, view_slots) = (dense.dimension, view.matrix.dimension);
    for row in 0..slots {
        let Some(from_row) = view.matrix.slot(dense.terms[row]) else {
            continue;
        };
        for column in 0..slots {
            let Some(from_column) = view.matrix.slot(dense.terms[column]) else {
                continue;
            };
            let (from, to) = (from_row * view_slots + from_column, row * slots + column);
            if view.matrix.stamps[from] != 0 {
                dense.bounds[to] = view.matrix.bounds[from];
                dense.proofs[to] = view.matrix.proofs[from];
                dense.stamps[to] = 1;
                dense.live += 1;
            }
        }
    }
    let mut fresh = vec![false; width];
    for slot in fresh.iter_mut().skip(core_terms) {
        *slot = true;
    }
    let mut pending = pending_implicit_edges(terms, universe, &fresh, &dense, ledger);
    let mut cells = seed.fresh_cells.clone();
    cells.sort_unstable();
    cells.dedup();
    for (left, right) in cells {
        if !universe.contains(left) || !universe.contains(right) {
            continue;
        }
        // The state's own selection, which the view may already improve on.
        if let Some((bound, proof)) = state.bounds.get(left, right) {
            pending.push_back((left, right, bound, proof));
            if bound.saturating_sub(1) < bound
                && let Some(parent) = view
                    .distinct_proofs
                    .get(&distinct_key(left, right, bound))
                    .copied()
            {
                let strengthened = ledger.intern(DerivationNode::StrengthenedBound {
                    left,
                    right,
                    bound: bound.saturating_sub(1),
                    weak: proof,
                    distinct: parent,
                });
                pending.push_back((left, right, bound.saturating_sub(1), strengthened));
            }
        }
    }
    let EdgeClosure {
        dense,
        distinct,
        distinct_proofs,
    } = insert_pending_edges(
        dense,
        view.distinct.clone(),
        view.distinct_proofs.clone(),
        pending,
        terms,
        ledger,
    );
    let mut contradiction = None;
    for id in terms.ids() {
        if let Some((bound, parent)) = dense.get(id, id) {
            if bound >= 0 {
                continue;
            }
            let candidate = ledger.intern(DerivationNode::L0Contradiction { term: id, parent });
            if contradiction.is_none_or(|current| ledger.better(candidate, current)) {
                contradiction = Some(candidate);
            }
        }
    }
    let closed = ClosedState {
        all_derivable: contradiction.is_some(),
        contradiction,
        matrix: dense,
        passive: passive_bounds(terms, universe, None),
        distinct,
        distinct_proofs,
        opaque: state.opaque.clone(),
        opaque_proofs: state.opaque_proofs.clone(),
    };
    Some(close_goal_contradictions(closed, goals, ledger))
}

fn close_goal_contradictions(
    mut closed: ClosedState,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
) -> ClosedState {
    if !closed.all_derivable {
        for goal in goals.ids() {
            if !closed.derives_goal(goal, GoalSign::Positive, goals)
                || !closed.derives_goal(goal, GoalSign::Negative, goals)
            {
                continue;
            }
            let positive = closed.goal_proof(goal, GoalSign::Positive, goals, ledger);
            let negative = closed.goal_proof(goal, GoalSign::Negative, goals, ledger);
            if let (Some(positive), Some(negative)) = (positive, negative) {
                let candidate = ledger.intern(DerivationNode::GoalContradiction {
                    goal,
                    positive,
                    negative,
                });
                if closed
                    .contradiction
                    .is_none_or(|current| ledger.better(candidate, current))
                {
                    closed.contradiction = Some(candidate);
                }
            }
        }
        closed.all_derivable = closed.contradiction.is_some();
    }
    closed
}

/// The terms an [ENT-4] closure computes rows for: zero, every endpoint of a
/// live relation or of an implicit edge between two nonzero terms, and every
/// term of a live signed goal. Any other term has no fact but its implicit
/// bounds through zero, so it is never a better middle than zero and its
/// closed row and column are zero's shifted by those bounds, which
/// [`ClosedState`] reads on demand instead of storing.
fn closure_universe(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ledger: Option<&DerivationLedger>,
) -> ActiveMiddles {
    #[cfg(test)]
    if tests::closing_every_term() {
        return ActiveMiddles(vec![true; terms.ids().count()]);
    }
    let ids = terms.ids().collect::<Vec<_>>();
    closure_middle_terms(state, terms, goals, &ids, ledger)
}

/// Terms that can improve a transitive path beyond the direct implicit path
/// through zero.
///
/// Every term remains an endpoint in the closed matrix. A term carrying only
/// its reflexive and integer-range edges cannot be a useful middle: entering
/// and leaving it adds a nonnegative range cycle to the path already available
/// through `ZERO` (and an equal path has greater proof depth). Live relation
/// endpoints can be useful, as can the endpoints of implicit edges between
/// distinct nonzero terms, including measure aliases and length/capacity
/// orderings. Opaque goals are included because their opposite sign can be proved
/// from a projected or normalized relation and form a contradiction.
///
/// Two kinds of edge admit no endpoint, because a path through `ZERO` is as
/// tight as each: an implicit edge `l - r <= b` between nonzero terms with
/// `b >= upper(l) + lower(r)`, the two terms' own implicit bounds through
/// zero, as `len <= cap` of a fixed-size place; and, given the ledger, a live
/// relation whose proof rests on implicit bounds alone. A path through a term
/// whose every edge is of these kinds is replaced, edge pair by edge pair, by
/// a path through zero of no greater bound, since the term's own range is
/// nonempty. Without these, a snapshot that stores the implicit cells among
/// the measures of every place the body ever named keeps all of them middles
/// for the rest of the body.
fn closure_middle_terms(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ids: &[TermId],
    ledger: Option<&DerivationLedger>,
) -> ActiveMiddles {
    let implicit_only =
        |proof: DerivationId| ledger.is_some_and(|ledger| ledger.implicit_only(proof));
    // `TermId` is the dense function-local term identity, so membership is a
    // direct index rather than a hashed probe. The old sets answered one probe
    // per live relation endpoint of every closure.
    let width = terms.ids().count();
    let mut available = vec![false; width];
    for id in ids {
        available[id.0 as usize] = true;
    }
    // The reference every elided closure is compared with uses every term
    // as a middle.
    #[cfg(test)]
    if tests::closing_every_term() {
        return ActiveMiddles(available);
    }
    let mut active = ActiveMiddles(vec![false; width]);
    let admit = |term: TermId, active: &mut ActiveMiddles| {
        if available[term.0 as usize] {
            active.0[term.0 as usize] = true;
        }
    };
    admit(ZERO, &mut active);
    // A term's bounds through zero: its stored zero cell where that is
    // tighter than its implicit bound. A stored zero cell no tighter than
    // the implicit bound says nothing the implicit bound does not. A zero
    // cell whose proof depends on a postcondition call is not used: the
    // ordinary layer, which removes such proofs, would otherwise lose a cell
    // it implies, since a snapshot gives ordinary fallbacks only to the
    // cells it stores. Without a ledger, as in the proof-free contradiction
    // probe, which reads no ordinary layer, every stored zero cell is used.
    let mut structure = implicit_structure(terms);
    let usable = |proof: DerivationId| {
        ledger.is_none_or(|ledger| !ledger.depends_on_postcondition_call(proof))
    };
    let upper = |term: TermId| {
        let implicit = structure.upper(term);
        let stored = state
            .bounds
            .get(term, ZERO)
            .filter(|(_, proof)| usable(*proof))
            .map(|(bound, _)| bound);
        match (stored, implicit) {
            (Some(stored), Some(implicit)) => Some(stored.min(implicit)),
            (stored, implicit) => stored.or(implicit),
        }
    };
    let lower = |term: TermId| {
        let implicit = structure.lower(term);
        let stored = state
            .bounds
            .get(ZERO, term)
            .filter(|(_, proof)| usable(*proof))
            .map(|(bound, _)| bound);
        match (stored, implicit) {
            (Some(stored), Some(implicit)) => Some(stored.min(implicit)),
            (stored, implicit) => stored.or(implicit),
        }
    };
    for (left, right, bound, proof) in state.bounds.cells() {
        let through_zero = if left == right {
            Some(0)
        } else if right == ZERO {
            structure.upper(left)
        } else if left == ZERO {
            structure.lower(right)
        } else {
            upper(left)
                .zip(lower(right))
                .map(|(upper, lower)| compose_transitive_bounds(upper, lower))
        };
        if !implicit_only(proof) && through_zero.is_none_or(|path| bound < path) {
            admit(left, &mut active);
            admit(right, &mut active);
        }
    }
    for &(left, right, difference) in state.distinct.iter() {
        if !state
            .distinct_proofs
            .get(&(left, right, difference))
            .is_some_and(|proof| implicit_only(*proof))
        {
            admit(left, &mut active);
            admit(right, &mut active);
        }
    }

    let mut pending_goals = state
        .opaque
        .iter()
        .map(|(goal, _)| *goal)
        .collect::<Vec<_>>();
    let mut visited_goals = HashSet::new();
    while let Some(goal) = pending_goals.pop() {
        if !visited_goals.insert(goal) {
            continue;
        }
        if let Some(relation) = goals.projection(goal) {
            for term in relation.terms() {
                admit(term, &mut active);
            }
        }
        if let Some(normalization) = goals.normalization(goal) {
            for relation in normalization.components.iter().flatten() {
                for term in relation.terms() {
                    admit(term, &mut active);
                }
            }
        }
        if let GoalExpression::Operation { arguments, .. } = goals.expression(goal) {
            pending_goals.extend(arguments.iter().filter_map(|argument| goals.id(argument)));
        }
    }

    // A standing relation can need transitivity without any written fact:
    // len(P) <= cap(P) and cap(P) == 0 imply len(P) == 0. Omitting capacity
    // as a middle loses that proof until an unrelated source read happens to
    // mention it. Derive this inventory from the complete implicit edge set,
    // so future measure rows cannot silently evade the same fixed point.
    // A component of the implicit edges that are tighter than the path
    // through zero enters whole when a fact admits one of its members, since
    // a path through it can then improve that member's relations. A component
    // no fact reaches is dormant: every edge leaving it reaches zero or is
    // dominated, so its members are read from its own closure through zero.
    let mut entered = Vec::new();
    for id in ids {
        if active.contains(*id) {
            let component = structure.find(*id);
            if structure.members.contains_key(&component) {
                entered.push(component);
            }
        }
    }
    entered.sort_unstable();
    entered.dedup();
    for component in entered {
        for member in &structure.members[&component] {
            admit(*member, &mut active);
        }
    }
    active
}

/// Dense membership of the terms the [ENT-4] fixed point uses as middles.
struct ActiveMiddles(Vec<bool>);

impl ActiveMiddles {
    fn contains(&self, term: TermId) -> bool {
        self.0[term.0 as usize]
    }
}

struct ClosedBoundCandidate {
    left: TermId,
    right: TermId,
    bound: i128,
    node: DerivationNode,
}

/// Dense scratch index for ENT-4's fixed point.
///
/// `TermId` is a dense function-local identity.  The closed result remains in
/// the long-lived maps above, but using tuple-key hash tables for every probe
/// in the transitivity cube repeatedly hashes the same two integers. This
/// index owns the evolving bounds while preserving TermId traversal and
/// proof-selection order; the settled maps are rebuilt once before the index
/// is discarded. Row summaries only reject dominated products, never supply
/// facts or proofs.
#[derive(Clone)]
struct DenseClosureBounds {
    /// The number of slots: the terms the closure computes rows for.
    dimension: usize,
    /// The number of terms registered when the closure was taken.
    term_count: usize,
    /// Each term's slot, or [`NO_SLOT`] for a term read through zero. Slots
    /// follow term order, so row-major order is sorted `(left, right)` order.
    slots: Vec<u32>,
    /// Each slot's term.
    terms: Vec<TermId>,
    /// Row-major cells over slots. A bound or proof is meaningful only where `stamps`
    /// marks the cell live; the transitivity cube reads the three columns of
    /// each probed cell from contiguous rows.
    bounds: Vec<i128>,
    proofs: Vec<DerivationId>,
    /// Zero for an absent cell, otherwise one more than the fixed-point round
    /// in which the cell last changed. Round zero covers both a cell carried
    /// in from the state and one an implicit fact established before the
    /// first round, which is what makes round 1 complete.
    stamps: Vec<u32>,
    /// Indexed by slot.
    rows: Vec<ClosureRowSummary>,
    live: usize,
    round: u32,
}

const NO_SLOT: u32 = u32::MAX;

fn term_at(index: usize) -> TermId {
    TermId(u32::try_from(index).expect("term index fits the u32 identity"))
}

/// Conservative bounds on one row's current cells. Maxima need not decrease
/// when a cell improves: overestimates only decline a pruning opportunity.
/// The minimum must follow every decrease, and the depth maximum must follow
/// a stronger bound supported by a deeper proof.
#[derive(Clone, Copy)]
struct ClosureRowSummary {
    live: usize,
    minimum: i128,
    maximum: i128,
    maximum_depth: u32,
}

impl Default for ClosureRowSummary {
    fn default() -> Self {
        Self {
            live: 0,
            minimum: i128::MAX,
            maximum: i128::MIN,
            maximum_depth: 0,
        }
    }
}

impl ClosureRowSummary {
    fn observe(&mut self, bound: i128, depth: u32, new_cell: bool) {
        self.live += usize::from(new_cell);
        self.minimum = self.minimum.min(bound);
        self.maximum = self.maximum.max(bound);
        self.maximum_depth = self.maximum_depth.max(depth);
    }

    fn rejects_product(
        &self,
        width: usize,
        first: i128,
        first_depth: u32,
        outgoing_minimum: i128,
    ) -> bool {
        if self.live != width {
            return false;
        }
        let lower = compose_transitive_bounds(first, outgoing_minimum);
        lower > self.maximum
            || (lower == self.maximum && first_depth.saturating_add(1) > self.maximum_depth)
    }
}

impl DenseClosureBounds {
    /// The store's cells among the terms of `universe`. A cell with any
    /// other endpoint is never tighter than that pair's reading through zero
    /// (see [`closure_middle_terms`]), which [`ClosedState`] gives instead.
    fn from_store(
        term_count: usize,
        store: &BoundStore,
        universe: &ActiveMiddles,
        ledger: &impl ClosureProofs,
    ) -> Self {
        let mut dense = Self::over(term_count, universe);
        for (left, right, bound, proof) in store.cells() {
            if dense.holds(left) && dense.holds(right) {
                dense.set(left, right, bound, proof, ledger.depth(proof));
            }
        }
        dense
    }

    /// [`Self::from_store`] without the row summaries only the unseeded
    /// fixed point's pruning reads; edge insertion and closed views never do.
    fn values_from_store(term_count: usize, store: &BoundStore, universe: &ActiveMiddles) -> Self {
        let mut dense = Self::over(term_count, universe);
        let width = dense.dimension;
        let sources = dense
            .terms
            .iter()
            .map(|term| store.slot(*term))
            .collect::<Vec<_>>();
        for (row, source_row) in sources.iter().enumerate() {
            let Some(source_row) = source_row else {
                continue;
            };
            for (column, source_column) in sources.iter().enumerate() {
                let Some(source_column) = source_column else {
                    continue;
                };
                let source = source_row * store.stride + source_column;
                let target = row * width + column;
                if store.present[source] {
                    dense.bounds[target] = store.bounds[source];
                    dense.proofs[target] = store.proofs[source];
                    dense.stamps[target] = 1;
                    dense.live += 1;
                }
            }
        }
        dense
    }

    /// An empty matrix with a slot for every one of `term_count` terms.
    fn new(term_count: usize) -> Self {
        Self::with_terms(term_count, (0..term_count).map(term_at).collect())
    }

    /// An empty matrix with a slot for each term of `universe`.
    fn over(term_count: usize, universe: &ActiveMiddles) -> Self {
        Self::with_terms(
            term_count,
            (0..term_count)
                .map(term_at)
                .filter(|term| universe.contains(*term))
                .collect(),
        )
    }

    fn with_terms(term_count: usize, terms: Vec<TermId>) -> Self {
        let dimension = terms.len();
        let count = dimension
            .checked_mul(dimension)
            .expect("ENT closure matrix exceeds the address space");
        let mut slots = vec![NO_SLOT; term_count];
        for (slot, term) in terms.iter().enumerate() {
            slots[term.0 as usize] = u32::try_from(slot).expect("slot fits the u32 identity");
        }
        Self {
            dimension,
            term_count,
            slots,
            terms,
            bounds: vec![i128::MAX; count],
            proofs: vec![DerivationId(0); count],
            stamps: vec![0; count],
            rows: vec![ClosureRowSummary::default(); dimension],
            live: 0,
            round: 0,
        }
    }

    /// The slot of `term`, if the closure computes a row for it.
    fn slot(&self, term: TermId) -> Option<usize> {
        self.slots
            .get(term.0 as usize)
            .filter(|slot| **slot != NO_SLOT)
            .map(|slot| *slot as usize)
    }

    fn holds(&self, term: TermId) -> bool {
        self.slot(term).is_some()
    }

    fn begin_round(&mut self) {
        self.round += 1;
    }

    /// Marks a closed core's cells stale and every other cell fresh for the
    /// first round, or returns `false` when the record claims no closed part.
    /// A core recorded over fewer terms treats every later term as fresh: its
    /// implicit bounds are the only facts it can carry.
    fn seed_from(&mut self, record: &ClosureRecord) -> bool {
        let mut fresh_rows_seed: Option<Vec<TermId>> = None;
        let (core_terms, fresh_terms, fresh_cells): (u32, &[TermId], &[(TermId, TermId)]) =
            match record {
                ClosureRecord::Unknown => return false,
                ClosureRecord::Closed { terms } => (*terms, &[], &[]),
                ClosureRecord::Core {
                    terms,
                    fresh_terms,
                    fresh_cells,
                    weakened_cells,
                } => {
                    // A weakened cell can leave any triangle through it open,
                    // and each such triangle has a premise in one of its
                    // endpoints' rows or columns.
                    let mut rows = fresh_rows_seed.take().unwrap_or_default();
                    for (left, right) in weakened_cells {
                        rows.push(*left);
                        rows.push(*right);
                    }
                    rows.extend(fresh_terms.iter().copied());
                    fresh_rows_seed = Some(rows);
                    (
                        *terms,
                        fresh_rows_seed.as_deref().unwrap_or_default(),
                        fresh_cells,
                    )
                }
            };
        // Loaded cells carry stamp 1. Starting the rounds one later makes
        // them stale, while anything set before the first round — a fresh
        // mark or an implicit bound — carries stamp 2 and stays fresh.
        self.round = 1;
        let width = self.dimension;
        let refresh = |dense: &mut Self, index: usize| {
            if dense.stamps[index] != 0 {
                dense.stamps[index] = 2;
            }
        };
        let mut fresh_rows = vec![false; width];
        for term in fresh_terms {
            if let Some(slot) = self.slot(*term) {
                fresh_rows[slot] = true;
            }
        }
        for (slot, term) in self.terms.iter().enumerate() {
            if term.0 >= core_terms {
                fresh_rows[slot] = true;
            }
        }
        for (slot, fresh) in fresh_rows.iter().enumerate() {
            if !*fresh {
                continue;
            }
            for other in 0..width {
                refresh(self, slot * width + other);
                refresh(self, other * width + slot);
            }
        }
        for (left, right) in fresh_cells {
            if let (Some(left), Some(right)) = (self.slot(*left), self.slot(*right)) {
                refresh(self, left * width + right);
            }
        }
        true
    }

    /// Whether this cell changed during the previous fixed-point round or
    /// later. Every cell reports fresh in round 1, so the first pass over the
    /// transitivity cube is the complete one.
    fn fresh(&self, left: TermId, right: TermId) -> bool {
        let stamp = self.stamps[self.index(left, right)];
        stamp == 0 || stamp >= self.round
    }

    fn index(&self, left: TermId, right: TermId) -> usize {
        let (Some(left), Some(right)) = (self.slot(left), self.slot(right)) else {
            panic!("ENT closure cell outside the closure's terms");
        };
        left * self.dimension + right
    }

    /// A present cell, or `None` for an absent one or a term without a slot.
    fn get(&self, left: TermId, right: TermId) -> Option<(i128, DerivationId)> {
        let index = self.slot(left)? * self.dimension + self.slot(right)?;
        (self.stamps[index] != 0).then(|| (self.bounds[index], self.proofs[index]))
    }

    fn product_cannot_improve(
        &self,
        left: TermId,
        middle: TermId,
        first: i128,
        first_depth: u32,
    ) -> bool {
        let (left, middle) = (self.slot(left), self.slot(middle));
        self.rows[left.expect("a closure row term")].rejects_product(
            self.dimension,
            first,
            first_depth,
            self.rows[middle.expect("a closure middle term")].minimum,
        )
    }

    fn set(&mut self, left: TermId, right: TermId, bound: i128, proof: DerivationId, depth: u32) {
        let index = self.index(left, right);
        let new_cell = self.stamps[index] == 0;
        if new_cell {
            self.live += 1;
        }
        self.rows[index / self.dimension].observe(bound, depth, new_cell);
        self.bounds[index] = bound;
        self.proofs[index] = proof;
        self.stamps[index] = self
            .round
            .checked_add(1)
            .expect("ENT closure rounds fit the u32 stamp space");
    }

    /// A cell of a closed matrix, or `None` for an absent cell, a term read
    /// through zero, or a term registered after the closure was taken.
    fn lookup(&self, left: TermId, right: TermId) -> Option<(i128, DerivationId)> {
        self.get(left, right)
    }

    /// Every present cell in row-major, that is sorted `(left, right)`, order.
    fn cells(&self) -> impl Iterator<Item = (TermId, TermId, i128, DerivationId)> + '_ {
        let width = self.dimension.max(1);
        self.stamps
            .iter()
            .enumerate()
            .filter(|(_, stamp)| **stamp != 0)
            .map(move |(index, _)| {
                (
                    self.terms[index / width],
                    self.terms[index % width],
                    self.bounds[index],
                    self.proofs[index],
                )
            })
    }
}

/// Offers every transitive candidate through one middle term, left rows in
/// `incoming` order and right columns in `outgoing` order.
///
/// This is [`reference_middle_products`] over the matrix's contiguous rows:
/// the same triples reach the same numeric and depth comparisons in the same
/// order, and every one that survives them goes through
/// `insert_closed_candidate`. Two facts let it read raw rows. Only the
/// middle's own left row can change a middle-row cell, and only the one it
/// has just visited, so a second premise is current when read and the fresh
/// middle-row columns need collecting only once before and once after that
/// row. And a left
/// row's first premise is taken once, as the reference takes it; the
/// reference re-reads only that premise's freshness, which changes within the
/// row only when a negative middle diagonal improves the premise itself. The
/// triples the reference then additionally visits pair the row's stale first
/// premise with an unchanged second one, so each rebuilds a candidate an
/// earlier round already offered against a conclusion that has only improved
/// since, and is rejected without a ledger change; reading the freshness once
/// skips exactly those triples.
fn middle_products<const PRUNE_ROWS: bool, const STRICT: bool>(
    dense: &mut DenseClosureBounds,
    middle: TermId,
    incoming: &[TermId],
    outgoing: &[TermId],
    ledger: &mut DerivationLedger,
) -> bool {
    let width = dense.dimension;
    let round = dense.round;
    let middle_slot = dense.slot(middle).expect("a closure middle term");
    let middle_row = middle_slot * width;
    let mut changed = false;
    // Columns whose second premise is fresh, for rows whose first premise is
    // not: the other columns of such a row are skipped. Only the middle's own
    // row can refresh a middle-row cell, so the list is rebuilt after it.
    let fresh_columns = |dense: &DenseClosureBounds| {
        outgoing
            .iter()
            .copied()
            .filter(|right| {
                dense.stamps[middle_row + dense.slots[right.0 as usize] as usize] >= round
            })
            .collect::<Vec<_>>()
    };
    let mut fresh_outgoing = None;
    for &left in incoming {
        let left_row = dense.slots[left.0 as usize] as usize * width;
        let first_cell = left_row + middle_slot;
        let first = dense.bounds[first_cell];
        let first_proof = dense.proofs[first_cell];
        let first_depth = ledger.depth(first_proof);
        if PRUNE_ROWS && dense.product_cannot_improve(left, middle, first, first_depth) {
            continue;
        }
        let columns = if dense.stamps[first_cell] >= round {
            outgoing
        } else {
            fresh_outgoing
                .get_or_insert_with(|| fresh_columns(dense))
                .as_slice()
        };
        for &right in columns {
            let column = dense.slots[right.0 as usize] as usize;
            let second_cell = middle_row + column;
            let via = first.saturating_add(dense.bounds[second_cell]);
            let current_cell = left_row + column;
            // An absent cell holds `i128::MAX`, which no composed bound
            // exceeds, so only a present cell's stamp needs reading here.
            let current_bound = dense.bounds[current_cell];
            if via > current_bound {
                continue;
            }
            let second_proof = dense.proofs[second_cell];
            if via == current_bound && dense.stamps[current_cell] != 0 {
                if STRICT {
                    continue;
                }
                let candidate_depth = first_depth
                    .max(ledger.depth(second_proof))
                    .saturating_add(1);
                if candidate_depth > ledger.depth(dense.proofs[current_cell]) {
                    continue;
                }
            }
            let node = DerivationNode::TransitiveBound {
                left,
                middle,
                right,
                bound: via,
                first: first_proof,
                second: second_proof,
            };
            changed |= insert_closed_candidate(
                dense,
                ClosedBoundCandidate {
                    left,
                    right,
                    bound: via,
                    node,
                },
                STRICT,
                ledger,
            );
        }
        if left == middle {
            fresh_outgoing = None;
        }
    }
    changed
}

/// The transitive product through one middle term, stated cell by cell.
/// Tests compare its complete result with [`middle_products`].
fn reference_middle_products<const PRUNE_ROWS: bool>(
    dense: &mut DenseClosureBounds,
    middle: TermId,
    incoming: &[TermId],
    outgoing: &[TermId],
    strict: bool,
    ledger: &mut DerivationLedger,
) -> bool {
    let mut changed = false;
    for &left in incoming {
        let (first, first_proof) = dense
            .get(left, middle)
            .expect("incoming closure key collected above");
        // These summaries can only prove that every scalar candidate
        // below would fail its existing numeric/depth comparison. A
        // missing destination cell or an equal-depth tie keeps the
        // original traversal, including its diagnostic selection.
        if PRUNE_ROWS
            && dense.product_cannot_improve(left, middle, first, ledger.depth(first_proof))
        {
            continue;
        }
        for &right in outgoing {
            if !dense.fresh(left, middle) && !dense.fresh(middle, right) {
                continue;
            }
            let (second, second_proof) = dense
                .get(middle, right)
                .expect("outgoing closure key collected above");
            let via = first.saturating_add(second);
            if let Some((current_bound, current_proof)) = dense.get(left, right) {
                if via > current_bound {
                    continue;
                }
                if via == current_bound {
                    if strict {
                        continue;
                    }
                    let candidate_depth = ledger
                        .depth(first_proof)
                        .max(ledger.depth(second_proof))
                        .saturating_add(1);
                    if candidate_depth > ledger.depth(current_proof) {
                        continue;
                    }
                }
            }
            let node = DerivationNode::TransitiveBound {
                left,
                middle,
                right,
                bound: via,
                first: first_proof,
                second: second_proof,
            };
            changed |= insert_closed_candidate(
                dense,
                ClosedBoundCandidate {
                    left,
                    right,
                    bound: via,
                    node,
                },
                strict,
                ledger,
            );
        }
    }
    changed
}

/// Offers one candidate to its cell. A strict insertion accepts only an absent
/// cell or a smaller bound; otherwise an equal bound is accepted when its
/// proof is shallower or wins the deterministic node tie order.
fn insert_closed_candidate(
    dense: &mut DenseClosureBounds,
    candidate: ClosedBoundCandidate,
    strict: bool,
    ledger: &mut DerivationLedger,
) -> bool {
    let ClosedBoundCandidate {
        left,
        right,
        bound,
        node,
    } = candidate;
    let accepted = match dense.get(left, right) {
        None => true,
        Some((current, _)) if bound < current => true,
        Some((current, proof)) if bound == current => {
            !strict && ledger.candidate_better(&node, proof)
        }
        Some(_) => false,
    };
    if !accepted {
        return false;
    }
    let proof = ledger.intern(node);
    dense.set(left, right, bound, proof, ledger.depth(proof));
    true
}

/// Materializes the [ENT-4] least closure before one event-kill batch.
///
/// A state already marked closed can be endpoint-filtered without another
/// closure: removing vertices from a closed difference-bound graph leaves all
/// survivor-to-survivor consequences that were true before the removal. A
/// non-closed state must take the complete fixed point first. In particular,
/// a killed middle can participate through an implicit type edge or through
/// disequality strengthening, neither of which an explicit-edge projection
/// can reproduce soundly by itself.
pub(crate) fn materialize_closure_before_kill(
    state: &mut FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
) {
    // A closed record covers the relations only: a signed goal can still
    // meet its opposite sign, whose contradiction the snapshot must promote.
    if state.all_derivable
        || (state.closure.is_closed_over(terms.ids().count()) && state.opaque.is_empty())
    {
        return;
    }
    // With no explicit relation or signed goal, closing can add only the
    // specification's pointwise implicit bounds. Eliminating one endpoint
    // from those bounds cannot create a new survivor-to-survivor conclusion,
    // and every later query recreates the same implicit bounds. Origins are
    // transfer metadata rather than independently derivable facts.
    if state.bounds.is_empty() && state.distinct.is_empty() && state.opaque.is_empty() {
        return;
    }
    let event = ledger.event(FlowEventKind::Snapshot, None);
    *state = materialize_closure_at(state, terms, goals, ledger, event);
}

/// Whether a closure of this state can start from a closed core rather than
/// from every live fact.
pub(crate) fn closure_is_seeded(state: &FactState) -> bool {
    !matches!(state.closure, ClosureRecord::Unknown)
}

/// The proof one snapshot files for one closed bound.
///
/// A bound whose closure proof is already the materialization of that same
/// bound was made independently live at an earlier snapshot and has not moved
/// since; wrapping it again would mint one node per bound per snapshot, which
/// over a body of many measured commits is quadratic in the term count and
/// linear in the number of kills. The earlier node is the same fact with the
/// same value and an earlier — that is, more honest — point of independence,
/// so it is reused.
fn materialized_bound_proof(
    ledger: &mut DerivationLedger,
    left: TermId,
    right: TermId,
    bound: i128,
    event: FlowEventId,
    parent: DerivationId,
    wrap_implicit: bool,
) -> DerivationId {
    // A bound resting on implicit bounds alone holds at every program point,
    // so it is already independently live without a snapshot boundary.
    if (!wrap_implicit && ledger.implicit_only(parent))
        || ledger.materializes_bound(parent, left, right, bound)
    {
        return parent;
    }
    ledger.intern(DerivationNode::MaterializedBound {
        left,
        right,
        bound,
        event,
        parent,
    })
}

/// Materializes the [ENT-4] least closure as a live flow state.
///
/// Ordinary queries can keep closure as an ephemeral view. S11 instead fixes
/// the complete post-capture closure *before* the counted loop's continuing
/// kill subtraction, so consequences whose support no longer includes a
/// mutable endpoint source must become independently live facts first.
/// Materializes one view at a structural snapshot event shared by all views.
pub(crate) fn materialize_closure_at(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
    event: FlowEventId,
) -> FactState {
    materialize_closure(state, terms, goals, ledger, event, false)
}

/// [`materialize_closure_at`] for S11's preheader snapshot, which files a
/// materialized proof at this event for every closed relation, implicit ones
/// included, because each counted root it captures names that snapshot as
/// its proof point.
pub(crate) fn materialize_counted_preheader_at(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
    event: FlowEventId,
) -> FactState {
    materialize_closure(state, terms, goals, ledger, event, true)
}

fn materialize_closure(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
    event: FlowEventId,
    wrap_implicit: bool,
) -> FactState {
    let closed = close(state, terms, goals, ledger);
    super::work::snapshot(closed.matrix.dimension, closed.matrix.live);
    if closed.all_derivable {
        let parent = closed.contradiction.expect("contradictory closure proof");
        let proof = ledger.intern(DerivationNode::MaterializedContradiction { event, parent });
        return FactState {
            all_derivable: true,
            contradiction: Some(proof),
            ..FactState::new()
        };
    }
    let needs_ordinary_fallback = closed.selected_relations_depend_on_postcondition_call(ledger);
    let mut bounds = BoundStore::default();
    for (left, right, bound, parent) in closed.matrix.cells() {
        let proof =
            materialized_bound_proof(ledger, left, right, bound, event, parent, wrap_implicit);
        bounds.store_single(left, right, bound, proof);
    }
    let mut distinct_proofs = HashMap::default();
    let mut distinct_keys: Vec<_> = closed.distinct.iter().copied().collect();
    distinct_keys.sort_unstable();
    for (left, right, difference) in distinct_keys {
        let parent = closed.distinct_proofs[&(left, right, difference)];
        let proof = if !wrap_implicit && ledger.implicit_only(parent) {
            parent
        } else {
            ledger.intern(DerivationNode::MaterializedDistinct {
                left,
                right,
                difference,
                event,
                parent,
            })
        };
        distinct_proofs.insert((left, right, difference), proof);
    }
    let mut opaque_proofs = HashMap::default();
    let mut opaque_keys: Vec<_> = closed.opaque.iter().copied().collect();
    opaque_keys.sort_unstable();
    for (goal, sign) in opaque_keys {
        let proof = ledger.intern(DerivationNode::MaterializedGoal {
            goal,
            sign,
            event,
            parent: closed.opaque_proofs[&(goal, sign)],
        });
        opaque_proofs.insert((goal, sign), proof);
    }
    let distinct_candidates = distinct_proofs
        .iter()
        .map(|(pair, proof)| (*pair, Candidates::One(*proof)))
        .collect();
    let mut materialized = FactState {
        closure: ClosureRecord::closed(terms.ids().count()),
        ordinary_closure: ClosureRecord::closed(terms.ids().count()),
        closed_view: std::cell::RefCell::new(None),
        view_seed: None,
        // The wrapped closure proofs keep the ancestry they wrap.
        postcondition_candidates: needs_ordinary_fallback,
        all_derivable: false,
        contradiction: None,
        bounds: Rc::new(bounds),
        distinct: Rc::new(closed.distinct.clone()),
        distinct_proofs: Rc::new(distinct_proofs),
        distinct_candidates: Rc::new(distinct_candidates),
        origins: state.origins.clone(),
        opaque: closed.opaque.clone(),
        opaque_proofs,
        goal_origins: state.goal_origins.clone(),
        ambiguous_goal_origins: state.ambiguous_goal_origins.clone(),
    };

    if !needs_ordinary_fallback {
        return materialized;
    }

    // Preserve the ordinary fallback when the canonical closure happened to
    // select an S12 proof. The second closure is not another flow walk: it is
    // the same snapshot over the same live state with S12 candidates removed,
    // and its proof candidates share the one structural snapshot event.
    let mut ordinary = state.clone();
    ordinary.retain_non_postcondition_candidates(ledger);
    let ordinary_closed = close(&ordinary, terms, goals, ledger);
    if !ordinary_closed.all_derivable {
        // Only a relation whose selected proof depends on a postcondition call
        // needs an ordinary candidate. Any other selection is derivable
        // without such calls, so it already equals the ordinary closure, which
        // has fewer facts and cannot be stronger.
        // A term without a row in the ordinary closure is read through zero
        // there, so every postcondition-dependent selection is looked up
        // rather than only the ordinary closure's stored cells.
        let dependent = materialized
            .bounds
            .cells()
            .filter(|(_, _, _, selected)| ledger.depends_on_postcondition_call(*selected))
            .map(|(left, right, _, _)| (left, right))
            .collect::<Vec<_>>();
        for (left, right) in dependent {
            let Some((bound, parent)) = ordinary_closed.cell(left, right, ledger) else {
                continue;
            };
            let proof =
                materialized_bound_proof(ledger, left, right, bound, event, parent, wrap_implicit);
            materialized.add_bound(left, right, bound, proof, ledger);
        }
        let mut keys = materialized
            .distinct_proofs
            .iter()
            .filter(|(_, proof)| ledger.depends_on_postcondition_call(**proof))
            .map(|(pair, _)| *pair)
            .filter(|pair| ordinary_closed.holds_distinct(*pair))
            .collect::<Vec<_>>();
        keys.sort_unstable();
        for (left, right, difference) in keys {
            let parent = ordinary_closed
                .distinct_proof((left, right, difference), ledger)
                .expect("the ordinary closure holds this disequality");
            let proof = if !wrap_implicit && ledger.implicit_only(parent) {
                parent
            } else {
                ledger.intern(DerivationNode::MaterializedDistinct {
                    left,
                    right,
                    difference,
                    event,
                    parent,
                })
            };
            materialized.add_distinct_candidate((left, right, difference), proof, ledger);
        }
    }
    materialized.closure = ClosureRecord::closed(terms.ids().count());
    materialized.ordinary_closure = if ordinary_closed.all_derivable {
        ClosureRecord::Unknown
    } else {
        ClosureRecord::closed(terms.ids().count())
    };
    materialized
}

/// [ENT-5] join of arm-exit states, each already taken after its scope-exit
/// kills. Each input is closed first; the join keeps, per ordered term pair,
/// the weakest bound held by all, and ENT-5's finite disequality candidates
/// held by all under ENT-4. The empty join is contradictory.
#[cfg(test)]
pub(crate) fn join(
    states: &[FactState],
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
) -> FactState {
    let event = ledger.event(FlowEventKind::Join, None);
    join_at(states, terms, goals, ledger, event)
}

/// Joins source fact states at one structural event.
pub(crate) fn join_at(
    states: &[FactState],
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
    event: FlowEventId,
) -> FactState {
    super::work::join();
    let mut joined = join_at_once(states, terms, goals, ledger, event, &[]);
    if joined.all_derivable {
        return joined;
    }
    if !joined.selected_relations_depend_on_postcondition_call(ledger) {
        return joined;
    }
    let mut ordinary_states = states.to_vec();
    let mut removed_postcondition_candidate = false;
    for state in &mut ordinary_states {
        removed_postcondition_candidate |= state.retain_non_postcondition_candidates(ledger);
    }
    if !removed_postcondition_candidate {
        // A postcondition-dependent selection with no removable candidate
        // gives the ordinary layer no closed selection to claim.
        joined.ordinary_closure = ClosureRecord::Unknown;
        return joined;
    }
    let ordinary = join_at_once(&ordinary_states, terms, goals, ledger, event, &[]);
    joined.ordinary_closure = if ordinary.all_derivable {
        ClosureRecord::Unknown
    } else {
        // A predecessor contradictory only through call-dependent facts
        // contributes nothing to the full join while its ordinary layer
        // contributes, so the ordinary join can hold rows for terms the full
        // join reads through zero. Every pair that receives an ordinary
        // fallback must hold the full join's own selection, at least as
        // strong as its reading, so the full join is retaken over those
        // terms as well.
        let ordinary_rows = ordinary.bounds.live_terms();
        if ordinary_rows
            .iter()
            .any(|term| joined.bounds.slot(*term).is_none())
        {
            joined = join_at_once(states, terms, goals, ledger, event, &ordinary_rows);
        }
        joined.merge_fallback_candidates(&ordinary, false, ledger);
        // A pair the ordinary join stores no cell for has a term read through
        // zero there; a postcondition-dependent selection of it still needs
        // that ordinary value as its fallback.
        let ordinary_closed = close(&ordinary, terms, goals, ledger);
        let dependent = joined
            .bounds
            .cells()
            .filter(|(left, right, _, selected)| {
                ledger.depends_on_postcondition_call(*selected)
                    && ordinary.bounds.get(*left, *right).is_none()
            })
            .map(|(left, right, _, _)| (left, right))
            .collect::<Vec<_>>();
        for (left, right) in dependent {
            if let Some((bound, proof)) = ordinary_closed.cell(left, right, ledger) {
                joined.add_bound(left, right, bound, proof, ledger);
            }
        }
        let mut distinct = joined
            .distinct_proofs
            .iter()
            .filter(|(pair, proof)| {
                ledger.depends_on_postcondition_call(**proof)
                    && !ordinary.distinct_candidates.contains_key(*pair)
                    && ordinary_closed.holds_distinct(**pair)
            })
            .map(|(pair, _)| *pair)
            .collect::<Vec<_>>();
        distinct.sort_unstable();
        for pair in distinct {
            if let Some(proof) = ordinary_closed.distinct_proof(pair, ledger) {
                joined.add_distinct_candidate(pair, proof, ledger);
            }
        }
        ClosureRecord::closed(terms.ids().count())
    };
    joined.closure = ClosureRecord::closed(terms.ids().count());
    joined
}

/// `also` names terms the join stores rows for beyond the rows of the
/// inputs' closures, read through each input's view.
fn join_at_once(
    states: &[FactState],
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
    event: FlowEventId,
    also: &[TermId],
) -> FactState {
    let _work = super::work::join_pass(states.len());
    // Close before filtering: a contradiction established immediately before
    // an edge is already the absorbing all-derivable state even when no kill
    // had occasion to materialize its flag.
    let closed: Vec<Rc<ClosedState>> = states
        .iter()
        .map(|state| close(state, terms, goals, ledger))
        .collect();
    let contributing: Vec<usize> = closed
        .iter()
        .enumerate()
        .filter_map(|(index, state)| (!state.contradictory()).then_some(index))
        .collect();
    let Some((&first_index, rest_indices)) = contributing.split_first() else {
        let parents = closed
            .iter()
            .enumerate()
            .map(|(ordinal, state)| JoinParent {
                ordinal: u32::try_from(ordinal)
                    .expect("ENT join predecessor ordinal exceeds the u32 identity space"),
                parent: state
                    .contradiction
                    .expect("every noncontributing join edge is contradictory"),
            })
            .collect();
        let proof = ledger.intern(DerivationNode::JoinContradiction { event, parents });
        return FactState {
            all_derivable: true,
            contradiction: Some(proof),
            ..FactState::new()
        };
    };
    let first = &closed[first_index];
    let mut bounds = BoundStore::default();
    // Every pair of terms some predecessor computed a row for. A term no
    // predecessor has a row for joins as zero's row shifted again.
    let mut row_flags = vec![false; terms.ids().count()];
    for index in &contributing {
        for (left, _, _, _) in closed[*index].matrix.cells() {
            row_flags[left.0 as usize] = true;
        }
    }
    for term in also {
        row_flags[term.0 as usize] = true;
    }
    let rows = terms
        .ids()
        .filter(|id| row_flags[id.0 as usize])
        .collect::<Vec<_>>();
    let pairs = rows
        .iter()
        .flat_map(|left| rows.iter().map(move |right| (*left, *right)))
        .collect::<Vec<_>>();
    let pairs_evaluated = pairs.len();
    for (left, right) in pairs {
        let pair = (left, right);
        let Some(bound) = first.value(left, right) else {
            continue;
        };
        let stored = first.matrix.lookup(left, right).map(|(_, proof)| proof);
        let mut weakest = bound;
        let mut same_proof = stored.is_some();
        let held = rest_indices.iter().all(|index| {
            closed[*index].value(left, right).is_some_and(|other| {
                same_proof &= other == bound
                    && closed[*index]
                        .matrix
                        .lookup(left, right)
                        .map(|(_, proof)| proof)
                        == stored;
                if other > weakest {
                    weakest = other;
                }
                true
            })
        });
        if held {
            // A temporary transitive/strengthened proof may still mention a
            // middle that the next kill removes. Even when all incoming
            // closures select it, the join must record when its conclusion
            // became independently live. Reuse only existing live facts or
            // implicit bounds, which hold at every program point.
            if let Some(shared) = stored.filter(|_| same_proof)
                && contributing.len() == closed.len()
                && (ledger.implicit_only(shared)
                    || matches!(
                        ledger.nodes[shared.0 as usize],
                        DerivationNode::SourceBound { left: l, right: r, bound: b, .. }
                            | DerivationNode::JoinBound { left: l, right: r, bound: b, .. }
                            | DerivationNode::MaterializedBound { left: l, right: r, bound: b, .. }
                            if (l, r, b) == (left, right, bound)
                    ))
            {
                bounds.store_single(left, right, bound, shared);
                continue;
            }
            let mut parents = Vec::with_capacity(states.len());
            for (ordinal, state) in closed.iter().enumerate() {
                let parent = if state.contradictory() {
                    state
                        .contradiction
                        .expect("contradictory predecessor proof")
                } else {
                    state
                        .bound_proof(pair.0, pair.1, weakest, ledger)
                        .expect("contributing predecessor proves joined bound")
                };
                parents.push(JoinParent {
                    ordinal: u32::try_from(ordinal)
                        .expect("ENT join predecessor ordinal exceeds the u32 identity space"),
                    parent,
                });
            }
            let proof = ledger.intern(DerivationNode::JoinBound {
                left: pair.0,
                right: pair.1,
                bound: weakest,
                event,
                parents,
            });
            bounds.store_single(pair.0, pair.1, weakest, proof);
        }
    }
    super::work::join_pairs(rows.len(), pairs_evaluated, bounds.live);
    // ENT-5's candidates: established offsets on contributing inputs plus
    // zero-offset disequalities materialized by closure. Derived-only
    // nonzero exclusions are queried by ENT-4, never enumerated here.
    let mut distinct_keys = contributing
        .iter()
        .flat_map(|index| {
            closed[*index]
                .distinct
                .iter()
                .copied()
                .filter(move |key| key.2 == 0 || states[*index].distinct.contains(key))
        })
        .collect::<Vec<_>>();
    distinct_keys.sort_unstable();
    distinct_keys.dedup();
    distinct_keys.retain(|pair| {
        contributing
            .iter()
            .all(|index| closed[*index].holds_distinct(*pair))
    });
    // A pair with a term no input has a row for is held through zero in each
    // input [ENT-4]; the join's own view derives it again from the joined
    // zero bounds unless the inputs bound the row term strictly above the
    // other term's range on one path and strictly below it on another, the
    // one case where the pair must be stored to stay derivable [ENT-5]. The
    // prefilter takes each side's least zero bound over the inputs holding
    // one.
    let mut outside: Option<Vec<TermId>> = None;
    for &term in &rows {
        if term == ZERO {
            continue;
        }
        let zero_bounds = contributing
            .iter()
            .map(|input| {
                let input = &closed[*input];
                (input.value(ZERO, term), input.value(term, ZERO))
            })
            .collect::<Vec<_>>();
        let lowest_from_zero = zero_bounds.iter().filter_map(|(from, _)| *from).min();
        let lowest_to_zero = zero_bounds.iter().filter_map(|(_, to)| *to).min();
        let (Some(lowest_from_zero), Some(lowest_to_zero)) = (lowest_from_zero, lowest_to_zero)
        else {
            continue;
        };
        if compose_transitive_bounds(lowest_from_zero, lowest_to_zero) > -2 {
            continue;
        }
        let joined_from_zero = zero_bounds
            .iter()
            .map(|(from, _)| *from)
            .collect::<Option<Vec<_>>>()
            .and_then(|all| all.into_iter().max());
        let joined_to_zero = zero_bounds
            .iter()
            .map(|(_, to)| *to)
            .collect::<Option<Vec<_>>>()
            .and_then(|all| all.into_iter().max());
        let outside = outside
            .get_or_insert_with(|| terms.ids().filter(|id| !row_flags[id.0 as usize]).collect());
        for &other in outside.iter() {
            if !zero_distinct_candidate(terms, other, term) {
                continue;
            }
            let to_zero = implicit_bound_between(terms, (other, ZERO)).map(|(bound, _)| bound);
            let from_zero = implicit_bound_between(terms, (ZERO, other)).map(|(bound, _)| bound);
            let derived = to_zero
                .zip(joined_from_zero)
                .is_some_and(|(to_zero, joined)| compose_transitive_bounds(to_zero, joined) <= -1)
                || joined_to_zero
                    .zip(from_zero)
                    .is_some_and(|(joined, from_zero)| {
                        compose_transitive_bounds(joined, from_zero) <= -1
                    });
            if derived {
                continue;
            }
            let pair = distinct_key(other, term, 0);
            if contributing
                .iter()
                .all(|input| closed[*input].holds_distinct(pair))
            {
                distinct_keys.push(pair);
            }
        }
    }
    distinct_keys.sort_unstable();
    distinct_keys.dedup();
    let distinct = distinct_keys.iter().copied().collect::<WordHashSet<_>>();
    let mut distinct_proofs = HashMap::default();
    for pair in distinct_keys {
        if contributing.len() == closed.len()
            && let Some(shared) = first.distinct_proofs.get(&pair).copied()
        {
            let independently_live = ledger.implicit_only(shared)
                || matches!(
                    ledger.nodes[shared.0 as usize],
                    DerivationNode::SourceDistinct { left, right, difference, .. }
                        | DerivationNode::JoinDistinct { left, right, difference, .. }
                        | DerivationNode::MaterializedDistinct { left, right, difference, .. }
                        if distinct_key(left, right, difference) == pair
                );
            if independently_live
                && rest_indices
                    .iter()
                    .all(|index| closed[*index].distinct_proofs.get(&pair) == Some(&shared))
            {
                distinct_proofs.insert(pair, shared);
                continue;
            }
        }
        let mut parents = Vec::with_capacity(states.len());
        for (ordinal, state) in closed.iter().enumerate() {
            let parent = if state.contradictory() {
                state
                    .contradiction
                    .expect("contradictory predecessor proof")
            } else {
                state
                    .distinct_proof(pair, ledger)
                    .expect("contributing predecessor holds the joined disequality")
            };
            parents.push(JoinParent {
                ordinal: u32::try_from(ordinal)
                    .expect("ENT join predecessor ordinal exceeds the u32 identity space"),
                parent,
            });
        }
        let proof = ledger.intern(DerivationNode::JoinDistinct {
            left: pair.0,
            right: pair.1,
            difference: pair.2,
            event,
            parents,
        });
        distinct_proofs.insert(pair, proof);
    }
    // Comparison and outcome origins are path conditions, not facts; one
    // survives a join only when every contributing path carries the same one.
    let mut opaque = first.opaque.clone();
    for index in rest_indices {
        opaque.retain(|fact| closed[*index].opaque.contains(fact));
    }
    let mut opaque_proofs = HashMap::default();
    let mut opaque_keys: Vec<_> = opaque.iter().copied().collect();
    opaque_keys.sort_unstable();
    for (goal, sign) in opaque_keys {
        if contributing.len() == closed.len() {
            let shared = first.opaque_proofs[&(goal, sign)];
            let independently_live = matches!(
                ledger.nodes[shared.0 as usize],
                DerivationNode::SourceGoal { goal: g, sign: s, .. }
                    | DerivationNode::BooleanLiteral { goal: g, sign: s }
                    | DerivationNode::JoinGoal { goal: g, sign: s, .. }
                    | DerivationNode::MaterializedGoal { goal: g, sign: s, .. }
                    if (g, s) == (goal, sign)
            );
            if independently_live
                && rest_indices
                    .iter()
                    .all(|index| closed[*index].opaque_proofs[&(goal, sign)] == shared)
            {
                opaque_proofs.insert((goal, sign), shared);
                continue;
            }
        }
        let mut parents = Vec::with_capacity(states.len());
        for (ordinal, state) in closed.iter().enumerate() {
            let parent = if state.contradictory() {
                state
                    .contradiction
                    .expect("contradictory predecessor proof")
            } else {
                state.opaque_proofs[&(goal, sign)]
            };
            parents.push(JoinParent {
                ordinal: u32::try_from(ordinal)
                    .expect("ENT join predecessor ordinal exceeds the u32 identity space"),
                parent,
            });
        }
        let proof = ledger.intern(DerivationNode::JoinGoal {
            goal,
            sign,
            event,
            parents,
        });
        opaque_proofs.insert((goal, sign), proof);
    }
    let contributing_states: Vec<&FactState> =
        contributing.iter().map(|index| &states[*index]).collect();
    let mut origins = contributing_states[0].origins.clone();
    let mut goal_origins = contributing_states[0].goal_origins.clone();
    let mut ambiguous_goal_origins = contributing_states[0].ambiguous_goal_origins.clone();
    for state in contributing_states.iter().skip(1) {
        origins.retain(|binding, relation| {
            state
                .origins
                .get(binding)
                .is_some_and(|other| other == relation)
        });
        goal_origins.retain(|binding, goal| {
            state
                .goal_origins
                .get(binding)
                .is_some_and(|other| other == goal)
        });
        ambiguous_goal_origins.retain(|binding| state.ambiguous_goal_origins.contains(binding));
    }
    let distinct_candidates = distinct_proofs
        .iter()
        .map(|(pair, proof)| (*pair, Candidates::One(*proof)))
        .collect();
    let postcondition_candidates = bounds
        .cells()
        .map(|(_, _, _, proof)| proof)
        .chain(distinct_proofs.values().copied())
        .any(|proof| ledger.depends_on_postcondition_call(proof));
    FactState {
        closure: ClosureRecord::closed(terms.ids().count()),
        ordinary_closure: ClosureRecord::closed(terms.ids().count()),
        closed_view: std::cell::RefCell::new(None),
        view_seed: None,
        // A merged ordinary candidate adds no postcondition ancestry.
        postcondition_candidates,
        all_derivable: false,
        contradiction: None,
        bounds: Rc::new(bounds),
        distinct: Rc::new(distinct),
        distinct_proofs: Rc::new(distinct_proofs),
        distinct_candidates: Rc::new(distinct_candidates),
        origins,
        opaque,
        opaque_proofs,
        goal_origins,
        ambiguous_goal_origins,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug)]
    pub(super) enum ClosureRoute {
        Remembered,
        Closed,
        Unseeded,
        Seeded,
        InsertionWithProofs,
        InsertionWithoutProofs,
        Repair,
        LargeFallback,
        RepairFallback,
        ViewSeed,
    }

    const CLOSURE_ROUTES: [ClosureRoute; 10] = [
        ClosureRoute::Remembered,
        ClosureRoute::Closed,
        ClosureRoute::Unseeded,
        ClosureRoute::Seeded,
        ClosureRoute::InsertionWithProofs,
        ClosureRoute::InsertionWithoutProofs,
        ClosureRoute::Repair,
        ClosureRoute::LargeFallback,
        ClosureRoute::RepairFallback,
        ClosureRoute::ViewSeed,
    ];

    thread_local! {
        /// When set, every seeded, inserted or remembered closure on this
        /// thread is recomputed from all live facts on a cloned state and
        /// ledger, and the two results must agree on every bound value,
        /// disequality and contradiction. Only the retained proofs may differ.
        static VERIFY_SEEDED_CLOSURE: Cell<bool> = const { Cell::new(false) };
        /// How many closures the switch has compared on this thread.
        static VERIFIED_CLOSURES: Cell<usize> = const { Cell::new(0) };
        static ROUTE_COUNTS: Cell<[usize; CLOSURE_ROUTES.len()]> = const { Cell::new([0; CLOSURE_ROUTES.len()]) };
    }

    pub(super) fn record_route(route: ClosureRoute) {
        if VERIFY_SEEDED_CLOSURE.with(Cell::get) {
            ROUTE_COUNTS.with(|counts| {
                let mut values = counts.get();
                values[route as usize] += 1;
                counts.set(values);
            });
        }
    }

    pub(super) fn verifying_seeded_closures() -> bool {
        let verifying = VERIFY_SEEDED_CLOSURE.with(Cell::get);
        if verifying {
            VERIFIED_CLOSURES.with(|count| count.set(count.get() + 1));
        }
        verifying
    }

    /// Every closed bound over every pair of the closure's terms, stored or
    /// read through zero for a term without a row.
    fn bound_values(closed: &ClosedState) -> Vec<(TermId, TermId, i128)> {
        let width = closed.matrix.term_count.max(closed.passive.bounds.len());
        let ids = (0..width)
            .map(|id| TermId(u32::try_from(id).expect("term index fits the u32 identity")))
            .collect::<Vec<_>>();
        ids.iter()
            .flat_map(|left| ids.iter().map(move |right| (*left, *right)))
            .filter_map(|(left, right)| closed.value(left, right).map(|bound| (left, right, bound)))
            .collect()
    }

    /// Zero-offset pairs and every offset candidate held by either view.
    fn distinct_pairs(closed: &ClosedState, other: &ClosedState) -> Vec<DistinctKey> {
        let width = closed.matrix.term_count.max(closed.passive.bounds.len());
        let ids = (0..width)
            .map(|id| TermId(u32::try_from(id).expect("term index fits the u32 identity")))
            .collect::<Vec<_>>();
        let mut candidates = ids
            .iter()
            .flat_map(|left| ids.iter().map(move |right| (*left, *right, 0)))
            .filter(|(left, right, _)| left < right)
            .chain(closed.distinct.iter().copied())
            .chain(other.distinct.iter().copied())
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        candidates.dedup();
        candidates
            .into_iter()
            .filter(|key| closed.holds_distinct(*key))
            .collect()
    }

    thread_local! {
        /// When set, closures on this thread compute a row for every term,
        /// as the reference every elided closure is compared with.
        static CLOSE_EVERY_TERM: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn closing_every_term() -> bool {
        CLOSE_EVERY_TERM.with(Cell::get)
    }

    /// Runs `run` with every closure on this thread computing every row.
    fn with_every_term<T>(run: impl FnOnce() -> T) -> T {
        let before = CLOSE_EVERY_TERM.with(|every| every.replace(true));
        let result = run();
        CLOSE_EVERY_TERM.with(|every| every.set(before));
        result
    }

    pub(super) fn assert_seeded_closure_matches_complete(
        state: &FactState,
        terms: &TermTable,
        goals: &GoalTable,
        ledger: &DerivationLedger,
        seeded: &ClosedState,
    ) {
        let mut unseeded = state.clone();
        unseeded.closure = ClosureRecord::Unknown;
        unseeded.view_seed = None;
        let mut complete_ledger = ledger.clone();
        let complete = with_every_term(|| {
            close_with_excluded_term(&unseeded, terms, goals, &mut complete_ledger, None)
        });
        assert_eq!(
            seeded.all_derivable, complete.all_derivable,
            "seeded closure contradiction differs from the complete closure"
        );
        if complete.all_derivable {
            return;
        }
        let (seeded_bounds, complete_bounds) = (bound_values(seeded), bound_values(&complete));
        if seeded_bounds != complete_bounds {
            let differing = seeded_bounds
                .iter()
                .filter(|cell| !complete_bounds.contains(cell))
                .chain(
                    complete_bounds
                        .iter()
                        .filter(|cell| !seeded_bounds.contains(cell)),
                )
                .take(8)
                .collect::<Vec<_>>();
            panic!("seeded closure bounds differ from the complete closure: {differing:?}");
        }
        assert_eq!(
            distinct_pairs(seeded, &complete),
            distinct_pairs(&complete, seeded),
            "seeded closure disequalities differ from the complete closure"
        );
        assert_eq!(seeded.opaque, complete.opaque);
        for goal in goals.ids() {
            for sign in [GoalSign::Positive, GoalSign::Negative] {
                assert_eq!(
                    seeded.derives_goal(goal, sign, goals),
                    complete.derives_goal(goal, sign, goals),
                    "seeded closure goal answer differs from the complete closure"
                );
            }
        }
    }

    // These are storage-domain checks, not a choice of source integer limits.
    // Before the repair, MIN with reversed endpoints panics in distinct_key.
    #[test]
    fn offset_extremes_canonicalize_in_both_orientations() {
        let x = TermId(1);
        for (offset, reversed) in [
            (i128::MIN, i128::MAX),
            (i128::MIN + 1, i128::MAX),
            (i128::MAX - 1, i128::MIN + 2),
            (i128::MAX, i128::MIN + 1),
        ] {
            assert_eq!(distinct_key(ZERO, x, offset), (ZERO, x, offset));
            assert_eq!(distinct_key(x, ZERO, offset), (ZERO, x, reversed));
        }
    }

    // Isolate rule 2 from type bounds: an actual i64 term's implicit range
    // would subsume a huge positive bound or contradict a huge negative one.
    // Before the repair, the MIN row panics at bound - 1 (or its reversal).
    #[test]
    fn offset_extremes_rule_two_saturates_without_requeueing_the_floor() {
        let mut terms = TermTable::new();
        let x = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::binding(BindingId(0)),
            IntegerType::I64,
        ));
        for (offset, reversed, strict) in [
            (i128::MIN, i128::MAX, i128::MIN),
            (i128::MIN + 1, i128::MAX, i128::MIN),
            (i128::MAX - 1, i128::MIN + 2, i128::MAX - 2),
            (i128::MAX, i128::MIN + 1, i128::MAX - 1),
        ] {
            for reverse in [false, true] {
                let (left, right, key) = if reverse {
                    (x, ZERO, (ZERO, x, reversed))
                } else {
                    (ZERO, x, (ZERO, x, offset))
                };
                let dense = DenseClosureBounds::with_terms(2, vec![ZERO, x]);
                let result = insert_pending_edges(
                    dense,
                    [key].into_iter().collect(),
                    [(key, DerivationId(0))].into_iter().collect(),
                    [(left, right, offset, DerivationId(0))]
                        .into_iter()
                        .collect(),
                    &terms,
                    &mut NoProofs,
                );
                assert_eq!(result.dense.get(left, right).map(|cell| cell.0), Some(strict));
            }
        }
    }

    // Before the repair, a canonically oriented MIN reaches unchecked
    // negation in the complete closure and in the contradiction probe.
    #[test]
    fn offset_extremes_close_probe_and_join_with_implicit_ranges() {
        let mut terms = TermTable::new();
        let x = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::binding(BindingId(0)),
            IntegerType::I64,
        ));
        let goals = GoalTable::default();
        for offset in [i128::MIN, i128::MIN + 1, i128::MAX - 1, i128::MAX] {
            for (left, right) in [(ZERO, x), (x, ZERO)] {
                let mut ledger = DerivationLedger::default();
                let event = ledger.event(FlowEventKind::S1, None);
                let relation = Relation::Distinct {
                    left,
                    right,
                    difference: offset,
                };
                let mut explicit = FactState::new();
                explicit.establish(&relation, &mut ledger, event);
                assert!(!contradiction_without_proofs(&explicit, &terms, &goals));
                let closed = close(&explicit, &terms, &goals, &mut ledger);
                assert!(!closed.contradictory());
                assert!(closed.derives(&relation));
                assert!(closed.relation_proof(&relation, &mut ledger).is_some());
                assert_seeded_closure_matches_complete(&explicit, &terms, &goals, &ledger, &closed);
                // No i64 value equals any of these offsets in either
                // orientation. The empty input derives the candidate too.
                let join_event = ledger.event(FlowEventKind::Join, None);
                for inputs in [
                    [explicit.clone(), FactState::new()],
                    [FactState::new(), explicit.clone()],
                ] {
                    let joined = join_at(&inputs, &terms, &goals, &mut ledger, join_event);
                    let key = distinct_key(left, right, offset);
                    assert!(joined.distinct.contains(&key));
                    assert!(joined.distinct_proofs.contains_key(&key));
                    let closed = close(&joined, &terms, &goals, &mut ledger);
                    assert!(!closed.contradictory());
                    assert!(closed.derives(&relation));
                    assert!(!closed.derives(&relation.negated()));
                }
            }
        }
    }

    // Negative origin projections use establish_from_proof; source facts
    // use establish. Both used to panic reversing the negated MIN equality.
    #[test]
    fn offset_extremes_negation_and_proved_projection_share_bound_arithmetic() {
        let x = TermId(1);
        for (offset, reverse_bound, negative_bound) in [
            (i128::MIN, i128::MAX, i128::MAX - 1),
            (i128::MIN + 1, i128::MAX, i128::MAX - 1),
            (i128::MAX - 1, i128::MIN + 2, i128::MIN + 1),
            (i128::MAX, i128::MIN + 1, i128::MIN),
        ] {
            for (left, right) in [(ZERO, x), (x, ZERO)] {
                let mut ledger = DerivationLedger::default();
                let event = ledger.event(FlowEventKind::S1, None);
                let distinct = Relation::Distinct {
                    left,
                    right,
                    difference: offset,
                };
                let equality = distinct.negated();
                let mut source = FactState::new();
                source.establish(&equality, &mut ledger, event);
                assert_eq!(source.bounds.get(left, right).unwrap().0, offset);
                assert_eq!(source.bounds.get(right, left).unwrap().0, reverse_bound);
                let proof = source.bound_parent(left, right, offset).unwrap();
                let mut projected = FactState::new();
                projected.establish_from_proof(&equality, proof, &ledger);
                assert_eq!(projected.bounds.get(right, left).unwrap().0, reverse_bound);
                let bound = Relation::Bound {
                    left,
                    right,
                    bound: offset,
                };
                assert_eq!(
                    bound.negated(),
                    Relation::Bound {
                        left: right,
                        right: left,
                        bound: negative_bound,
                    }
                );
            }
        }
    }

    /// The old state drops every nonzero Distinct relation. These checks
    /// therefore fail at storage, before either closure implementation can
    /// tighten the later bound.
    #[test]
    fn offset_disequalities_strengthen_later_bounds_in_both_orientations() {
        let mut terms = TermTable::new();
        let mut place = |binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::spelled(
                    super::super::term::PlaceRoot::Binding(BindingId(binding)),
                    false,
                    Vec::new(),
                ),
                IntegerType::I64,
            ))
        };
        let x = place(0);
        let y = place(1);
        let middle = place(2);
        let goals = GoalTable::default();
        for difference in [4, -4, i128::from(i64::MIN), i128::from(u64::MAX) - 1] {
            for reverse in [false, true] {
                let mut ledger = DerivationLedger::default();
                let event = ledger.event(FlowEventKind::S1, None);
                let mut state = FactState::new();
                let distinct = Relation::Distinct {
                    left: x,
                    right: y,
                    difference,
                };
                state.establish(&distinct, &mut ledger, event);
                assert!(state.distinct.contains(&distinct_key(x, y, difference)));
                let initial = close(&state, &terms, &goals, &mut ledger);
                assert!(initial.derives(&distinct));
                let snapshot = state.numeric_snapshot();
                assert_eq!(snapshot.live_l0_relations(), state.live_l0_relations());
                materialize_closure_before_kill(&mut state, &terms, &goals, &mut ledger);
                let snapshot_proof = state.distinct_proofs[&distinct_key(x, y, difference)];
                assert!(matches!(ledger.nodes[snapshot_proof.0 as usize],
                    DerivationNode::MaterializedDistinct { difference: held, .. } if held == difference
                ));
                let (left, right, offset) = if reverse {
                    (y, x, -difference)
                } else {
                    (x, y, difference)
                };
                for relation in [
                    Relation::Bound {
                        left,
                        right: middle,
                        bound: 0,
                    },
                    Relation::Bound {
                        left: middle,
                        right,
                        bound: offset,
                    },
                ] {
                    state.establish(&relation, &mut ledger, event);
                }
                let closed = close(&state, &terms, &goals, &mut ledger);
                assert!(!closed.contradictory());
                assert_eq!(closed.tight_bound(left, right), Some(offset - 1));
                assert_seeded_closure_matches_complete(&state, &terms, &goals, &ledger, &closed);
                let proof = closed
                    .relation_proof(
                        &Relation::Bound {
                            left,
                            right,
                            bound: offset - 1,
                        },
                        &mut ledger,
                    )
                    .unwrap();
                assert!(matches!(ledger.nodes[proof.0 as usize],
                    DerivationNode::StrengthenedBound { bound, .. } if bound == offset - 1
                ));
                materialize_closure_before_kill(&mut state, &terms, &goals, &mut ledger);
                state.kill(|term| term == middle);
                assert!(close(&state, &terms, &goals, &mut ledger).derives_bound(
                    left,
                    right,
                    offset - 1
                ));
                state.kill(|term| term == x);
                assert!(!state.distinct.contains(&distinct_key(x, y, difference)));
            }
        }
    }

    #[test]
    fn offset_disequality_conflict_and_wrong_offset_use_the_general_rule() {
        let mut terms = TermTable::new();
        let x = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::spelled(
                super::super::term::PlaceRoot::Binding(BindingId(0)),
                false,
                Vec::new(),
            ),
            IntegerType::U64,
        ));
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        state.establish(
            &Relation::Distinct {
                left: x,
                right: ZERO,
                difference: 3,
            },
            &mut ledger,
            event,
        );
        state.establish(
            &Relation::Bound {
                left: x,
                right: ZERO,
                bound: 4,
            },
            &mut ledger,
            event,
        );
        assert!(!close(&state, &terms, &goals, &mut ledger).derives_bound(x, ZERO, 3));
        // Unlike the wrong-offset control, the equality conflict would be
        // missed before offset disequalities became live facts.
        state.establish(
            &Relation::Equal {
                left: x,
                right: ZERO,
                difference: 3,
            },
            &mut ledger,
            event,
        );
        assert!(contradiction_without_proofs(&state, &terms, &goals));
        assert!(close(&state, &terms, &goals, &mut ledger).contradictory());
    }

    #[test]
    fn offset_join_keeps_explicit_and_strict_bound_proofs_but_drops_a_missing_offset() {
        let mut terms = TermTable::new();
        let x = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::spelled(
                super::super::term::PlaceRoot::Binding(BindingId(0)),
                false,
                Vec::new(),
            ),
            IntegerType::U64,
        ));
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut explicit = FactState::new();
        let relation = Relation::Distinct {
            left: x,
            right: ZERO,
            difference: 4,
        };
        explicit.establish(&relation, &mut ledger, event);
        let mut strict = FactState::new();
        strict.establish(
            &Relation::Bound {
                left: x,
                right: ZERO,
                bound: 3,
            },
            &mut ledger,
            event,
        );
        let join_event = ledger.event(FlowEventKind::Join, None);
        let joined = join_at(
            &[strict.clone(), explicit.clone()],
            &terms,
            &goals,
            &mut ledger,
            join_event,
        );
        let key = distinct_key(x, ZERO, 4);
        // Before Q148 the explicit input has no stored offset candidate.
        let proof = joined.distinct_proofs[&key];
        assert!(matches!(
            ledger.nodes[proof.0 as usize],
            DerivationNode::JoinDistinct { difference: -4, .. }
        ));
        let closed = close(&joined, &terms, &goals, &mut ledger);
        assert!(closed.derives(&relation));
        assert!(!closed.derives_bound(x, ZERO, 3));
        assert!(!closed.derives_bound(ZERO, x, -5));
        // The other predecessor order must preserve the same candidate.
        let reversed = join_at(
            &[explicit.clone(), strict.clone()],
            &terms,
            &goals,
            &mut ledger,
            join_event,
        );
        assert!(close(&reversed, &terms, &goals, &mut ledger).derives(&relation));

        // Q160's boundary: neither strict-bound input establishes != 4.
        // Even an explicit candidate on a contradictory input cannot add it.
        let mut contradictory = explicit.clone();
        contradictory.establish(
            &Relation::Equal {
                left: x,
                right: ZERO,
                difference: 4,
            },
            &mut ledger,
            event,
        );
        for lower in [5, 100] {
            let mut high = FactState::new();
            high.establish(
                &Relation::Bound {
                    left: ZERO,
                    right: x,
                    bound: -lower,
                },
                &mut ledger,
                event,
            );
            for include_contradictory in [false, true] {
                let mut inputs = vec![strict.clone(), high.clone()];
                if include_contradictory {
                    inputs.push(contradictory.clone());
                }
                let boundary = join_at(&inputs, &terms, &goals, &mut ledger, join_event);
                assert!(!boundary.distinct.contains(&key));
                assert!(!close(&boundary, &terms, &goals, &mut ledger).derives(&relation));
            }
        }
        let missing = join_at(
            &[explicit, FactState::new()],
            &terms,
            &goals,
            &mut ledger,
            join_event,
        );
        assert!(!close(&missing, &terms, &goals, &mut ledger).derives(&relation));
    }

    #[test]
    fn retained_constant_terms_do_not_bypass_the_offset_join_boundary() {
        let mut terms = TermTable::new();
        let x = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::binding(BindingId(0)),
            IntegerType::U64,
        ));
        let four = terms.intern(TermKind::Constant(4));
        let goals = GoalTable::default();
        let excluded = Relation::Distinct {
            left: x,
            right: ZERO,
            difference: 4,
        };
        // The optimized join's outside-row scan and the complete closure's
        // materialized pairs used to retain x != Constant(4), evading Q160.
        for every_term in [false, true] {
            let run = || {
                let mut ledger = DerivationLedger::default();
                let event = ledger.event(FlowEventKind::S1, None);
                let mut inputs = Vec::new();
                for (left, right, bound) in [(x, ZERO, 3), (ZERO, x, -5)] {
                    let mut state = FactState::new();
                    state.establish(&Relation::Bound { left, right, bound }, &mut ledger, event);
                    assert!(close(&state, &terms, &goals, &mut ledger).derives(&excluded));
                    materialize_closure_before_kill(&mut state, &terms, &goals, &mut ledger);
                    assert!(!state.distinct.contains(&distinct_key(x, four, 0)));
                    inputs.push(state);
                }
                let mut joined = join(&inputs, &terms, &goals, &mut ledger);
                assert!(!joined.distinct.contains(&distinct_key(x, four, 0)));
                joined.establish(
                    &Relation::Bound {
                        left: x,
                        right: ZERO,
                        bound: 4,
                    },
                    &mut ledger,
                    event,
                );
                let closed = close(&joined, &terms, &goals, &mut ledger);
                assert!(!closed.derives(&excluded));
                assert!(!closed.derives_bound(x, ZERO, 3));
                assert_seeded_closure_matches_complete(&joined, &terms, &goals, &ledger, &closed);
            };
            if every_term {
                with_every_term(run);
            } else {
                run();
            }
        }
    }

    /// Compiles a real program with every seeded, inserted or remembered
    /// closure checked against the complete closure of the same state, as the
    /// flow walk reaches its kill, join, strengthening and term-growth paths.
    #[test]
    fn seeded_closures_match_complete_closures_on_real_programs() {
        // The small real program and existing Result cases exercise closure
        // records through the ordinary walk. The additional observation here
        // is agreement with an eager closure at every intermediate proof point;
        // the corpus's ordinary verdict checks do not inspect those states.
        let bundles: [&[(&str, &[u8])]; 3] = [
            &[(
                "utf8parse.wf",
                include_bytes!("../../../../tests/programs/utf8parse.wf"),
            )],
            &[(
                "result-value-transport.wf",
                include_bytes!(
                    "../../../../tests/conformance/cases/fn9-pos-result-value-transport.wf"
                ),
            )],
            &[(
                "result-conditional-joins.wf",
                include_bytes!(
                    "../../../../tests/conformance/cases/fn9-pos-result-conditional-joins.wf"
                ),
            )],
        ];
        VERIFY_SEEDED_CLOSURE.with(|verify| verify.set(true));
        VERIFIED_CLOSURES.with(|count| count.set(0));
        for bundle in bundles {
            let inputs = bundle
                .iter()
                .map(|(name, source)| crate::SourceInput::new(name, source))
                .collect::<Vec<_>>();
            crate::compile(&inputs, crate::CompilerLimits::default())
                .expect("a verified real program still compiles");
        }
        VERIFY_SEEDED_CLOSURE.with(|verify| verify.set(false));
        // The analysis runs on this thread, so the switch must have compared
        // closures; a silent miss would make the test vacuous.
        assert!(VERIFIED_CLOSURES.with(Cell::get) > 0);
    }
    use crate::DeclarationId;
    use crate::semantic::entailment::{RelationProvenance, VerifiedPostconditionSummary};
    use crate::semantic::model::{ContractQueryId, FunctionId};

    #[test]
    fn row_summary_skips_only_scalar_rejections() {
        let bounds = [i128::MIN, -2, 0, 2, i128::MAX];
        let depths = [0_u32, 2, u32::MAX];
        let cells = bounds
            .into_iter()
            .flat_map(|bound| depths.map(|depth| (bound, depth)))
            .collect::<Vec<_>>();
        let mut skipped = 0;
        for &left in &cells {
            for &right in &cells {
                let mut row = ClosureRowSummary::default();
                row.observe(left.0, left.1, true);
                row.observe(right.0, right.1, true);
                for &(first, first_depth) in &cells {
                    for minimum in bounds {
                        if !row.rejects_product(2, first, first_depth, minimum) {
                            continue;
                        }
                        skipped += 1;
                        // The outgoing summary constrains only the numeric
                        // minimum; every second-parent depth is possible.
                        for &(second, second_depth) in &cells {
                            if second < minimum {
                                continue;
                            }
                            for (current, current_depth) in [left, right] {
                                let via = first.saturating_add(second);
                                let depth = first_depth.max(second_depth).saturating_add(1);
                                assert!(
                                    via > current || (via == current && depth > current_depth),
                                    "first={first}/{first_depth}, second={second}/{second_depth}, current={current}/{current_depth}"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(skipped > 0, "the comparison must exercise skipped products");
    }

    #[test]
    fn row_summary_tracks_improvements_and_retains_ties_and_missing_cells() {
        let mut row = ClosureRowSummary::default();
        row.observe(7, 2, true);
        assert!(!row.rejects_product(2, 100, 10, 0));
        row.observe(3, 1, true);
        assert!(!row.rejects_product(2, 7, 1, 0), "equal-depth ties remain");
        assert!(row.rejects_product(2, 7, 2, 0));
        row.observe(-5, 9, false);
        assert_eq!(row.live, 2);
        assert_eq!(row.minimum, -5);
        assert_eq!(row.maximum, 7, "a stale high maximum is conservative");
        assert_eq!(row.maximum_depth, 9);
        assert!(!row.rejects_product(2, 7, 2, 0));
    }

    #[test]
    fn row_pruning_and_contiguous_products_preserve_complete_facts_and_selected_derivations() {
        let mut terms = TermTable::new();
        let places = [0, 1, 2, 3].map(|binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::spelled(
                    super::super::term::PlaceRoot::Binding(BindingId(binding)),
                    false,
                    Vec::new(),
                ),
                IntegerType::U8,
            ))
        });
        let values = [1_i128, 2, 4, 5];
        let edges = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (0, 2),
            (2, 0),
            (1, 3),
            (3, 1),
        ];
        for (mask, satisfiable) in (0_u32..256).flat_map(|mask| [(mask, true), (mask, false)]) {
            let mut ledger = DerivationLedger::default();
            let event = ledger.event(FlowEventKind::S1, None);
            let mut state = FactState::new();
            for offset in 0..edges.len() {
                let index = if mask & 1 == 0 {
                    offset
                } else {
                    edges.len() - 1 - offset
                };
                if mask & (1 << index) == 0 {
                    continue;
                }
                let (left, right) = edges[index];
                // Each graph has this concrete model. Varied slack gives
                // numeric improvements, equal paths and weak bounds that a
                // disequality can strengthen in a subsequent round.
                // The unsatisfiable variant lowers some slack below zero, so
                // negative cycles and contradictory diagonals arise mid-traversal.
                let slack = i128::from((mask + index as u32) % 3)
                    - if satisfiable { 0 } else { i128::from(mask % 4) };
                state.establish_bound_with_proof(
                    places[left],
                    places[right],
                    values[left] - values[right] + slack,
                    &mut ledger,
                    event,
                );
            }
            state.establish_distinct_with_proof(places[0], places[1], 0, &mut ledger, event);
            for excluded in [None, Some(places[2])] {
                let mut original_ledger = ledger.clone();
                let original = close_with_row_pruning::<false, true>(
                    &state,
                    &terms,
                    &GoalTable::default(),
                    &mut original_ledger,
                    excluded,
                );
                if satisfiable {
                    assert!(
                        !original.all_derivable,
                        "generated graph {mask} has a model"
                    );
                }
                for (prune, reference) in [(false, false), (true, true), (true, false)] {
                    let mut candidate_ledger = ledger.clone();
                    let close = match (prune, reference) {
                        (false, false) => close_with_row_pruning::<false, false>,
                        (true, true) => close_with_row_pruning::<true, true>,
                        _ => close_with_row_pruning::<true, false>,
                    };
                    let candidate = close(
                        &state,
                        &terms,
                        &GoalTable::default(),
                        &mut candidate_ledger,
                        excluded,
                    );
                    let label = format!(
                        "graph {mask}/{satisfiable}, excluded {excluded:?}, pruned {prune}, reference {reference}"
                    );
                    assert_eq!(candidate.all_derivable, original.all_derivable, "{label}");
                    assert_eq!(candidate.contradiction, original.contradiction, "{label}");
                    assert_eq!(
                        candidate.matrix.cells().collect::<Vec<_>>(),
                        original.matrix.cells().collect::<Vec<_>>(),
                        "{label}"
                    );
                    assert_eq!(candidate.distinct, original.distinct, "{label}");
                    assert_eq!(
                        candidate.distinct_proofs, original.distinct_proofs,
                        "{label}"
                    );
                    assert_eq!(candidate.opaque, original.opaque, "{label}");
                    assert_eq!(candidate.opaque_proofs, original.opaque_proofs, "{label}");
                    assert_eq!(candidate_ledger, original_ledger, "{label}");
                }
            }
        }
    }

    #[test]
    fn dormant_const_length_aliases_still_transfer_ranges_and_equality() {
        let mut terms = TermTable::new();
        let parameter = terms.intern(TermKind::ConstParameter(
            DeclarationId::from_index(0).expect("zero declaration identity exists"),
            IntegerType::U64,
        ));
        let length = |terms: &mut TermTable, binding| {
            let term = terms.intern(TermKind::Measure(
                CheckedMeasure::Length,
                super::super::term::ResolvedPlace::spelled(
                    super::super::term::PlaceRoot::Binding(BindingId(binding)),
                    false,
                    Vec::new(),
                ),
            ));
            terms.set_measure_bound(term, MeasureBound::Equal(parameter));
            term
        };
        let first = length(&mut terms, 0);
        let second = length(&mut terms, 1);
        let mut ledger = DerivationLedger::default();
        let closed = close(
            &FactState::new(),
            &terms,
            &GoalTable::default(),
            &mut ledger,
        );

        assert!(closed.derives_bound(first, second, 0));
        assert!(closed.derives_bound(second, first, 0));
        assert!(closed.derives_bound(parameter, ZERO, u64::MAX.into()));
        assert!(closed.derives_bound(ZERO, parameter, 0));
    }

    /// A predecessor contradictory only through call-dependent facts is
    /// neutral in the full join and a live input of the ordinary join, so the
    /// ordinary join can hold rows for terms the full join answers through
    /// Z. The full join must then be retaken over those terms: otherwise the
    /// ordinary fallback for such a pair, merged into a store that has no
    /// cell for it, would become the full selection, weaker than the view.
    /// The input is not promoted, as the result-image joins of the flow
    /// leave their inputs.
    #[test]
    fn a_join_retakes_rows_the_ordinary_layer_adds() {
        let mut terms = TermTable::new();
        let place = |binding, ty| {
            TermKind::Place(
                super::super::term::ResolvedPlace::binding(BindingId(binding)),
                ty,
            )
        };
        let t = terms.intern(place(0, IntegerType::U8));
        let x = terms.intern(place(1, IntegerType::I32));
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        // The dead input: an ordinary fact on `t` and a call-dependent
        // contradiction `Z - Z <= -1`, nothing on `x`.
        let mut dead = FactState::new();
        dead.establish(
            &Relation::Bound {
                left: t,
                right: ZERO,
                bound: 100,
            },
            &mut ledger,
            event,
        );
        let impossible = Relation::Bound {
            left: ZERO,
            right: ZERO,
            bound: -1,
        };
        let call = postcondition_call_proof(&mut ledger, impossible.clone());
        dead.establish_from_proof(&impossible, call, &ledger);
        // The live input: `x` bounded below ordinarily and more tightly by a
        // call; `t` holds no fact.
        let mut live = FactState::new();
        live.establish(
            &Relation::Bound {
                left: ZERO,
                right: x,
                bound: -300,
            },
            &mut ledger,
            event,
        );
        let tighter = Relation::Bound {
            left: ZERO,
            right: x,
            bound: -400,
        };
        let call = postcondition_call_proof(&mut ledger, tighter.clone());
        live.establish_from_proof(&tighter, call, &ledger);
        assert!(close(&dead, &terms, &goals, &mut ledger).contradictory());
        let join_event = ledger.event(FlowEventKind::Join, None);
        let joined = join_at(&[dead, live], &terms, &goals, &mut ledger, join_event);
        // The full join is the live input: `t - x` through Z is 255 - 400.
        let closed = close(&joined, &terms, &goals, &mut ledger);
        assert_eq!(closed.tight_bound(t, x), Some(-145));
        assert_eq!(closed.tight_bound(ZERO, x), Some(-400));
        // The ordinary join has both inputs: the dead one knows nothing of
        // `x`, so the ordinary layer bounds `x` only by its type, and its
        // `t - x` is the weaker of the two inputs' pairs, the dead input's
        // `t <= 100` composed with the type range of `x`.
        let mut ordinary = joined.clone();
        ordinary.retain_non_postcondition_candidates(&ledger);
        let closed = close(&ordinary, &terms, &goals, &mut ledger);
        assert_eq!(closed.tight_bound(ZERO, x), Some(i128::from(i32::MAX) + 1));
        assert_eq!(
            closed.tight_bound(t, x),
            Some(100 + i128::from(i32::MAX) + 1)
        );
        assert_eq!(closed.tight_bound(t, ZERO), Some(255));
    }

    /// A term with no stored relation is answered through Z by every view, so
    /// a join stores a disequality on it only where the joined Z bounds no
    /// longer imply it: here one path puts `b` above the whole `u8` range of
    /// `a` and the other below it, each path derives `a != b`, and the join
    /// of the two Z bounds on `b` says nothing about `a`.
    #[test]
    fn a_join_keeps_a_disequality_held_through_zero_on_opposite_sides() {
        let mut terms = TermTable::new();
        let a = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::binding(BindingId(0)),
            IntegerType::U8,
        ));
        let b = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::binding(BindingId(1)),
            IntegerType::I32,
        ));
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut above = FactState::new();
        above.establish(
            &Relation::Bound {
                left: ZERO,
                right: b,
                bound: -300,
            },
            &mut ledger,
            event,
        );
        let mut below = FactState::new();
        below.establish(
            &Relation::Bound {
                left: b,
                right: ZERO,
                bound: -5,
            },
            &mut ledger,
            event,
        );
        let distinct = Relation::Distinct {
            left: a,
            right: b,
            difference: 0,
        };
        for state in [&above, &below] {
            assert!(close(state, &terms, &goals, &mut ledger).derives(&distinct));
            assert!(!state.bounds.slot(a).is_some());
        }
        let joined = join(&[above, below], &terms, &goals, &mut ledger);
        let closed = close(&joined, &terms, &goals, &mut ledger);
        assert!(closed.derives(&distinct));
        assert!(closed.tight_bound(a, b).is_some_and(|bound| bound > -1));
        assert!(closed.tight_bound(b, a).is_some_and(|bound| bound > -1));
        assert!(joined.distinct.contains(&(a, b, 0)));
        // A disequality the joined bounds imply is not stored: `c` holds
        // only its range and `b` stays above it on both paths.
        let c = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::binding(BindingId(2)),
            IntegerType::U8,
        ));
        let mut high = FactState::new();
        high.establish(
            &Relation::Bound {
                left: ZERO,
                right: b,
                bound: -400,
            },
            &mut ledger,
            event,
        );
        let mut above = FactState::new();
        above.establish(
            &Relation::Bound {
                left: ZERO,
                right: b,
                bound: -300,
            },
            &mut ledger,
            event,
        );
        let joined = join(&[above, high], &terms, &goals, &mut ledger);
        let closed = close(&joined, &terms, &goals, &mut ledger);
        assert!(closed.derives(&Relation::Distinct {
            left: c,
            right: b,
            difference: 0,
        }));
        assert!(!joined.distinct.contains(&(b, c, 0)));
        assert!(!joined.bounds.slot(c).is_some());
    }

    /// A term active only through a call-dependent disequality has no row in
    /// the ordinary closure, which answers its pairs through Z. The
    /// materialization must still give every call-dependent cell of such a
    /// term its ordinary fallback, so that removing the call-dependent
    /// candidates leaves the bound the ordinary layer derives.
    #[test]
    fn a_materialized_ordinary_layer_keeps_a_bound_on_a_distinct_only_term() {
        let mut terms = TermTable::new();
        let place = |binding, ty| {
            TermKind::Place(
                super::super::term::ResolvedPlace::binding(BindingId(binding)),
                ty,
            )
        };
        let t = terms.intern(place(0, IntegerType::U8));
        let y = terms.intern(place(1, IntegerType::U8));
        let x = terms.intern(place(2, IntegerType::I32));
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        state.establish(
            &Relation::Bound {
                left: ZERO,
                right: x,
                bound: -300,
            },
            &mut ledger,
            event,
        );
        let stronger = Relation::Bound {
            left: ZERO,
            right: x,
            bound: -400,
        };
        let call = postcondition_call_proof(&mut ledger, stronger.clone());
        state.establish_from_proof(&stronger, call, &ledger);
        let distinct = Relation::Distinct {
            left: t,
            right: y,
            difference: 0,
        };
        let call = postcondition_call_proof(&mut ledger, distinct.clone());
        state.establish_from_proof(&distinct, call, &ledger);
        let mut direct = state.clone();
        direct.retain_non_postcondition_candidates(&ledger);
        let direct = close(&direct, &terms, &goals, &mut ledger);
        assert_eq!(direct.tight_bound(t, x), Some(-45));
        let snapshot = ledger.event(FlowEventKind::Snapshot, None);
        let materialized = materialize_closure_at(&state, &terms, &goals, &mut ledger, snapshot);
        assert_eq!(
            close(&materialized, &terms, &goals, &mut ledger).tight_bound(t, x),
            Some(-145)
        );
        let mut ordinary = materialized.clone();
        ordinary.retain_non_postcondition_candidates(&ledger);
        let closed = close(&ordinary, &terms, &goals, &mut ledger);
        assert_eq!(closed.tight_bound(t, x), Some(-45));
        assert_eq!(closed.tight_bound(x, t), direct.tight_bound(x, t));
        assert!(!closed.derives(&distinct));
    }

    #[test]
    fn an_empty_join_remains_the_contradictory_all_derivable_state() {
        let mut ledger = DerivationLedger::default();
        let joined = join(&[], &TermTable::new(), &GoalTable::default(), &mut ledger);
        assert!(joined.all_derivable);
        let proof = joined
            .contradiction
            .expect("the empty join has one exact contradiction proof");
        assert!(matches!(
            ledger.nodes[proof.0 as usize],
            DerivationNode::JoinContradiction { ref parents, event }
                if parents.is_empty()
                    && ledger.events[event.0 as usize].kind == FlowEventKind::Join
        ));
        ledger.add_root(DerivationRootKind::BoundsObligation(0), proof);
        let remap = ledger.finish();
        assert_eq!(ledger.nodes.len(), 1);
        assert_eq!(ledger.roots.len(), 1);
        assert_eq!(remap[proof.0 as usize], Some(DerivationId(0)));
    }

    #[test]
    fn one_structural_fact_has_one_derivation_identity() {
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S5, None);
        let mut state = FactState::new();
        let first = state.establish_bound_with_proof(ZERO, ZERO, 0, &mut ledger, event);
        let second = state.establish_bound_with_proof(ZERO, ZERO, 0, &mut ledger, event);
        ledger.add_root(DerivationRootKind::BoundsObligation(0), first);

        assert_eq!(ledger.events.len(), 1);
        assert_eq!(first, second);
        assert_eq!(ledger.nodes.len(), 1);

        let remap = ledger.finish();
        assert_eq!(ledger.events.len(), 1);
        assert_eq!(ledger.nodes.len(), 1);
        assert!(
            ledger
                .nodes
                .iter()
                .all(|node| node_event(node) == Some(FlowEventId(0)))
        );
        assert!(remap[first.0 as usize].is_some());
    }

    #[test]
    fn provenance_variants_with_equal_raw_ids_have_distinct_ordering_and_ledger_identity() {
        let call = NodePath {
            components: vec![3],
        };
        let relation = Relation::Bound {
            left: ZERO,
            right: ZERO,
            bound: 0,
        };
        let make_node = |summary| DerivationNode::PostconditionCall {
            detail: Box::new(PostconditionCallDetail {
                call: call.clone(),
                relation: relation.clone(),
                summary: VerifiedPostconditionSummaryRef { summary },
                substitutions: Vec::new(),
                transfer_events: Vec::new(),
                parents: Vec::new(),
            }),
        };
        let verified = make_node(RelationProvenance::Verified(VerifiedPostconditionSummary {
            function: FunctionId(7),
            block: NodePath {
                components: vec![4],
            },
            relation_ordinal: 0,
            component: 11,
        }));
        let formal = make_node(RelationProvenance::FormalBoundary {
            query: ContractQueryId(7),
            actual: FunctionId(11),
            premises: Vec::new(),
        });

        assert_ne!(
            compare_node_ties(&verified, &formal),
            std::cmp::Ordering::Equal
        );
        let mut ledger = DerivationLedger::default();
        assert_ne!(ledger.intern(verified), ledger.intern(formal));
    }

    #[test]
    fn pre_kill_closure_composes_bounds_in_relation_direction() {
        let mut terms = TermTable::new();
        let mut place = |binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::spelled(
                    super::super::term::PlaceRoot::Binding(BindingId(binding)),
                    false,
                    Vec::new(),
                ),
                IntegerType::U8,
            ))
        };
        let x = place(0);
        let middle = place(1);
        let ceiling = place(2);
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        state.establish(
            &Relation::Bound {
                left: x,
                right: middle,
                bound: 0,
            },
            &mut ledger,
            event,
        );
        state.establish(
            &Relation::Bound {
                left: middle,
                right: ceiling,
                bound: 0,
            },
            &mut ledger,
            event,
        );

        materialize_closure_before_kill(&mut state, &terms, &GoalTable::default(), &mut ledger);
        state.kill(|term| term == middle);

        assert_eq!(
            state.bounds.get(x, ceiling).map(|(bound, _)| bound),
            Some(0)
        );
        let proof = state.bounds.get(x, ceiling).expect("bound present").1;
        assert!(matches!(
            ledger.nodes[proof.0 as usize],
            DerivationNode::MaterializedBound {
                left,
                right,
                bound: 0,
                ..
            } if left == x && right == ceiling
        ));
        assert!(
            state
                .bounds
                .cells()
                .all(|(left, right, _, _)| left != middle && right != middle),
            "the killed endpoint itself must not survive pre-kill closure"
        );
    }

    /// A snapshot re-materializes every bound the state carries, and a body
    /// of many measured commits takes one snapshot per kill over a fact state
    /// whose bounds are quadratic in its terms. Wrapping an unchanged bound's
    /// existing materialization in a second one at every snapshot is one
    /// derivation node per bound per kill and nothing else, so the existing
    /// node is reused and the ledger stops growing with the number of kills.
    #[test]
    fn a_later_snapshot_reuses_the_materialization_of_an_unchanged_bound() {
        let mut terms = TermTable::new();
        let mut place = |binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::spelled(
                    super::super::term::PlaceRoot::Binding(BindingId(binding)),
                    false,
                    Vec::new(),
                ),
                IntegerType::U8,
            ))
        };
        let left = place(0);
        let right = place(1);
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        state.establish(
            &Relation::Bound {
                left,
                right,
                bound: 0,
            },
            &mut ledger,
            event,
        );

        let goals = GoalTable::default();
        let first = ledger.event(FlowEventKind::Snapshot, None);
        let once = materialize_closure_at(&state, &terms, &goals, &mut ledger, first);
        let after_one = ledger.nodes.len();
        let second = ledger.event(FlowEventKind::Snapshot, None);
        let twice = materialize_closure_at(&once, &terms, &goals, &mut ledger, second);

        assert_eq!(once.bounds.get(left, right).expect("bound present").0, 0);
        assert_eq!(twice.bounds.get(left, right).expect("bound present").0, 0);
        assert_eq!(
            once.bounds.get(left, right).expect("bound present").1,
            twice.bounds.get(left, right).expect("bound present").1,
            "an unchanged bound keeps the materialization it already had"
        );
        assert_eq!(
            ledger.nodes.len(),
            after_one,
            "a second snapshot over an unchanged state interns no further bound node"
        );
    }

    #[test]
    fn pre_kill_closure_never_preserves_a_killed_conclusion_endpoint() {
        let mut terms = TermTable::new();
        let mut place = |binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::spelled(
                    super::super::term::PlaceRoot::Binding(BindingId(binding)),
                    false,
                    Vec::new(),
                ),
                IntegerType::U8,
            ))
        };
        let x = place(0);
        let middle = place(1);
        let ceiling = place(2);
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        for relation in [
            Relation::Bound {
                left: x,
                right: middle,
                bound: 0,
            },
            Relation::Bound {
                left: middle,
                right: ceiling,
                bound: 0,
            },
        ] {
            state.establish(&relation, &mut ledger, event);
        }

        materialize_closure_before_kill(&mut state, &terms, &GoalTable::default(), &mut ledger);
        state.kill(|term| term == x || term == middle);

        assert!(state.bounds.get(x, ceiling).is_none());
        assert!(state.bounds.cells().all(|(left, right, _, _)| {
            ![x, middle].contains(&left) && ![x, middle].contains(&right)
        }));
    }

    #[test]
    fn pre_kill_closure_preserves_disequality_strengthening_through_a_killed_middle() {
        let mut terms = TermTable::new();
        let mut place = |binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::spelled(
                    super::super::term::PlaceRoot::Binding(BindingId(binding)),
                    false,
                    Vec::new(),
                ),
                IntegerType::U8,
            ))
        };
        let x = place(0);
        let middle = place(1);
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        for relation in [
            Relation::Bound {
                left: x,
                right: middle,
                bound: 0,
            },
            Relation::Distinct {
                left: x,
                right: middle,
                difference: 0,
            },
            Relation::Bound {
                left: middle,
                right: ZERO,
                bound: 3,
            },
        ] {
            state.establish(&relation, &mut ledger, event);
        }

        materialize_closure_before_kill(&mut state, &terms, &GoalTable::default(), &mut ledger);
        state.kill(|term| term == middle);

        assert_eq!(state.bounds.get(x, ZERO).map(|(bound, _)| bound), Some(2));
        let proof = state.bounds.get(x, ZERO).expect("bound present").1;
        assert!(matches!(
            ledger.nodes[proof.0 as usize],
            DerivationNode::MaterializedBound {
                left,
                right,
                bound: 2,
                ..
            } if left == x && right == ZERO
        ));
    }

    #[test]
    fn a_shared_temporary_join_proof_becomes_live_before_its_middle_dies() {
        let mut terms = TermTable::new();
        let [left, middle, right] = [0, 1, 2].map(|binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::binding(BindingId(binding)),
                IntegerType::I32,
            ))
        });
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let source = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        for (left, right, bound) in [(left, middle, 0), (middle, right, -1)] {
            state.establish(&Relation::Bound { left, right, bound }, &mut ledger, source);
        }
        let event = ledger.event(FlowEventKind::Join, None);
        let mut joined = join_at(&[state.clone(), state], &terms, &goals, &mut ledger, event);
        let (bound, proof) = joined.bounds.get(left, right).expect("joined consequence");
        assert_eq!(bound, -1);
        assert!(matches!(
            ledger.nodes[proof.0 as usize],
            DerivationNode::JoinBound { event: held, .. } if held == event
        ));
        let distinct = joined.distinct_proofs[&distinct_key(left, right, 0)];
        assert!(matches!(
            ledger.nodes[distinct.0 as usize],
            DerivationNode::JoinDistinct { event: held, .. } if held == event
        ));
        // The next join may reuse those independently live conclusions.
        let next = ledger.event(FlowEventKind::Join, None);
        let again = join_at(
            &[joined.clone(), joined.clone()],
            &terms,
            &goals,
            &mut ledger,
            next,
        );
        assert_eq!(again.bounds.get(left, right), Some((bound, proof)));
        assert_eq!(
            again.distinct_proofs[&distinct_key(left, right, 0)],
            distinct
        );

        materialize_closure_before_kill(&mut joined, &terms, &goals, &mut ledger);
        joined.kill(|term| term == middle);
        assert_eq!(joined.bounds.get(left, right), Some((bound, proof)));
        assert_eq!(
            joined.distinct_proofs[&distinct_key(left, right, 0)],
            distinct
        );
        // Independence does not let a conclusion outlive its own support.
        joined.kill(|term| term == right);
        assert!(joined.bounds.get(left, right).is_none());
        assert!(!joined.distinct.contains(&distinct_key(left, right, 0)));
    }

    #[test]
    fn a_neutral_contradiction_does_not_make_an_ordinary_join_fact_call_dependent() {
        let mut terms = TermTable::new();
        let left = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::binding(BindingId(0)),
            IntegerType::I32,
        ));
        let goals = GoalTable::default();
        for call_dependent in [false, true] {
            let mut ledger = DerivationLedger::default();
            let source = ledger.event(FlowEventKind::S1, None);
            let mut live = FactState::new();
            live.establish(
                &Relation::Bound {
                    left,
                    right: ZERO,
                    bound: 0,
                },
                &mut ledger,
                source,
            );
            let impossible = Relation::Bound {
                left: ZERO,
                right: ZERO,
                bound: -1,
            };
            let mut dead = FactState::new();
            if call_dependent {
                let proof = postcondition_call_proof(&mut ledger, impossible.clone());
                dead.establish_from_proof(&impossible, proof, &ledger);
            } else {
                dead.establish(&impossible, &mut ledger, source);
            }
            let event = ledger.event(FlowEventKind::Join, None);
            let mut joined = join_at(&[live, dead], &terms, &goals, &mut ledger, event);
            let (_, proof) = joined.bounds.get(left, ZERO).unwrap();
            let DerivationNode::JoinBound { parents, .. } = &ledger.nodes[proof.0 as usize] else {
                panic!("join boundary");
            };
            assert_eq!(
                parents.len(),
                2,
                "the neutral predecessor's proof is retained"
            );
            assert!(!ledger.depends_on_postcondition_call(proof));
            joined.retain_non_postcondition_candidates(&ledger);
            assert!(close(&joined, &terms, &goals, &mut ledger).derives_bound(left, ZERO, 0));
            ledger.add_root(DerivationRootKind::BoundsObligation(0), proof);
            let remap = ledger.finish();
            let proof = remap[proof.0 as usize].expect("join root retained");
            assert!(
                !ledger.depends_on_postcondition_call(proof),
                "finalization preserves the dependency boundary"
            );
        }
    }

    #[test]
    fn ordinary_fallback_candidates_survive_join_and_materialization() {
        let mut terms = TermTable::new();
        let left = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::spelled(
                super::super::term::PlaceRoot::Binding(BindingId(0)),
                false,
                Vec::new(),
            ),
            IntegerType::I32,
        ));
        let right = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::spelled(
                super::super::term::PlaceRoot::Binding(BindingId(1)),
                false,
                Vec::new(),
            ),
            IntegerType::I32,
        ));
        let pair = (left, right);
        let ordinary = Relation::Bound {
            left: pair.0,
            right: pair.1,
            bound: 0,
        };
        let s12 = Relation::Bound {
            left: pair.0,
            right: pair.1,
            bound: -1,
        };
        let mut ledger = DerivationLedger::default();
        let source_event = ledger.event(FlowEventKind::S5, None);
        let mut state = FactState::new();
        state.establish(&ordinary, &mut ledger, source_event);
        let ordinary_proof = state.bounds.get(pair.0, pair.1).expect("bound present").1;
        assert!(!ledger.depends_on_postcondition_call(ordinary_proof));
        let call = ledger.intern(DerivationNode::PostconditionCall {
            detail: Box::new(PostconditionCallDetail {
                call: NodePath {
                    components: vec![0],
                },
                relation: s12.clone(),
                summary: VerifiedPostconditionSummaryRef {
                    summary: crate::semantic::entailment::RelationProvenance::Verified(
                        VerifiedPostconditionSummary {
                            function: FunctionId(0),
                            block: NodePath {
                                components: vec![0, 0],
                            },
                            relation_ordinal: 0,
                            component: 0,
                        },
                    ),
                },
                substitutions: Vec::new(),
                transfer_events: Vec::new(),
                parents: Vec::new(),
            }),
        });
        assert!(ledger.depends_on_postcondition_call(call));
        state.establish_from_proof(&s12, call, &ledger);
        assert_eq!(
            state.bounds.get(pair.0, pair.1).expect("bound present").0,
            -1
        );

        let snapshot_event = ledger.event(FlowEventKind::Snapshot, None);
        let mut materialized = materialize_closure_at(
            &state,
            &terms,
            &GoalTable::default(),
            &mut ledger,
            snapshot_event,
        );
        materialized.retain_non_postcondition_candidates(&ledger);
        assert_eq!(
            materialized
                .bounds
                .get(pair.0, pair.1)
                .expect("bound present")
                .0,
            0
        );

        let join_event = ledger.event(FlowEventKind::Join, None);
        let mut joined = join_at(
            &[state.clone(), state],
            &terms,
            &GoalTable::default(),
            &mut ledger,
            join_event,
        );
        joined.retain_non_postcondition_candidates(&ledger);
        assert_eq!(
            joined.bounds.get(pair.0, pair.1).expect("bound present").0,
            0
        );

        ledger.add_root(DerivationRootKind::BoundsObligation(0), ordinary_proof);
        ledger.add_root(DerivationRootKind::BoundsObligation(1), call);
        let remap = ledger.finish();
        let ordinary_proof = remap[ordinary_proof.0 as usize].expect("ordinary proof retained");
        let call = remap[call.0 as usize].expect("S12 proof retained");
        assert!(!ledger.depends_on_postcondition_call(ordinary_proof));
        assert!(ledger.depends_on_postcondition_call(call));
    }

    /// A live ledger clone carries the same interning state and therefore
    /// preserves identities without reconstructing compiler-generated data.
    #[test]
    fn interning_into_a_cloned_ledger_keeps_the_original_identity() {
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S5, None);
        let relation = Relation::Bound {
            left: ZERO,
            right: ZERO,
            bound: 0,
        };
        let node = DerivationNode::SourceBound {
            relation,
            left: ZERO,
            right: ZERO,
            bound: 0,
            event,
        };
        let original = ledger.intern(node.clone());
        let nodes = ledger.nodes.len();

        let copy = ledger.clone();
        assert_eq!(copy.nodes.len(), nodes);
        let mut copy = copy.clone();
        assert_eq!(copy.nodes.len(), nodes);
        assert_eq!(copy.intern(node.clone()), original);
        assert_eq!(copy.nodes.len(), nodes);
        assert_eq!(copy.intern(node), original);
        assert_eq!(copy.nodes.len(), nodes);
    }

    /// `settle` is final: semantic analysis performs no later interning. Its
    /// [DIAG-2] byte
    /// metric is the only number that could carry a trace of it:
    /// `finish_with_event_roots` restores every vector `settle` releases at
    /// the retained length, except `roots`, whose capacity it carries through
    /// because it rewrites each root in place. A `settle` that shrank `roots`
    /// would leave a finished ledger reporting fewer bytes than the same
    /// analysis that never settled, which is what this asserts it does not.
    #[test]
    fn settling_a_ledger_does_not_move_the_finished_byte_metric() {
        fn rooted_ledger() -> DerivationLedger {
            let mut ledger = DerivationLedger::default();
            let event = ledger.event(FlowEventKind::S5, None);
            let node = DerivationNode::SourceBound {
                relation: Relation::Bound {
                    left: ZERO,
                    right: ZERO,
                    bound: 0,
                },
                left: ZERO,
                right: ZERO,
                bound: 0,
                event,
            };
            let interned = ledger.intern(node);
            ledger.add_root(DerivationRootKind::BoundsObligation(0), interned);
            ledger
        }

        let mut settled = rooted_ledger();
        settled.settle();
        settled.finish();
        let mut plain = rooted_ledger();
        plain.finish();

        assert!(
            plain.roots.capacity() > plain.roots.len(),
            "an unsettled `roots` with no spare capacity would make this vacuous"
        );
        assert_eq!(settled.metrics, plain.metrics);
    }

    /// The derivation arena is one flat array of entries per checked function,
    /// so the widest variant sets the memory cost of the whole check.
    /// The transitivity and join steps that make up almost every entry need
    /// 48 bytes; the S12 call and delivery-join evidence, and the relation of
    /// every other postcondition step, are held out of line to keep them from
    /// widening those.
    #[test]
    fn the_derivation_arena_entry_stays_narrow() {
        assert!(
            size_of::<DerivationNode>() <= 64,
            "one derivation arena entry grew to {} bytes",
            size_of::<DerivationNode>()
        );
    }

    /// The ENT-4 fixed point revisits a transitivity triple only when one of
    /// its premise cells changed since that triple was last offered. A
    /// disequality strengthens `a - b <= 0` to `-1` only after the first
    /// transitivity pass, so the strengthened edge must still reach `c`
    /// through the later rounds. A freshness rule that stopped one round too
    /// early would leave `a - c <= -1` underived.
    #[test]
    fn a_strengthened_bound_still_propagates_in_a_later_closure_round() {
        let mut terms = TermTable::new();
        let mut place = |binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::spelled(
                    super::super::term::PlaceRoot::Binding(BindingId(binding)),
                    false,
                    Vec::new(),
                ),
                IntegerType::I32,
            ))
        };
        let a = place(0);
        let b = place(1);
        let c = place(2);
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S5, None);
        let mut state = FactState::new();
        state.establish(
            &Relation::Bound {
                left: a,
                right: b,
                bound: 0,
            },
            &mut ledger,
            event,
        );
        state.establish(
            &Relation::Bound {
                left: b,
                right: c,
                bound: 0,
            },
            &mut ledger,
            event,
        );
        state.establish_distinct_with_proof(a, b, 0, &mut ledger, event);

        let closed = close(&state, &terms, &GoalTable::default(), &mut ledger);
        assert!(!closed.all_derivable);
        assert!(closed.derives_bound(a, b, -1));
        assert!(closed.derives_bound(a, c, -1));
    }

    /// Weakened cells that share an endpoint with a negative cycle among raw
    /// cells used to be repaired forever, each pass lowering them by the cycle
    /// weight. The repair now yields to the seeded fixed point, and the
    /// proof-free probe to its complete pass, which report the contradiction.
    #[test]
    fn a_negative_cycle_behind_weakened_cells_is_reported_not_repaired_forever() {
        let mut terms = TermTable::new();
        let place = |terms: &mut TermTable, binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::binding(BindingId(binding)),
                IntegerType::U8,
            ))
        };
        let a = place(&mut terms, 0);
        let c = place(&mut terms, 1);
        let b = place(&mut terms, 2);
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let call = ledger.intern(DerivationNode::PostconditionCall {
            detail: Box::new(PostconditionCallDetail {
                call: NodePath {
                    components: vec![0],
                },
                relation: Relation::Bound {
                    left: b,
                    right: a,
                    bound: 0,
                },
                summary: VerifiedPostconditionSummaryRef {
                    summary: crate::semantic::entailment::RelationProvenance::Verified(
                        VerifiedPostconditionSummary {
                            function: FunctionId(0),
                            block: NodePath {
                                components: vec![0, 0],
                            },
                            relation_ordinal: 0,
                            component: 0,
                        },
                    ),
                },
                substitutions: Vec::new(),
                transfer_events: Vec::new(),
                parents: Vec::new(),
            }),
        });
        let mut state = FactState::new();
        for middle in [a, c] {
            state.establish_from_proof(
                &Relation::Bound {
                    left: b,
                    right: middle,
                    bound: 0,
                },
                call,
                &ledger,
            );
        }
        let snapshot = ledger.event(FlowEventKind::Snapshot, None);
        let mut state = materialize_closure_at(&state, &terms, &goals, &mut ledger, snapshot);
        // An S12 holder kill removes the call-derived candidates while their
        // terms survive, leaving weakened cells in the closure record.
        state.kill_proof_candidates(&ledger, |_, _, proof| {
            ledger.depends_on_postcondition_call(proof)
        });
        let event = ledger.event(FlowEventKind::S5, None);
        for (left, right) in [(a, c), (c, a)] {
            state.establish(
                &Relation::Bound {
                    left,
                    right,
                    bound: -1,
                },
                &mut ledger,
                event,
            );
        }
        assert!(contradiction_without_proofs(&state, &terms, &goals));
        assert!(close(&state, &terms, &goals, &mut ledger).contradictory());
    }

    /// Reimporting a numeric snapshot must not schedule its complete matrix
    /// for closure again. Additional candidates still matter after a call's
    /// authority is removed, even when they do not improve the full layer.
    #[test]
    fn repeated_snapshot_imports_preserve_closure_and_ordinary_fallbacks() {
        let mut terms = TermTable::new();
        let [left, middle, right] = [0, 1, 2].map(|binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::binding(BindingId(binding)),
                IntegerType::U8,
            ))
        });
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        for (left, right, bound) in [(left, middle, 10), (middle, right, 4)] {
            state.establish(&Relation::Bound { left, right, bound }, &mut ledger, event);
        }
        for relation in [
            Relation::Bound {
                left,
                right: middle,
                bound: 0,
            },
            Relation::Distinct {
                left: middle,
                right,
                difference: 0,
            },
        ] {
            let proof = postcondition_call_proof(&mut ledger, relation.clone());
            state.establish_from_proof(&relation, proof, &ledger);
        }
        materialize_closure_before_kill(&mut state, &terms, &goals, &mut ledger);
        // Equal-value witness replacement must refresh the proof-bearing
        // view even though no numeric closure work is needed. Removing the
        // new witnesses must reveal the original still-live candidates.
        let mut swapped = state.clone();
        let old_bound = swapped.bounds.get(left, middle).unwrap();
        let pair = distinct_key(middle, right, 0);
        let old_distinct = swapped.distinct_proofs[&pair];
        let cached = close(&swapped, &terms, &goals, &mut ledger);
        swapped.establish(
            &Relation::Bound {
                left,
                right: middle,
                bound: 0,
            },
            &mut ledger,
            event,
        );
        swapped.establish(
            &Relation::Distinct {
                left: middle,
                right,
                difference: 0,
            },
            &mut ledger,
            event,
        );
        let new_bound = swapped.bounds.get(left, middle).unwrap();
        let new_distinct = swapped.distinct_proofs[&pair];
        assert_ne!(old_bound.1, new_bound.1);
        assert_ne!(old_distinct, new_distinct);
        assert!(!ledger.depends_on_postcondition_call(new_bound.1));
        assert!(!ledger.depends_on_postcondition_call(new_distinct));
        assert!(swapped.closure.is_closed_over(terms.ids().count()));
        assert!(!Rc::ptr_eq(
            &cached,
            &close(&swapped, &terms, &goals, &mut ledger)
        ));
        swapped.kill_proof_candidates(&ledger, |_, _, proof| {
            proof == new_bound.1 || proof == new_distinct
        });
        assert_eq!(swapped.bounds.get(left, middle), Some(old_bound));
        assert_eq!(swapped.distinct_proofs[&pair], old_distinct);
        let restored = close(&swapped, &terms, &goals, &mut ledger);
        assert_seeded_closure_matches_complete(&swapped, &terms, &goals, &ledger, &restored);

        for (relation, proof) in state.l0_candidates() {
            state.establish_from_proof(&relation, proof, &ledger);
        }
        let weaker = Relation::Bound {
            left,
            right: middle,
            bound: 20,
        };
        state.establish(&weaker, &mut ledger, event);
        assert!(state.closure.is_closed_over(terms.ids().count()));
        assert!(state.ordinary_closure.is_closed_over(terms.ids().count()));
        assert!(
            state
                .bounds
                .candidates((left, middle))
                .iter()
                .any(|(bound, _)| *bound == 20)
        );

        // These ordinary facts change only the fallback layer: the full
        // layer already knows the stronger bound and the same disequality.
        state.establish(
            &Relation::Bound {
                left,
                right: middle,
                bound: 5,
            },
            &mut ledger,
            event,
        );
        state.establish(
            &Relation::Distinct {
                left: middle,
                right,
                difference: 0,
            },
            &mut ledger,
            event,
        );
        assert!(state.closure.is_closed_over(terms.ids().count()));
        state.retain_non_postcondition_candidates(&ledger);
        let closed = close(&state, &terms, &goals, &mut ledger);
        assert!(closed.derives_bound(left, right, 9));
        assert!(!closed.derives_bound(left, right, 8));
        assert!(closed.distinct.contains(&distinct_key(middle, right, 0)));
        assert_seeded_closure_matches_complete(&state, &terms, &goals, &ledger, &closed);
    }

    fn bound_store_proof(
        ledger: &mut DerivationLedger,
        event: FlowEventId,
        bound: i128,
    ) -> DerivationId {
        ledger.intern(DerivationNode::SourceBound {
            relation: Relation::Bound {
                left: TermId(1),
                right: TermId(2),
                bound,
            },
            left: TermId(1),
            right: TermId(2),
            bound,
            event,
        })
    }

    /// A pair keeps its least candidate selected and every other candidate
    /// in reserve: removing the selection promotes the best survivor, removing
    /// a reserve keeps the selection, and removing all clears the pair.
    #[test]
    fn a_bound_store_selects_the_least_candidate_and_promotes_survivors() {
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let (weak, middle, strong) = (
            bound_store_proof(&mut ledger, event, 5),
            bound_store_proof(&mut ledger, event, 3),
            bound_store_proof(&mut ledger, event, 1),
        );
        let pair = (TermId(1), TermId(2));
        let mut store = BoundStore::default();
        store.add_candidate(pair, (5, weak), &ledger);
        store.add_candidate(pair, (1, strong), &ledger);
        store.add_candidate(pair, (3, middle), &ledger);
        store.add_candidate(pair, (3, middle), &ledger);
        assert_eq!(store.get(pair.0, pair.1), Some((1, strong)));
        assert_eq!(store.candidates(pair).len(), 3);
        assert_eq!(
            store.candidate_minimum(pair, |proof| proof != strong),
            Some(3)
        );

        store.retain_candidates(pair, |(_, proof)| proof != weak, &ledger);
        assert_eq!(store.get(pair.0, pair.1), Some((1, strong)));
        store.retain_candidates(pair, |(_, proof)| proof != strong, &ledger);
        assert_eq!(store.get(pair.0, pair.1), Some((3, middle)));
        assert_eq!(store.candidates(pair), vec![(3, middle)]);
        store.retain_candidates(pair, |_| false, &ledger);
        assert_eq!(store.get(pair.0, pair.1), None);
        assert!(store.is_empty());

        // A later, larger term re-lays the store out without losing a cell.
        store.store_single(pair.0, pair.1, 3, middle);
        store.store_single(TermId(90), TermId(0), -7, strong);
        assert_eq!(
            store.cells().collect::<Vec<_>>(),
            vec![
                (TermId(1), TermId(2), 3, middle),
                (TermId(90), TermId(0), -7, strong)
            ]
        );
    }

    /// A remembered closed view is reused only for unchanged content: a new
    /// relation clears it, and the next closure sees the relation.
    #[test]
    fn a_remembered_closed_view_is_cleared_by_a_new_relation() {
        let mut terms = TermTable::new();
        let place = |terms: &mut TermTable, binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::binding(BindingId(binding)),
                IntegerType::U8,
            ))
        };
        let (left, right) = (place(&mut terms, 0), place(&mut terms, 1));
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        let first = close(&state, &terms, &goals, &mut ledger);
        assert!(Rc::ptr_eq(
            &first,
            &close(&state, &terms, &goals, &mut ledger)
        ));
        assert!(!first.derives_bound(left, right, 0));

        let copy = state.clone();
        state.establish(
            &Relation::Bound {
                left,
                right,
                bound: 0,
            },
            &mut ledger,
            event,
        );
        let second = close(&state, &terms, &goals, &mut ledger);
        assert!(!Rc::ptr_eq(&first, &second));
        assert!(second.derives_bound(left, right, 0));
        assert!(Rc::ptr_eq(
            &first,
            &close(&copy, &terms, &goals, &mut ledger)
        ));
    }

    /// A standing measure fact that replaces a different one can weaken an
    /// implicit bound a remembered view already used, so it ends the view
    /// seed and the next closure holds only the new fact.
    #[test]
    fn a_replaced_standing_measure_fact_ends_the_view_seed() {
        let mut terms = TermTable::new();
        let measure = terms.intern(TermKind::Measure(
            CheckedMeasure::Length,
            super::super::term::ResolvedPlace::binding(BindingId(0)),
        ));
        terms.set_measure_bound(measure, MeasureBound::Constant(7));
        let other = terms.intern(TermKind::Place(
            super::super::term::ResolvedPlace::binding(BindingId(1)),
            IntegerType::U8,
        ));
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        let bound = |left, right, bound| Relation::Bound { left, right, bound };
        state.establish(&bound(other, ZERO, 5), &mut ledger, event);
        assert!(close(&state, &terms, &goals, &mut ledger).derives_bound(measure, ZERO, 7));
        terms.set_measure_bound(measure, MeasureBound::Constant(9));
        state.establish(&bound(ZERO, other, 0), &mut ledger, event);
        let closed = close(&state, &terms, &goals, &mut ledger);
        assert!(!closed.derives_bound(measure, ZERO, 7));
        assert!(closed.derives_bound(measure, ZERO, 9));
    }

    /// `A - B <= 5` from the body is implied through zero by `A <= 5` from a
    /// postcondition call and `B >= 0`, but the ordinary layer, without the
    /// call, has only the body's cell; snapshots must keep it.
    #[test]
    fn a_cell_implied_through_a_postcondition_zero_cell_survives_snapshots() {
        let mut terms = TermTable::new();
        let mut place = |binding| {
            terms.intern(TermKind::Place(
                super::super::term::ResolvedPlace::binding(BindingId(binding)),
                IntegerType::U8,
            ))
        };
        let (a, b, c) = (place(0), place(1), place(2));
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S1, None);
        let mut state = FactState::new();
        let bound = |left, right, bound| Relation::Bound { left, right, bound };
        let call = postcondition_call_proof(&mut ledger, bound(a, ZERO, 5));
        state.establish_from_proof(&bound(a, ZERO, 5), call, &ledger);
        state.establish(&bound(a, b, 5), &mut ledger, event);
        materialize_closure_before_kill(&mut state, &terms, &goals, &mut ledger);
        state.establish(&bound(c, ZERO, 9), &mut ledger, event);
        materialize_closure_before_kill(&mut state, &terms, &goals, &mut ledger);
        state.retain_non_postcondition_candidates(&ledger);
        let closed = close(&state, &terms, &goals, &mut ledger);
        assert!(closed.derives_bound(a, b, 5));
        assert!(!closed.derives_bound(a, ZERO, 5));
    }

    fn postcondition_call_proof(ledger: &mut DerivationLedger, relation: Relation) -> DerivationId {
        ledger.intern(DerivationNode::PostconditionCall {
            detail: Box::new(PostconditionCallDetail {
                call: NodePath {
                    components: vec![0],
                },
                relation,
                summary: VerifiedPostconditionSummaryRef {
                    summary: crate::semantic::entailment::RelationProvenance::Verified(
                        VerifiedPostconditionSummary {
                            function: FunctionId(0),
                            block: NodePath {
                                components: vec![0, 0],
                            },
                            relation_ordinal: 0,
                            component: 0,
                        },
                    ),
                },
                substitutions: Vec::new(),
                transfer_events: Vec::new(),
                parents: Vec::new(),
            }),
        })
    }

    /// A fixed linear congruential sequence: generated cases are reproducible
    /// without a random-number dependency.
    fn next_step(seed: &mut u64) -> usize {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (*seed >> 33) as usize
    }

    /// An eager test reference: close each predecessor from all of its own
    /// live facts, intersect values, and always give the result a new join
    /// boundary. It does not read the optimized state's closure record or
    /// reuse its materialized/joined output. A one-edge join is a snapshot.
    fn reference_join_layer(
        states: &[FactState],
        terms: &TermTable,
        goals: &GoalTable,
        ledger: &mut DerivationLedger,
        event: FlowEventId,
    ) -> FactState {
        let closed = states
            .iter()
            .map(|state| {
                let mut state = state.clone();
                state.closure = ClosureRecord::Unknown;
                state.view_seed = None;
                with_every_term(|| close_with_excluded_term(&state, terms, goals, ledger, None))
            })
            .collect::<Vec<_>>();
        let parents = |proofs: Vec<DerivationId>| {
            proofs
                .into_iter()
                .enumerate()
                .map(|(ordinal, parent)| JoinParent {
                    ordinal: u32::try_from(ordinal).unwrap(),
                    parent,
                })
                .collect::<Vec<_>>()
        };
        let Some(first) = closed.iter().find(|state| !state.all_derivable) else {
            let proof = ledger.intern(DerivationNode::JoinContradiction {
                event,
                parents: parents(
                    closed
                        .iter()
                        .map(|state| state.contradiction.unwrap())
                        .collect(),
                ),
            });
            return FactState::contradictory(proof);
        };
        let mut result = FactState::new();
        for (left, right, bound, _) in first.matrix.cells() {
            let weakest = closed.iter().filter(|state| !state.all_derivable).try_fold(
                bound,
                |bound, state| {
                    state
                        .matrix
                        .lookup(left, right)
                        .map(|(held, _)| bound.max(held))
                },
            );
            let Some(bound) = weakest else {
                continue;
            };
            let proofs = closed
                .iter()
                .map(|state| {
                    state
                        .contradiction
                        .unwrap_or_else(|| state.bound_proof(left, right, bound, ledger).unwrap())
                })
                .collect();
            let proof = ledger.intern(DerivationNode::JoinBound {
                left,
                right,
                bound,
                event,
                parents: parents(proofs),
            });
            result.add_bound(left, right, bound, proof, ledger);
        }
        let mut distinct = closed
            .iter()
            .filter(|state| !state.contradictory())
            .flat_map(|state| state.distinct.iter().copied())
            .collect::<Vec<_>>();
        distinct.sort_unstable();
        distinct.dedup();
        for (left, right, difference) in distinct {
            let proofs = closed
                .iter()
                .map(|state| {
                    state
                        .contradiction
                        .or_else(|| state.distinct_proof((left, right, difference), ledger))
                })
                .collect::<Option<Vec<_>>>();
            if let Some(proofs) = proofs {
                let proof = ledger.intern(DerivationNode::JoinDistinct {
                    left,
                    right,
                    difference,
                    event,
                    parents: parents(proofs),
                });
                result.add_distinct_candidate((left, right, difference), proof, ledger);
            }
        }
        let mut opaque = first.opaque.iter().copied().collect::<Vec<_>>();
        opaque.sort_unstable();
        for (goal, sign) in opaque {
            let proofs = closed
                .iter()
                .map(|state| {
                    state
                        .contradiction
                        .or_else(|| state.opaque_proofs.get(&(goal, sign)).copied())
                })
                .collect::<Option<Vec<_>>>();
            if let Some(proofs) = proofs {
                let proof = ledger.intern(DerivationNode::JoinGoal {
                    goal,
                    sign,
                    event,
                    parents: parents(proofs),
                });
                result.opaque.insert((goal, sign));
                result.opaque_proofs.insert((goal, sign), proof);
            }
        }
        result
    }

    fn reference_join(
        states: &[FactState],
        terms: &TermTable,
        goals: &GoalTable,
        ledger: &mut DerivationLedger,
    ) -> FactState {
        let event = ledger.event(FlowEventKind::Join, None);
        let mut full = reference_join_layer(states, terms, goals, ledger, event);
        if full.all_derivable {
            return full;
        }
        let mut ordinary = states.to_vec();
        for state in &mut ordinary {
            // Do not trust the optimized candidate-presence flag or record.
            state.kill_proof_candidates(ledger, |_, _, proof| {
                ledger.depends_on_postcondition_call(proof)
            });
        }
        let ordinary = reference_join_layer(&ordinary, terms, goals, ledger, event);
        assert!(!ordinary.all_derivable);
        // Keep every ordinary candidate, even when full selected an ordinary
        // proof already. This reference deliberately does not share the
        // optimized fallback omission rule.
        for (left, right, bound, proof) in ordinary.bounds.cells() {
            full.add_bound(left, right, bound, proof, ledger);
        }
        for (&pair, &proof) in ordinary.distinct_proofs.iter() {
            full.add_distinct_candidate(pair, proof, ledger);
        }
        full
    }

    fn assert_flow_states_agree(
        states: &[FactState; 2],
        terms: &TermTable,
        goals: &GoalTable,
        ledgers: &mut [DerivationLedger; 2],
    ) {
        // Compare both layers, so loss of an unselected ordinary candidate
        // cannot hide behind an unchanged selected bound.
        for ordinary in [false, true] {
            let mut views = states.clone();
            if ordinary {
                for (state, ledger) in views.iter_mut().zip(ledgers.iter()) {
                    state.kill_proof_candidates(ledger, |_, _, proof| {
                        ledger.depends_on_postcondition_call(proof)
                    });
                }
            }
            views[1].closure = ClosureRecord::Unknown;
            views[1].closed_view.take();
            views[1].view_seed = None;
            let fast = close(&views[0], terms, goals, &mut ledgers[0]);
            let reference = with_every_term(|| close(&views[1], terms, goals, &mut ledgers[1]));
            assert_eq!(
                fast.all_derivable, reference.all_derivable,
                "ordinary={ordinary}"
            );
            if fast.all_derivable {
                continue;
            }
            let actual = bound_values(&fast);
            let expected = bound_values(&reference);
            assert_eq!(actual.len(), expected.len(), "ordinary={ordinary}");
            for (actual, expected) in actual.iter().zip(&expected) {
                assert_eq!(actual, expected, "ordinary={ordinary}");
            }
            assert_eq!(
                distinct_pairs(&fast, &reference),
                distinct_pairs(&reference, &fast),
                "ordinary={ordinary}"
            );
            assert_eq!(fast.opaque, reference.opaque, "ordinary={ordinary}");
            for goal in goals.ids() {
                for sign in [GoalSign::Positive, GoalSign::Negative] {
                    assert_eq!(
                        fast.derives_goal(goal, sign, goals),
                        reference.derives_goal(goal, sign, goals)
                    );
                }
            }
        }
    }

    /// Random flows over a few terms interleave source and postcondition
    /// relations, disequalities, materialized kills, holder kills that weaken
    /// selections, ordinary views, joins with an earlier copy and newly
    /// registered terms. After every step each closure and contradiction probe
    /// of the live states is compared with the complete closure. Independently
    /// advanced reference states also check what each transition preserved.
    #[test]
    fn generated_flows_close_incrementally_like_the_complete_closure() {
        VERIFY_SEEDED_CLOSURE.with(|verify| verify.set(true));
        VERIFIED_CLOSURES.with(|count| count.set(0));
        ROUTE_COUNTS.with(|counts| counts.set([0; CLOSURE_ROUTES.len()]));
        let mut actions = [0_usize; 13];
        for case in 0..400_u64 {
            let mut seed = case.wrapping_mul(0x9e37_79b9_7f4a_7c15).wrapping_add(1);
            let mut terms = TermTable::new();
            let mut binding = 0;
            let mut places = Vec::new();
            let mut register = |terms: &mut TermTable, places: &mut Vec<TermId>| {
                let ty = if binding % 2 == 0 {
                    IntegerType::U8
                } else {
                    IntegerType::I32
                };
                places.push(terms.intern(TermKind::Place(
                    super::super::term::ResolvedPlace::binding(BindingId(binding)),
                    ty,
                )));
                binding += 1;
            };
            for _ in 0..4 {
                register(&mut terms, &mut places);
            }
            places.push(terms.intern(TermKind::Constant(3)));
            let measure = terms.intern(TermKind::Measure(
                CheckedMeasure::Length,
                super::super::term::ResolvedPlace::binding(BindingId(100)),
            ));
            terms.set_measure_bound(measure, MeasureBound::Constant(7));
            places.push(measure);
            // The place's capacity and head join its length in one implicit
            // component through `len <= cap` and `head <= cap`, which the
            // relations below sometimes reach and sometimes leave dormant;
            // a second place's component no relation ever names.
            for sibling in [CheckedMeasure::Capacity, CheckedMeasure::Head] {
                places.push(terms.intern(TermKind::Measure(
                    sibling,
                    super::super::term::ResolvedPlace::binding(BindingId(100)),
                )));
            }
            let dormant = terms.intern(TermKind::Measure(
                CheckedMeasure::Length,
                super::super::term::ResolvedPlace::binding(BindingId(102)),
            ));
            terms.set_measure_bound(dormant, MeasureBound::Constant(5));
            for sibling in [CheckedMeasure::Capacity, CheckedMeasure::Head] {
                terms.intern(TermKind::Measure(
                    sibling,
                    super::super::term::ResolvedPlace::binding(BindingId(102)),
                ));
            }
            let mut goals = GoalTable::default();
            let goal = goals.intern(
                GoalExpression::Datum(super::super::super::goal::GoalDatum::Place {
                    root: BindingId(101),
                    projections: Vec::new(),
                    ty: super::super::super::model::CheckedType::Bool,
                }),
                None,
                None,
                vec![GoalSupport {
                    root: BindingId(101),
                    projections: Vec::new(),
                    measure: None,
                }],
            );
            let mut ledgers = [DerivationLedger::default(), DerivationLedger::default()];
            let events = ledgers
                .each_mut()
                .map(|ledger| ledger.event(FlowEventKind::S1, None));
            let mut states = [FactState::new(), FactState::new()];
            let mut earlier: Option<[FactState; 2]> = None;
            let mut trace = Vec::new();
            for _ in 0..24 {
                let left = places[next_step(&mut seed) % places.len()];
                let right = places[next_step(&mut seed) % places.len()];
                let bound = i128::try_from(next_step(&mut seed) % 7).expect("small") - 3;
                let action = next_step(&mut seed) % actions.len();
                trace.push((action, left, right, bound));
                actions[action] += 1;
                if action == 10 {
                    register(&mut terms, &mut places);
                }
                for index in 0..2 {
                    let state = &mut states[index];
                    let ledger = &mut ledgers[index];
                    let event = events[index];
                    match action {
                        0..=2 if left != right => {
                            state.establish(&Relation::Bound { left, right, bound }, ledger, event);
                        }
                        3 if left != right => {
                            state.establish(
                                &Relation::Distinct {
                                    left,
                                    right,
                                    difference: bound,
                                },
                                ledger,
                                event,
                            );
                        }
                        4 | 5 if left != right => {
                            let relation = Relation::Bound { left, right, bound };
                            let call = postcondition_call_proof(ledger, relation.clone());
                            state.establish_from_proof(&relation, call, ledger);
                        }
                        6 => {
                            if index == 0 {
                                materialize_closure_before_kill(state, &terms, &goals, ledger);
                            } else {
                                *state = reference_join(
                                    std::slice::from_ref(state),
                                    &terms,
                                    &goals,
                                    ledger,
                                );
                            }
                            state.kill(|term| term == left);
                        }
                        7 => {
                            state.kill_proof_candidates(ledger, |from, to, proof| {
                                ledger.depends_on_postcondition_call(proof)
                                    && (from == left || to == left)
                            });
                        }
                        8 => {
                            let mut ordinary = state.clone();
                            ordinary.retain_non_postcondition_candidates(ledger);
                            let _ = close(&ordinary, &terms, &goals, ledger);
                        }
                        9 => {
                            if let Some(copy) = &earlier {
                                let incoming = [state.clone(), copy[index].clone()];
                                *state = if index == 0 {
                                    let join = ledger.event(FlowEventKind::Join, None);
                                    join_at(&incoming, &terms, &goals, ledger, join)
                                } else {
                                    reference_join(&incoming, &terms, &goals, ledger)
                                };
                            }
                        }
                        11 => state.establish_goal(
                            goal,
                            if bound < 0 {
                                GoalSign::Negative
                            } else {
                                GoalSign::Positive
                            },
                            ledger,
                            event,
                        ),
                        12 => state.kill_goals(|held| held == goal),
                        _ => {}
                    }
                    if index == 0 {
                        // A closed record claims its stored selections are
                        // the closure of its facts, in each layer.
                        for ordinary_layer in [false, true] {
                            let mut probe = state.clone();
                            if ordinary_layer {
                                probe.retain_non_postcondition_candidates(ledger);
                            }
                            if probe.all_derivable
                                || !probe.closure.is_closed_over(terms.ids().count())
                            {
                                continue;
                            }
                            let mut unrecorded = probe.clone();
                            unrecorded.closure = ClosureRecord::Unknown;
                            unrecorded.view_seed = None;
                            let mut scratch = ledger.clone();
                            let complete = with_every_term(|| {
                                close_with_excluded_term(
                                    &unrecorded,
                                    &terms,
                                    &goals,
                                    &mut scratch,
                                    None,
                                )
                            });
                            if complete.all_derivable {
                                continue;
                            }
                            for (left, right, bound, _) in probe.bounds.cells() {
                                assert_eq!(
                                    complete.value(left, right),
                                    Some(bound),
                                    "a closed record holds a cell weaker than its closure"
                                );
                            }
                        }
                        // Keep the view on the actual optimized state so the
                        // comparison of its clone also checks memo reuse.
                        let _ = close(state, &terms, &goals, ledger);
                    }
                    let _ = contradiction_without_proofs(state, &terms, &goals);
                }
                if action == 9 {
                    earlier = if earlier.is_none() {
                        Some(states.clone())
                    } else {
                        None
                    };
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    assert_flow_states_agree(&states, &terms, &goals, &mut ledgers);
                }));
                assert!(result.is_ok(), "case {case}: {trace:?}");
                if let Some(copy) = &earlier {
                    assert_flow_states_agree(copy, &terms, &goals, &mut ledgers);
                }
            }
        }
        VERIFY_SEEDED_CLOSURE.with(|verify| verify.set(false));
        assert!(VERIFIED_CLOSURES.with(Cell::get) > 0);
        assert!(actions.into_iter().all(|count| count > 0));
        for (route, count) in CLOSURE_ROUTES.into_iter().zip(ROUTE_COUNTS.with(Cell::get)) {
            assert!(count > 0, "generated flows never exercised {route:?}");
        }
    }
}
