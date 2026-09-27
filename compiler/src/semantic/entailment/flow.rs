//! [ENT-3] flow of facts over the conservative structural graph [FN-1], with
//! [ENT-5] kills, joins, and the no-induction loop rule, and [ENT-6]
//! obligation discharge with residual rendering.
//!
//! The walker carries the live fact state forward through the checked
//! statement tree, which is the structural graph: statements sequence, match
//! arms fork and join, loops iterate through their break edges, and
//! `return`/`give`/`break`/`propagate` leave scopes on edges. Scope-exit
//! kills always apply on the edge, before any join at the edge's target.
//!
//! The analysis is divided along its writers
//! (`design/compiler/engine-components.md`). [`Input`] is what it reads and
//! never changes; [`Vocabulary`] holds the terms, goals, affine atoms and the
//! one derivation ledger every judgment is stated in; [`Output`] is what it
//! publishes; [`Frames`] belongs to the one walk. A method takes the narrowest
//! of these its work needs: [`Reasoning`] lends the inputs and the vocabulary
//! to forming, closing and proving, [`Judging`] adds the outputs a judgment
//! is recorded in, and [`Analyzer`] owns all four for the walk.
//!
//! This module holds those types, the per-path states and the entry points.
//! The components are its children: the fact [`domain`] and its joins, kill
//! and scope [`events`], [`goals`], the one [`prover`] dispatcher, the
//! [`judge`]ments, the [ENT-3] fact [`sources`] with Result transport in
//! [`results`], conversions in [`conversions`] and the [ENT-3.S7] operation
//! table in [`operation_facts`], [`postconditions`], loop [`invariants`],
//! [PRF-1] [`certificates`], the [`walk`], the [`loop_summary`], and
//! canonical [`render`]ing.

mod certificates;
mod conversions;
mod domain;
mod events;
mod goals;
mod invariants;
mod judge;
mod loop_summary;
mod operation_facts;
mod postconditions;
mod prover;
mod render;
mod results;
mod sources;
mod walk;

use certificates::*;
use domain::*;
use events::*;
use goals::*;
use invariants::*;
use judge::*;
use loop_summary::*;
use postconditions::*;
use prover::*;
use walk::*;

use sources::{bound_place, capture_counted_preheader};

use super::super::postcondition::PostconditionPlace;
use results::ResultEvidence;
use sources::{MeasureCarry, ValueImage};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use super::super::goal::{
    CheckedRequirement, ConcreteGoal, EvaluatedValueOccurrence, GoalDatum, GoalExpression,
    GoalOperation, GoalProjection,
};
use super::super::model::expression_children;
use super::super::model::{
    BindingId, CheckedAffineExpression, CheckedAffineExpressionKind, CheckedAffineRelation,
    CheckedArrayRoot, CheckedBooleanOperation, CheckedConst, CheckedConstructor,
    CheckedContainerRoot, CheckedConversionMode, CheckedExpression, CheckedFloatOperation,
    CheckedFunction, CheckedIntegerOperation, CheckedLoopId, CheckedLoopInvariant, CheckedMatchArm,
    CheckedMeasure, CheckedMode, CheckedNominalKind, CheckedNumericType, CheckedPlaceStep,
    CheckedProofMultiplicity, CheckedProofUseSource, CheckedRangeSource, CheckedSetTarget,
    CheckedStatement, CheckedType, CheckedValue, FloatType, IntegerType, MeasureCell, MeasuredKind,
    SubscriptedTerm,
};
use super::super::permission::{PermissionSeparationProof, PermissionSeparationQuery};
use super::super::places::{
    BindingSummary, CaptureId, CapturedRange, CapturedTerm, CapturedValue, NamingForm, PlaceMap,
    PlaceStep, ResolvedPlace, SeparationOracle, WindowPart, named_place,
};
use super::super::postcondition::{
    CheckedPostcondition, NormalizedRelation, PostconditionPlaceRoot, PostconditionReturnDatum,
    PostconditionReturnPlace, PostconditionReturnPlaceRoot, RelationDatum, RelationTemplate,
};
use super::affine::{
    AffineCheckError, AffineCheckLimit, AffineCheckState, AffineCoefficient, AffineForm,
    AffineInequality, AffineTermId, MAX_CERTIFICATE_PREMISES, ScaledAffinePremise,
    integer_tightenings, interval_maximum, interval_proves, sum_explicit_inequalities,
    sum_explicit_scaled_inequalities,
};
use super::polynomial::{CertificatePolynomial, PolynomialError};
use super::state::{
    AffinePremiseUse, ClosedState, CountedRootAtom, DerivationId, DerivationInventory,
    DerivationLedger, DerivationNode, DerivationRootKind, FactState, FlowEventId, FlowEventKind,
    GoalId, GoalNormalization, GoalSign, GoalSupport, GoalTable, IndexCaptureSubstitution,
    IndexSeparationDetail, JoinParent, PostconditionCallSubstitution, RangeSeparationDetail,
    RangeSeparationOrdering, Relation, SourceAffineFactRef, SourceLoopInvariantRef, WordHashMap,
    close, close_excluding_term, closure_is_seeded, contradiction_without_proofs, join_at,
    materialize_closure_at, materialize_closure_before_kill,
};
use super::term::{
    CountedCaptureSide, MeasureBound, MeasurePlacement, PlaceRoot, TermId, TermKind, TermTable,
    ZERO, integer_value, type_range,
};
use super::{
    BoundsRequest, CallGoalDisposition, CallGoalEvidence, CallGoalOutcome, CallTransport,
    ContractGoalOutcome, CountedDerivationSet, EntailmentContext, FunctionEntailment,
    FunctionPostconditionProof, JoinedSourceProofProvenance, LoopInvariantOutcome,
    LoopInvariantProof, ObligationFamily, ObligationOutcome, PostconditionAggregate,
    PostconditionDisposition, PostconditionEntryImage, PostconditionEntryImageOutcome,
    PostconditionExit, SourceProofCertificateFailure, SourceProofCheck, SourceProofOutcome,
    VerifiedPostconditionSummaryRef, fragment_type, overflow_conjuncts_for_values,
};

/// One [ENT-5] kill event gathered from a statement or expression.
#[derive(Clone, Debug, Eq, PartialEq)]
enum KillEvent {
    /// (a) a `set` commit or (b) a boundary-projected callee write. An
    /// element write targets indexed element storage, which never kills a
    /// length fact [ENT-5].
    Write {
        place: ResolvedPlace,
        element: bool,
        source: crate::NodePath,
    },
    /// (c) a consuming use of a binding.
    Consume {
        binding: BindingId,
        source: crate::NodePath,
    },
    /// An affine borrow-holder occurrence whose checked move identity is not
    /// represented by its referent value type. The pre-v0.28 L0 flow did not
    /// apply this event to ordinary facts; FN-9 consumes it only for the new
    /// view-independent entry-image lifetime, preserving no-ensures behavior.
    EntryImageHolderConsume {
        binding: BindingId,
        source: crate::NodePath,
    },
    /// A callee write projected through a directly transferred holder. The
    /// old fact flow did not recognize that checked argument shape; retaining
    /// it as entry-image-only keeps that path unchanged while FN-9 observes
    /// the required effect kill.
    EntryImageHolderWrite {
        place: ResolvedPlace,
        element: bool,
        source: crate::NodePath,
    },
}

impl KillEvent {
    fn source(&self) -> &crate::NodePath {
        match self {
            Self::Write { source, .. }
            | Self::Consume { source, .. }
            | Self::EntryImageHolderConsume { source, .. }
            | Self::EntryImageHolderWrite { source, .. } => source,
        }
    }

    /// The binding the event's place is rooted at; a constant root has none.
    fn root_binding(&self) -> Option<BindingId> {
        match self {
            Self::Write { place, .. } | Self::EntryImageHolderWrite { place, .. } => {
                match place.root {
                    PlaceRoot::Binding(binding) => Some(binding),
                    PlaceRoot::Constant(_) => None,
                }
            }
            Self::Consume { binding, .. } | Self::EntryImageHolderConsume { binding, .. } => {
                Some(*binding)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EntryImageRecord {
    datum: PostconditionEntryImage,
    /// The selected type of the operand's place, which [MSR-1]'s measure
    /// former reads to know what it is measuring.
    ty: CheckedType,
    place: ResolvedPlace,
    holders: Vec<BindingId>,
}

/// A `loop` frame collecting break-edge states for the continuation join.
struct LoopFrame {
    id: CheckedLoopId,
    invariant_declarations: Box<[crate::DeclarationId]>,
    scope_depth: usize,
    /// The compiler-owned counted binder while this is a `for` frame. An
    /// ordinary `loop` has no binder and contributes no affine index image.
    counted_binder: Option<BindingId>,
    /// Immutable numeric values available after the preheader's continuing
    /// kills. Atoms first read or produced in the body are not invariant
    /// merely because their source binding has one spelling.
    invariant_atoms: HashSet<AffineTermId>,
    /// Present only for a counted range. A break through this frame leaves
    /// the private endpoint-capture scope as well as source binding scopes.
    capture_path: Option<Vec<u32>>,
    breaks: Vec<ProofFlowState>,
}

/// The [ENT-3] facts one `match` scrutinee admits at its arms' entries: the
/// S1 comparison relation, taken positively on `True()` and exactly negated
/// on `False()`. Every other arm establishes nothing of its own; an `Ok`
/// arm's success facts arrive through the scrutinee's conditional Result
/// context [ENT-5].
#[derive(Default)]
struct ArmFacts {
    node_path: Option<crate::NodePath>,
    comparison: Option<Relation>,
    goals: Vec<GoalId>,
}

/// A value initializer collecting give-edge states for its continuation.
struct GiveFrame {
    scope_depth: usize,
    loop_depth: usize,
    node_path: crate::NodePath,
    binding: BindingId,
    result_type: CheckedType,
    gives: Vec<ProofFlowState>,
    give_goal_origins: Vec<Option<GoalId>>,
    delivery_images: Vec<ProofFlowState>,
    delivery_edges: Vec<crate::NodePath>,
}

/// Stable source and substitution identity for one [GIVE-1] edge.
struct DeliveryEdgeContext<'a> {
    statement: &'a crate::NodePath,
    carrier_binding: BindingId,
    receiver_binding: BindingId,
    carrier: TermId,
    receiver: TermId,
    event: FlowEventId,
}

/// The value-initializer receiver and lexical boundary shared by its gives.
struct DeliveryImageContext<'a> {
    statement: &'a crate::NodePath,
    receiver_binding: BindingId,
    receiver_type: CheckedType,
    scope_depth: usize,
    loop_depth: usize,
}

/// Stable receiver identity shared by all delivery edges at one join.
struct DeliveryJoinContext<'a> {
    statement: &'a crate::NodePath,
    receiver_binding: BindingId,
    receiver: TermId,
    event: FlowEventId,
}

/// The source facts carried through one structural walk.
#[derive(Clone, Debug, Default)]
struct ProofFlowState {
    facts: FactState,
    results: BTreeMap<BindingId, ResultEvidence>,
    /// Proof-carrying [OWN-7] separations available on this edge. Captured
    /// positions are immutable, but a proof about them is available only
    /// where the judgment that established it dominates this state.
    separations: SeparationLedger,
    /// First invalidating event for each relation-template entry image. This
    /// state branches with the same structural flow; `None` means the image
    /// is still live.
    entry_images: Vec<Option<FlowEventId>>,
    /// Exact integer value images and active source-proved loop invariants.
    /// Executing a statement computes the runtime value represented here.
    affine: AffineFlowState,
    /// [DIAG-1] every binding at which a kill event on some path to this edge
    /// roots its place [ENT-5]. It branches and joins with the rest of the
    /// state, so a write in one arm of a conditional is no write in a
    /// sibling arm, and a loop header carries every continuing kill of its
    /// body before the body is walked. A failed judgment retains it so that
    /// its repair offers a `requires` only over parameters that still hold
    /// their entry values where the goal is asked.
    written: BTreeSet<BindingId>,
    /// The kill events applied on this path since the innermost enclosing
    /// loop head, kept only where debug assertions are on: at the loop's back
    /// edge every one must be an event of the summary its head subtracted
    /// [ENT-5].
    continuing: Vec<KillEvent>,
}

impl ProofFlowState {
    /// Records on this edge the bindings these events root their places at.
    fn record_writes(&mut self, events: &[KillEvent]) {
        self.written
            .extend(events.iter().filter_map(KillEvent::root_binding));
    }

    /// What a judgment recorded at this edge retains of [`Self::written`]:
    /// all of it when the judgment failed, and nothing when it succeeded.
    fn written_before(&self, discharged: bool) -> Vec<BindingId> {
        if discharged {
            Vec::new()
        } else {
            self.written.iter().copied().collect()
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct AffineFlowState {
    values: WordHashMap<BindingId, AffineForm>,
    /// Current measure images belong to this control-flow edge. Queries mint
    /// images lazily, but cloning a predecessor isolates its later kills.
    measure_atoms: RefCell<WordHashMap<TermId, AffineForm>>,
    /// Each range formation's endpoint images [REF-4], filed under its start
    /// endpoint's source occurrence, which names that one formation. Nothing
    /// is filed under a capture naming no single evaluation, so one
    /// formation's image never answers for another's [OWN-7, ENT-3.S6].
    ranges: WordHashMap<CaptureId, AffineRangeImage>,
    indices: WordHashMap<CaptureId, AffineForm>,
    /// One atom standing for the whole value of a binding whose image is not
    /// already a single atom, minted on first demand.
    ///
    /// A local's image is transparent — `let stride = width + padding;` gives
    /// `stride` the image `width + padding` — which is what an affine relation
    /// wants and what [PRF-1]'s fold cannot use: a product over `stride`
    /// distributes into its operands and no admitted multiplication matches
    /// the pieces. This map is the opposite handle on the same binding, one
    /// value the certificate can name, and the fact published beside it keeps
    /// the transparent reading available to everything else. Keyed and killed
    /// exactly as `values` is, so a write mints a fresh handle for a fresh
    /// value.
    opaque_values: WordHashMap<BindingId, AffineForm>,
    /// Every published affine conclusion at this control-flow point. Fact
    /// identity is only the canonical inequality over immutable value images;
    /// evidence is retained solely to explain a selected derivation.
    facts: Vec<ActiveAffineFact>,
    /// Exact immutable theorem image published by each resolved invariant
    /// declaration. Resolution owns visibility; this map carries only the
    /// canonical proposition proved at that declaration's execution point.
    published_invariants: HashMap<crate::DeclarationId, AffineInequality>,
}

/// The affine images of one range formation's two captured endpoints
/// [REF-4], with the node the formation stands at.
#[derive(Clone, Debug, Eq, PartialEq)]
struct AffineRangeImage {
    source: crate::NodePath,
    start: AffineForm,
    end: AffineForm,
}

struct PermissionSeparationAttempt {
    query: PermissionSeparationQuery,
    attempted: bool,
    discharged: bool,
    derivations: Vec<DerivationId>,
}

/// Exact `stride * i + base` decomposition for one counted binder. Both
/// components are invariant affine values; no general polynomial search is
/// involved in recognizing this deliberately finite PAR-2 family.
struct CountedValueImage {
    stride: AffineForm,
    base: AffineForm,
}

/// The numeric/logical proof state at one exact control-flow point.
#[derive(Clone, Copy)]
struct ProofContext<'a> {
    facts: &'a FactState,
    affine: &'a AffineFlowState,
    closed: Option<&'a ProofClosure>,
}

impl<'a> ProofContext<'a> {
    fn new(facts: &'a FactState, affine: &'a AffineFlowState) -> Self {
        Self {
            facts,
            affine,
            closed: None,
        }
    }

    fn close(
        self,
        terms: &TermTable,
        goals: &GoalTable,
        ledger: &mut DerivationLedger,
    ) -> Rc<ClosedState> {
        if let Some(closed) = self.closed.filter(|closed| closed.matches(terms, goals)) {
            Rc::clone(&closed.state)
        } else {
            close(self.facts, terms, goals, ledger)
        }
    }
}

/// A query-only view of one immutable entering fact state. It never becomes
/// live facts or escapes the premise loop that owns that state borrow. The
/// fact state's remembered view would also serve the loop's repeated
/// closures; this explicit view keeps the premise loop's reuse independent of
/// how that memo is keyed.
struct ProofClosure {
    term_revision: usize,
    goal_revision: usize,
    state: Rc<ClosedState>,
    /// The ordered L0-to-affine index of this same entering value map. The
    /// premise loop borrows that map immutably for the view's entire life.
    /// Registering a term or changing goal metadata invalidates both views.
    affine_index: std::cell::RefCell<Option<Rc<AffineL0Index>>>,
}

impl ProofClosure {
    fn new(
        facts: &FactState,
        terms: &TermTable,
        goals: &GoalTable,
        ledger: &mut DerivationLedger,
    ) -> Self {
        Self {
            term_revision: terms.revision(),
            goal_revision: goals.revision(),
            state: close(facts, terms, goals, ledger),
            affine_index: std::cell::RefCell::new(None),
        }
    }

    fn matches(&self, terms: &TermTable, goals: &GoalTable) -> bool {
        self.term_revision == terms.revision() && self.goal_revision == goals.revision()
    }

    fn affine_index(&self, terms: &TermTable, goals: &GoalTable) -> Option<Rc<AffineL0Index>> {
        self.matches(terms, goals)
            .then(|| self.affine_index.borrow().as_ref().map(Rc::clone))
            .flatten()
    }
}

/// One consumer-normalized numeric/logical proposition. A signed goal keeps
/// the finite Boolean structure written at a call boundary and lets the proof
/// entry normalize its ordering leaves under that fixed structure. An
/// ordering goal is the exact L0 relation selected for a function
/// postcondition. Either consumer may additionally provide the unique affine
/// inequality for a direct-root proposition; the proof entry never invents
/// another formula.
enum ProofGoal<'a> {
    /// One canonical affine target with the right operand retained by its
    /// source normalization for the complete MSR-4 disposition.
    Affine {
        inequality: &'a AffineInequality,
        /// The exact right-hand term retained by source normalization, when
        /// it has an L0 spelling. Never reconstructed from coefficients.
        right: Option<TermId>,
    },
    /// PRF-1 admits written relation premises and judges certificate
    /// redundancy through AUTO alone. Neither query may borrow MSR-4's
    /// Step 6 route, which a blockless INV-1 target may use.
    AutomaticAffine { inequality: &'a AffineInequality },
    Signed {
        expression: &'a GoalExpression,
        affine: Option<&'a AffineInequality>,
    },
    Ordering {
        relation: &'a Relation,
        affine: Option<&'a [AffineInequality]>,
    },
    /// One OP-2 exact-integer domain proposition. The dispatcher first checks
    /// its finite goal/L0 normalization, then the fixed affine clauses, then
    /// the one fixed two-operand interval-product rule. The consumer chooses
    /// none of those routes and performs no second query.
    IntegerDomain(IntegerDomainGoal<'a>),
    ConversionDomain {
        canonical: &'a GoalExpression,
        operand: Option<TermId>,
        image: Option<&'a AffineForm>,
    },
    /// One exact relation `left - right <= bound`. OP-4 submits the
    /// same proposition through this entry; the finite signed form and the
    /// explicitly prepared affine forms are alternate representations of
    /// that proposition, not additional queries by either consumer.
    BoundedRelation(BoundedRelationGoal<'a>),
    /// One fixed ordering relation exposed through an optional finite goal
    /// normalization. OP-9 supplies both identities when its source operand
    /// belongs to the goal fragment, and only the relation otherwise.
    NormalizedOrdering {
        goal: Option<GoalId>,
        relation: Option<&'a Relation>,
        affine: Option<&'a AffineInequality>,
        right: Option<TermId>,
        upper_bound: Option<NumericUpperBoundRequest<'a>>,
    },
}

#[derive(Clone, Copy)]
struct BoundedRelationGoal<'a> {
    canonical: Option<&'a GoalExpression>,
    request: Option<BoundsRequest>,
    direct_affine: Option<&'a AffineInequality>,
    fixed_affine_bridge: Option<FixedAffineBoundBridge<'a>>,
    affine_left: Option<&'a AffineForm>,
}

/// One fixed affine first step followed by ordinary difference closure.  The
/// consumer supplies the exact first-step inequality and middle term; the
/// checker derives the remaining middle-to-right bound from the submitted
/// relation, so no route or coefficient search is introduced.
#[derive(Clone, Copy)]
struct FixedAffineBoundBridge<'a> {
    target: &'a AffineInequality,
    middle: TermId,
    left_to_middle_bound: i128,
}

#[derive(Clone)]
struct NumericAffineTarget {
    inequality: AffineInequality,
    right: Option<TermId>,
}

struct IntegerDomainGoal<'a> {
    canonical: Option<GoalId>,
    operation: CheckedIntegerOperation,
    operand_type: CheckedType,
    components: &'a [BoundsRequest],
    affine_clauses: Option<&'a [Vec<NumericAffineTarget>]>,
    affine_product: Option<&'a AffineIntegerProduct>,
}

/// Optional numeric projection requested by a consumer of one proved
/// ordering. `admitted` is the ceiling stated by that exact ordering; `term`
/// and `affine` are its two current-context value images, when available.
/// Projection can tighten the admitted ceiling, but never decides whether the
/// ordering itself is proved.
#[derive(Clone, Copy)]
struct NumericUpperBoundRequest<'a> {
    term: Option<TermId>,
    affine: Option<&'a AffineForm>,
    admitted: i128,
}

#[derive(Clone, Copy)]
struct ProvedNumericUpperBound {
    value: i128,
    derivation: DerivationId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProofDisposition {
    Proved,
    Refuted,
    Unknown,
}

/// [MSR-4] the disposition of one [INV-1] target, which no Goal identity
/// carries: refuted when the state derives the negation of one of its bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetDisposition {
    Proved,
    Refuted,
    Unproved,
}

/// Complete route selected by one [`Reasoning::prove`] call.  The signed
/// ordinary route retains every simultaneously available finite ground so
/// FN-8 can preserve its existing evidence payload without running a second
/// query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProofRoute {
    Contradiction,
    SignedOrdinary {
        opaque: bool,
        projection: bool,
        normalization: bool,
        introduction: bool,
    },
    FiniteGoal,
    L0,
    Affine,
}

struct ProofResult {
    disposition: ProofDisposition,
    route: Option<ProofRoute>,
    derivation: Option<DerivationId>,
    numeric_upper_bound: Option<ProvedNumericUpperBound>,
    /// The interval [ENT-6]'s fixed interval-product rule proved for an
    /// admitted non-constant multiplication. Carried out of the judgment so
    /// [ENT-3.S7]'s multiplication row publishes exactly the measurement the
    /// domain decision consumed, rather than proving the same endpoints a
    /// second time.
    product_interval: Option<AffineProductInterval>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActiveAffineFact {
    inequality: AffineInequality,
    evidence: AffineFactEvidence,
    /// Enclosing loop assumptions on which this fact still depends. Removing
    /// any listed loop removes the fact; an empty list is path-stable.
    active_loops: Vec<CheckedLoopId>,
}

/// Captured values of one admitted unsigned division. These identities are
/// immutable, like product atoms: a replacement binding gets a fresh image.
#[derive(Clone, Debug)]
struct CapturedUnsignedDivision {
    quotient: AffineForm,
    dividend: AffineForm,
    divisor: AffineForm,
    parent: DerivationId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AffineFactEvidence {
    Source(SourceAffineFactRef),
    Derivation(DerivationId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AffineAtom {
    /// Source integer class used only when a live binding maps directly to
    /// this atom and ordinary L0 facts can therefore tighten its interval.
    ty: IntegerType,
    /// Intrinsic mathematical interval of this atom. Ordinary unknown values
    /// use their full source-type range. A structural affine join may instead
    /// allocate a delta atom whose bounds are the exact minimum and maximum
    /// incoming constants, including negative constants.
    minimum: i128,
    maximum: i128,
    /// Whether a structural join minted this atom to stand for the spread of
    /// its inputs' constants. A delta atom is an ordinary shared atom between
    /// joins, so correlations formed over it survive; a later join folds it
    /// back into the interval it stands for so that nested joins reach the
    /// image their flat equivalent reaches [ENT-6].
    join_delta: bool,
}

/// One input image prepared for a structural join [ENT-6]: the part of it no
/// earlier join minted, and the closed constant interval the folded delta
/// atoms and the written constant together contribute.
struct FoldedJoinImage {
    form: AffineForm,
    minimum: i128,
    maximum: i128,
}

struct PostconditionExitProof {
    disposition: PostconditionDisposition,
    derivation: Option<DerivationId>,
}

struct AffineConsequenceProof {
    premises: Vec<AffinePremiseUse>,
    parents: Vec<DerivationId>,
}

struct AffineL0Candidate {
    term: TermId,
    value: AffineForm,
}

struct AffineL0Entry {
    inequality: AffineInequality,
    left: TermId,
    right: TermId,
    bound: i128,
}

#[derive(Default)]
struct AffineL0Index {
    entries: Vec<AffineL0Entry>,
    by_terms: WordHashMap<Box<[AffineCoefficient]>, usize>,
}

impl AffineL0Index {
    fn entry(&self, terms: &[AffineCoefficient]) -> Option<&AffineL0Entry> {
        self.by_terms.get(terms).map(|index| &self.entries[*index])
    }
}

/// Immutable endpoint information for one atom in a single DIRECT/AUTO
/// traversal. The endpoint's original L0 witness is retained for diagnostics.
struct AffineAtomInterval {
    minimum: i128,
    maximum: i128,
    minimum_parent: Option<(TermId, TermId, i128)>,
    maximum_parent: Option<(TermId, TermId, i128)>,
}

/// Preparation shared by one fixed candidate traversal, never by different
/// program points. Only requested atom endpoints are memoized; every residual
/// still executes the same checked arithmetic and ordered proof rules.
struct AffineDirectQuery<'a> {
    l0: &'a AffineL0Index,
    values: &'a AffineFlowState,
    closed: &'a ClosedState,
    intervals: WordHashMap<AffineTermId, AffineAtomInterval>,
    measures: Option<WordHashMap<AffineTermId, Vec<TermId>>>,
}

impl<'a> AffineDirectQuery<'a> {
    fn new(l0: &'a AffineL0Index, values: &'a AffineFlowState, closed: &'a ClosedState) -> Self {
        Self {
            l0,
            values,
            closed,
            intervals: WordHashMap::default(),
            measures: None,
        }
    }
}

struct AutomaticAffinePremise {
    inequality: AffineInequality,
    source: Option<SourceAffineFactRef>,
    parent: Option<DerivationId>,
}

struct AffineIntervalEndpointProof {
    value: i128,
    consequence: AffineConsequenceProof,
}

struct AffineClosedIntervalProof {
    minimum: AffineIntervalEndpointProof,
    maximum: AffineIntervalEndpointProof,
}

struct AffineIntegerProduct {
    left: AffineForm,
    right: AffineForm,
    ty: IntegerType,
}

/// One written multiplicity, resolved where the certificate is checked.
///
/// A named multiplicity carries the value image its binding holds at the
/// entering program point, so the scaling step reads a value rather than a
/// name and a later write cannot change what was scaled.
#[derive(Clone, Debug)]
enum CertificateMultiplicity {
    Literal(i128),
    Value(AffineForm),
}

/// The accumulated [PRF-1] certificate sum.
///
/// A bare-decimal certificate stays in `Affine` for its whole accumulation and
/// reaches the residual as the inequality it always formed. The first term
/// multiplicity moves the accumulation to `Nonlinear`, where it stays until
/// the nonlinear monomials fold back to admitted products.
enum CertificateSum {
    Empty,
    Affine(AffineInequality),
    Nonlinear(CertificatePolynomial),
}

/// Why one accumulation step could not form, before it is attributed to the
/// written entry that caused it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CertificateStepFailure {
    Overflow,
    UseCapacity,
    Formation,
    InvalidFactor,
}

impl From<AffineCheckError> for CertificateStepFailure {
    fn from(error: AffineCheckError) -> Self {
        match error {
            AffineCheckError::ArithmeticOverflow => Self::Overflow,
            AffineCheckError::LimitExceeded(AffineCheckLimit::CertificatePremises) => {
                Self::UseCapacity
            }
            AffineCheckError::LimitExceeded(_) | AffineCheckError::CoefficientMismatch => {
                Self::Formation
            }
            AffineCheckError::InvalidCertificateFactor => Self::InvalidFactor,
        }
    }
}

impl From<PolynomialError> for CertificateStepFailure {
    fn from(error: PolynomialError) -> Self {
        match error {
            PolynomialError::ArithmeticOverflow => Self::Overflow,
            // A degree-three product means one written multiplicity scaled a
            // premise that already carried a nonlinear monomial; nothing in
            // [PRF-1] forms such a step, so it stops as a formation failure
            // rather than as a claim about the writer's factor.
            PolynomialError::DegreeExceeded | PolynomialError::LimitExceeded => Self::Formation,
        }
    }
}

/// What the fixed interval-product rule proved about one admitted
/// multiplication: the inclusive interval its four endpoint products bound,
/// and the affine consequences that proved the operand endpoints.
#[derive(Clone, Debug, Eq, PartialEq)]
struct AffineProductInterval {
    minimum: i128,
    maximum: i128,
    consequences: Box<[DerivationId]>,
}

#[derive(Clone, Copy)]
enum IntegerDomainPlanKind {
    Conjunction,
    SignedDivision,
}

struct IntegerDomainPlan {
    components: Vec<BoundsRequest>,
    kind: IntegerDomainPlanKind,
}

impl IntegerDomainPlan {
    fn normalization(&self) -> GoalNormalization {
        let components = self.components.iter().map(request_relation).collect();
        match self.kind {
            IntegerDomainPlanKind::Conjunction => GoalNormalization::conjunction(components),
            IntegerDomainPlanKind::SignedDivision => GoalNormalization::signed_division(components),
        }
    }
}

#[derive(Clone, Copy)]
struct IntegerDomainOperand {
    term: Option<TermId>,
    constant: Option<i128>,
}

/// Transient A0 evidence for an exact root user call. The flow consumes it
/// once, after call effects, to publish ordinary or conditional facts; it
/// never enters the checked expression tree or survives for later substitution.
#[derive(Clone, Debug)]
struct PreparedCall {
    /// The selected executable callee [CALL-6, ENT-3.S13]. Direct calls
    /// publish its verified FN-9 relations; bound calls use its parameter
    /// metadata while `postconditions` carries the formal FN-5 surface.
    callee: super::super::model::FunctionId,
    /// Exact [FN-5] publication surface selected for this call. A bound call
    /// carries formal relations with FN-4/FN-9 authority; a direct call
    /// carries its callee's verified relations.
    postconditions: Vec<AvailablePostcondition>,
    call: crate::NodePath,
    parents: Vec<DerivationId>,
    transfer_events: Vec<FlowEventId>,
    kills: Vec<KillEvent>,
    /// The bounds by `r.len` the call's entry state proves [WIN-2]: every
    /// part a row names is interpreted at call entry, so each of `kills` is
    /// judged against this one set.
    live: LiveBounds,
}

impl PreparedCall {
    /// The separations each of `kills` is judged under: the edge's ledger
    /// and the bounds by `r.len` the call's entry state proved [WIN-2, ENT-5].
    fn entry_separations<'call>(
        &'call self,
        ledger: &'call SeparationLedger,
    ) -> EventSeparations<'call> {
        EventSeparations {
            ledger,
            live: &self.live,
        }
    }
}

/// Result of judging one expression in source evaluation order.
///
/// `reached` is independent of postcondition preparation: it says that every
/// acceptance-bearing judgment needed to produce this value succeeded, so the
/// expression may receive an admitted structural identity. A successfully
/// evaluated call may have no verified postcondition summary and therefore no
/// `prepared_call`.
struct ExpressionJudgment {
    prepared_call: Option<PreparedCall>,
    reached: bool,
}

#[derive(Clone, Debug)]
struct AvailablePostcondition {
    relation: RelationTemplate,
    variant: Option<crate::BuiltinPreludeId>,
    field: Option<crate::BuiltinPreludeId>,
    authority: super::RelationProvenance,
}

/// [MSR-3] one payload placement's datums, held between the mint before the
/// `match` consumes its scrutinee and the establishment at the arm binder
/// that names the payload.
struct PayloadPlacement {
    /// The tag of the arm these datums reach; every other arm binds no
    /// payload of this enum.
    tag: u32,
    carried: Vec<(u32, MeasureCarry)>,
}

#[derive(Clone)]
struct InstantiatedPostcondition {
    relation: Relation,
    substitutions: Vec<PostconditionCallSubstitution>,
}

#[derive(Clone, Copy)]
struct DirectReceiverRoute {
    binding: BindingId,
    formal: u32,
    ty: CheckedType,
}

struct DirectReceiverCandidate {
    route: DirectReceiverRoute,
    available: AvailablePostcondition,
    instantiated: InstantiatedPostcondition,
}

/// The [ENT-5] loop rule's structural kill summary of one loop body.
#[derive(Default)]
struct LoopKills {
    events: Vec<KillEvent>,
    /// Statement/expression event groups retain semantic evaluation order
    /// within one carrier while the reachability walk scans statements in
    /// reverse. Entry-image invalidation reorders only these groups by source,
    /// never the argument-consume/callee-write events inside a group.
    entry_image_groups: Vec<LoopKillEventGroup>,
    /// Every binding named as a `set` target. An ordinary-let origin is valid
    /// only while its bound value has no intervening whole, field, or element
    /// mutation; the narrower comparison/outcome origins can only inhabit
    /// nonprojectable Bool/outcome bindings, so this same set is exact there.
    set_bindings: HashSet<BindingId>,
}

struct LoopKillEventGroup {
    owner: crate::NodePath,
    range: std::ops::Range<usize>,
}

impl LoopKills {
    fn push_event_group(&mut self, events: Vec<KillEvent>) {
        let Some(owner) = events
            .iter()
            .map(KillEvent::source)
            .min_by(|left, right| left.components().cmp(right.components()))
            .cloned()
        else {
            return;
        };
        let start = self.events.len();
        self.events.extend(events);
        self.entry_image_groups.push(LoopKillEventGroup {
            owner,
            range: start..self.events.len(),
        });
    }
}

// A continuing scope-exit edge can close only scopes opened inside the target
// loop body. No binding from such a scope can support a fact in the pre-loop
// state this summary filters. An edge that closes a pre-loop binding's scope
// necessarily leaves the target body and is non-continuing, so kill event (d)
// needs no payload in `LoopKills`.

/// Non-local successors visible while asking whether an edge inside one loop
/// body can reach that loop's next iteration head without leaving the body.
/// Targets outside the body are deliberately absent and therefore do not
/// reach the head.
#[derive(Default)]
struct LoopReachability {
    breaks: Vec<(CheckedLoopId, bool)>,
    gives: Vec<bool>,
}

impl LoopReachability {
    fn break_reaches(&self, target: CheckedLoopId) -> bool {
        self.breaks
            .iter()
            .rev()
            .find_map(|(id, reaches)| (*id == target).then_some(*reaches))
            .unwrap_or(false)
    }
}

pub(super) fn analyze(
    function: &CheckedFunction,
    context: &EntailmentContext<'_>,
) -> FunctionEntailment {
    let mut entailment = analyze_candidate_inner(function, context);
    finish(&mut entailment);
    entailment
}

/// Builds one optimistic per-function FN-9 proof batch without pruning or
/// remapping its derivation ledger. The checker calls [`finish`] after the
/// component publication decision.
pub(super) fn analyze_candidate(
    function: &CheckedFunction,
    context: &EntailmentContext<'_>,
) -> FunctionEntailment {
    let mut entailment = analyze_candidate_inner(function, context);
    entailment.derivations.settle();
    entailment
}

/// Runs one [FN-4] declaration-level implication query through the same S4
/// premise establishment and AUTO proof entry as an ordinary call goal. The
/// synthetic function has no body: its parameters are alpha-renamed contract
/// datums, and no hypothetical premise escapes this query as a body fact.
pub(super) fn contract_implies(
    function: &CheckedFunction,
    context: &EntailmentContext<'_>,
    goal: &GoalExpression,
) -> FunctionEntailment {
    let mut analyzer = Analyzer::new(context, function);
    analyzer.input.collect_bindings();
    let mut state = ProofFlowState::default();
    analyzer
        .reasoning()
        .initialize_affine_parameters(&mut state.affine);
    for (ordinal, requirement) in function.requirements.iter().enumerate() {
        let event = analyzer
            .vocabulary
            .proof_event(FlowEventKind::S4, Some(&requirement.clause));
        analyzer
            .judging()
            .establish_requires_facts(requirement, &mut state.facts, event);
        analyzer
            .reasoning()
            .establish_requirement_affine_images(requirement, ordinal, &mut state);
    }
    let Some(goal) = analyzer.input.body_goal_expression(goal) else {
        return FunctionEntailment::default();
    };
    let goal = ConcreteGoal::new(goal);
    let (disposition, evidence, derivation) = analyzer
        .reasoning()
        .call_goal_disposition(&goal, ProofContext::new(&state.facts, &state.affine));
    if let Some(root) = derivation {
        analyzer
            .vocabulary
            .derivations
            .add_root(DerivationRootKind::ContractGoal(0), root);
    }
    let (terms, measure_bounds) = analyzer.vocabulary.terms.into_inventory();
    FunctionEntailment {
        contract_goals: vec![ContractGoalOutcome {
            goal,
            disposition,
            evidence,
            derivation,
        }],
        derivations: analyzer.vocabulary.derivations,
        inventory: DerivationInventory {
            terms,
            measure_bounds,
            goals: analyzer.vocabulary.goals.into_inventory(),
        },
        ..FunctionEntailment::default()
    }
}

fn analyze_candidate_inner(
    function: &CheckedFunction,
    context: &EntailmentContext<'_>,
) -> FunctionEntailment {
    let run = run(function, context);
    let mut entailment = FunctionEntailment {
        body_disposition: run.body_disposition,
        obligations: run.obligations,
        call_goals: run.call_goals,
        contract_goals: Vec::new(),
        counted_derivations: run.counted_derivations,
        loop_invariants: run.loop_invariants,
        source_proofs: run.source_proofs,
        joined_source_proofs: run.joined_source_proofs,
        postconditions: run.postconditions,
        boolean_decompositions: run.boolean_decompositions,
        permission_separations: run.permission_separations,
        answers: Vec::new(),
        unrecorded: Vec::new(),
        derivations: run.derivations,
        inventory: run.inventory,
    };
    (entailment.answers, entailment.unrecorded) = super::answer_records(function, &entailment);
    entailment
}

struct AnalysisRun {
    body_disposition: super::super::model::CheckedBodyDisposition,
    obligations: Vec<ObligationOutcome>,
    call_goals: Vec<CallGoalOutcome>,
    counted_derivations: Vec<CountedDerivationSet>,
    loop_invariants: Vec<LoopInvariantOutcome>,
    source_proofs: Vec<SourceProofOutcome>,
    joined_source_proofs: Vec<JoinedSourceProofProvenance>,
    postconditions: Vec<super::FunctionPostconditionProof>,
    boolean_decompositions: Vec<super::BooleanGoalDecomposition>,
    permission_separations: Vec<PermissionSeparationProof>,
    derivations: DerivationLedger,
    inventory: DerivationInventory,
}

fn run(function: &CheckedFunction, context: &EntailmentContext<'_>) -> AnalysisRun {
    let mut analyzer = Analyzer::new(context, function);
    analyzer.input.collect_bindings();
    analyzer.input.collect_postcondition_entry_images();
    let mut state = ProofFlowState {
        entry_images: vec![None; analyzer.input.entry_images.len()],
        ..ProofFlowState::default()
    };
    analyzer
        .reasoning()
        .initialize_affine_parameters(&mut state.affine);
    // [MSR-3] the entry placement, before every other source: one immutable
    // datum per measure of a parameter any declared relation names, equal to
    // that measure at body entry.
    analyzer.reasoning().establish_entry_datums(&mut state);
    analyzer
        .frames
        .scopes
        .push(function.parameters.iter().map(|p| p.binding).collect());
    // [ENT-3] S4: every substituted `requires` goal independently enters the
    // body state in source order. No clause derives another clause, but a
    // clause's places are formed in the state the earlier clauses built
    // [ENT-2, FN-8], so their subscripts are judged just before it enters.
    for (ordinal, requirement) in function.requirements.iter().enumerate() {
        if let Some(places) = function.requirement_places.get(ordinal) {
            analyzer.judge_clause_places(places, &mut state);
        }
        let event = analyzer
            .vocabulary
            .proof_event(FlowEventKind::S4, Some(&requirement.clause));
        analyzer
            .judging()
            .establish_requires_facts(requirement, &mut state.facts, event);
        analyzer
            .reasoning()
            .establish_requirement_affine_images(requirement, ordinal, &mut state);
    }
    let body_disposition = {
        let closed = close(
            &state.facts,
            &analyzer.vocabulary.terms,
            &analyzer.vocabulary.goals,
            &mut analyzer.vocabulary.derivations,
        );
        match closed.contradiction_proof() {
            Some(contradiction) => {
                analyzer
                    .vocabulary
                    .derivations
                    .add_root(DerivationRootKind::BodyEntryContradiction, contradiction);
                super::super::model::CheckedBodyDisposition::Uninhabited { contradiction }
            }
            None => super::super::model::CheckedBodyDisposition::Inhabited,
        }
    };
    if matches!(
        body_disposition,
        super::super::model::CheckedBodyDisposition::Inhabited
    ) {
        analyzer.judging().initialize_postcondition_proofs();
    }
    if let Some(body) = &function.body {
        analyzer.walk_block(body, &mut state);
    }
    analyzer.judging().reject_unjudged_separations();
    analyzer.frames.scopes.pop();
    analyzer.judging().finalize_postcondition_aggregates();
    let permission_separations = analyzer.judging().finalize_permission_separations();
    assert_eq!(
        analyzer.vocabulary.completed_counted_roots, analyzer.vocabulary.encountered_counted,
        "every encountered counted statement must publish one complete S11 root group"
    );
    let (terms, measure_bounds) = analyzer.vocabulary.terms.into_inventory();
    let inventory = DerivationInventory {
        terms,
        measure_bounds,
        goals: analyzer.vocabulary.goals.into_inventory(),
    };
    AnalysisRun {
        body_disposition,
        obligations: analyzer.output.obligations,
        call_goals: analyzer.output.call_goals,
        counted_derivations: analyzer.output.counted_derivations,
        loop_invariants: analyzer.output.loop_invariants,
        source_proofs: analyzer.output.source_proofs,
        joined_source_proofs: analyzer.output.joined_source_proofs,
        postconditions: analyzer.output.postconditions,
        boolean_decompositions: analyzer.output.boolean_decompositions,
        permission_separations,
        derivations: analyzer.vocabulary.derivations,
        inventory,
    }
}

impl<'check, 'unit> Analyzer<'check, 'unit> {
    fn new(context: &'check EntailmentContext<'unit>, function: &'check CheckedFunction) -> Self {
        Self {
            input: Input {
                context,
                function,
                places: PlaceMap::default(),
                entry_images: Vec::new(),
                postcondition_entry_images: Vec::new(),
            },
            vocabulary: Vocabulary {
                terms: TermTable::new(),
                goals: GoalTable::default(),
                derivations: DerivationLedger::default(),
                affine_atoms: Vec::new(),
                measure_terms_seen: Vec::new(),
                measure_terms_scanned: 0,
                product_atoms: HashMap::new(),
                handle_images: HashMap::new(),
                invariant_targets: HashMap::new(),
                unsigned_divisions: Vec::new(),
                encountered_counted: 0,
                completed_counted_roots: 0,
                s12_roots: 0,
                contract_call_roots: 0,
                delivery_give_roots: HashSet::new(),
                delivery_join_roots: 0,
            },
            output: Output {
                obligations: Vec::new(),
                call_goals: Vec::new(),
                counted_derivations: Vec::new(),
                loop_invariants: Vec::new(),
                source_proofs: Vec::new(),
                joined_source_proofs: Vec::new(),
                postconditions: Vec::new(),
                boolean_decompositions: Vec::new(),
                permission_separations: function
                    .permission_separation_queries
                    .iter()
                    .cloned()
                    .map(|query| PermissionSeparationAttempt {
                        query,
                        attempted: false,
                        discharged: true,
                        derivations: Vec::new(),
                    })
                    .collect(),
                judged_separations: HashSet::new(),
            },
            frames: Frames {
                scopes: Vec::new(),
                loops: Vec::new(),
                gives: Vec::new(),
                product_intervals: HashMap::new(),
                product_operands: HashMap::new(),
            },
        }
    }
}

/// Finalizes the sole function-local derivation ledger after the optimistic
/// FN-9 component batch has been accepted.
pub(super) fn finish(entailment: &mut FunctionEntailment) {
    let event_roots = entailment
        .postconditions
        .iter()
        .flat_map(|proof| &proof.exits)
        .flat_map(|exit| &exit.entry_images)
        .filter_map(|image| image.invalidation)
        .collect::<Vec<_>>();
    let remap = entailment.derivations.finish_with_event_roots(&event_roots);
    if let super::super::model::CheckedBodyDisposition::Uninhabited { contradiction } =
        &mut entailment.body_disposition
    {
        *contradiction = remap
            .nodes
            .get(contradiction.0 as usize)
            .copied()
            .flatten()
            .expect("body-entry contradiction root retained by finish");
    }
    for outcome in &mut entailment.obligations {
        outcome.derivation = outcome
            .derivation
            .and_then(|id| remap.nodes.get(id.0 as usize).copied().flatten());
        outcome.allocation_length_upper_bound_derivation = outcome
            .allocation_length_upper_bound_derivation
            .and_then(|id| remap.nodes.get(id.0 as usize).copied().flatten());
        for partition in &mut outcome.range_partitions {
            for parent in [
                &mut partition.stride_nonnegative,
                &mut partition.base_nonnegative,
            ] {
                *parent = remap
                    .nodes
                    .get(parent.0 as usize)
                    .copied()
                    .flatten()
                    .expect("required range partition proof retained by finish");
            }
        }
    }
    for outcome in &mut entailment.call_goals {
        outcome.derivation = outcome
            .derivation
            .and_then(|id| remap.nodes.get(id.0 as usize).copied().flatten());
    }
    for outcome in &mut entailment.contract_goals {
        outcome.derivation = outcome
            .derivation
            .and_then(|id| remap.nodes.get(id.0 as usize).copied().flatten());
    }
    for proof in &mut entailment.permission_separations {
        for derivation in &mut proof.derivations {
            *derivation = remap
                .nodes
                .get(derivation.0 as usize)
                .copied()
                .flatten()
                .expect("successful PAR-1 range proof retained by its exact query root");
        }
    }
    for counted in &mut entailment.counted_derivations {
        remap_counted_derivations(counted, &remap.nodes);
    }
    for postcondition in &mut entailment.postconditions {
        remap_postcondition(postcondition, &remap.nodes, &remap.events);
    }
}

fn remap_counted_derivations(counted: &mut CountedDerivationSet, remap: &[Option<DerivationId>]) {
    let remap_parent = |parent: &mut DerivationId| {
        *parent = remap
            .get(parent.0 as usize)
            .copied()
            .flatten()
            .expect("counted S11 root parent retained by the sole ledger root channel");
    };
    for parent in [
        &mut counted.lower_capture_eq_endpoint.forward.parent,
        &mut counted.lower_capture_eq_endpoint.reverse.parent,
        &mut counted.upper_capture_eq_endpoint.forward.parent,
        &mut counted.upper_capture_eq_endpoint.reverse.parent,
        &mut counted.binder_eq_lower_capture.forward.parent,
        &mut counted.binder_eq_lower_capture.reverse.parent,
        &mut counted.lower_capture_le_binder.atomic.parent,
        &mut counted.binder_lt_upper_capture.atomic.parent,
    ] {
        remap_parent(parent);
    }
}

fn remap_postcondition(
    proof: &mut FunctionPostconditionProof,
    nodes: &[Option<DerivationId>],
    events: &[Option<FlowEventId>],
) {
    for exit in &mut proof.exits {
        for image in &mut exit.entry_images {
            if let Some(old) = image.invalidation {
                image.invalidation = Some(
                    events
                        .get(old.0 as usize)
                        .copied()
                        .flatten()
                        .expect("required entry-image invalidation event retained by finish"),
                );
            }
        }
        match exit.disposition {
            PostconditionDisposition::Discharged => {
                let old = exit
                    .derivation
                    .expect("every discharged postcondition exit has a required root");
                exit.derivation = Some(
                    nodes
                        .get(old.0 as usize)
                        .copied()
                        .flatten()
                        .expect("required postcondition exit root retained by finish"),
                );
            }
            PostconditionDisposition::Refuted | PostconditionDisposition::Unproved => {
                assert!(exit.derivation.is_none());
            }
        }
    }
    let remap_aggregate = |aggregate: &mut PostconditionAggregate| {
        if aggregate.discharged {
            let old = aggregate
                .derivation
                .expect("every discharged postcondition aggregate has a required root");
            aggregate.derivation = Some(
                nodes
                    .get(old.0 as usize)
                    .copied()
                    .flatten()
                    .expect("required postcondition aggregate root retained by finish"),
            );
        } else {
            assert!(aggregate.derivation.is_none());
        }
    };
    remap_aggregate(&mut proof.aggregate);
}

/// [OWN-7]'s three proof-carrying separations, recorded in the structural
/// flow state; [WIN-2]'s rows bound by `r.len` are never recorded.
///
/// A captured index or endpoint is an immutable mathematical value [OWN-7,
/// REF-1], so the proposition remains true; its proof is nevertheless
/// available only on edges dominated by the judgment. Joins therefore retain
/// only entries every reachable predecessor carries. A pair with no entry
/// answers `false`, which is [OWN-7]'s own default: a pair no admitted family
/// discharges overlaps.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SeparationLedger {
    distinct_indices: std::collections::HashSet<(CaptureId, CaptureId)>,
    disjoint_ranges: std::collections::HashSet<(CapturedRange, CapturedRange)>,
    /// Indices proved outside a range: before its start, at or after its
    /// end, or beside an empty range [OWN-7].
    outside_ranges: std::collections::HashSet<(CaptureId, CapturedRange)>,
}

impl SeparationLedger {
    fn record_indices_distinct(&mut self, left: CapturedValue, right: CapturedValue) {
        self.distinct_indices.insert((left.capture, right.capture));
        self.distinct_indices.insert((right.capture, left.capture));
    }

    fn record_ranges_disjoint(&mut self, left: CapturedRange, right: CapturedRange) {
        self.disjoint_ranges.insert((left, right));
        self.disjoint_ranges.insert((right, left));
    }

    fn record_index_outside_range(&mut self, index: CapturedValue, range: CapturedRange) {
        self.outside_ranges.insert((index.capture, range));
    }

    fn intersection<'a>(mut ledgers: impl Iterator<Item = &'a Self>) -> Self {
        let Some(first) = ledgers.next() else {
            return Self::default();
        };
        let rest = ledgers.collect::<Vec<_>>();
        let mut common = first.clone();
        common.distinct_indices.retain(|pair| {
            rest.iter()
                .all(|ledger| ledger.distinct_indices.contains(pair))
        });
        common.disjoint_ranges.retain(|pair| {
            rest.iter()
                .all(|ledger| ledger.disjoint_ranges.contains(pair))
        });
        common.outside_ranges.retain(|pair| {
            rest.iter()
                .all(|ledger| ledger.outside_ranges.contains(pair))
        });
        common
    }
}

impl SeparationOracle for SeparationLedger {
    fn indices_distinct(&self, left: CapturedValue, right: CapturedValue) -> bool {
        self.distinct_indices
            .contains(&(left.capture, right.capture))
    }

    fn ranges_disjoint(&self, left: CapturedRange, right: CapturedRange) -> bool {
        self.disjoint_ranges.contains(&(left, right))
    }

    /// The ledger carries no liveness: `r.len` is mutable, so a proof of
    /// `i < r.len` holds at the event it was made for and nowhere after it.
    /// An [ENT-5] event judges its kills under [`EventSeparations`], which
    /// adds the liveness that event's own entry state proves [WIN-2].
    fn index_is_live(&self, _window: &ResolvedPlace, _index: CapturedValue) -> bool {
        false
    }

    fn index_outside_range(&self, index: CapturedValue, range: CapturedRange) -> bool {
        self.outside_ranges.contains(&(index.capture, range))
    }

    /// Like liveness, a range's bound by `r.len` holds at the event it was
    /// proved for and is never carried along the edge [WIN-2].
    fn range_within_length(&self, _window: &ResolvedPlace, _range: CapturedRange) -> bool {
        false
    }

    /// The ledger answers at one program point: the actuals of one call
    /// against its writes, or a fact's place against a write at that
    /// write's entry. Both places read that state's `r.len`.
    fn window_length_is_shared(&self, _window: &ResolvedPlace) -> bool {
        true
    }
}

/// The window positions one event's entry state bounds by their window's
/// length [WIN-2], each keyed by the resolved window it reads: the indices
/// it proves live, and the ranges it proves to end at or below `r.len`.
#[derive(Clone, Debug, Default)]
struct LiveBounds {
    indices: HashSet<(ResolvedPlace, CapturedValue)>,
    ranges: HashSet<(ResolvedPlace, CapturedRange)>,
}

/// [ENT-5, WIN-2] the separations one kill event is judged under: the edge's
/// ledger, and the bounds by `r.len` the event's entry state proves.
///
/// A fact's place need not have been formed where the event happens: its
/// index may never have been bounded, as in a place a callee's `ensures`
/// published, and one that was bounded where it was formed is live at a
/// later event only while nothing has moved `r.len` below it. A part write
/// therefore kills every fact below `r[i]` unless the event's entry state
/// derives `i < r.len` [ENT-6], which is what makes the append slot of a
/// `place_back` distinct from `r[i]` rather than possibly `r[i]` itself; a
/// fact below `r[lo..hi]` likewise needs `hi <= r.len` derived there.
struct EventSeparations<'event> {
    ledger: &'event SeparationLedger,
    live: &'event LiveBounds,
}

impl SeparationOracle for EventSeparations<'_> {
    fn indices_distinct(&self, left: CapturedValue, right: CapturedValue) -> bool {
        self.ledger.indices_distinct(left, right)
    }

    fn ranges_disjoint(&self, left: CapturedRange, right: CapturedRange) -> bool {
        self.ledger.ranges_disjoint(left, right)
    }

    fn index_is_live(&self, window: &ResolvedPlace, index: CapturedValue) -> bool {
        self.live.indices.contains(&(window.clone(), index))
    }

    fn index_outside_range(&self, index: CapturedValue, range: CapturedRange) -> bool {
        self.ledger.index_outside_range(index, range)
    }

    fn range_within_length(&self, window: &ResolvedPlace, range: CapturedRange) -> bool {
        self.live.ranges.contains(&(window.clone(), range))
    }

    fn window_length_is_shared(&self, window: &ResolvedPlace) -> bool {
        self.ledger.window_length_is_shared(window)
    }
}

/// One function's analysis, divided along its writers
/// (`design/compiler/engine-components.md`): what it reads and never changes,
/// the vocabulary its judgments are stated in, what it publishes, and the
/// frames of the one walk that owns event order.
struct Analyzer<'check, 'unit> {
    input: Input<'check, 'unit>,
    vocabulary: Vocabulary,
    output: Output,
    frames: Frames,
}

/// What the analysis reads: the program-level context, the checked function,
/// and what is resolved from the function once before the walk.
struct Input<'check, 'unit> {
    context: &'check EntailmentContext<'unit>,
    function: &'check CheckedFunction,
    /// [REF-1] place resolution for this function.
    places: PlaceMap,
    entry_images: Vec<EntryImageRecord>,
    /// Global entry-image indices used by each source-ordered relation. The
    /// flow state tracks invalidation once per structural image, while each
    /// FN-9 proof consults only the images its own relation references.
    postcondition_entry_images: Vec<Vec<usize>>,
}

/// The terms, goals, affine atoms and derivation ledger every judgment is
/// stated in, with the canonical records keyed by them and the ordinals that
/// number the ledger's retained roots.
struct Vocabulary {
    terms: TermTable,
    goals: GoalTable,
    derivations: DerivationLedger,
    /// Function-local mathematical atoms allocated in structural execution
    /// order. They are ordinary checker state and are discarded with the
    /// analysis.
    affine_atoms: Vec<AffineAtom>,
    /// Every measure term registered so far, and how much of the term
    /// registry the scan that found them has covered.
    measure_terms_seen: Vec<TermId>,
    measure_terms_scanned: usize,
    /// What every admitted exact product equals, as value identities: the atom
    /// the multiplication bound, and the two operand atoms it is the product
    /// of.
    ///
    /// An `AffineTermId` names one immutable value, so this map needs no kill
    /// and no join: a write to the product or to an operand mints a new atom,
    /// which is simply absent here, while the old atoms keep denoting the old
    /// values and the recorded equality stays true. [PRF-1] reads it to fold a
    /// term-scaled premise's nonlinear monomials back to affine.
    product_atoms: HashMap<AffineTermId, (AffineTermId, AffineTermId)>,
    /// What each minted opaque handle stands for. An `AffineTermId` is one
    /// immutable value identity, so this needs no kill and no join, exactly as
    /// `product_atoms` does.
    handle_images: HashMap<AffineTermId, AffineForm>,
    /// Canonical immutable target formed at each invariant declaration.
    ///
    /// This table is deliberately separate from flow availability. A named
    /// PRF-1 `use` must form its written certificate from the declaration's
    /// proposition even on a path where that proposition is unavailable;
    /// availability is checked later as an independent premise judgment.
    invariant_targets: HashMap<crate::DeclarationId, Result<AffineInequality, AffineCheckError>>,
    /// Source-establishment order is the specified tie-break for matching
    /// a product against more than one captured division.
    unsigned_divisions: Vec<CapturedUnsignedDivision>,
    encountered_counted: u32,
    completed_counted_roots: u32,
    s12_roots: u32,
    contract_call_roots: u32,
    delivery_give_roots: HashSet<DerivationId>,
    delivery_join_roots: u32,
}

/// What the analysis publishes: every judgment's outcome, the retained
/// derivation sets, and which separation questions have been asked.
struct Output {
    obligations: Vec<ObligationOutcome>,
    call_goals: Vec<CallGoalOutcome>,
    counted_derivations: Vec<CountedDerivationSet>,
    loop_invariants: Vec<LoopInvariantOutcome>,
    source_proofs: Vec<SourceProofOutcome>,
    joined_source_proofs: Vec<JoinedSourceProofProvenance>,
    postconditions: Vec<super::FunctionPostconditionProof>,
    /// O11 candidate decomposition sets, recorded at
    /// signed-goal establishments and never established as facts.
    boolean_decompositions: Vec<super::BooleanGoalDecomposition>,
    /// Optional [PAR-1] range questions. Each one is evaluated only at its
    /// first statement's entry and meets every visit with logical AND.
    permission_separations: Vec<PermissionSeparationAttempt>,
    /// The effect and demanded reference-preservation questions already
    /// judged, distinguished by query even when they share a write event.
    judged_separations: HashSet<usize>,
}

/// The walk's own frames: open scopes, loops and value initializers, and the
/// measurements one judgment hands to the binding the walk reaches next.
struct Frames {
    /// Lexical scope stack: the bindings declared in each open block.
    scopes: Vec<Vec<BindingId>>,
    loops: Vec<LoopFrame>,
    gives: Vec<GiveFrame>,
    /// The interval [ENT-6]'s interval-product rule proved at each admitted
    /// non-constant multiplication, with that domain's derivation, keyed by
    /// that operation's own node. The domain is judged while the initializer
    /// is walked and [ENT-3.S7]'s multiplication row establishes at the
    /// binding the walk then reaches, so the measurement waits here between
    /// the two rather than being proved again.
    product_intervals: HashMap<crate::NodePath, (AffineProductInterval, Option<DerivationId>)>,
    /// Which exact multiplications discharged their [OP-2] domain over affine
    /// operand images, keyed by the operation's own node. Read once at the
    /// binding the walk then reaches, exactly as the interval above is. It
    /// records only that the domain held: which values the fold names is a
    /// separate question the binding answers.
    product_operands: HashMap<crate::NodePath, (DerivationId, u32)>,
}

/// The inputs with the vocabulary: what forming, closing and proving a
/// judgment reads and extends, and nothing a judgment publishes.
struct Reasoning<'a, 'check, 'unit> {
    input: &'a Input<'check, 'unit>,
    vocabulary: &'a mut Vocabulary,
}

/// What recording a judgment needs: the reasoning it is made with and the
/// outputs it is published to, but no walk frame.
struct Judging<'a, 'check, 'unit> {
    input: &'a Input<'check, 'unit>,
    vocabulary: &'a mut Vocabulary,
    output: &'a mut Output,
}

impl<'check, 'unit> Analyzer<'check, 'unit> {
    fn reasoning(&mut self) -> Reasoning<'_, 'check, 'unit> {
        Reasoning {
            input: &self.input,
            vocabulary: &mut self.vocabulary,
        }
    }

    fn judging(&mut self) -> Judging<'_, 'check, 'unit> {
        Judging {
            input: &self.input,
            vocabulary: &mut self.vocabulary,
            output: &mut self.output,
        }
    }
}

impl<'check, 'unit> Judging<'_, 'check, 'unit> {
    fn reasoning(&mut self) -> Reasoning<'_, 'check, 'unit> {
        Reasoning {
            input: self.input,
            vocabulary: self.vocabulary,
        }
    }
}

impl Input<'_, '_> {
    fn collect_bindings(&mut self) {
        self.places = PlaceMap::for_function(self.function);
    }
}

// ------------------------------------------------------------------
// Kill collection from expressions
// ------------------------------------------------------------------

// ------------------------------------------------------------------
// Loop kill summary
// ------------------------------------------------------------------

#[cfg(test)]
mod proof_closure_tests {
    use super::*;

    #[test]
    fn an_unchanged_entering_context_reuses_its_closed_view() {
        let facts = FactState::new();
        let affine = AffineFlowState::default();
        let terms = TermTable::new();
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let closed = ProofClosure::new(&facts, &terms, &goals, &mut ledger);
        let index = Rc::new(AffineL0Index::default());
        closed.affine_index.replace(Some(Rc::clone(&index)));
        let context = ProofContext {
            facts: &facts,
            affine: &affine,
            closed: Some(&closed),
        };
        for _ in 0..3 {
            let view = context.close(&terms, &goals, &mut ledger);
            assert!(Rc::ptr_eq(&view, &closed.state));
            assert!(view.derives_bound(ZERO, ZERO, 0));
            assert!(Rc::ptr_eq(
                &closed.affine_index(&terms, &goals).unwrap(),
                &index
            ));
        }
    }

    #[test]
    fn new_terms_and_changed_standing_measure_bounds_rebuild_the_view() {
        let facts = FactState::new();
        let affine = AffineFlowState::default();
        let mut terms = TermTable::new();
        let goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let closed = ProofClosure::new(&facts, &terms, &goals, &mut ledger);
        closed.affine_index.replace(Some(Rc::default()));
        let term = terms.intern(TermKind::Measure(
            CheckedMeasure::Length,
            ResolvedPlace::spelled(PlaceRoot::Binding(BindingId(0)), false, Vec::new()),
        ));
        let context = ProofContext {
            facts: &facts,
            affine: &affine,
            closed: Some(&closed),
        };
        assert!(closed.affine_index(&terms, &goals).is_none());
        assert!(!Rc::ptr_eq(
            &context.close(&terms, &goals, &mut ledger),
            &closed.state
        ));

        let closed = ProofClosure::new(&facts, &terms, &goals, &mut ledger);
        closed.affine_index.replace(Some(Rc::default()));
        let count = terms.ids().count();
        terms.set_measure_bound(term, MeasureBound::Constant(7));
        assert_eq!(count, terms.ids().count());
        assert!(closed.affine_index(&terms, &goals).is_none());
        let context = ProofContext {
            facts: &facts,
            affine: &affine,
            closed: Some(&closed),
        };
        let view = context.close(&terms, &goals, &mut ledger);
        assert!(!Rc::ptr_eq(&view, &closed.state));
        assert!(view.derives_bound(term, ZERO, 7));
        assert!(view.derives_bound(ZERO, term, -7));
    }

    #[test]
    fn an_existing_goal_receiving_a_projection_invalidates_the_view() {
        let facts = FactState::new();
        let affine = AffineFlowState::default();
        let terms = TermTable::new();
        let mut goals = GoalTable::default();
        let mut ledger = DerivationLedger::default();
        let expression = GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Bool(true)));
        let goal = goals.intern(expression.clone(), None, None, Vec::new());
        let closed = ProofClosure::new(&facts, &terms, &goals, &mut ledger);
        closed.affine_index.replace(Some(Rc::default()));
        let count = goals.ids().count();
        let same = goals.intern(
            expression,
            Some(Relation::Bound {
                left: ZERO,
                right: ZERO,
                bound: 0,
            }),
            None,
            Vec::new(),
        );
        assert_eq!(goal, same);
        assert_eq!(count, goals.ids().count());
        assert!(closed.affine_index(&terms, &goals).is_none());
        let context = ProofContext {
            facts: &facts,
            affine: &affine,
            closed: Some(&closed),
        };
        assert!(!Rc::ptr_eq(
            &context.close(&terms, &goals, &mut ledger),
            &closed.state
        ));
    }
}

#[cfg(test)]
mod indexed_goal_kill_tests {
    use super::*;

    #[test]
    fn signed_indexed_facts_follow_offset_events_and_scope_exits() {
        let constant_ids = HashMap::new();
        let const_parameter_types = HashMap::new();
        let context = EntailmentContext {
            declarations: &[],
            callees: &[],
            constants: &[],
            constant_ids: &constant_ids,
            const_parameter_types: &const_parameter_types,
            nominals: &[],
            elements: &[],
            contract_queries: &[],
            verified_postconditions: &[],
            verified_postcondition_proofs: &[],
            binding_names: &[],
        };
        let function = CheckedFunction {
            formal_hypothesis: false,
            id: crate::semantic::model::FunctionId(0),
            declaration: crate::DeclarationId::from_index(0).unwrap(),
            module: crate::ModuleId::BUNDLE_ROOT,
            name: String::new(),
            symbol: String::new(),
            function_actuals: Vec::new(),
            region_parameters: Vec::new(),
            parameters: Vec::new(),
            result_mode: CheckedMode::Own,
            result: CheckedType::Unit,
            declared_state_writes: Vec::new(),
            requirements: Vec::new(),
            requirement_places: Vec::new(),
            postconditions: Vec::new(),
            body: None,
            reference_origins: Vec::new(),
            body_disposition: Default::default(),
            allocates: false,
            call_separations: Vec::new(),
            permission_separation_queries: Vec::new(),
            obligations: Vec::new(),
            entailment: FunctionEntailment::default(),
        };
        let mut analyzer = Analyzer::new(&context, &function);
        let index = BindingId(1);
        let unrelated = BindingId(2);
        // The offset is below a preceding literal subscript: every index
        // in a selected place contributes support, not only its first one.
        let projections = vec![
            GoalProjection::Subscript(CapturedValue::new(
                CaptureId::source(0),
                CapturedTerm::Literal(0),
            )),
            GoalProjection::Subscript(CapturedValue::new(
                CaptureId::source(1),
                CapturedTerm::Binding(index),
            )),
        ];
        let goal = analyzer.vocabulary.goals.intern(
            GoalExpression::Datum(GoalDatum::Place {
                root: BindingId(0),
                projections: projections.clone(),
                ty: CheckedType::Bool,
            }),
            None,
            None,
            vec![GoalSupport {
                root: BindingId(0),
                projections,
                measure: None,
            }],
        );
        let separations = SeparationLedger::default();
        for sign in [GoalSign::Positive, GoalSign::Negative] {
            for binding in [index, unrelated] {
                let source = crate::NodePath {
                    components: Vec::new(),
                };
                let events = [
                    KillEvent::Write {
                        place: ResolvedPlace::binding(binding),
                        element: false,
                        source: source.clone(),
                    },
                    KillEvent::Consume { binding, source },
                ];
                for event in events {
                    let mut facts = FactState::new();
                    let established = analyzer
                        .vocabulary
                        .derivations
                        .event(FlowEventKind::S1, None);
                    facts.establish_goal(
                        goal,
                        sign,
                        &mut analyzer.vocabulary.derivations,
                        established,
                    );
                    analyzer
                        .reasoning()
                        .apply_kills_one(&separations, &mut facts, &[event]);
                    let closed = close(
                        &facts,
                        &analyzer.vocabulary.terms,
                        &analyzer.vocabulary.goals,
                        &mut analyzer.vocabulary.derivations,
                    );
                    assert_eq!(
                        closed.derives_goal(goal, sign, &analyzer.vocabulary.goals),
                        binding == unrelated,
                        "event must remove exactly the goals that read its binding"
                    );
                }
                let mut facts = FactState::new();
                let established = analyzer
                    .vocabulary
                    .derivations
                    .event(FlowEventKind::S1, None);
                facts.establish_goal(
                    goal,
                    sign,
                    &mut analyzer.vocabulary.derivations,
                    established,
                );
                let exited = HashSet::from([binding]);
                materialize_closure_before_kill(
                    &mut facts,
                    &analyzer.vocabulary.terms,
                    &analyzer.vocabulary.goals,
                    &mut analyzer.vocabulary.derivations,
                );
                facts.kill_goals(|candidate| {
                    analyzer.reasoning().scope_kills_goal(candidate, &exited)
                });
                let closed = close(
                    &facts,
                    &analyzer.vocabulary.terms,
                    &analyzer.vocabulary.goals,
                    &mut analyzer.vocabulary.derivations,
                );
                assert_eq!(
                    closed.derives_goal(goal, sign, &analyzer.vocabulary.goals),
                    binding == unrelated,
                    "scope exit must remove exactly the goals that read its binding"
                );
            }
        }
    }
}

#[cfg(test)]
mod range_argument_kill_tests {
    use super::*;
    use crate::semantic::model::{CheckedElement, CheckedRangeRoot};

    const ORIGIN: BindingId = BindingId(0);
    const VIEW: BindingId = BindingId(1);

    fn literal(occurrence: u32, value: u64) -> CapturedValue {
        CapturedValue::new(CaptureId::source(occurrence), CapturedTerm::Literal(value))
    }

    fn range(occurrence: u32, start: u64, end: u64) -> CapturedRange {
        CapturedRange {
            start: literal(occurrence, start),
            end: literal(occurrence + 1, end),
        }
    }

    fn place(path: Vec<PlaceStep>) -> ResolvedPlace {
        ResolvedPlace {
            root: PlaceRoot::Binding(ORIGIN),
            path,
        }
    }

    fn endpoint(value: u64) -> Box<CheckedExpression> {
        Box::new(CheckedExpression::Constant(CheckedValue::Integer {
            ty: IntegerType::U64,
            bits: value,
        }))
    }

    /// `&origin[lo..hi]` or `&deref(view)[lo..hi]` formed at a call argument.
    fn formation(
        source: CheckedRangeSource,
        captured: CapturedRange,
        start: u64,
        end: u64,
    ) -> CheckedExpression {
        CheckedExpression::RangeOf {
            carrier: crate::NodePath {
                components: vec![0],
            },
            source,
            element: CheckedElement(0),
            element_type: CheckedType::Integer(IntegerType::U64),
            start: endpoint(start),
            end: endpoint(end),
            obligation: crate::NodePath {
                components: vec![1],
            },
            captured,
        }
    }

    fn storage_source() -> CheckedRangeSource {
        CheckedRangeSource::Storage(CheckedContainerRoot {
            root: PlaceRoot::Binding(ORIGIN),
            path: Vec::new(),
            ty: CheckedType::Integer(IntegerType::U64),
        })
    }

    fn view_source() -> CheckedRangeSource {
        CheckedRangeSource::Range(CheckedRangeRoot {
            binding: VIEW,
            element: CheckedElement(0),
            element_type: CheckedType::Integer(IntegerType::U64),
        })
    }

    /// Runs `check` on the analyzer of one bodiless function whose binding i
    /// names the places `reference_origins[i]` lists [REF-1].
    fn with_analyzer<R>(
        reference_origins: Vec<Vec<ResolvedPlace>>,
        check: impl FnOnce(&mut Analyzer<'_, '_>) -> R,
    ) -> R {
        let constant_ids = HashMap::new();
        let const_parameter_types = HashMap::new();
        let context = EntailmentContext {
            declarations: &[],
            callees: &[],
            constants: &[],
            constant_ids: &constant_ids,
            const_parameter_types: &const_parameter_types,
            nominals: &[],
            elements: &[],
            contract_queries: &[],
            verified_postconditions: &[],
            verified_postcondition_proofs: &[],
            binding_names: &[],
        };
        let function = CheckedFunction {
            formal_hypothesis: false,
            id: crate::semantic::model::FunctionId(0),
            declaration: crate::DeclarationId::from_index(0).unwrap(),
            module: crate::ModuleId::BUNDLE_ROOT,
            name: String::new(),
            symbol: String::new(),
            function_actuals: Vec::new(),
            region_parameters: Vec::new(),
            parameters: Vec::new(),
            result_mode: CheckedMode::Own,
            result: CheckedType::Unit,
            declared_state_writes: Vec::new(),
            requirements: Vec::new(),
            requirement_places: Vec::new(),
            postconditions: Vec::new(),
            body: None,
            reference_origins,
            body_disposition: Default::default(),
            allocates: false,
            call_separations: Vec::new(),
            permission_separation_queries: Vec::new(),
            obligations: Vec::new(),
            entailment: FunctionEntailment::default(),
        };
        let mut analyzer = Analyzer::new(&context, &function);
        analyzer.input.places = PlaceMap::for_function(&function);
        check(&mut analyzer)
    }

    /// [OWN-7, REF-4] a range formation's endpoint images are filed only
    /// under a source occurrence, which names that one formation. Filed under
    /// the capture that names no single evaluation, one formation's images
    /// would answer for every range carrying that capture, so such a
    /// formation files nothing and keeps its own formation's images apart.
    #[test]
    fn a_range_image_is_filed_only_under_its_own_formation() {
        with_analyzer(Vec::new(), |analyzer| {
            let carrier = crate::NodePath {
                components: vec![0],
            };
            let mut affine = AffineFlowState::default();
            let shared = CapturedRange {
                start: CapturedValue::unknown(),
                end: CapturedValue::unknown(),
            };
            analyzer.reasoning().file_range_image(
                &carrier,
                shared,
                &endpoint(4),
                &endpoint(4),
                &mut affine,
            );
            assert!(
                affine.ranges.is_empty(),
                "a capture shared by formations files no image: {:?}",
                affine.ranges
            );
            let own = range(20, 1, 3);
            analyzer.reasoning().file_range_image(
                &carrier,
                own,
                &endpoint(1),
                &endpoint(3),
                &mut affine,
            );
            let image = affine
                .ranges
                .get(&own.start.capture)
                .expect("a formation files its images under its own start occurrence");
            assert_eq!(
                (&image.start, &image.end),
                (&AffineForm::constant(1), &AffineForm::constant(3))
            );
            assert_eq!(affine.ranges.len(), 1);
        });
    }

    /// [EFF-5, REF-4, CALL-3] a range formed at a call names the path a bound
    /// range reference would name, so its projected write kills a measure of
    /// an element that range may contain and keeps the origin's own measure
    /// and the range's own measure. Before this path existed, an inline
    /// actual named no referent and its write killed nothing.
    #[test]
    fn an_inline_range_actual_names_its_formation_path_for_kills() {
        let outer = range(10, 0, 3);
        // `view` names `origin[0..3]`.
        let origins = vec![Vec::new(), vec![place(vec![PlaceStep::Range(outer)])]];
        with_analyzer(origins, |analyzer| {
            an_inline_range_actual_kills(analyzer, outer)
        });
    }

    fn an_inline_range_actual_kills(analyzer: &mut Analyzer<'_, '_>, outer: CapturedRange) {
        let inner = range(20, 1, 3);
        let direct = formation(storage_source(), inner, 1, 3);
        assert_eq!(
            analyzer.input.argument_referents(&direct),
            vec![(place(vec![PlaceStep::Range(inner)]), false)]
        );
        let reslice = formation(view_source(), inner, 1, 3);
        assert_eq!(
            analyzer.input.argument_referents(&reslice),
            vec![(
                place(vec![PlaceStep::Range(outer), PlaceStep::Range(inner)]),
                false
            )]
        );

        let call = crate::NodePath {
            components: vec![2],
        };
        let separations = SeparationLedger::default();
        for (argument, prefix) in [
            (&direct, Vec::new()),
            (&reslice, vec![PlaceStep::Range(outer)]),
        ] {
            let mut events = Vec::new();
            analyzer
                .input
                .collect_view_write_kills(argument, &call, &mut events);
            let [event] = events.as_slice() else {
                panic!("one written range names one event: {events:?}");
            };
            let mut selected = prefix.clone();
            selected.push(PlaceStep::Index(literal(30, 1)));
            let mut own = prefix;
            own.push(PlaceStep::Range(inner));
            let [selected, origin, own] = [selected, Vec::new(), own].map(|path| {
                analyzer
                    .vocabulary
                    .terms
                    .intern(TermKind::Measure(CheckedMeasure::Length, place(path)))
            });
            assert!(
                analyzer
                    .reasoning()
                    .event_kills_term(&separations, selected, event),
                "the write may replace the selected element, so its measure dies"
            );
            assert!(
                !analyzer
                    .reasoning()
                    .event_kills_term(&separations, origin, event),
                "an element write reaches no measure of the origin place [CALL-3]"
            );
            assert!(
                !analyzer
                    .reasoning()
                    .event_kills_term(&separations, own, event),
                "an element write reaches no measure of the range reference [CALL-3]"
            );
        }
    }
}

#[cfg(test)]
mod goal_origin_kill_tests {
    use super::super::state::{FactState, GoalId};
    use super::invalidate_goal_origin_for_set;
    use crate::semantic::model::{BindingId, CheckedSetTarget, CheckedType, CheckedWritablePlace};

    #[test]
    fn a_projected_set_invalidates_the_aggregate_ordinary_let_origin() {
        let binding = BindingId(0);
        let mut state = FactState::default();
        state.goal_origins.insert(binding, GoalId(0));
        let target = CheckedSetTarget::Place(CheckedWritablePlace {
            binding,
            fields: vec![1],
            mode: crate::semantic::model::CheckedMode::Own,
            ty: CheckedType::Bool,
            declares: false,
        });

        invalidate_goal_origin_for_set(&mut state, &target);

        assert!(!state.goal_origins.contains_key(&binding));
    }
}

#[cfg(test)]
mod affine_pair_tests {
    use super::{
        AffineCheckState, AffineInequality, AffineTermId, AutomaticAffinePremise,
        first_two_premise_candidate, interval_proves,
    };

    fn inequality(terms: &[(u32, i128)], upper: i128) -> AffineInequality {
        let terms = terms
            .iter()
            .map(|&(term, coefficient)| (AffineTermId::from_index(term), coefficient))
            .collect::<Vec<_>>();
        AffineInequality::from_terms(&terms, upper, &mut AffineCheckState::new())
            .expect("test inequality is representable")
    }

    fn premise(inequality: AffineInequality) -> AutomaticAffinePremise {
        AutomaticAffinePremise {
            inequality,
            source: None,
            parent: None,
        }
    }

    fn interval_closes_without_atoms(
        residual: &AffineInequality,
        check: &mut AffineCheckState,
    ) -> Option<()> {
        interval_proves(residual, |_| None, check)
            .ok()
            .filter(|proved| *proved)
            .map(|_| ())
    }

    /// The pair enumeration hands each accumulated sum to its caller, so these
    /// tests take the residual against the target exactly where the semantic
    /// checker does.
    fn interval_closes_after(
        target: &AffineInequality,
    ) -> impl FnMut(&AffineInequality, &mut AffineCheckState) -> Option<()> {
        move |sum, check| {
            let residual = AffineInequality::residual_after(target, sum, check).ok()?;
            interval_closes_without_atoms(&residual, check)
        }
    }

    #[test]
    fn two_premise_enumeration_includes_one_fact_used_twice() {
        let target = inequality(&[(0, 2)], 0);
        let premises = [premise(inequality(&[(0, 1)], 0))];
        let selected = first_two_premise_candidate(
            &premises,
            &mut AffineCheckState::new(),
            interval_closes_after(&target),
        );
        assert!(matches!(selected, Some((0, 0, ()))));
    }

    #[test]
    fn two_independent_facts_close_while_three_remain_outside_the_pair_rule() {
        let premises = [
            premise(inequality(&[(0, 1)], 0)),
            premise(inequality(&[(1, 1)], 0)),
            premise(inequality(&[(2, 1)], 0)),
        ];
        let two = first_two_premise_candidate(
            &premises,
            &mut AffineCheckState::new(),
            interval_closes_after(&inequality(&[(0, 1), (1, 1)], 0)),
        );
        assert!(two.is_some());

        let three = first_two_premise_candidate(
            &premises,
            &mut AffineCheckState::new(),
            interval_closes_after(&inequality(&[(0, 1), (1, 1), (2, 1)], 0)),
        );
        assert!(three.is_none());
    }

    #[test]
    fn premise_and_term_order_do_not_change_pair_acceptance() {
        let forward = [
            premise(inequality(&[(0, 1)], 1)),
            premise(inequality(&[(0, 2), (1, -2)], 0)),
            premise(inequality(&[(0, -1), (1, 2)], 0)),
        ];
        let reverse = [
            premise(inequality(&[(0, 2), (1, -1)], 0)),
            premise(inequality(&[(0, -2), (1, 2)], 0)),
            premise(inequality(&[(1, 1)], 1)),
        ];
        let forward_result = first_two_premise_candidate(
            &forward,
            &mut AffineCheckState::new(),
            interval_closes_after(&inequality(&[(0, 1)], 0)),
        );
        let reverse_result = first_two_premise_candidate(
            &reverse,
            &mut AffineCheckState::new(),
            interval_closes_after(&inequality(&[(1, 1)], 0)),
        );
        assert!(forward_result.is_some());
        assert!(reverse_result.is_some());
    }

    #[test]
    fn one_unrepresentable_pair_does_not_hide_a_later_pair() {
        let premises = [
            premise(inequality(&[(0, i128::MAX)], 0)),
            premise(inequality(&[(0, 2), (1, -2)], 0)),
            premise(inequality(&[(0, -1), (1, 2)], 0)),
        ];
        let selected = first_two_premise_candidate(
            &premises,
            &mut AffineCheckState::new(),
            interval_closes_after(&inequality(&[(0, 1)], 0)),
        );
        assert!(matches!(selected, Some((1, 2, ()))));
    }
}

/// The [MSR-1] measured type of one checked type, if the measure table gives
/// it a row.
/// The written constant one measured type carries, when a cell of its
/// [MSR-1] row is that constant [MSR-2].
const fn type_constant(ty: CheckedType) -> Option<CheckedConst> {
    match ty {
        CheckedType::Array { length, .. } => Some(length),
        CheckedType::Window { capacity, .. } => capacity,
        _ => None,
    }
}

const fn measured_kind(ty: CheckedType) -> Option<MeasuredKind> {
    ty.measured()
}
