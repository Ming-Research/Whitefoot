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
//! The [ENT-3] fact sources themselves — which checked shape establishes
//! which relation — live in [`sources`]; this module owns the graph, the
//! kills, the joins, and the obligation judgment, and calls into the sources
//! at each establishment point.

mod conversions;
mod operation_facts;
mod results;
mod sources;

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
    CheckedContainerRoot, CheckedConversionMode, CheckedEnumType, CheckedExpression,
    CheckedFloatOperation, CheckedFunction, CheckedIntegerOperation, CheckedLoopId,
    CheckedLoopInvariant, CheckedMatchArm, CheckedMeasure, CheckedMode, CheckedNominalKind,
    CheckedNumericType, CheckedPlaceStep, CheckedProofMultiplicity, CheckedProofUseSource,
    CheckedRangeSource, CheckedSetTarget, CheckedStatement, CheckedType, CheckedValue, FloatType,
    IntegerType, MeasureCell, MeasuredKind, SubscriptedTerm,
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

/// Adds `events` to a path's continuing record, each once.
fn record_continuing(continuing: &mut Vec<KillEvent>, events: &[KillEvent]) {
    for event in events {
        if !continuing.contains(event) {
            continuing.push(event.clone());
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

/// Complete route selected by one [`Analyzer::prove`] call.  The signed
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

/// Exhausts the unordered coefficient-one premise pairs, including `(p, p)`.
///
/// The callback receives each accumulated pair sum and owns the residual
/// against the target. Candidate arithmetic is isolated: an unrepresentable
/// pair is skipped and cannot hide a later representable witness. Returning
/// after a successful callback is acceptance-order independent because every
/// earlier candidate has already failed and adding another premise cannot
/// remove an existing pair from this source-shaped finite set.
fn first_two_premise_candidate<T>(
    premises: &[AutomaticAffinePremise],
    check: &mut AffineCheckState,
    mut prove: impl FnMut(&AffineInequality, &mut AffineCheckState) -> Option<T>,
) -> Option<(usize, usize, T)> {
    for first in 0..premises.len() {
        for second in first..premises.len() {
            let pair = [
                premises[first].inequality.clone(),
                premises[second].inequality.clone(),
            ];
            let Ok(sum) = sum_explicit_inequalities(&pair, check) else {
                continue;
            };
            if let Some(proof) = prove(&sum, check) {
                return Some((first, second, proof));
            }
        }
    }
    None
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
    /// The indices the call's entry state proves live [WIN-2]: every part a
    /// row names is interpreted at call entry, so each of `kills` is judged
    /// against this one set.
    live: LiveIndices,
}

impl PreparedCall {
    /// The separations each of `kills` is judged under: the edge's ledger
    /// and the liveness the call's entry state proved [WIN-2, ENT-5].
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
    analyzer.collect_bindings();
    let mut state = ProofFlowState::default();
    analyzer.initialize_affine_parameters(&mut state.affine);
    for (ordinal, requirement) in function.requirements.iter().enumerate() {
        let event = analyzer.proof_event(FlowEventKind::S4, Some(&requirement.clause));
        analyzer.establish_requires_facts(requirement, &mut state.facts, event);
        analyzer.establish_requirement_affine_images(requirement, ordinal, &mut state);
    }
    let Some(goal) = analyzer.body_goal_expression(goal) else {
        return FunctionEntailment::default();
    };
    let goal = ConcreteGoal::new(goal);
    let (disposition, evidence, derivation) =
        analyzer.call_goal_disposition(&goal, ProofContext::new(&state.facts, &state.affine));
    if let Some(root) = derivation {
        analyzer
            .derivations
            .add_root(DerivationRootKind::ContractGoal(0), root);
    }
    let (terms, measure_bounds) = analyzer.terms.into_inventory();
    FunctionEntailment {
        contract_goals: vec![ContractGoalOutcome {
            goal,
            disposition,
            evidence,
            derivation,
        }],
        derivations: analyzer.derivations,
        inventory: DerivationInventory {
            terms,
            measure_bounds,
            goals: analyzer.goals.into_inventory(),
        },
        ..FunctionEntailment::default()
    }
}

fn analyze_candidate_inner(
    function: &CheckedFunction,
    context: &EntailmentContext<'_>,
) -> FunctionEntailment {
    let run = run(function, context);
    FunctionEntailment {
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
        derivations: run.derivations,
        inventory: run.inventory,
    }
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
    analyzer.collect_bindings();
    analyzer.collect_postcondition_entry_images();
    let mut state = ProofFlowState {
        entry_images: vec![None; analyzer.entry_images.len()],
        ..ProofFlowState::default()
    };
    analyzer.initialize_affine_parameters(&mut state.affine);
    // [MSR-3] the entry placement, before every other source: one immutable
    // datum per measure of a parameter any declared relation names, equal to
    // that measure at body entry.
    analyzer.establish_entry_datums(&mut state);
    analyzer
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
        let event = analyzer.proof_event(FlowEventKind::S4, Some(&requirement.clause));
        analyzer.establish_requires_facts(requirement, &mut state.facts, event);
        analyzer.establish_requirement_affine_images(requirement, ordinal, &mut state);
    }
    let body_disposition = {
        let closed = close(
            &state.facts,
            &analyzer.terms,
            &analyzer.goals,
            &mut analyzer.derivations,
        );
        match closed.contradiction_proof() {
            Some(contradiction) => {
                analyzer
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
        analyzer.initialize_postcondition_proofs();
    }
    if let Some(body) = &function.body {
        analyzer.walk_block(body, &mut state);
    }
    analyzer.reject_unjudged_separations();
    analyzer.scopes.pop();
    analyzer.finalize_postcondition_aggregates();
    let permission_separations = analyzer.finalize_permission_separations();
    assert_eq!(
        analyzer.completed_counted_roots, analyzer.encountered_counted,
        "every encountered counted statement must publish one complete S11 root group"
    );
    let (terms, measure_bounds) = analyzer.terms.into_inventory();
    let inventory = DerivationInventory {
        terms,
        measure_bounds,
        goals: analyzer.goals.into_inventory(),
    };
    AnalysisRun {
        body_disposition,
        obligations: analyzer.obligations,
        call_goals: analyzer.call_goals,
        counted_derivations: analyzer.counted_derivations,
        loop_invariants: analyzer.loop_invariants,
        source_proofs: analyzer.source_proofs,
        joined_source_proofs: analyzer.joined_source_proofs,
        postconditions: analyzer.postconditions,
        boolean_decompositions: analyzer.boolean_decompositions,
        permission_separations,
        derivations: analyzer.derivations,
        inventory,
    }
}

impl<'check, 'unit> Analyzer<'check, 'unit> {
    fn new(context: &'check EntailmentContext<'unit>, function: &'check CheckedFunction) -> Self {
        Self {
            context,
            function,
            places: PlaceMap::default(),
            judged_separations: HashSet::new(),
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
            terms: TermTable::new(),
            goals: GoalTable::default(),
            derivations: DerivationLedger::default(),
            obligations: Vec::new(),
            product_intervals: HashMap::new(),
            product_operands: HashMap::new(),
            product_atoms: HashMap::new(),
            unsigned_divisions: Vec::new(),
            handle_images: HashMap::new(),
            call_goals: Vec::new(),
            counted_derivations: Vec::new(),
            loop_invariants: Vec::new(),
            source_proofs: Vec::new(),
            joined_source_proofs: Vec::new(),
            invariant_targets: HashMap::new(),
            postconditions: Vec::new(),
            boolean_decompositions: Vec::new(),
            entry_images: Vec::new(),
            postcondition_entry_images: Vec::new(),
            affine_atoms: Vec::new(),
            measure_terms_seen: Vec::new(),
            measure_terms_scanned: 0,
            encountered_counted: 0,
            completed_counted_roots: 0,
            s12_roots: 0,
            contract_call_roots: 0,
            delivery_give_roots: HashSet::new(),
            delivery_join_roots: 0,
            scopes: Vec::new(),
            loops: Vec::new(),
            gives: Vec::new(),
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

/// The [ENT-2] goal projection one resolved-path step spells, where that
/// language has one.
///
/// A goal datum's place is a tracked place [ENT-2] clause (a) or a measure
/// place clause (b): field selections, `deref` wrappings, subscripts, and the
/// one range step the image of an anonymous `&[T]` actual carries [REF-4]. A
/// payload step preserves the selected variant and field. A window part is
/// effect vocabulary, not a value projection. Support may conservatively
/// widen to a prefix, but a value identity must never be shortened that way.
fn goal_projection_of_step(step: &PlaceStep) -> Option<GoalProjection> {
    match step {
        PlaceStep::Deref => Some(GoalProjection::Deref),
        PlaceStep::Field(field) => Some(GoalProjection::Field(*field)),
        PlaceStep::Payload { variant, field } => Some(GoalProjection::Payload {
            variant: *variant,
            field: *field,
        }),
        PlaceStep::Index(offset) => Some(GoalProjection::Subscript(offset.goal_identity())),
        // [REF-4] a range step's endpoint captures identify the formation
        // whose immutable affine image gives the anonymous range its length.
        // Canonicalizing those captures by endpoint spelling would merge two
        // formations that read the same binding at different times.
        PlaceStep::Range(range) => Some(GoalProjection::Range(*range)),
        PlaceStep::Part(_) | PlaceStep::Measure(_) | PlaceStep::Descendant(_) => None,
    }
}

/// [OWN-7]'s two proof-carrying separations and [WIN-2]'s one conditional
/// row, recorded in the structural flow state.
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
    not_last: std::collections::HashSet<(ResolvedPlace, CaptureId)>,
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
        common
            .not_last
            .retain(|pair| rest.iter().all(|ledger| ledger.not_last.contains(pair)));
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

    fn index_is_not_last(&self, window: &ResolvedPlace, index: CapturedValue) -> bool {
        self.not_last.contains(&(window.clone(), index.capture))
    }

    /// The ledger answers at one program point: the actuals of one call
    /// against its writes, or a fact's place against a write at that
    /// write's entry. Both places read that state's `r.len`.
    fn window_length_is_shared(&self, _window: &ResolvedPlace) -> bool {
        true
    }
}

/// The window indices one event's entry state proves live [WIN-2], each
/// keyed by the resolved window it indexes.
type LiveIndices = HashSet<(ResolvedPlace, CapturedValue)>;

/// [ENT-5, WIN-2] the separations one kill event is judged under: the edge's
/// ledger, and the indices the event's entry state proves live.
///
/// A fact's place need not have been formed where the event happens: its
/// index may never have been bounded, as in a place a callee's `ensures`
/// published, and one that was bounded where it was formed is live at a
/// later event only while nothing has moved `r.len` below it. A part write
/// therefore kills every fact below `r[i]` unless the event's entry state
/// derives `i < r.len` [ENT-6], which is what makes the append slot of a
/// `place_back` distinct from `r[i]` rather than possibly `r[i]` itself.
struct EventSeparations<'event> {
    ledger: &'event SeparationLedger,
    live: &'event LiveIndices,
}

impl SeparationOracle for EventSeparations<'_> {
    fn indices_distinct(&self, left: CapturedValue, right: CapturedValue) -> bool {
        self.ledger.indices_distinct(left, right)
    }

    fn ranges_disjoint(&self, left: CapturedRange, right: CapturedRange) -> bool {
        self.ledger.ranges_disjoint(left, right)
    }

    fn index_is_live(&self, window: &ResolvedPlace, index: CapturedValue) -> bool {
        self.live.contains(&(window.clone(), index))
    }

    fn index_is_not_last(&self, window: &ResolvedPlace, index: CapturedValue) -> bool {
        self.ledger.index_is_not_last(window, index)
    }

    fn window_length_is_shared(&self, window: &ResolvedPlace) -> bool {
        self.ledger.window_length_is_shared(window)
    }
}

struct Analyzer<'check, 'unit> {
    context: &'check EntailmentContext<'unit>,
    function: &'check CheckedFunction,
    /// [REF-1] place resolution for this function.
    places: PlaceMap,
    /// The effect and demanded reference-preservation questions already
    /// judged, distinguished by query even when they share a write event.
    judged_separations: HashSet<usize>,
    /// Optional [PAR-1] range questions. Each one is evaluated only at its
    /// first statement's entry and meets every visit with logical AND.
    permission_separations: Vec<PermissionSeparationAttempt>,
    terms: TermTable,
    goals: GoalTable,
    derivations: DerivationLedger,
    obligations: Vec<ObligationOutcome>,
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
    /// Source-establishment order is the specified tie-break for matching
    /// a product against more than one captured division.
    unsigned_divisions: Vec<CapturedUnsignedDivision>,
    /// What each minted opaque handle stands for. An `AffineTermId` is one
    /// immutable value identity, so this needs no kill and no join, exactly as
    /// `product_atoms` does.
    handle_images: HashMap<AffineTermId, AffineForm>,
    call_goals: Vec<CallGoalOutcome>,
    counted_derivations: Vec<CountedDerivationSet>,
    loop_invariants: Vec<LoopInvariantOutcome>,
    source_proofs: Vec<SourceProofOutcome>,
    joined_source_proofs: Vec<JoinedSourceProofProvenance>,
    /// Canonical immutable target formed at each invariant declaration.
    ///
    /// This table is deliberately separate from flow availability. A named
    /// PRF-1 `use` must form its written certificate from the declaration's
    /// proposition even on a path where that proposition is unavailable;
    /// availability is checked later as an independent premise judgment.
    invariant_targets: HashMap<crate::DeclarationId, Result<AffineInequality, AffineCheckError>>,
    postconditions: Vec<super::FunctionPostconditionProof>,
    /// O11 candidate decomposition sets, recorded at
    /// signed-goal establishments and never established as facts.
    boolean_decompositions: Vec<super::BooleanGoalDecomposition>,
    entry_images: Vec<EntryImageRecord>,
    /// Global entry-image indices used by each source-ordered relation. The
    /// flow state tracks invalidation once per structural image, while each
    /// FN-9 proof consults only the images its own relation references.
    postcondition_entry_images: Vec<Vec<usize>>,
    /// Function-local mathematical atoms allocated in structural execution
    /// order. They are ordinary checker state and are discarded with the
    /// analysis.
    affine_atoms: Vec<AffineAtom>,
    /// Every measure term registered so far, and how much of the term
    /// registry the scan that found them has covered.
    measure_terms_seen: Vec<TermId>,
    measure_terms_scanned: usize,
    encountered_counted: u32,
    completed_counted_roots: u32,
    s12_roots: u32,
    contract_call_roots: u32,
    delivery_give_roots: HashSet<DerivationId>,
    delivery_join_roots: u32,
    /// Lexical scope stack: the bindings declared in each open block.
    scopes: Vec<Vec<BindingId>>,
    loops: Vec<LoopFrame>,
    gives: Vec<GiveFrame>,
}

impl Analyzer<'_, '_> {
    fn initialize_affine_parameters(&mut self, state: &mut AffineFlowState) {
        let parameters = self
            .function
            .parameters
            .iter()
            .filter_map(|parameter| match (parameter.mode, parameter.ty) {
                (CheckedMode::Own, CheckedType::Integer(ty)) => Some((parameter.binding, ty)),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (binding, ty) in parameters {
            let value = self.new_affine_atom(ty);
            state.values.insert(binding, value);
        }
    }

    fn initialize_postcondition_proofs(&mut self) {
        let aggregate = || PostconditionAggregate {
            discharged: false,
            derivation: None,
        };
        self.postconditions = self
            .function
            .postconditions
            .iter()
            .enumerate()
            .map(|(ordinal, postcondition)| FunctionPostconditionProof {
                block: postcondition.selector.block.clone(),
                selector: postcondition.selector.selector.clone(),
                relation_ordinal: u32::try_from(ordinal)
                    .expect("postcondition relation ordinal exceeds u32"),
                summary: None,
                exits: Vec::new(),
                aggregate: aggregate(),
            })
            .collect();
    }

    fn finalize_postcondition_aggregates(&mut self) {
        for index in 0..self.postconditions.len() {
            let block = self.postconditions[index].block.clone();
            let relation_ordinal = self.postconditions[index].relation_ordinal;
            let parents = self.postconditions[index]
                .exits
                .iter()
                .map(|exit| {
                    (exit.disposition == PostconditionDisposition::Discharged)
                        .then_some(exit.derivation)
                        .flatten()
                })
                .collect::<Option<Vec<_>>>();
            self.postconditions[index].aggregate =
                self.retain_postcondition_aggregate(&block, relation_ordinal, parents);
        }
    }

    fn retain_postcondition_aggregate(
        &mut self,
        block: &crate::NodePath,
        relation_ordinal: u32,
        parents: Option<Vec<DerivationId>>,
    ) -> PostconditionAggregate {
        if self.function.formal_hypothesis || self.function.body.is_none() {
            let node = self
                .derivations
                .intern(super::state::DerivationNode::SignatureContract {
                    block: block.clone(),
                    relation_ordinal,
                });
            self.derivations.add_root(
                DerivationRootKind::PostconditionAggregate { relation_ordinal },
                node,
            );
            return PostconditionAggregate {
                discharged: true,
                derivation: Some(node),
            };
        }
        let Some(parents) = parents.filter(|parents| !parents.is_empty()) else {
            return PostconditionAggregate {
                discharged: false,
                derivation: None,
            };
        };
        let node = self
            .derivations
            .intern(super::state::DerivationNode::PostconditionAggregate {
                block: block.clone(),
                relation_ordinal,
                parents,
            });
        self.derivations.add_root(
            DerivationRootKind::PostconditionAggregate { relation_ordinal },
            node,
        );
        PostconditionAggregate {
            discharged: true,
            derivation: Some(node),
        }
    }

    fn judge_postcondition_return(
        &mut self,
        statement: &crate::NodePath,
        states: &ProofFlowState,
        affine_result: Option<&AffineForm>,
        value_reached: bool,
        forwarded: &[Option<ResultEvidence>],
    ) {
        if self.postconditions.is_empty() {
            return;
        }
        for index in 0..self.function.postconditions.len() {
            let postcondition = &self.function.postconditions[index];
            let Some(selected) = postcondition
                .selected_returns
                .iter()
                .find(|selected| selected.statement == *statement)
                .cloned()
            else {
                continue;
            };
            let forwarded = forwarded
                .get(postcondition.selector.ordinal as usize)
                .and_then(Option::as_ref);
            let conditional = selected
                .values
                .iter()
                .any(|value| matches!(value, Some(PostconditionReturnDatum::ResultPayload { .. })));
            if conditional && forwarded.is_some_and(|result| result.definitely_err) {
                // An outcome with no possible success supplies no selected
                // success return, including after copies and value joins.
                continue;
            }
            let mut conditional_facts = states.facts.clone();
            if conditional && let Some(result) = forwarded {
                if result.facts.all_derivable {
                    conditional_facts.promote_to_contradiction(result.facts.contradiction);
                }
                for (relation, parent) in result.facts.l0_candidates() {
                    conditional_facts.establish_from_proof(&relation, parent, &self.derivations);
                }
            }
            // [CALL-4] one term per declared result ordinal, in written order.
            let results = selected
                .values
                .iter()
                .map(|value| {
                    value
                        .as_ref()
                        .and_then(|value| self.postcondition_return_term(value))
                })
                .collect::<Vec<_>>();
            // [CALL-4] an ordinal whose destination is no [ENT-2] place
            // makes only the relations naming it unavailable, which is what a
            // measured result's value term is: the clause names its measure
            // and never the value.
            let Some(relation) =
                self.instantiate_postcondition_relation(postcondition, &results, &selected.values)
            else {
                continue;
            };
            // [MSR-4] the affine route over the relation's own instantiated
            // terms, which is what carries a measure operand: a measure has
            // an affine atom [MSR-4] and no result value image, so the datum
            // route below reaches it nowhere. The datum route stays for a
            // fragment result whose returned expression has a richer image
            // than its place.
            let affine_target = self
                .affine_relation_target(&relation, &states.affine)
                .or_else(|| {
                    affine_result.and_then(|result| {
                        self.postcondition_affine_target(postcondition, result, &states.affine)
                    })
                });
            let residual = self.render_relation(&relation);

            let entry_images = self.postcondition_entry_images[index]
                .iter()
                .map(|entry_index| PostconditionEntryImageOutcome {
                    datum: self.entry_images[*entry_index].datum.clone(),
                    invalidation: states.entry_images[*entry_index],
                })
                .collect::<Vec<_>>();
            let occurrence = self.postconditions[index].exits.len();
            let relation_ordinal = self.postconditions[index].relation_ordinal;
            let unavailable = !value_reached
                || entry_images
                    .iter()
                    .any(|image| image.invalidation.is_some());
            let complete = self.judge_postcondition(
                relation_ordinal,
                occurrence,
                statement,
                &relation,
                &conditional_facts,
                affine_target.as_deref(),
                &states.affine,
                unavailable,
            );
            self.postconditions[index].exits.push(PostconditionExit {
                statement: statement.clone(),
                relation,
                residual,
                entry_images,
                disposition: complete.disposition,
                derivation: complete.derivation,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn judge_postcondition(
        &mut self,
        relation_ordinal: u32,
        occurrence: usize,
        statement: &crate::NodePath,
        relation: &Relation,
        state: &FactState,
        affine_target: Option<&[AffineInequality]>,
        affine_state: &AffineFlowState,
        unavailable: bool,
    ) -> PostconditionExitProof {
        let context = ProofContext::new(state, affine_state);
        if unavailable {
            return PostconditionExitProof {
                disposition: PostconditionDisposition::Unproved,
                derivation: None,
            };
        }
        let proof = self.prove(
            context,
            ProofGoal::Ordering {
                relation,
                affine: affine_target,
            },
        );
        if proof.disposition == ProofDisposition::Proved {
            let parent = proof
                .derivation
                .expect("a proved postcondition relation must retain its local derivation");
            let node = self
                .derivations
                .intern(super::state::DerivationNode::PostconditionExit {
                    statement: statement.clone(),
                    relation_ordinal,
                    relation: Box::new(relation.clone()),
                    parent,
                });
            self.derivations.add_root(
                DerivationRootKind::PostconditionExit {
                    relation_ordinal,
                    occurrence: u32::try_from(occurrence)
                        .expect("postcondition exits exceed the u32 identity space"),
                },
                node,
            );
            PostconditionExitProof {
                disposition: PostconditionDisposition::Discharged,
                derivation: Some(node),
            }
        } else {
            PostconditionExitProof {
                disposition: if proof.disposition == ProofDisposition::Refuted {
                    PostconditionDisposition::Refuted
                } else {
                    PostconditionDisposition::Unproved
                },
                derivation: None,
            }
        }
    }

    /// [MSR-4] the affine target of one already-instantiated ordering
    /// relation, read through the affine image of each of its two terms.
    ///
    /// A measure term, a measure datum and an integer binding each have an
    /// image, so this reaches every relation whose operands the affine domain
    /// carries — which is what a clause over a run's measures is.
    fn affine_relation_target(
        &mut self,
        relation: &Relation,
        state: &AffineFlowState,
    ) -> Option<Vec<AffineInequality>> {
        let (left, right, difference, equal) = match relation {
            Relation::Bound { left, right, bound } => (*left, *right, *bound, false),
            Relation::Equal {
                left,
                right,
                difference,
            } => (*left, *right, *difference, true),
            _ => return None,
        };
        let left = self.affine_term_value(left, state)?;
        let right = self.affine_term_value(right, state)?;
        let mut check = AffineCheckState::new();
        let right = right
            .add(&AffineForm::constant(difference), &mut check)
            .ok()?;
        Self::affine_ordering_targets(&left, &right, equal)
    }

    /// Equality is the conjunction of its two ordinary affine bounds. Both
    /// are checked by the same deterministic numeric derivation as <=.
    fn affine_ordering_targets(
        left: &AffineForm,
        right: &AffineForm,
        equal: bool,
    ) -> Option<Vec<AffineInequality>> {
        let mut check = AffineCheckState::new();
        let mut targets = vec![AffineInequality::from_forms(left, right, &mut check).ok()?];
        if equal {
            targets.push(AffineInequality::from_forms(right, left, &mut check).ok()?);
        }
        Some(targets)
    }

    fn postcondition_affine_target(
        &self,
        postcondition: &CheckedPostcondition,
        result: &AffineForm,
        state: &AffineFlowState,
    ) -> Option<Vec<AffineInequality>> {
        let operands = postcondition
            .relation
            .operands
            .iter()
            .map(|operand| {
                self.postcondition_affine_datum(&operand.datum, result, state)?
                    .add(
                        &AffineForm::constant(operand.displacement),
                        &mut AffineCheckState::new(),
                    )
                    .ok()
            })
            .collect::<Option<Vec<_>>>()?;
        let (left, right, strict, equal) = match postcondition.relation.normalized {
            NormalizedRelation::UpperBound {
                left,
                right,
                strict,
            } => (left, right, strict, false),
            NormalizedRelation::Equal => (0, 1, false, true),
            NormalizedRelation::NotEqual => return None,
        };
        let left = operands.get(left as usize)?;
        let right = operands.get(right as usize)?;
        let right = if strict {
            right
                .subtract(&AffineForm::constant(1), &mut AffineCheckState::new())
                .ok()?
        } else {
            right.clone()
        };
        Self::affine_ordering_targets(left, &right, equal)
    }

    fn postcondition_affine_datum(
        &self,
        datum: &RelationDatum,
        result: &AffineForm,
        state: &AffineFlowState,
    ) -> Option<AffineForm> {
        match datum {
            RelationDatum::Result {
                ty: CheckedType::Integer(_),
                ..
            } => Some(result.clone()),
            RelationDatum::Parameter {
                ordinal,
                projections,
                ty: CheckedType::Integer(_),
            } if projections.is_empty() => {
                let binding = self.function.parameters.get(*ordinal as usize)?.binding;
                state.values.get(&binding).cloned()
            }
            RelationDatum::NamedConst {
                declaration,
                projections,
                ty: CheckedType::Integer(_),
            } if projections.is_empty() => self
                .context
                .constant(*declaration)
                .and_then(|constant| Self::postcondition_affine_constant(&constant.value)),
            RelationDatum::Literal { value, .. } => Self::postcondition_affine_constant(value),
            RelationDatum::Result { .. }
            | RelationDatum::Parameter { .. }
            | RelationDatum::NamedConst { .. }
            | RelationDatum::Measure(..) => None,
        }
    }

    fn postcondition_affine_constant(value: &CheckedValue) -> Option<AffineForm> {
        let value = match value {
            CheckedValue::Integer { ty, bits } => integer_value(*ty, *bits),
            CheckedValue::NumericIdentity {
                ty: CheckedType::Integer(_),
                one,
            } => i128::from(*one),
            _ => return None,
        };
        Some(AffineForm::constant(value))
    }

    /// Converts one already-substituted callable-boundary ordering predicate
    /// to its unique affine inequality. Unsupported goal shapes simply retain
    /// the ordinary L0 result; no alternate formula is guessed.
    fn affine_goal_ordering_target(
        &mut self,
        expression: &GoalExpression,
        state: &AffineFlowState,
    ) -> Option<AffineInequality> {
        self.affine_signed_goal_ordering_target(expression, state, GoalSign::Positive)
    }

    /// Retains the written right operand after the comparison and truth sign
    /// choose their fixed orientation. A compound operand without an L0 term
    /// has no right bridge; its coefficient vector cannot invent one.
    fn signed_goal_right_term(
        &mut self,
        expression: &GoalExpression,
        sign: GoalSign,
    ) -> Option<TermId> {
        let GoalExpression::Operation {
            row: GoalOperation::Integer { operation, .. },
            arguments,
            ..
        } = expression
        else {
            return None;
        };
        let [left, right] = arguments.as_slice() else {
            return None;
        };
        let reversed = match operation {
            CheckedIntegerOperation::Less | CheckedIntegerOperation::LessEqual => false,
            CheckedIntegerOperation::Greater | CheckedIntegerOperation::GreaterEqual => true,
            _ => return None,
        } ^ (sign == GoalSign::Negative);
        self.goal_side(if reversed { left } else { right })
            .map(|(term, _)| term)
    }

    /// S4 captures only non-L0 affine ordering leaves already established by
    /// the requirement's fixed signed decomposition. The immutable images
    /// cannot be retargeted by a later scalar assignment or measure kill.
    fn establish_requirement_affine_images(
        &mut self,
        requirement: &crate::semantic::goal::CheckedRequirement,
        ordinal: usize,
        state: &mut ProofFlowState,
    ) {
        let Some(expression) = self.body_requirement_goal(requirement) else {
            return;
        };
        let goal = self.intern_goal_expression(expression);
        let mut members = vec![(goal, GoalSign::Positive)];
        members.extend(self.signed_boolean_decomposition(goal, GoalSign::Positive, &state.facts));
        for (member, (goal, sign)) in members.into_iter().enumerate() {
            // Existing L0 projections remain on their ordinary route; putting
            // them in this list would widen AUTO's bounded premise sums.
            if self.goals.projection(goal).is_some() {
                continue;
            }
            let expression = self.goals.expression(goal).clone();
            let Some(inequality) =
                self.affine_signed_goal_ordering_target(&expression, &state.affine, sign)
            else {
                continue;
            };
            let parent = state
                .facts
                .opaque_proofs
                .get(&(goal, sign))
                .copied()
                .expect("every S4 decomposition member is established");
            let parent =
                self.derivations
                    .intern(super::state::DerivationNode::RequirementAffineImage {
                        goal,
                        sign,
                        parent,
                    });
            self.derivations.add_root(
                DerivationRootKind::RequirementAffineImage {
                    requirement: u32::try_from(ordinal).expect("requirement ordinal exceeds u32"),
                    member: u32::try_from(member).expect("decomposition ordinal exceeds u32"),
                },
                parent,
            );
            state.affine.facts.push(ActiveAffineFact {
                inequality,
                evidence: AffineFactEvidence::Derivation(parent),
                active_loops: Vec::new(),
            });
        }
    }

    /// Converts either truth sign of one callable-boundary ordering leaf to
    /// its unique affine inequality. Boolean composition uses this same leaf
    /// normalization instead of adding a call-specific affine fallback.
    fn affine_signed_goal_ordering_target(
        &mut self,
        expression: &GoalExpression,
        state: &AffineFlowState,
        sign: GoalSign,
    ) -> Option<AffineInequality> {
        let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation,
                    operand_type: CheckedType::Integer(_),
                },
            arguments,
            result: CheckedType::Bool,
            ..
        } = expression
        else {
            return None;
        };
        let [written_left, written_right] = arguments.as_slice() else {
            return None;
        };
        let left = self.affine_goal_value(written_left, state)?;
        let right = self.affine_goal_value(written_right, state)?;
        let mut check = AffineCheckState::new();
        let (left, right, strict) = match operation {
            CheckedIntegerOperation::LessEqual => (left, right, false),
            CheckedIntegerOperation::Less => (left, right, true),
            CheckedIntegerOperation::GreaterEqual => (right, left, false),
            CheckedIntegerOperation::Greater => (right, left, true),
            _ => return None,
        };
        let (left, right, strict) = match sign {
            GoalSign::Positive => (left, right, strict),
            // Integer order is total: not(left <= right) is right < left,
            // while not(left < right) is right <= left.
            GoalSign::Negative => (right, left, !strict),
        };
        let right = if strict {
            right.subtract(&AffineForm::constant(1), &mut check).ok()?
        } else {
            right
        };
        AffineInequality::from_forms(&left, &right, &mut check).ok()
    }

    /// Reads the mathematical value of the fixed affine subset admitted in a
    /// concrete call goal. Every place must be an unprojected current integer
    /// binding, and multiplication must have a literal/constant side.
    fn affine_goal_value(
        &mut self,
        expression: &GoalExpression,
        state: &AffineFlowState,
    ) -> Option<AffineForm> {
        match expression {
            GoalExpression::Datum(GoalDatum::Literal(value)) => {
                Self::postcondition_affine_constant(value)
            }
            GoalExpression::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty: CheckedType::Integer(_),
            }) if projections.is_empty() => self
                .context
                .constant(*declaration)
                .and_then(|constant| Self::postcondition_affine_constant(&constant.value)),
            GoalExpression::Datum(GoalDatum::Place {
                root,
                projections,
                ty: CheckedType::Integer(_),
            }) if projections.is_empty() => state.values.get(root).cloned(),
            GoalExpression::Operation {
                row:
                    GoalOperation::NumericConversion {
                        mode: CheckedConversionMode::Exact,
                        source: CheckedNumericType::Integer(_),
                        destination: CheckedNumericType::Integer(_),
                    },
                arguments,
                result: CheckedType::Integer(_),
                ..
            } => {
                let [value] = arguments.as_slice() else {
                    return None;
                };
                self.affine_goal_value(value, state)
            }
            GoalExpression::Operation {
                row: GoalOperation::Integer { operation, .. },
                arguments,
                result: CheckedType::Integer(_),
                ..
            } => {
                let [left, right] = arguments.as_slice() else {
                    return None;
                };
                let left = self.affine_goal_value(left, state)?;
                let right = self.affine_goal_value(right, state)?;
                let mut check = AffineCheckState::new();
                match operation {
                    CheckedIntegerOperation::AddExact => left.add(&right, &mut check).ok(),
                    CheckedIntegerOperation::SubtractExact => {
                        left.subtract(&right, &mut check).ok()
                    }
                    CheckedIntegerOperation::MultiplyExact => {
                        if left.terms().is_empty() {
                            right.scale(left.constant_value(), &mut check).ok()
                        } else if right.terms().is_empty() {
                            left.scale(right.constant_value(), &mut check).ok()
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            // [MSR-4] a measure term's image in the affine domain is its own
            // compiler-owned atom, exactly as it is for a term the flow
            // already carries. Without this a goal over a measure reaches
            // only the L0 route, and every filling loop's row requirement —
            // `built.len < built.cap` under `built.len + at >= n` — is
            // unproved for want of the domain the rule names.
            GoalExpression::Operation {
                row:
                    GoalOperation::ArrayMeasure { .. }
                    | GoalOperation::BufferMeasure { .. }
                    | GoalOperation::ContainerMeasure { .. },
                ..
            } => {
                let term = self.goal_operand(expression)?;
                Some(self.measure_atom(term, state))
            }
            GoalExpression::Datum(
                GoalDatum::Parameter { .. } | GoalDatum::EvaluatedValue { .. },
            )
            | GoalExpression::Datum(GoalDatum::NamedConst { .. })
            | GoalExpression::Datum(GoalDatum::Place { .. })
            | GoalExpression::Operation { .. } => None,
        }
    }

    fn instantiate_postcondition_relation(
        &mut self,
        postcondition: &CheckedPostcondition,
        results: &[Option<TermId>],
        returns: &[Option<PostconditionReturnDatum>],
    ) -> Option<Relation> {
        let operands = postcondition
            .relation
            .operands
            .iter()
            .map(|operand| {
                Some((
                    self.postcondition_relation_term(&operand.datum, results, returns)?,
                    operand.displacement,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let [first, second] = operands.as_slice() else {
            return None;
        };
        // [FN-9] each side's displacement folds into the one constant a
        // difference bound carries: `l + a <cmp> r + b` is
        // `l - r <cmp> b - a`.
        let gap = second.1.checked_sub(first.1)?;
        match postcondition.relation.normalized {
            NormalizedRelation::Equal => Some(Relation::Equal {
                left: first.0,
                right: second.0,
                difference: gap,
            }),
            NormalizedRelation::NotEqual => Some(if first.0 <= second.0 {
                Relation::Distinct {
                    left: first.0,
                    right: second.0,
                    difference: gap,
                }
            } else {
                Relation::Distinct {
                    left: second.0,
                    right: first.0,
                    difference: gap.checked_neg()?,
                }
            }),
            NormalizedRelation::UpperBound {
                left,
                right,
                strict,
            } => {
                let lower = *operands.get(left as usize)?;
                let upper = *operands.get(right as usize)?;
                Some(Relation::Bound {
                    left: lower.0,
                    right: upper.0,
                    bound: upper
                        .1
                        .checked_sub(lower.1)?
                        .checked_sub(i128::from(strict))?,
                })
            }
        }
    }

    fn postcondition_relation_term(
        &mut self,
        datum: &RelationDatum,
        results: &[Option<TermId>],
        returns: &[Option<PostconditionReturnDatum>],
    ) -> Option<TermId> {
        match datum {
            // [CALL-4] the datum names one declared result ordinal, and the
            // destination supplies that ordinal's term.
            RelationDatum::Result { ordinal, .. } => *results.get(*ordinal as usize)?,
            RelationDatum::Parameter {
                ordinal,
                projections,
                ty,
            } => {
                let binding = self.function.parameters.get(*ordinal as usize)?.binding;
                let projections = self.body_projections(PlaceRoot::Binding(binding), projections);
                self.postcondition_place_term(PlaceRoot::Binding(binding), projections, *ty)
            }
            RelationDatum::NamedConst {
                declaration,
                projections,
                ty,
            } => self.postcondition_named_const_term(*declaration, projections, *ty),
            RelationDatum::Literal { value, .. } => self.postcondition_constant_term(value),
            RelationDatum::Measure(measure, place) => match place.root {
                // [MSR-3] an `own` or shared-borrow parameter's measure in an
                // `ensures` denotes that parameter's entry datum, which the
                // entry placement minted and which nothing kills. The live
                // term is not read here: a body that writes the parameter
                // back still means the entry value.
                PostconditionPlaceRoot::Parameter { ordinal } => {
                    let kind = Self::entry_datum_kind(ordinal, &place.projections, *measure);
                    if let Some(datum) = self.terms.interned(&kind) {
                        return Some(datum);
                    }
                    let binding = self.function.parameters.get(ordinal as usize)?.binding;
                    let projections =
                        self.body_projections(PlaceRoot::Binding(binding), &place.projections);
                    self.postcondition_measure_term(
                        *measure,
                        PlaceRoot::Binding(binding),
                        projections,
                        place.ty,
                        self.formal_is_range(ordinal),
                    )
                }
                PostconditionPlaceRoot::ExitParameter { ordinal } => {
                    let binding = self.function.parameters.get(ordinal as usize)?.binding;
                    let projections =
                        self.body_projections(PlaceRoot::Binding(binding), &place.projections);
                    self.postcondition_measure_term(
                        *measure,
                        PlaceRoot::Binding(binding),
                        projections,
                        place.ty,
                        self.formal_is_range(ordinal),
                    )
                }
                // [CALL-4] a measure over a result place is instantiated at
                // that ordinal's own destination: at an exit, the place the
                // selected return hands back.
                PostconditionPlaceRoot::Result { ordinal } => {
                    let datum = returns.get(ordinal as usize)?.as_ref()?;
                    let PostconditionReturnDatum::Place(returned) = datum else {
                        return None;
                    };
                    let root = self.postcondition_return_place_root(returned.root)?;
                    let mut projections = returned.projections.clone();
                    projections.extend_from_slice(&place.projections);
                    self.postcondition_measure_term(*measure, root, &projections, place.ty, false)
                }
            },
        }
    }

    fn postcondition_return_term(&mut self, datum: &PostconditionReturnDatum) -> Option<TermId> {
        match datum {
            PostconditionReturnDatum::ResultPayload { ty } => {
                fragment_type(*ty).map(|ty| self.terms.intern(TermKind::ResultPayload(ty)))
            }
            PostconditionReturnDatum::Place(place) => self.postcondition_return_place_term(place),
            PostconditionReturnDatum::Literal { value, .. } => {
                self.postcondition_constant_term(value)
            }
            PostconditionReturnDatum::Measure(measure, place) => {
                let root = self.postcondition_return_place_root(place.root)?;
                self.postcondition_measure_term(
                    *measure,
                    root,
                    &place.projections,
                    place.ty,
                    place.range_referent,
                )
            }
        }
    }

    fn postcondition_return_place_term(
        &mut self,
        place: &PostconditionReturnPlace,
    ) -> Option<TermId> {
        if let PostconditionReturnPlaceRoot::NamedConst(declaration) = place.root {
            return self.postcondition_named_const_term(declaration, &place.projections, place.ty);
        }
        let root = self.postcondition_return_place_root(place.root)?;
        self.postcondition_place_term(root, &place.projections, place.ty)
    }

    fn postcondition_return_place_root(
        &self,
        root: PostconditionReturnPlaceRoot,
    ) -> Option<PlaceRoot> {
        match root {
            PostconditionReturnPlaceRoot::Binding(binding) => Some(PlaceRoot::Binding(binding)),
            PostconditionReturnPlaceRoot::NamedConst(declaration) => Some(PlaceRoot::Constant(
                *self.context.constant_ids.get(&declaration)?,
            )),
        }
    }

    fn postcondition_named_const_term(
        &mut self,
        declaration: crate::DeclarationId,
        projections: &[GoalProjection],
        ty: CheckedType,
    ) -> Option<TermId> {
        if projections.is_empty()
            && let Some(term) = self
                .context
                .constant(declaration)
                .and_then(|constant| self.postcondition_constant_term(&constant.value))
        {
            return Some(term);
        }
        let root = PlaceRoot::Constant(*self.context.constant_ids.get(&declaration)?);
        self.postcondition_place_term(root, projections, ty)
    }

    /// [ENT-2, MSR-6] every occurrence of a symbolic const parameter names
    /// its original declaration and keeps that declaration's written type.
    /// A storage extent or a differently typed formal does not create another
    /// constant identity or grant that parameter a different interval.
    fn const_parameter_term(&mut self, declaration: crate::DeclarationId) -> TermId {
        let ty = self.context.const_parameter_types[&declaration];
        self.terms.intern(TermKind::ConstParameter(declaration, ty))
    }

    fn postcondition_constant_term(&mut self, value: &CheckedValue) -> Option<TermId> {
        if let CheckedValue::ConstGeneric { declaration, .. } = value {
            return Some(self.const_parameter_term(*declaration));
        }
        let value = match value {
            CheckedValue::Integer { ty, bits } => integer_value(*ty, *bits),
            CheckedValue::NumericIdentity {
                ty: CheckedType::Integer(_),
                one,
            } => i128::from(*one),
            _ => return None,
        };
        Some(self.terms.intern(TermKind::Constant(value)))
    }

    fn postcondition_place_term(
        &mut self,
        root: PlaceRoot,
        projections: &[GoalProjection],
        ty: CheckedType,
    ) -> Option<TermId> {
        let fragment = fragment_type(ty)?;
        let projections = projections
            .iter()
            .map(|projection| projection.place_step())
            .collect::<Vec<_>>();
        let path = ResolvedPlace {
            root,
            path: projections,
        };
        Some(self.terms.intern(TermKind::Place(path, fragment)))
    }

    /// [TYPE-8, REF-4] whether the formal at this ordinal is a `&[T]`, whose
    /// one [MSR-1] row is the range's and not the element type's.
    fn formal_is_range(&self, ordinal: u32) -> bool {
        self.function
            .parameters
            .get(ordinal as usize)
            .is_some_and(|parameter| parameter.mode == CheckedMode::Range)
    }

    fn postcondition_measure_term(
        &mut self,
        measure: CheckedMeasure,
        root: PlaceRoot,
        projections: &[GoalProjection],
        ty: CheckedType,
        range_referent: bool,
    ) -> Option<TermId> {
        let projections = projections
            .iter()
            .map(|projection| projection.place_step())
            .collect::<Vec<_>>();
        // [MSR-1] gives `&[T]` a row of its own, and [TYPE-8] makes the
        // range kind a mode and not a type: the checked type of a `&[T]`
        // parameter is its element type, so the measured row cannot be
        // recovered from it and comes from the parameter's mode instead.
        // Without this a clause naming `deref(part).len` of a range
        // parameter had no term at all, so [FN-9] selected no exit for it.
        let measured = if range_referent && projections.is_empty() {
            MeasuredKind::Range
        } else {
            measured_kind(ty)?
        };
        // [MSR-2] the written constant a cell the table fixes as the type's
        // own reads: an `Array`'s length or a constant window's capacity.
        let array_length = type_constant(ty);
        Some(self.measure_term(
            measure,
            ResolvedPlace {
                root,
                path: projections,
            },
            measured,
            array_length,
        ))
    }

    /// The one former of every [MSR-1] measure term.
    ///
    /// Every measure of one place is formed together, because [MSR-2]'s
    /// standing facts relate them to each other: the value the table fixes
    /// for a cell, the equality of a table cell to another measure, and the
    /// orderings `P.len <= P.cap` and `P.head <= P.cap`. A site that
    /// names only one measure still needs the others to exist for those
    /// facts to have terms to relate, and all three have empty support beyond
    /// P's own, so forming them together costs nothing a program can observe.
    fn measure_term(
        &mut self,
        measure: CheckedMeasure,
        path: ResolvedPlace,
        measured: MeasuredKind,
        array_length: Option<CheckedConst>,
    ) -> TermId {
        let extent = self.intern_measure(CheckedMeasure::Length, &path);
        // [MSR-1]'s table, read once per cell.
        for cell_measure in [
            CheckedMeasure::Length,
            CheckedMeasure::Capacity,
            CheckedMeasure::Head,
        ] {
            let term = self.intern_measure(cell_measure, &path);
            let bound = match cell_measure.cell(measured) {
                MeasureCell::ExactConstant(value) => {
                    Some(MeasureBound::Constant(i128::from(value)))
                }
                MeasureCell::ExactExtent => match array_length {
                    Some(CheckedConst::Value(value)) => {
                        Some(MeasureBound::Constant(i128::from(value)))
                    }
                    Some(CheckedConst::Parameter(declaration)) => {
                        Some(MeasureBound::Equal(self.const_parameter_term(declaration)))
                    }
                    // A symbolic derived length has no [ENT-2] term form; the
                    // concrete instance, whose length is a value, restates the
                    // constant bound.
                    Some(CheckedConst::Derived(_)) => None,
                    // A runtime extent: `cap` is equal to it, `len` is it.
                    None => (cell_measure != CheckedMeasure::Length)
                        .then_some(MeasureBound::Equal(extent)),
                },
                // [MSR-2]: a measure the table fixes as the type's own
                // written constant is a standing fact with empty support; a
                // run's `cap` is that constant and a `Vector`'s is not.
                MeasureCell::ExactTypeConstant => match array_length {
                    Some(CheckedConst::Value(value)) => {
                        Some(MeasureBound::Constant(i128::from(value)))
                    }
                    Some(CheckedConst::Parameter(declaration)) => {
                        Some(MeasureBound::Equal(self.const_parameter_term(declaration)))
                    }
                    Some(CheckedConst::Derived(_)) | None => None,
                },
                // An independent runtime quantity of the value's own
                // descriptor: the standing facts [MSR-2] already publishes
                // relate it to the others, and it carries no bound of its own.
                MeasureCell::ExactRuntime | MeasureCell::Bounded | MeasureCell::Absent => None,
            };
            if let Some(bound) = bound {
                self.terms.set_measure_bound(term, bound);
            }
        }
        self.intern_measure(measure, &path)
    }

    fn intern_measure(&mut self, measure: CheckedMeasure, path: &ResolvedPlace) -> TermId {
        self.terms.intern(TermKind::Measure(measure, path.clone()))
    }

    fn available_postconditions(
        &self,
        function: super::super::model::FunctionId,
    ) -> Vec<AvailablePostcondition> {
        self.context
            .verified_postconditions(function)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(postcondition, proof)| {
                Some(AvailablePostcondition {
                    relation: postcondition.relation.clone(),
                    variant: postcondition.selector.variant,
                    field: postcondition
                        .selector
                        .field
                        .as_ref()
                        .map(|field| field.declaration),
                    authority: super::RelationProvenance::Verified(proof.summary.clone()?),
                })
            })
            .collect()
    }

    /// The relations one exact call may publish [FN-5]. A direct call uses
    /// its actual's verified FN-9 surface. A bound call uses the retained
    /// formal surface only when its exact FN-4 query is discharged and every
    /// actual premise selected by that query has an earlier-component FN-9
    /// summary. A zero-premise implication needs only its retained query.
    fn available_call_postconditions(
        &self,
        function: super::super::model::FunctionId,
        formal: Option<&super::super::model::CheckedCallContract>,
    ) -> Vec<AvailablePostcondition> {
        let Some(formal) = formal else {
            return self.available_postconditions(function);
        };
        formal
            .postconditions
            .iter()
            .filter_map(|boundary| {
                let query = self.context.contract_query(boundary.query)?;
                let [outcome] = query.proof.contract_goals.as_slice() else {
                    return None;
                };
                if outcome.disposition != CallGoalDisposition::Discharged
                    || outcome.derivation.is_none()
                    || query.instance != Some(self.function.id)
                    || query.goal != boundary.selector.block
                    || query.premises.len() != boundary.actual_premises.len()
                {
                    return None;
                }
                let premises = boundary
                    .actual_premises
                    .iter()
                    .zip(&query.premises)
                    .map(|(ordinal, clause)| {
                        let (postcondition, proof) =
                            self.context.verified_postcondition(function, *ordinal)?;
                        let summary = proof.summary.as_ref()?;
                        (postcondition.selector.block == *clause
                            && summary.function == function
                            && summary.relation_ordinal == *ordinal)
                            .then(|| summary.clone())
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(AvailablePostcondition {
                    relation: boundary.relation.clone(),
                    variant: boundary.selector.variant,
                    field: boundary
                        .selector
                        .field
                        .as_ref()
                        .map(|field| field.declaration),
                    authority: super::RelationProvenance::FormalBoundary {
                        query: boundary.query,
                        actual: function,
                        premises,
                    },
                })
            })
            .collect()
    }

    /// [REF-1, MSR-2] the reference variables a place reads through.
    ///
    /// A measure term's support contains every reference variable any prefix
    /// of its place reads through, so ending one of those bindings ends the
    /// fact. v0.59 walked a holder chain; v0.60 asks the reference summary,
    /// which is where a reference's path set now lives.
    fn append_holder_chain(&self, binding: BindingId, holders: &mut Vec<BindingId>) {
        if !self.places.is_reference(binding) {
            return;
        }
        if !holders.contains(&binding) {
            holders.push(binding);
        }
    }

    fn collect_checked_argument_holders(
        &self,
        argument: &CheckedExpression,
        holders: &mut Vec<BindingId>,
    ) {
        match argument {
            CheckedExpression::Binding { binding, .. }
            | CheckedExpression::Project { binding, .. }
            | CheckedExpression::DerefAddressed { binding, .. } => {
                self.append_holder_chain(*binding, holders);
            }
            CheckedExpression::BorrowAddressed { root, .. } => {
                if let Some(binding) = root.binding() {
                    self.append_holder_chain(binding, holders);
                }
            }
            CheckedExpression::BufferMeasure { root, .. } => {
                self.append_holder_chain(root.binding, holders);
            }
            CheckedExpression::RangeMeasure { root, .. } => {
                self.append_holder_chain(root.binding, holders);
            }
            CheckedExpression::RangeElementMeasure { place, .. }
            | CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => {
                self.append_holder_chain(place.root.binding, holders);
            }
            CheckedExpression::RangeOf { source, .. } => {
                if let Some(binding) = source.binding() {
                    self.append_holder_chain(binding, holders);
                }
            }
            CheckedExpression::ArrayMeasure {
                root: CheckedArrayRoot::Binding { binding, .. },
                ..
            } => self.append_holder_chain(*binding, holders),
            // These checked wrappers are one read of their nested place. They
            // do not create a second consume, but M must retain the holder on
            // which the resulting caller image depends.
            CheckedExpression::BoxDeref { value, .. }
            | CheckedExpression::ProjectValue { value, .. } => {
                self.collect_checked_argument_holders(value, holders);
            }
            _ => {}
        }
    }

    fn collect_goal_image_holders(&self, argument: &GoalExpression, holders: &mut Vec<BindingId>) {
        match argument {
            GoalExpression::Datum(GoalDatum::Place {
                root, projections, ..
            }) => {
                let support = GoalSupport {
                    root: *root,
                    projections: projections.clone(),
                    measure: None,
                };
                let (_, image_holders) = self.resolve_goal_support(&support);
                for holder in image_holders {
                    if !holders.contains(&holder) {
                        holders.push(holder);
                    }
                }
            }
            GoalExpression::Operation { arguments, .. } => {
                for argument in arguments {
                    self.collect_goal_image_holders(argument, holders);
                }
            }
            GoalExpression::Datum(_) => {}
        }
    }

    fn call_argument_holder_chain(
        &self,
        argument: &CheckedExpression,
        goal_argument: &GoalExpression,
    ) -> Vec<BindingId> {
        let mut holders = Vec::new();
        self.collect_checked_argument_holders(argument, &mut holders);
        self.collect_goal_image_holders(goal_argument, &mut holders);
        holders
    }

    fn postcondition_term_live_holders(&self, term: TermId) -> Vec<BindingId> {
        let mut holders = Vec::new();
        match self.terms.kind(term) {
            TermKind::Place(place, _) | TermKind::Measure(_, place) => {
                let PlaceRoot::Binding(root) = place.root else {
                    return holders;
                };
                let support = GoalSupport {
                    root,
                    projections: place
                        .path
                        .iter()
                        .map_while(goal_projection_of_step)
                        .collect(),
                    measure: match self.terms.kind(term) {
                        TermKind::Measure(measure, _) => Some(*measure),
                        _ => None,
                    },
                };
                let (_, projected_holders) = self.resolve_goal_support(&support);
                holders.extend(projected_holders);
            }
            TermKind::Zero | TermKind::Constant(_) | TermKind::ConstParameter(..) => {}
            TermKind::CountedCapture { .. }
            | TermKind::IndexCapture { .. }
            | TermKind::ResultPayload(_)
            | TermKind::CommitValue { .. }
            | TermKind::CallDatum { .. }
            | TermKind::EntryDatum { .. }
            | TermKind::MeasureDatum { .. } => {}
        }
        holders
    }

    fn s12_transfer_event_kills_substitution(
        &self,
        separations: &dyn SeparationOracle,
        substitution: &PostconditionCallSubstitution,
        event: &KillEvent,
    ) -> bool {
        // [MSR-3] a call datum contains no place and denotes the operand's
        // value at the pre-transfer point, so no event at or after the call
        // can invalidate a relation stated over it.
        if substitution.datum {
            return false;
        }
        let holder_consumed = match event {
            KillEvent::Consume { binding, .. }
            | KillEvent::EntryImageHolderConsume { binding, .. } => {
                substitution.transfer_holders.contains(binding)
            }
            _ => false,
        };
        if holder_consumed {
            return true;
        }
        match event {
            KillEvent::EntryImageHolderWrite {
                place,
                element,
                source,
            } => self.event_kills_term(
                separations,
                substitution.term,
                &KillEvent::Write {
                    place: place.clone(),
                    element: *element,
                    source: source.clone(),
                },
            ),
            _ => self.event_kills_term(separations, substitution.term, event),
        }
    }

    fn s12_candidate_term_killed(
        &self,
        separations: &dyn SeparationOracle,
        term: TermId,
        event: &KillEvent,
    ) -> bool {
        let live_holders = self.postcondition_term_live_holders(term);
        let live_holder_killed = match event {
            KillEvent::Consume { binding, .. }
            | KillEvent::EntryImageHolderConsume { binding, .. } => live_holders.contains(binding),
            KillEvent::Write {
                place,
                element: false,
                ..
            }
            | KillEvent::EntryImageHolderWrite {
                place,
                element: false,
                ..
            } => live_holders
                .iter()
                .any(|holder| self.write_replaces_live_holder(*holder, place)),
            KillEvent::Write { element: true, .. }
            | KillEvent::EntryImageHolderWrite { element: true, .. } => false,
        };
        if live_holder_killed {
            return true;
        }
        match event {
            KillEvent::EntryImageHolderConsume { binding, source } => self.event_kills_term(
                separations,
                term,
                &KillEvent::Consume {
                    binding: *binding,
                    source: source.clone(),
                },
            ),
            KillEvent::EntryImageHolderWrite {
                place,
                element,
                source,
            } => self.event_kills_term(
                separations,
                term,
                &KillEvent::Write {
                    place: place.clone(),
                    element: *element,
                    source: source.clone(),
                },
            ),
            _ => self.event_kills_term(separations, term, event),
        }
    }

    /// Whether one write definitely replaces the target selected by a live
    /// reference holder.
    ///
    /// A postcondition substitution that still reads a holder dies when that
    /// holder is rebound. A write below its referent does not rebind it: the
    /// ordinary term-support judgment below decides whether that field,
    /// element, descriptor word, or whole-value write reaches the term. The
    /// former prefix-overlap test conflated those questions and discarded a
    /// window-length relation on `swap(&deref(holder)[i], ...)` even though
    /// [MSR-2] makes the indexed element disjoint from the descriptor.
    fn write_replaces_live_holder(&self, holder: BindingId, written: &ResolvedPlace) -> bool {
        if written.root == PlaceRoot::Binding(holder) && written.path.is_empty() {
            return true;
        }
        let holder_targets = self.places.resolve(PlaceRoot::Binding(holder), &[]);
        let written_targets = self.places.resolve(written.root, &written.path);
        if holder_targets.is_empty() || written_targets.is_empty() {
            return true;
        }
        let same_target = |left: &ResolvedPlace, right: &ResolvedPlace| {
            left.contains(right) && right.contains(left)
        };
        holder_targets.len() == written_targets.len()
            && holder_targets.iter().all(|holder| {
                written_targets
                    .iter()
                    .any(|write| same_target(holder, write))
            })
            && written_targets.iter().all(|write| {
                holder_targets
                    .iter()
                    .any(|holder| same_target(holder, write))
            })
    }

    fn s12_candidate_scope_kills_term(&self, term: TermId, exited: &HashSet<BindingId>) -> bool {
        self.scope_kills_term(term, exited)
            || self
                .postcondition_term_live_holders(term)
                .iter()
                .any(|holder| exited.contains(holder))
    }

    fn s12_substitutions_survive(
        &self,
        separations: &dyn SeparationOracle,
        substitutions: &[PostconditionCallSubstitution],
        events: &[KillEvent],
        call_transfer: bool,
    ) -> bool {
        substitutions.iter().all(|substitution| {
            if call_transfer && substitution.exit_state {
                return true;
            }
            events.iter().all(|event| {
                !self.s12_transfer_event_kills_substitution(separations, substitution, event)
            })
        })
    }

    fn kill_s12_candidates_for_event(
        &self,
        separations: &dyn SeparationOracle,
        state: &mut FactState,
        event: &KillEvent,
    ) {
        if !state.may_hold_postcondition_candidates() {
            return;
        }
        state.kill_proof_candidates(&self.derivations, |left, right, proof| {
            self.derivations.depends_on_postcondition_call(proof)
                && (self.s12_candidate_term_killed(separations, left, event)
                    || self.s12_candidate_term_killed(separations, right, event))
        });
    }

    fn kill_s12_candidates_for_scope(&self, state: &mut FactState, exited: &HashSet<BindingId>) {
        if !state.may_hold_postcondition_candidates() {
            return;
        }
        state.kill_proof_candidates(&self.derivations, |left, right, proof| {
            self.derivations.depends_on_postcondition_call(proof)
                && (self.s12_candidate_scope_kills_term(left, exited)
                    || self.s12_candidate_scope_kills_term(right, exited))
        });
    }

    fn call_parameter_place(
        &self,
        actual: &GoalExpression,
        projections: &[GoalProjection],
    ) -> Option<(PlaceRoot, Vec<GoalProjection>)> {
        let GoalExpression::Datum(datum) = actual else {
            return None;
        };
        let (root, actual_projections) = match datum {
            GoalDatum::Place {
                root, projections, ..
            } => (PlaceRoot::Binding(*root), projections),
            GoalDatum::NamedConst {
                declaration,
                projections,
                ..
            } => (
                PlaceRoot::Constant(*self.context.constant_ids.get(declaration)?),
                projections,
            ),
            GoalDatum::Parameter { .. }
            | GoalDatum::EvaluatedValue { .. }
            | GoalDatum::Literal(_) => return None,
        };
        Some((
            root,
            actual_projections
                .iter()
                .chain(projections)
                .copied()
                .collect(),
        ))
    }

    fn call_parameter_term(
        &mut self,
        actual: &GoalExpression,
        projections: &[GoalProjection],
        ty: CheckedType,
        measure: Option<CheckedMeasure>,
        mode: CheckedMode,
    ) -> Option<TermId> {
        let projections = if mode == CheckedMode::Own {
            projections
        } else {
            let (GoalProjection::Deref, remaining) = projections.split_first()? else {
                return None;
            };
            remaining
        };
        if projections.is_empty() && measure.is_none() {
            return (actual.ty() == ty)
                .then(|| self.goal_operand(actual))
                .flatten();
        }
        let (root, projections) = self.call_parameter_place(actual, projections)?;
        if let Some(measure) = measure {
            // [TYPE-8, REF-4] the callee's own parameter kind supplies the
            // [MSR-1] row: the caller's actual may be any place that names a
            // range, and the element type it carries has no row at all.
            self.postcondition_measure_term(
                measure,
                root,
                &projections,
                ty,
                mode == CheckedMode::Range,
            )
        } else {
            self.postcondition_place_term(root, &projections, ty)
        }
    }

    /// [MSR-3] the identity of one call datum: the call, the formal ordinal,
    /// the operand's ordered projections, and which [MSR-1] measure of the
    /// operand the datum denotes, if any.
    fn call_datum_kind(
        call: &crate::NodePath,
        formal: u32,
        projections: &[GoalProjection],
        measure: Option<CheckedMeasure>,
        ty: super::super::model::IntegerType,
    ) -> TermKind {
        TermKind::CallDatum {
            call_path: call.components().to_vec(),
            formal,
            projections: projections
                .iter()
                .map(|projection| projection.place_step())
                .collect(),
            measure,
            ty,
        }
    }

    /// [ENT-3.S13, MSR-3] mints, at one call's pre-transfer point, the call
    /// datum of every `own` operand any declared relation of the resolved
    /// callee names, and establishes it equal to that operand's pre-transfer
    /// term.
    ///
    /// The equality is stated here, before the call's own consumes and
    /// kills, so [ENT-5]'s pre-kill closure carries the datum's consequences
    /// across them. The datum itself contains no place, so nothing kills it:
    /// that is exactly why a relation naming a consumed `own` operand's
    /// measure means what it reads as at the caller, and why the consume the
    /// same statement performs cannot delete it.
    fn establish_call_datums(
        &mut self,
        function: super::super::model::FunctionId,
        call: &crate::NodePath,
        goal_arguments: &[GoalExpression],
        postconditions: &[AvailablePostcondition],
        state: &mut ProofFlowState,
    ) {
        let Some(callee) = self.context.callee(function) else {
            return;
        };
        let parameter_modes = callee.parameter_modes.clone();
        let mut operands: Vec<(
            u32,
            Vec<GoalProjection>,
            Option<CheckedMeasure>,
            CheckedType,
        )> = Vec::new();
        for available in postconditions {
            for operand in &available.relation.operands {
                let datum = &operand.datum;
                match datum {
                    RelationDatum::Parameter {
                        ordinal,
                        projections,
                        ty,
                    } => operands.push((*ordinal, projections.clone(), None, *ty)),
                    // A result-rooted measure names no operand and mints no
                    // call datum [CALL-4].
                    RelationDatum::Measure(measure, place) => {
                        if let PostconditionPlaceRoot::Parameter { ordinal } = place.root {
                            operands.push((
                                ordinal,
                                place.projections.clone(),
                                Some(*measure),
                                place.ty,
                            ));
                        }
                    }
                    RelationDatum::Result { .. }
                    | RelationDatum::NamedConst { .. }
                    | RelationDatum::Literal { .. } => {}
                }
            }
        }
        let event = self.proof_event(FlowEventKind::S13, Some(call));
        for (ordinal, projections, measure, ty) in operands {
            let Some(mode) = parameter_modes.get(ordinal as usize).copied() else {
                continue;
            };
            if mode != CheckedMode::Own
                && !(measure.is_some() && matches!(mode, CheckedMode::Reference))
            {
                continue;
            }
            let Some(datum_type) = (if measure.is_some() {
                Some(super::super::model::IntegerType::U64)
            } else {
                fragment_type(ty)
            }) else {
                continue;
            };
            let kind = Self::call_datum_kind(call, ordinal, &projections, measure, datum_type);
            if self.terms.interned(&kind).is_some() {
                continue;
            }
            let Some(actual) = goal_arguments.get(ordinal as usize) else {
                continue;
            };
            let Some(term) = self.call_parameter_term(actual, &projections, ty, measure, mode)
            else {
                continue;
            };
            // A datum denotes the operand's value at this point. When the
            // pre-transfer term is already immutable and has empty support,
            // it denotes exactly that and nothing can retarget it, so the
            // datum is that term: minting a second one would add an
            // indirection to every derivation and hide the writer's own
            // constant behind it in a diagnostic.
            if self.immortal_term(term) {
                continue;
            }
            let datum = self.terms.intern(kind);
            self.adopt_measure_atom(datum, term, &state.affine);
            state.facts.establish(
                &Relation::Equal {
                    left: datum,
                    right: term,
                    difference: 0,
                },
                &mut self.derivations,
                event,
            );
        }
    }

    /// Whether one term is already immutable with empty support, so that no
    /// [ENT-5] event can change what it denotes [ENT-2, MSR-3].
    fn immortal_term(&self, term: TermId) -> bool {
        matches!(
            self.terms.kind(term),
            TermKind::Zero
                | TermKind::Constant(_)
                | TermKind::ConstParameter(..)
                | TermKind::CountedCapture { .. }
                | TermKind::IndexCapture { .. }
                | TermKind::ResultPayload(_)
                | TermKind::CommitValue { .. }
                | TermKind::CallDatum { .. }
        )
    }

    /// The already-minted call datum of one `own` operand, when this call
    /// established one [MSR-3].
    fn interned_call_datum(
        &self,
        call: &crate::NodePath,
        formal: u32,
        projections: &[GoalProjection],
        measure: Option<CheckedMeasure>,
        ty: CheckedType,
    ) -> Option<TermId> {
        let datum_type = if measure.is_some() {
            super::super::model::IntegerType::U64
        } else {
            fragment_type(ty)?
        };
        self.terms.interned(&Self::call_datum_kind(
            call,
            formal,
            projections,
            measure,
            datum_type,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn instantiate_call_postcondition_relation(
        &mut self,
        function: super::super::model::FunctionId,
        call_path: &crate::NodePath,
        template: &RelationTemplate,
        checked_arguments: &[CheckedExpression],
        arguments: &[GoalExpression],
        results: &[Option<TermId>],
        result_places: &[Option<(PlaceRoot, Vec<GoalProjection>, CheckedType)>],
    ) -> Option<InstantiatedPostcondition> {
        let parameter_modes = self.context.callee(function)?.parameter_modes.clone();
        let mut substitutions = Vec::new();
        let mut operands = Vec::with_capacity(template.operands.len());
        for (operand, term_operand) in template.operands.iter().enumerate() {
            let datum = &term_operand.datum;
            let (term, formal) = match datum {
                // [CALL-4] the destination supplies one term per declared
                // result ordinal; an ordinal with none makes only this
                // relation unavailable.
                RelationDatum::Result { ordinal, .. } => {
                    ((*results.get(*ordinal as usize)?)?, None)
                }
                RelationDatum::Parameter {
                    ordinal,
                    projections,
                    ty,
                } => match self.interned_call_datum(call_path, *ordinal, projections, None, *ty) {
                    // [MSR-3] an `own` operand denotes this call's call
                    // datum, which has empty support.
                    Some(datum) => (datum, Some((*ordinal, true))),
                    None => (
                        self.call_parameter_term(
                            arguments.get(*ordinal as usize)?,
                            projections,
                            *ty,
                            None,
                            *parameter_modes.get(*ordinal as usize)?,
                        )?,
                        Some((*ordinal, false)),
                    ),
                },
                RelationDatum::NamedConst {
                    declaration,
                    projections,
                    ty,
                } => (
                    self.postcondition_named_const_term(*declaration, projections, *ty)?,
                    None,
                ),
                RelationDatum::Literal { value, .. } => {
                    (self.postcondition_constant_term(value)?, None)
                }
                RelationDatum::Measure(measure, place) => match place.root {
                    PostconditionPlaceRoot::Parameter { ordinal } => {
                        match self.interned_call_datum(
                            call_path,
                            ordinal,
                            &place.projections,
                            Some(*measure),
                            place.ty,
                        ) {
                            Some(datum) => (datum, Some((ordinal, true))),
                            None => (
                                self.call_parameter_term(
                                    arguments.get(ordinal as usize)?,
                                    &place.projections,
                                    place.ty,
                                    Some(*measure),
                                    *parameter_modes.get(ordinal as usize)?,
                                )?,
                                Some((ordinal, false)),
                            ),
                        }
                    }
                    PostconditionPlaceRoot::ExitParameter { ordinal } => (
                        self.call_parameter_term(
                            arguments.get(ordinal as usize)?,
                            &place.projections,
                            place.ty,
                            Some(*measure),
                            *parameter_modes.get(ordinal as usize)?,
                        )?,
                        Some((ordinal, false)),
                    ),
                    // [CALL-4] the destination supplies one place per
                    // declared result ordinal, and this operand is that
                    // place's measure rather than its value.
                    PostconditionPlaceRoot::Result { ordinal } => {
                        let (root, destination, _) =
                            result_places.get(ordinal as usize)?.as_ref()?;
                        // [CALL-4, MSR-1] the clause's own projections below
                        // the result ordinal continue the destination's path:
                        // `result.inner.len` names the measure of the cell
                        // content the binder holds [TYPE-9] and not a measure
                        // of the cell, which has no row at all.
                        let mut projections = destination.clone();
                        projections.extend(place.projections.iter().cloned());
                        (
                            self.postcondition_measure_term(
                                *measure,
                                *root,
                                &projections,
                                place.ty,
                                false,
                            )?,
                            None,
                        )
                    }
                },
            };
            if let Some((formal, datum)) = formal {
                substitutions.push(PostconditionCallSubstitution {
                    operand: u32::try_from(operand)
                        .expect("postcondition operands exceed the u32 identity space"),
                    formal,
                    term,
                    transfer_holders: self.call_argument_holder_chain(
                        checked_arguments.get(formal as usize)?,
                        arguments.get(formal as usize)?,
                    ),
                    datum,
                    exit_state: matches!(
                        &term_operand.datum,
                        RelationDatum::Measure(
                            _,
                            PostconditionPlace {
                                root: PostconditionPlaceRoot::ExitParameter { .. },
                                ..
                            }
                        )
                    ),
                });
            }
            operands.push((term, term_operand.displacement));
        }
        let [first, second] = operands.as_slice() else {
            return None;
        };
        // [FN-9] each side's displacement folds into the one constant a
        // difference bound carries.
        let gap = second.1.checked_sub(first.1)?;
        let relation = match template.normalized {
            NormalizedRelation::Equal => Relation::Equal {
                left: first.0,
                right: second.0,
                difference: gap,
            },
            NormalizedRelation::NotEqual => {
                if first.0 <= second.0 {
                    Relation::Distinct {
                        left: first.0,
                        right: second.0,
                        difference: gap,
                    }
                } else {
                    Relation::Distinct {
                        left: second.0,
                        right: first.0,
                        difference: gap.checked_neg()?,
                    }
                }
            }
            NormalizedRelation::UpperBound {
                left,
                right,
                strict,
            } => {
                let lower = *operands.get(left as usize)?;
                let upper = *operands.get(right as usize)?;
                Relation::Bound {
                    left: lower.0,
                    right: upper.0,
                    bound: upper
                        .1
                        .checked_sub(lower.1)?
                        .checked_sub(i128::from(strict))?,
                }
            }
        };
        Some(InstantiatedPostcondition {
            relation,
            substitutions,
        })
    }

    fn selected_call_summary(
        &self,
        available: &AvailablePostcondition,
    ) -> Option<VerifiedPostconditionSummaryRef> {
        Some(VerifiedPostconditionSummaryRef {
            summary: available.authority.clone(),
        })
    }

    fn retain_postcondition_call(
        &mut self,
        instantiated: &InstantiatedPostcondition,
        available: &AvailablePostcondition,
        prepared: &PreparedCall,
    ) -> Option<DerivationId> {
        let summary = self.selected_call_summary(available)?;
        Some(
            self.derivations
                .intern(super::state::DerivationNode::PostconditionCall {
                    detail: Box::new(super::state::PostconditionCallDetail {
                        call: prepared.call.clone(),
                        relation: instantiated.relation.clone(),
                        summary,
                        substitutions: instantiated.substitutions.clone(),
                        transfer_events: prepared.transfer_events.clone(),
                        parents: prepared.parents.clone(),
                    }),
                }),
        )
    }

    fn retain_direct_result(
        &mut self,
        statement: &crate::NodePath,
        binding: BindingId,
        instantiated: &InstantiatedPostcondition,
        available: &AvailablePostcondition,
        prepared: &PreparedCall,
        state: &mut FactState,
    ) {
        let Some(call) = self.retain_postcondition_call(instantiated, available, prepared) else {
            return;
        };
        let route =
            self.derivations
                .intern(super::state::DerivationNode::PostconditionDirectResult {
                    statement: statement.clone(),
                    binding,
                    relation: Box::new(instantiated.relation.clone()),
                    parent: call,
                });
        let occurrence = self.s12_roots;
        self.s12_roots = self
            .s12_roots
            .checked_add(1)
            .expect("S12 roots exceed the u32 identity space");
        self.derivations.add_root(
            DerivationRootKind::PostconditionDirectResult { occurrence },
            route,
        );
        state.establish_from_proof(&instantiated.relation, route, &self.derivations);
    }

    fn establish_direct_result(
        &mut self,
        statement: &crate::NodePath,
        binding: BindingId,
        value: &CheckedExpression,
        prepared: &PreparedCall,
        states: &mut ProofFlowState,
    ) {
        let CheckedExpression::UserCall {
            function,
            call,
            arguments,
            goal_arguments,
            result,
            ..
        } = value
        else {
            return;
        };
        if *function != prepared.callee || *call != prepared.call {
            return;
        }
        // [CALL-4] the destination is one term when the ordinal's value is an
        // [ENT-2] term, and is always the place a measure over that ordinal is
        // taken over. A measured result has the second and not the first.
        let result_term = fragment_type(*result)
            .and_then(|_| self.postcondition_place_term(PlaceRoot::Binding(binding), &[], *result));
        let result_place = Some((PlaceRoot::Binding(binding), Vec::new(), *result));
        for available in prepared.postconditions.iter().cloned() {
            if available.variant.is_some()
                || !available
                    .relation
                    .operands
                    .iter()
                    .any(|term| term.contains_result())
            {
                continue;
            }
            let Some(instantiated) = self.instantiate_call_postcondition_relation(
                *function,
                call,
                &available.relation,
                arguments,
                goal_arguments,
                &[result_term],
                std::slice::from_ref(&result_place),
            ) else {
                continue;
            };
            if !self.s12_substitutions_survive(
                &prepared.entry_separations(&states.separations),
                &instantiated.substitutions,
                &prepared.kills,
                true,
            ) {
                continue;
            }
            self.retain_direct_result(
                statement,
                binding,
                &instantiated,
                &available,
                prepared,
                &mut states.facts,
            );
        }
    }

    /// [ENT-3.S12, CALL-4] establishes, at each destination of a binder or
    /// target list, every published relation naming that destination's result
    /// ordinal.
    ///
    /// The destinations are given in written order, so destination i is
    /// result ordinal i; `extra_kills` are the events the same statement's
    /// commits contribute, which a substitution must survive exactly as it
    /// must survive the call's own.
    fn establish_result_list_destinations(
        &mut self,
        statement: &crate::NodePath,
        destinations: &[Option<(BindingId, Vec<GoalProjection>, CheckedType)>],
        value: &CheckedExpression,
        prepared: &PreparedCall,
        extra_kills: &[KillEvent],
        states: &mut ProofFlowState,
    ) {
        let CheckedExpression::UserCall {
            function,
            call,
            arguments,
            goal_arguments,
            ..
        } = value
        else {
            return;
        };
        if *function != prepared.callee || *call != prepared.call {
            return;
        }
        // One term per result ordinal, in written order. A subscript place is
        // no [ENT-2] term and a non-fragment ordinal carries no relation
        // datum, so either leaves its ordinal without a term and makes only
        // the relations naming it unavailable.
        let mut result_terms = Vec::with_capacity(destinations.len());
        // [CALL-4] the same destination is also the place a measure over that
        // result ordinal is taken over, which a measured ordinal has and a
        // fragment-integer value term does not.
        let mut result_places = Vec::with_capacity(destinations.len());
        let mut anchor = None;
        for destination in destinations {
            let term = destination.as_ref().and_then(|(binding, fields, ty)| {
                fragment_type(*ty)?;
                let term = self.postcondition_place_term(PlaceRoot::Binding(*binding), fields, *ty);
                if term.is_some() && anchor.is_none() {
                    anchor = Some(*binding);
                }
                term
            });
            result_places.push(destination.as_ref().map(|(binding, fields, ty)| {
                if anchor.is_none() {
                    anchor = Some(*binding);
                }
                (PlaceRoot::Binding(*binding), fields.clone(), *ty)
            }));
            result_terms.push(term);
        }
        let Some(anchor) = anchor else {
            return;
        };
        for available in prepared.postconditions.iter().cloned() {
            // A variant-routed relation is restricted to its arm [CALL-6];
            // a binder or target list enters no arm.
            if available.variant.is_some()
                || !available
                    .relation
                    .operands
                    .iter()
                    .any(|term| term.contains_result())
            {
                continue;
            }
            let Some(instantiated) = self.instantiate_call_postcondition_relation(
                *function,
                call,
                &available.relation,
                arguments,
                goal_arguments,
                &result_terms,
                &result_places,
            ) else {
                continue;
            };
            if !self.s12_substitutions_survive(
                &prepared.entry_separations(&states.separations),
                &instantiated.substitutions,
                &prepared.kills,
                true,
            ) || !self.s12_substitutions_survive(
                &states.separations,
                &instantiated.substitutions,
                extra_kills,
                false,
            ) {
                continue;
            }
            self.retain_direct_result(
                statement,
                anchor,
                &instantiated,
                &available,
                prepared,
                &mut states.facts,
            );
        }
    }

    fn receiver_argument_overlaps(
        &self,
        separations: &dyn SeparationOracle,
        expression: &CheckedExpression,
        receiver: &ResolvedPlace,
    ) -> bool {
        if self
            .read_place_path(expression)
            .is_some_and(|place| self.resolved_places_overlap(separations, &place, receiver))
        {
            return true;
        }
        if self
            .argument_referents(expression)
            .iter()
            .any(|(place, _)| self.resolved_places_overlap(separations, place, receiver))
        {
            return true;
        }
        expression_children(expression)
            .into_iter()
            .any(|child| self.receiver_argument_overlaps(separations, child, receiver))
    }

    fn direct_receiver_route(
        &self,
        separations: &dyn SeparationOracle,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        prepared: &PreparedCall,
    ) -> Option<DirectReceiverRoute> {
        let CheckedSetTarget::Place(target) = target else {
            return None;
        };
        let CheckedExpression::UserCall {
            function,
            call,
            arguments,
            result,
            ..
        } = value
        else {
            return None;
        };
        if *function != prepared.callee
            || *call != prepared.call
            || !target.fields.is_empty()
            || self.is_holder(target.binding)
            || *result != target.ty
            || fragment_type(target.ty).is_none()
        {
            return None;
        }
        let receiver = ResolvedPlace {
            root: PlaceRoot::Binding(target.binding),
            path: Vec::new(),
        };
        let mut selected = None;
        for (formal, argument) in arguments.iter().enumerate() {
            let exact = matches!(
                argument,
                CheckedExpression::Binding {
                    binding,
                    ty,
                    consume_root: false,
                    ..
                } if *binding == target.binding && *ty == target.ty
            );
            if exact {
                if selected.is_some() {
                    return None;
                }
                selected = Some(
                    u32::try_from(formal)
                        .expect("call argument ordinal exceeds the u32 identity space"),
                );
            } else if self.receiver_argument_overlaps(separations, argument, &receiver) {
                return None;
            }
        }
        Some(DirectReceiverRoute {
            binding: target.binding,
            formal: selected?,
            ty: target.ty,
        })
    }

    fn retain_direct_receiver(
        &mut self,
        statement: &crate::NodePath,
        candidate: &DirectReceiverCandidate,
        target_event: FlowEventId,
        prepared: &PreparedCall,
        state: &mut FactState,
    ) {
        let Some(call) =
            self.retain_postcondition_call(&candidate.instantiated, &candidate.available, prepared)
        else {
            return;
        };
        let proof =
            self.derivations
                .intern(super::state::DerivationNode::PostconditionDirectReceiver {
                    statement: statement.clone(),
                    binding: candidate.route.binding,
                    receiver_formal: candidate.route.formal,
                    relation: Box::new(candidate.instantiated.relation.clone()),
                    target_event,
                    parent: call,
                });
        let occurrence = self.s12_roots;
        self.s12_roots = self
            .s12_roots
            .checked_add(1)
            .expect("S12 roots exceed the u32 identity space");
        self.derivations.add_root(
            DerivationRootKind::PostconditionDirectReceiver { occurrence },
            proof,
        );
        state.establish_from_proof(&candidate.instantiated.relation, proof, &self.derivations);
    }

    fn prepare_direct_receiver(
        &mut self,
        separations: &SeparationLedger,
        route: DirectReceiverRoute,
        value: &CheckedExpression,
        prepared: &PreparedCall,
        target_events: &[KillEvent],
    ) -> Vec<DirectReceiverCandidate> {
        let CheckedExpression::UserCall {
            function,
            arguments,
            goal_arguments,
            ..
        } = value
        else {
            return Vec::new();
        };
        let Some(result_term) =
            self.postcondition_place_term(PlaceRoot::Binding(route.binding), &[], route.ty)
        else {
            return Vec::new();
        };
        prepared
            .postconditions
            .iter()
            .cloned()
            .filter_map(|available| {
                if available.variant.is_some()
                    || !available
                        .relation
                        .operands
                        .iter()
                        .any(|term| term.contains_result())
                {
                    return None;
                }
                let instantiated = self.instantiate_call_postcondition_relation(
                    *function,
                    &prepared.call,
                    &available.relation,
                    arguments,
                    goal_arguments,
                    &[Some(result_term)],
                    &[],
                )?;
                if instantiated
                    .substitutions
                    .iter()
                    .any(|substitution| substitution.formal == route.formal)
                    || !self.s12_substitutions_survive(
                        &prepared.entry_separations(separations),
                        &instantiated.substitutions,
                        &prepared.kills,
                        true,
                    )
                    || !self.s12_substitutions_survive(
                        separations,
                        &instantiated.substitutions,
                        target_events,
                        false,
                    )
                {
                    return None;
                }
                Some(DirectReceiverCandidate {
                    route,
                    available,
                    instantiated,
                })
            })
            .collect()
    }

    fn establish_direct_receiver(
        &mut self,
        statement: &crate::NodePath,
        candidate: &DirectReceiverCandidate,
        prepared: &PreparedCall,
        target_event: FlowEventId,
        states: &mut ProofFlowState,
    ) {
        self.retain_direct_receiver(
            statement,
            candidate,
            target_event,
            prepared,
            &mut states.facts,
        );
    }

    fn replace_relation_term(relation: &Relation, from: TermId, to: TermId) -> Relation {
        let replace = |term| if term == from { to } else { term };
        match relation {
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
                // Ordering the pair reverses the difference with it.
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
                        difference: -difference,
                    }
                }
            }
        }
    }

    fn retain_counted_derivations(&mut self, occurrence: u32, counted: CountedDerivationSet) {
        assert_eq!(
            occurrence, self.completed_counted_roots,
            "counted S11 groups must complete in statement-walk order"
        );
        let atoms = [
            (
                CountedRootAtom::LowerCaptureToEndpoint,
                counted.lower_capture_eq_endpoint.forward.parent,
            ),
            (
                CountedRootAtom::LowerEndpointToCapture,
                counted.lower_capture_eq_endpoint.reverse.parent,
            ),
            (
                CountedRootAtom::UpperCaptureToEndpoint,
                counted.upper_capture_eq_endpoint.forward.parent,
            ),
            (
                CountedRootAtom::UpperEndpointToCapture,
                counted.upper_capture_eq_endpoint.reverse.parent,
            ),
            (
                CountedRootAtom::BinderToLowerCapture,
                counted.binder_eq_lower_capture.forward.parent,
            ),
            (
                CountedRootAtom::LowerCaptureToBinder,
                counted.binder_eq_lower_capture.reverse.parent,
            ),
            (
                CountedRootAtom::LowerCaptureLeBinder,
                counted.lower_capture_le_binder.atomic.parent,
            ),
            (
                CountedRootAtom::BinderLtUpperCapture,
                counted.binder_lt_upper_capture.atomic.parent,
            ),
        ];
        for (atom, parent) in atoms {
            self.derivations
                .add_root(DerivationRootKind::CountedS11 { occurrence, atom }, parent);
        }
        self.counted_derivations.push(counted);
        self.completed_counted_roots = self
            .completed_counted_roots
            .checked_add(1)
            .expect("counted S11 root groups exceed the u32 identity space");
    }

    fn proof_event(
        &mut self,
        kind: FlowEventKind,
        node_path: Option<&crate::NodePath>,
    ) -> FlowEventId {
        self.derivations.event(kind, node_path.cloned())
    }

    fn expression_node_path(expression: &CheckedExpression) -> Option<&crate::NodePath> {
        expression.carrier()
    }

    // ------------------------------------------------------------------
    // Binding prepass
    // ------------------------------------------------------------------

    fn summary(&self, binding: BindingId) -> Option<&BindingSummary> {
        self.places.summary(binding)
    }

    fn collect_bindings(&mut self) {
        self.places = PlaceMap::for_function(self.function);
    }

    fn collect_postcondition_entry_images(&mut self) {
        let mut data = Vec::new();
        let mut relation_images = Vec::with_capacity(self.function.postconditions.len());
        for postcondition in &self.function.postconditions {
            let mut indices = Vec::new();
            for operand in &postcondition.relation.operands {
                let datum = match &operand.datum {
                    RelationDatum::Parameter {
                        ordinal,
                        projections,
                        ty,
                    } => Some((
                        PostconditionEntryImage {
                            parameter: *ordinal,
                            projections: projections.clone(),
                            measure: None,
                        },
                        *ty,
                    )),
                    RelationDatum::Measure(measure, place) => match place.root {
                        PostconditionPlaceRoot::Parameter { ordinal } => Some((
                            PostconditionEntryImage {
                                parameter: ordinal,
                                projections: place.projections.clone(),
                                measure: Some(*measure),
                            },
                            place.ty,
                        )),
                        // A result place is not a parameter entry image.
                        PostconditionPlaceRoot::Result { .. }
                        | PostconditionPlaceRoot::ExitParameter { .. } => None,
                    },
                    RelationDatum::Result { .. }
                    | RelationDatum::NamedConst { .. }
                    | RelationDatum::Literal { .. } => None,
                };
                if let Some(datum) = datum {
                    let index = data
                        .iter()
                        .position(|existing: &(PostconditionEntryImage, CheckedType)| {
                            existing.0 == datum.0
                        })
                        .unwrap_or_else(|| {
                            let index = data.len();
                            data.push(datum);
                            index
                        });
                    if !indices.contains(&index) {
                        indices.push(index);
                    }
                }
            }
            relation_images.push(indices);
        }
        self.entry_images = data
            .into_iter()
            .map(|(datum, ty)| {
                let parameter = self
                    .function
                    .parameters
                    .get(datum.parameter as usize)
                    .expect("checked postcondition parameter ordinal must resolve");
                let projections = self
                    .body_projections(PlaceRoot::Binding(parameter.binding), &datum.projections)
                    .to_vec();
                let support = GoalSupport {
                    root: parameter.binding,
                    projections,
                    measure: datum.measure,
                };
                let (place, holders) = self.resolve_goal_support(&support);
                EntryImageRecord {
                    datum,
                    ty,
                    place,
                    holders,
                }
            })
            .collect();
        self.postcondition_entry_images = relation_images;
    }

    /// [MSR-3] the entry placement: at body entry, per parameter of measured
    /// type and per measure any declared relation names, one compiler-owned
    /// immutable datum established equal to that measure.
    ///
    /// The datum contains no place, so no [ENT-5] event kills it. That is
    /// what makes an `ensures` naming an `own` parameter's measure denote the
    /// entry value even where the body writes that parameter back with a
    /// [LIV-2] `set`, and it is the callee-side half of the denotation
    /// [MSR-3]'s table gives the same operand at a caller.
    fn establish_entry_datums(&mut self, state: &mut ProofFlowState) {
        if self.entry_images.is_empty() {
            return;
        }
        let event = self.proof_event(FlowEventKind::Entry, None);
        for index in 0..self.entry_images.len() {
            let image = self.entry_images[index].datum.clone();
            let ty = self.entry_images[index].ty;
            let Some(measure) = image.measure else {
                continue;
            };
            let Some(parameter) = self.function.parameters.get(image.parameter as usize) else {
                continue;
            };
            let binding = parameter.binding;
            let projections =
                self.body_projections(PlaceRoot::Binding(binding), &image.projections);
            let Some(live) = self.postcondition_measure_term(
                measure,
                PlaceRoot::Binding(binding),
                projections,
                ty,
                self.formal_is_range(image.parameter),
            ) else {
                continue;
            };
            let datum = self.terms.intern(Self::entry_datum_kind(
                image.parameter,
                &image.projections,
                measure,
            ));
            self.adopt_measure_atom(datum, live, &state.affine);
            state.facts.establish(
                &Relation::Equal {
                    left: datum,
                    right: live,
                    difference: 0,
                },
                &mut self.derivations,
                event,
            );
        }
    }

    /// [MSR-3] the identity of one entry datum: the formal ordinal, the
    /// operand's ordered projections, and which [MSR-1] measure of it the
    /// datum denotes.
    fn entry_datum_kind(
        formal: u32,
        projections: &[GoalProjection],
        measure: CheckedMeasure,
    ) -> TermKind {
        TermKind::EntryDatum {
            formal,
            projections: projections
                .iter()
                .map(|projection| projection.place_step())
                .collect(),
            measure,
        }
    }

    /// [REF-1] a reference variable is not storage of its own.
    ///
    /// v0.59 spelled a holder's referent by inserting a `deref` step into
    /// every term over it. v0.60 resolves the root instead: a place rooted at
    /// a reference variable is replaced by the path that reference names, so
    /// no step is synthesized here and the checked tree's own `deref` nodes
    /// are the only ones a path carries [TYPE-7].
    const fn is_holder(&self, _binding: BindingId) -> bool {
        false
    }

    /// [REF-1] the body-side reading of one clause place's projections.
    ///
    /// A clause names a reference parameter's referent as `deref(p)`, and the
    /// declaration-boundary template keeps that step as its leading
    /// projection because a caller substitutes the actual's own path for the
    /// formal and consumes exactly it [FN-8, CALL-6]. Inside the body the
    /// parameter name *is* the path, so the step names no place of its own
    /// and is dropped: that is the identity every other term over `deref(p)`
    /// already carries, `is_holder` above having synthesized none.
    fn body_projections<'projections>(
        &self,
        root: PlaceRoot,
        projections: &'projections [GoalProjection],
    ) -> &'projections [GoalProjection] {
        let PlaceRoot::Binding(binding) = root else {
            return projections;
        };
        if !self.places.is_reference(binding) {
            return projections;
        }
        match projections.split_first() {
            Some((GoalProjection::Deref, rest)) => rest,
            _ => projections,
        }
    }

    // ------------------------------------------------------------------
    // Place resolution and support
    // ------------------------------------------------------------------

    /// The [REF-1] resolved place one term's path names, which replaces a
    /// reference-variable root by the path that reference names.
    ///
    /// A join may give a reference variable more than one path [REF-1]; a
    /// term may substitute an origin only when it is unique. Otherwise its
    /// reference-local identity denotes the selected referent, without
    /// asserting equality to any one possible origin. Kill judgments must
    /// still consider every origin through `resolved_places_overlap`.
    fn resolve(&self, place: &ResolvedPlace) -> ResolvedPlace {
        let candidates = self.places.resolve(place.root, &place.path);
        match candidates.as_slice() {
            [unique] => unique.clone(),
            _ => place.clone(),
        }
    }

    /// [REF-1, ENT-5] a write can invalidate a fact through any member of
    /// either path set. Taking only the first member loses real writes at
    /// a control-flow join.
    fn resolved_places_overlap(
        &self,
        separations: &dyn SeparationOracle,
        left: &ResolvedPlace,
        right: &ResolvedPlace,
    ) -> bool {
        let lefts = self.places.resolve(left.root, &left.path);
        let rights = self.places.resolve(right.root, &right.path);
        if lefts.is_empty() || rights.is_empty() {
            // The place prepass represents an unresolved reference with no
            // candidates. It is unknown storage, not proof of separation.
            return true;
        }
        lefts.iter().any(|left| {
            rights
                .iter()
                .any(|right| self.places.overlaps(separations, left, right))
        })
    }

    /// Whether a kill event kills a fact supported by `term` [ENT-5].
    fn event_kills_term(
        &self,
        separations: &dyn SeparationOracle,
        term: TermId,
        event: &KillEvent,
    ) -> bool {
        match self.terms.kind(term) {
            TermKind::Zero | TermKind::Constant(_) | TermKind::ConstParameter(..) => false,
            // Counted captures and commit values are immutable. A counted
            // capture dies with its construct-scope exit, handled separately
            // from source-place write/consume events; a commit value names one
            // evaluated value that no later event can change.
            TermKind::CountedCapture { .. }
            | TermKind::IndexCapture { .. }
            | TermKind::ResultPayload(_)
            | TermKind::CommitValue { .. }
            | TermKind::CallDatum { .. }
            | TermKind::EntryDatum { .. }
            | TermKind::MeasureDatum { .. } => false,
            // [ENT-5] a clause (b) place's written offsets are part of its
            // support, so a write to one kills the term exactly as it kills a
            // measure over the same place. A clause (a) place has none.
            TermKind::Place(place, _) => {
                (match event {
                    KillEvent::Write { place: written, .. }
                    | KillEvent::EntryImageHolderWrite { place: written, .. } => {
                        self.resolved_places_overlap(separations, place, written)
                    }
                    KillEvent::Consume { binding, .. } => {
                        place.root == PlaceRoot::Binding(*binding)
                    }
                    KillEvent::EntryImageHolderConsume { .. } => false,
                }) || self.event_kills_offset_support(separations, place, event)
            }
            // [MSR-2] a measure term's support is its place's DESCRIPTOR
            // storage, which is the resolved place of P itself and not of
            // P's root: a write to a sibling field of P overlaps neither.
            TermKind::Measure(measure, place) => {
                let support = self.resolve(place);
                self.event_kills_measure(separations, *measure, &support, place.root, event)
                    || self.event_kills_offset_support(separations, &support, event)
            }
        }
    }

    /// [MSR-2] whether one event kills a measure of `support`.
    ///
    /// A write at an element position of P carries the written element's own
    /// place, `P[i]`, so it overlaps the descriptor storage of `P[i]` and
    /// none of P's own: it kills every measure of `P[i]` and no measure of P.
    /// A write of a whole value writes everything under it, so it kills the
    /// measures of every place it reaches. Two subscripts of one base are the
    /// same step of that reach unless their offsets are provably distinct
    /// [OWN-7].
    ///
    /// A written place that names a measure word of P -- `writes(window.len)`
    /// in an [OP-10] row, which [WIN-2] states "is itself a write target and
    /// overlaps no slot" -- does not contain P, and the containment above
    /// alone would let the pre-call relations of P survive the operation that
    /// changes them. Such a write reaches exactly the word it names, because
    /// [EFF-2]'s both-ways check is what admits the row: a body that changed
    /// another word of P's descriptor would exhibit a write the row does not
    /// carry. It reaches no element storage either, so the measures of an
    /// element of P survive it.
    fn event_kills_measure(
        &self,
        separations: &dyn SeparationOracle,
        measure: CheckedMeasure,
        support: &ResolvedPlace,
        root: PlaceRoot,
        event: &KillEvent,
    ) -> bool {
        match event {
            KillEvent::Write {
                place: written,
                element,
                ..
            }
            | KillEvent::EntryImageHolderWrite {
                place: written,
                element,
                ..
            } => self.write_overlaps_measure(separations, written, support, measure, *element),
            KillEvent::Consume { binding, .. } => root == PlaceRoot::Binding(*binding),
            KillEvent::EntryImageHolderConsume { .. } => false,
        }
    }

    /// [ENT-5, MSR-2] a measure's mutable support is its descriptor word.
    /// Whole-value writes overlap it, element writes below it do not, and
    /// an uncertain index may select the measured element. Definite path
    /// identity is insufficient here: failure to prove two captures equal
    /// must never preserve a fact about storage the write may replace.
    fn write_overlaps_measure(
        &self,
        separations: &dyn SeparationOracle,
        written: &ResolvedPlace,
        support: &ResolvedPlace,
        measure: CheckedMeasure,
        element_write: bool,
    ) -> bool {
        // [REF-4, MSR-2] a range reference's `len` is the immutable
        // descriptor value captured at formation. An element write through
        // any range view and a window-part or descriptor-word write on its
        // backing window therefore leave it unchanged. The ordinary path
        // walk cannot discover this after two unequal range steps:
        // conservative range overlap stops it before the final
        // Index/Measure or Range/Part pair. A measured element has an Index
        // after its range step and therefore does not take this case.
        //
        // Both classifiers require complete, nonempty resolution. A holder
        // rebind is a whole-place write with no Part/Measure suffix, while an
        // unresolved write remains conservative and reaches the descriptor.
        if self.is_range_descriptor_support(support)
            && (element_write || self.is_window_extent_write(written))
        {
            return false;
        }
        let mut descriptor = support.clone();
        descriptor.path.push(PlaceStep::Measure(measure));
        // The fact's own place goes first: [WIN-2]'s liveness is a question
        // about the window above its index [`Self::event_live_indices`].
        self.resolved_places_overlap(separations, &descriptor, written)
    }

    /// Whether `support` is the range value itself, rather than an element
    /// selected from it. A unique resolved origin ends in its captured range
    /// step; a joined reference may have several such origins.
    fn is_range_descriptor_support(&self, support: &ResolvedPlace) -> bool {
        let resolved = self.places.resolve(support.root, &support.path);
        !resolved.is_empty()
            && resolved
                .iter()
                .all(|place| matches!(place.path.last(), Some(PlaceStep::Range(_))))
    }

    /// Whether every resolved write reaches one named part or descriptor
    /// word of a window. These suffixes are the exact [WIN-2] effect-row
    /// targets; a field, holder, or whole-owner replacement does not qualify.
    fn is_window_extent_write(&self, written: &ResolvedPlace) -> bool {
        let resolved = self.places.resolve(written.root, &written.path);
        !resolved.is_empty()
            && resolved.iter().all(|place| {
                matches!(
                    place.path.last(),
                    Some(PlaceStep::Part(_) | PlaceStep::Measure(_))
                )
            })
    }

    /// [ENT-5] whether one event writes or consumes a binding an offset
    /// occurring in `support` reads.
    ///
    /// The support of a measure term over P contains the support of every
    /// offset occurring in P, so a write to that offset's own binding kills
    /// the measure at every level it occurs in.
    fn event_kills_offset_support(
        &self,
        separations: &dyn SeparationOracle,
        support: &ResolvedPlace,
        event: &KillEvent,
    ) -> bool {
        support.path.iter().any(|step| {
            let PlaceStep::Index(offset) = step else {
                return false;
            };
            let Some(binding) = offset.support() else {
                return false;
            };
            let read = ResolvedPlace::binding(binding);
            match event {
                KillEvent::Write { place: written, .. }
                | KillEvent::EntryImageHolderWrite { place: written, .. } => {
                    self.resolved_places_overlap(separations, &read, written)
                }
                KillEvent::Consume {
                    binding: consumed, ..
                } => binding == *consumed,
                KillEvent::EntryImageHolderConsume { .. } => false,
            }
        })
    }

    /// Whether leaving the scopes of `exited` kills a fact supported by
    /// `term`: the support contains every tracked place's root binding and
    /// every holder read through, which is the spelling root here.
    fn scope_kills_term(&self, term: TermId, exited: &HashSet<BindingId>) -> bool {
        match self.terms.kind(term) {
            TermKind::Zero | TermKind::Constant(_) | TermKind::ConstParameter(..) => false,
            TermKind::CountedCapture { .. }
            | TermKind::IndexCapture { .. }
            | TermKind::ResultPayload(_)
            | TermKind::CommitValue { .. }
            | TermKind::CallDatum { .. }
            | TermKind::EntryDatum { .. }
            | TermKind::MeasureDatum { .. } => false,
            // [ENT-5] the support of every offset occurring in the place is
            // part of the term's own support, so a term dies with the scope
            // of an offset's binding exactly as it dies with its root's.
            TermKind::Place(place, _) | TermKind::Measure(_, place) => {
                let rooted = match place.root {
                    PlaceRoot::Binding(binding) => exited.contains(&binding),
                    PlaceRoot::Constant(_) => false,
                };
                rooted
                    || place.path.iter().any(|projection| {
                        matches!(projection, PlaceStep::Index(offset)
                            if offset.support().is_some_and(|binding| exited.contains(&binding)))
                    })
            }
        }
    }

    fn resolve_goal_support(&self, support: &GoalSupport) -> (ResolvedPlace, Vec<BindingId>) {
        let mut resolved = ResolvedPlace {
            root: PlaceRoot::Binding(support.root),
            path: Vec::new(),
        };
        // Goal support is already in body-place form. A reference root is the
        // holder of that place, while every projection is a real step below
        // its referent and must remain part of the support.
        let holders = self
            .places
            .is_reference(support.root)
            .then_some(support.root)
            .into_iter()
            .collect::<Vec<_>>();
        resolved.path.extend(
            support
                .projections
                .iter()
                .map(|projection| projection.place_step()),
        );
        (resolved, holders)
    }

    fn event_kills_goal(
        &self,
        separations: &dyn SeparationOracle,
        goal: GoalId,
        event: &KillEvent,
    ) -> bool {
        self.goals.support(goal).iter().any(|support| {
            let (place, holders) = self.resolve_goal_support(support);
            // [ENT-5, MSR-2] a current place reads every named offset as
            // well as the selected storage. Goals and L0 terms must lose
            // that place's identity on the same offset event.
            if self.event_kills_offset_support(separations, &place, event) {
                return true;
            }
            match event {
                // [MSR-2] a write at an element position carries the written
                // element's own place, `P[i]`, so it reaches the descriptor
                // storage of `P[i]` and none of P's own. A measure goal over
                // a place the written place is a prefix of therefore dies
                // and one over P does not, which is the same sentence
                // `event_kills_measure` reads for an L0 measure term. The
                // blanket "an element write kills no measure goal" this
                // replaces was [ENT-5]'s element-position carve-out, which
                // was only ever true of a table with no measured element
                // type.
                KillEvent::Write {
                    place: written,
                    element,
                    ..
                }
                | KillEvent::EntryImageHolderWrite {
                    place: written,
                    element,
                    ..
                } if support.measure.is_some() => {
                    // [MSR-2, WIN-2] a row naming a measure word of P --
                    // `writes(window.len)` in an [OP-10] row -- writes exactly
                    // that word of P's own descriptor, which contains no
                    // storage the place test above reaches. The L0 measure
                    // term reads the same two sentences in
                    // `event_kills_measure`, and a goal over the same measure
                    // must die on the same event: otherwise the goal-level
                    // reading of `deref(p).len` survives the operation that
                    // changed it and stands beside the row's own post-state
                    // relation as a contradiction.
                    support.measure.is_some_and(|measure| {
                        self.write_overlaps_measure(separations, written, &place, measure, *element)
                    })
                }
                KillEvent::Write { place: written, .. }
                | KillEvent::EntryImageHolderWrite { place: written, .. } => {
                    self.resolved_places_overlap(separations, &place, written)
                }
                KillEvent::Consume { binding, .. } => {
                    holders.contains(binding) || place.root == PlaceRoot::Binding(*binding)
                }
                KillEvent::EntryImageHolderConsume { .. } => false,
            }
        })
    }

    /// An ordinary-let origin is available only while the binding whose
    /// initializer it describes has not itself been written or consumed.
    /// This key guard is separate from the goal's value support: invalidating
    /// it stops future alias expansion without erasing a signed snapshot fact
    /// that an earlier branch already established.
    fn event_kills_goal_origin_binding(
        &self,
        separations: &dyn SeparationOracle,
        binding: BindingId,
        event: &KillEvent,
    ) -> bool {
        match event {
            KillEvent::Write { place, .. } | KillEvent::EntryImageHolderWrite { place, .. } => {
                self.resolved_places_overlap(separations, &ResolvedPlace::binding(binding), place)
            }
            KillEvent::Consume {
                binding: consumed, ..
            } => binding == *consumed,
            KillEvent::EntryImageHolderConsume { .. } => false,
        }
    }

    fn scope_kills_goal(&self, goal: GoalId, exited: &HashSet<BindingId>) -> bool {
        self.goals.support(goal).iter().any(|support| {
            let (place, holders) = self.resolve_goal_support(support);
            holders.iter().any(|holder| exited.contains(holder))
                || matches!(place.root, PlaceRoot::Binding(binding) if exited.contains(&binding))
                || place.path.iter().any(|projection| {
                    matches!(projection, PlaceStep::Index(offset)
                        if offset.support().is_some_and(|binding| exited.contains(&binding)))
                })
        })
    }

    /// Contradiction is absorbing. Promote the complete combined closure
    /// before every kill entry so a write cannot erase one premise and make
    /// an unreachable point reachable again.
    fn promote_contradiction(&mut self, state: &mut FactState) {
        if state.all_derivable {
            return;
        }
        // A seeded closure costs about what the proof-free probe does over a
        // closed core and answers the same question directly; the probe stays
        // the cheaper test for a state with no closed part.
        if !closure_is_seeded(state)
            && !contradiction_without_proofs(state, &self.terms, &self.goals)
        {
            return;
        }
        let closed = close(state, &self.terms, &self.goals, &mut self.derivations);
        if closed.contradictory() {
            state.promote_to_contradiction(closed.contradiction_proof());
        }
    }

    fn promote_flow_contradiction(&mut self, states: &mut ProofFlowState) {
        self.promote_contradiction(&mut states.facts);
    }

    /// Materializes the complete [ENT-4] closure before an event-kill batch.
    ///
    /// The existing event predicates remain the sole authority for which
    /// terms, goals, and origins die. Materialization only makes every
    /// survivor-to-survivor consequence independently live before one of its
    /// supporting endpoints disappears.
    fn materialize_before_event_kill(&mut self, state: &mut FactState, events: &[KillEvent]) {
        if events.is_empty() {
            return;
        }
        materialize_closure_before_kill(state, &self.terms, &self.goals, &mut self.derivations);
    }

    fn apply_kills_one(
        &mut self,
        separations: &dyn SeparationOracle,
        state: &mut FactState,
        events: &[KillEvent],
    ) {
        if events.is_empty() {
            return;
        }
        self.materialize_before_event_kill(state, events);
        state.kill(|term| {
            events
                .iter()
                .any(|event| self.event_kills_term(separations, term, event))
        });
        for event in events {
            self.kill_s12_candidates_for_event(separations, state, event);
        }
        state.kill_goals(|goal| {
            events
                .iter()
                .any(|event| self.event_kills_goal(separations, goal, event))
        });
        state.goal_origins.retain(|binding, _| {
            !events
                .iter()
                .any(|event| self.event_kills_goal_origin_binding(separations, *binding, event))
        });
        state.ambiguous_goal_origins.retain(|binding| {
            !events
                .iter()
                .any(|event| self.event_kills_goal_origin_binding(separations, *binding, event))
        });
    }

    fn apply_kills(&mut self, states: &mut ProofFlowState, events: &[KillEvent]) {
        if events.is_empty() {
            return;
        }
        self.record_continuing(states, events);
        self.promote_flow_contradiction(states);
        let ledger = states.separations.clone();
        let live = self.event_live_indices(states, events);
        let separations = EventSeparations {
            ledger: &ledger,
            live: &live,
        };
        self.kill_result_evidence(states, events);
        self.kill_path_components(&separations, states, events, None);
    }

    /// One batch of kill events on the path components after the Result
    /// states: the facts, the affine images and the entry images, in that
    /// order, and the path's record of written bindings [DIAG-1]. Every kill
    /// transfer but a loop head's applies its events through here, so a new
    /// path component joins every one of them at once;
    /// [`Self::apply_loop_kills`] applies the same components, the written
    /// record included, from its summary.
    fn kill_path_components(
        &mut self,
        separations: &dyn SeparationOracle,
        states: &mut ProofFlowState,
        events: &[KillEvent],
        shared_event: Option<FlowEventId>,
    ) {
        self.apply_kills_one(separations, &mut states.facts, events);
        self.apply_affine_kills(separations, &mut states.affine, events);
        self.invalidate_entry_images(states, separations, events, shared_event);
        states.record_writes(events);
    }

    /// Applies each event on its own and in order, invalidating entry images
    /// under the transfer event `transfer_event` mints for it before it
    /// applies: the form a postcondition-carrying call or receiver write
    /// needs, where [`Self::apply_kills`] applies one batch. Returns the
    /// indices the events' entry state proves live [WIN-2].
    fn apply_kills_each(
        &mut self,
        states: &mut ProofFlowState,
        events: &[KillEvent],
        mut transfer_event: impl FnMut(&mut Self, &KillEvent) -> FlowEventId,
    ) -> LiveIndices {
        if !events.is_empty() {
            self.promote_flow_contradiction(states);
        }
        self.record_continuing(states, events);
        let ledger = states.separations.clone();
        let live = self.event_live_indices(states, events);
        let separations = EventSeparations {
            ledger: &ledger,
            live: &live,
        };
        self.kill_result_evidence(states, events);
        for event in events {
            let proof_event = transfer_event(self, event);
            self.kill_path_components(
                &separations,
                states,
                std::slice::from_ref(event),
                Some(proof_event),
            );
        }
        live
    }

    /// Keeps each event applied on a path inside a loop, where debug
    /// assertions are on, for the back-edge check against that loop's summary.
    fn record_continuing(&self, states: &mut ProofFlowState, events: &[KillEvent]) {
        if cfg!(debug_assertions) && !self.loops.is_empty() {
            record_continuing(&mut states.continuing, events);
        }
    }

    /// [ENT-5] a path reaching a loop's back edge applied only events the
    /// loop's summary subtracted at its head; an event the summary misses
    /// would leave a stale fact at the head.
    fn debug_assert_summarized(state: &ProofFlowState, kills: &LoopKills) {
        debug_assert!(
            state
                .continuing
                .iter()
                .all(|event| kills.events.contains(event)),
            "[ENT-5] a continuing kill is missing from its loop summary: {:?}",
            state
                .continuing
                .iter()
                .find(|event| !kills.events.contains(event))
        );
    }

    /// [WIN-2, ENT-5] the indices the entry state of `events` proves live.
    ///
    /// Only an index directly below a window whose `next`, `free` or `last`
    /// one of the events writes is asked about, through the place that holds
    /// it: a term's, a goal's or an entry image's own path, whose prefix
    /// above the index is the window. The bound `i < r.len` is the one
    /// [OP-4] owed where the subscript was formed, judged again here over the
    /// same terms. A prefix that resolves to more than one place names no one
    /// window whose length a proof bounds, so its index stays unproved.
    fn event_live_indices(&mut self, states: &ProofFlowState, events: &[KillEvent]) -> LiveIndices {
        let mut written_parts = HashSet::new();
        for event in events {
            let (KillEvent::Write { place, .. } | KillEvent::EntryImageHolderWrite { place, .. }) =
                event
            else {
                continue;
            };
            for written in self.places.resolve(place.root, &place.path) {
                for (depth, step) in written.path.iter().enumerate() {
                    if matches!(
                        step,
                        PlaceStep::Part(WindowPart::Next | WindowPart::Free | WindowPart::Last)
                    ) {
                        written_parts.insert((written.root, depth));
                    }
                }
            }
        }
        let mut live = LiveIndices::new();
        if written_parts.is_empty() {
            return live;
        }
        let mut places = Vec::new();
        for term in self.terms.ids() {
            if let TermKind::Place(place, _) | TermKind::Measure(_, place) = self.terms.kind(term) {
                places.push(place.clone());
            }
        }
        for goal in self.goals.ids() {
            for support in self.goals.support(goal) {
                places.push(self.resolve_goal_support(support).0);
            }
        }
        places.extend(self.entry_images.iter().map(|image| image.place.clone()));
        let mut asked = HashSet::new();
        for place in places {
            for (depth, step) in place.path.iter().enumerate() {
                let PlaceStep::Index(index) = *step else {
                    continue;
                };
                let window = ResolvedPlace {
                    root: place.root,
                    path: place.path[..depth].to_vec(),
                };
                if !asked.insert((window.clone(), index)) {
                    continue;
                }
                let resolved = self.places.resolve(window.root, &window.path);
                let [resolved] = resolved.as_slice() else {
                    continue;
                };
                if written_parts.contains(&(resolved.root, resolved.path.len()))
                    && self.index_live_proof(&window, index, states).is_some()
                {
                    live.insert((resolved.clone(), index));
                }
            }
        }
        live
    }

    /// [WIN-2] the proof `states` gives of `index < len(window)`: the bound
    /// a subscript's [OP-4] obligation states where its place is formed, over
    /// the same length term and the same offset.
    fn index_live_proof(
        &mut self,
        window: &ResolvedPlace,
        index: CapturedValue,
        states: &ProofFlowState,
    ) -> Option<ProofResult> {
        let length = self
            .terms
            .interned(&TermKind::Measure(CheckedMeasure::Length, window.clone()))?;
        let offset = match (index.capture, index.term) {
            // A place's term identity reads a binding offset by its spelling,
            // whose current value is the binding's own term [ENT-2].
            (CaptureId::SpellingDetermined, CapturedTerm::Binding(binding)) => self.terms.interned(
                &TermKind::Place(ResolvedPlace::binding(binding), IntegerType::U64),
            ),
            _ => self.captured_index_term(index),
        }?;
        let affine_offset = match index.capture {
            CaptureId::Source(_) => states.affine.indices.get(&index.capture).cloned(),
            _ => None,
        }
        .or_else(|| self.affine_term_value(offset, &states.affine));
        let direct_affine = affine_offset.as_ref().and_then(|offset| {
            let length = self.measure_atom(length, &states.affine);
            AffineInequality::from_bounded_forms(offset, &length, -1, &mut AffineCheckState::new())
                .ok()
        });
        let proof = self.prove(
            ProofContext::new(&states.facts, &states.affine),
            ProofGoal::BoundedRelation(BoundedRelationGoal {
                canonical: None,
                request: Some(BoundsRequest {
                    left: Some(offset),
                    right: length,
                    bound: -1,
                    distinct: false,
                }),
                direct_affine: direct_affine.as_ref(),
                fixed_affine_bridge: None,
                affine_left: affine_offset.as_ref(),
            }),
        );
        (proof.disposition == ProofDisposition::Proved).then_some(proof)
    }

    fn event_kills_entry_image(
        &self,
        separations: &dyn SeparationOracle,
        image: &EntryImageRecord,
        event: &KillEvent,
    ) -> bool {
        match event {
            KillEvent::Write { element: true, .. }
            | KillEvent::EntryImageHolderWrite { element: true, .. }
                if image.datum.measure.is_some() =>
            {
                false
            }
            KillEvent::Write { place, .. } | KillEvent::EntryImageHolderWrite { place, .. } => {
                self.resolved_places_overlap(separations, &image.place, place)
            }
            KillEvent::Consume { binding, .. }
            | KillEvent::EntryImageHolderConsume { binding, .. } => {
                image.holders.contains(binding) || image.place.root == PlaceRoot::Binding(*binding)
            }
        }
    }

    fn invalidate_entry_images(
        &mut self,
        states: &mut ProofFlowState,
        separations: &dyn SeparationOracle,
        events: &[KillEvent],
        shared_event: Option<FlowEventId>,
    ) {
        if self.entry_images.is_empty() {
            return;
        }
        for event in events {
            let killed = self
                .entry_images
                .iter()
                .enumerate()
                .filter_map(|(index, image)| {
                    // [MSR-3] a measure operand denotes the entry datum, and
                    // no [ENT-5] event kills a datum. Only a non-measure
                    // operand still reads the live place and can lose it.
                    (image.datum.measure.is_none()
                        && states.entry_images[index].is_none()
                        && self.event_kills_entry_image(separations, image, event))
                    .then_some(index)
                })
                .collect::<Vec<_>>();
            if killed.is_empty() {
                continue;
            }
            let invalidation = shared_event.unwrap_or_else(|| {
                self.proof_event(
                    FlowEventKind::PostconditionEntryImageInvalidation,
                    Some(event.source()),
                )
            });
            for index in killed {
                states.entry_images[index] = Some(invalidation);
            }
        }
    }

    /// Applies the scope-exit kills for every scope deeper than `depth`,
    /// as the edge event ordered before any join [ENT-5].
    fn exit_scopes_to_one(&mut self, state: &mut FactState, depth: usize) {
        let exited: HashSet<BindingId> =
            self.scopes.iter().skip(depth).flatten().copied().collect();
        if exited.is_empty() {
            return;
        }
        state.kill(|term| self.scope_kills_term(term, &exited));
        self.kill_s12_candidates_for_scope(state, &exited);
        state.kill_goals(|goal| self.scope_kills_goal(goal, &exited));
        state.origins.retain(|binding, _| !exited.contains(binding));
        state
            .goal_origins
            .retain(|binding, _| !exited.contains(binding));
        state
            .ambiguous_goal_origins
            .retain(|binding| !exited.contains(binding));
    }

    /// Applies only the lexical support kills. Delivery images call this
    /// directly because they were already constructed from a closed source
    /// state and must retain only their explicit PostconditionGive roots.
    fn kill_scopes_to(&mut self, states: &mut ProofFlowState, depth: usize) {
        self.promote_flow_contradiction(states);
        self.exit_scope_components(states, depth);
    }

    /// The scope-exit kills for every scope deeper than `depth` on each path
    /// component: the Result states, the facts and the affine images, in that
    /// order. Both scope transfers apply them through here.
    fn exit_scope_components(&mut self, states: &mut ProofFlowState, depth: usize) {
        self.exit_result_scopes(states, depth);
        self.exit_scopes_to_one(&mut states.facts, depth);
        self.exit_affine_scopes_to(&mut states.affine, depth);
    }

    fn exit_affine_scopes_to(&mut self, state: &mut AffineFlowState, depth: usize) {
        let exited = self
            .scopes
            .iter()
            .skip(depth)
            .flatten()
            .copied()
            .collect::<HashSet<_>>();
        state.values.retain(|binding, _| !exited.contains(binding));
        // A measure of a place rooted in an exited binding has no image past
        // that edge, exactly as the binding itself has none, so the next
        // occurrence of that term mints a fresh atom [MSR-2].
        let stale: Vec<TermId> = state
            .measure_atoms
            .borrow()
            .keys()
            .copied()
            .filter(|term| {
                self.measure_term_root(*term)
                    .is_some_and(|binding| exited.contains(&binding))
            })
            .collect();
        for term in stale {
            state.measure_atoms.get_mut().remove(&term);
        }
    }

    fn exit_scopes_to(&mut self, states: &mut ProofFlowState, depth: usize) {
        let has_exited_bindings = self
            .scopes
            .iter()
            .skip(depth)
            .any(|scope| !scope.is_empty());
        if !has_exited_bindings {
            return;
        }
        // [ENT-4, ENT-5]: a local term may be the middle vertex of a proof
        // whose conclusion names only values that remain live. Fix the least
        // closure while that vertex still exists, then let the ordinary scope
        // kill remove every materialized fact whose own support still names
        // the exiting scope.
        let snapshot = self.derivations.event(FlowEventKind::Snapshot, None);
        states.facts = materialize_closure_at(
            &states.facts,
            &self.terms,
            &self.goals,
            &mut self.derivations,
            snapshot,
        );
        // Materialization has already promoted any relation or goal
        // contradiction. Apply only the endpoint projection here.
        self.exit_scope_components(states, depth);
    }

    /// Applies the private capture-scope kill of one counted construct.
    fn exit_counted_capture_scope_one(&mut self, state: &mut FactState, range_path: &[u32]) {
        state.kill(|term| {
            matches!(
                self.terms.kind(term),
                TermKind::CountedCapture { range_path: path, .. } if path == range_path
            )
        });
    }

    fn exit_counted_capture_scope(&mut self, states: &mut ProofFlowState, range_path: &[u32]) {
        self.promote_flow_contradiction(states);
        for result in states.results.values_mut() {
            self.refresh_result(result, &states.facts);
            materialize_closure_before_kill(
                &mut result.facts,
                &self.terms,
                &self.goals,
                &mut self.derivations,
            );
            self.exit_counted_capture_scope_one(&mut result.facts, range_path);
        }
        self.exit_counted_capture_scope_one(&mut states.facts, range_path);
    }

    fn remove_active_loop_invariants(
        state: &mut AffineFlowState,
        loop_id: CheckedLoopId,
        declarations: &[crate::DeclarationId],
    ) {
        state
            .facts
            .retain(|fact| !fact.active_loops.contains(&loop_id));
        for declaration in declarations {
            state.published_invariants.remove(declaration);
        }
    }

    /// Returns the one deterministic premise traversal used by every affine
    /// consumer: insertion order with later occurrences of the same canonical
    /// inequality removed, regardless of evidence category.
    fn canonical_affine_facts(facts: &[ActiveAffineFact]) -> Vec<&ActiveAffineFact> {
        let mut seen = HashSet::new();
        facts
            .iter()
            .filter(|fact| seen.insert(fact.inequality.clone()))
            .collect()
    }

    fn affine_facts(state: &AffineFlowState) -> Vec<ActiveAffineFact> {
        Self::canonical_affine_facts(&state.facts)
            .into_iter()
            .cloned()
            .collect()
    }

    fn affine_fact_uses_only_outer_values(
        &self,
        inequality: &AffineInequality,
        state: &AffineFlowState,
        binder: BindingId,
    ) -> bool {
        let mut live_terms = state
            .values
            .iter()
            .filter(|(binding, _)| **binding != binder)
            .flat_map(|(_, value)| value.terms().iter().map(|coefficient| coefficient.term()))
            .collect::<HashSet<_>>();
        // [MSR-1, MSR-4] a measure of a place live at the continuation is an
        // outer value exactly as an integer binding is: it has one atom, that
        // atom is retargeted only by the events that kill the term [MSR-2],
        // and a conclusion over it therefore says the same thing after the
        // loop that it said inside. Without this every filling loop's exit
        // exports nothing and the `ensures` its body was written for is
        // unproved at the return.
        live_terms.extend(
            state
                .measure_atoms
                .borrow()
                .values()
                .flat_map(|value| value.terms().iter().map(|coefficient| coefficient.term())),
        );
        inequality
            .terms()
            .iter()
            .all(|coefficient| live_terms.contains(&coefficient.term()))
    }

    /// Applies capture-scope kills for every loop frame crossed by a
    /// non-local edge. Ordinary loop frames carry no private captures.
    fn exit_counted_loops_from(&mut self, states: &mut ProofFlowState, loop_depth: usize) {
        let loops = self
            .loops
            .iter()
            .skip(loop_depth)
            .map(|frame| {
                (
                    frame.id,
                    frame.capture_path.clone(),
                    frame.invariant_declarations.clone(),
                )
            })
            .collect::<Vec<_>>();
        for (loop_id, path, declarations) in loops {
            if let Some(path) = path {
                self.exit_counted_capture_scope(states, &path);
            }
            Self::remove_active_loop_invariants(&mut states.affine, loop_id, &declarations);
        }
    }

    // ------------------------------------------------------------------
    // Terms and relations from checked expressions
    // ------------------------------------------------------------------

    /// CONST-2's total field selection substitutes the immutable initializer.
    /// Preserve that known scalar while reads still carry the constant's
    /// actual storage identity. A subscript remains an OP-4 operation rather
    /// than adding a new automatic constant-evaluation family here.
    fn constant_storage_scalar(&self, expression: &CheckedExpression) -> Option<&CheckedValue> {
        let CheckedExpression::ReadStorage { root, .. } = expression else {
            return None;
        };
        let PlaceRoot::Constant(id) = root.root else {
            return None;
        };
        let declaration = self.context.constant_declaration(id)?;
        let mut value = &self.context.constant(declaration)?.value;
        for step in &root.path {
            let CheckedPlaceStep::Field(index) = step else {
                return None;
            };
            let CheckedValue::Struct { fields, .. } = value else {
                return None;
            };
            value = fields.get(*index as usize)?;
        }
        (self.is_copy(root.ty) && value.ty() == root.ty).then_some(value)
    }

    /// Reads an expression as a term or constant [ENT-2]; anything else is
    /// no operand and establishes or derives nothing.
    fn read_operand(&mut self, expression: &CheckedExpression) -> Option<TermId> {
        if let Some(value) = self.constant_storage_scalar(expression).cloned() {
            return self.read_operand(&CheckedExpression::Constant(value));
        }
        match expression {
            // [MSR-6] a const generic read as a value is the symbolic
            // constant term [ENT-2] clause (c) fixes; a concrete [FN-2]
            // instance has already folded it to an integer constant.
            CheckedExpression::Constant(CheckedValue::ConstGeneric { declaration, .. }) => {
                return Some(self.const_parameter_term(*declaration));
            }
            CheckedExpression::Constant(CheckedValue::Integer { ty, bits })
            | CheckedExpression::NamedConstant {
                value: CheckedValue::Integer { ty, bits },
                ..
            } => {
                return Some(
                    self.terms
                        .intern(TermKind::Constant(integer_value(*ty, *bits))),
                );
            }
            _ => {}
        }
        let fragment = fragment_type(expression.ty())?;
        let path = self.read_place_path(expression)?;
        let kind = match path.path.as_slice() {
            projections
                if projections
                    .iter()
                    .all(|projection| matches!(projection, PlaceStep::Field(_))) =>
            {
                TermKind::Place(path, fragment)
            }
            _ => TermKind::Place(path, fragment),
        };
        Some(self.terms.intern(kind))
    }

    /// Reconstructs the exact source-order place path retained by the checked
    /// expression. This is deliberately recursive: field selection may occur
    /// before or after a deref, and nested boxes may introduce more than one
    /// deref. [ENT-2] distinguishes those canonical spellings.
    fn read_place_path(&self, expression: &CheckedExpression) -> Option<ResolvedPlace> {
        match expression {
            CheckedExpression::Binding { binding, .. } => Some(ResolvedPlace {
                root: PlaceRoot::Binding(*binding),
                path: Vec::new(),
            }),
            CheckedExpression::Project {
                binding,
                fields,
                consume_root: false,
                ..
            } => Some(ResolvedPlace {
                root: PlaceRoot::Binding(*binding),
                path: fields.iter().copied().map(PlaceStep::Field).collect(),
            }),
            CheckedExpression::DerefAddressed { binding, .. } => Some(ResolvedPlace {
                root: PlaceRoot::Binding(*binding),
                // A reference binding is the body-local name of its referent
                // path [REF-1]. The written `deref` is that boundary wrapper;
                // concrete Box content steps are appended below.
                path: Vec::new(),
            }),
            CheckedExpression::BoxDeref { value, .. } => {
                let mut path = self.read_place_path(value)?;
                path.path.push(PlaceStep::Deref);
                Some(path)
            }
            CheckedExpression::ProjectValue { value, field, .. } => {
                let mut path = self.read_place_path(value)?;
                path.path.push(PlaceStep::Field(*field));
                Some(path)
            }
            // [ENT-2] clause (b): a subscripted place is a term exactly when
            // its final step selects a readonly field of one fragment type.
            // Its offsets are part of its identity, so only a place whose
            // every offset was captured is one; the measure former keeps its
            // own row in `measure_operand`.
            CheckedExpression::ReadStorage { root, .. }
                if root.readonly_field_term(self.context.nominals)
                    == Some(SubscriptedTerm::Represented) =>
            {
                Some(self.container_root_path(root))
            }
            CheckedExpression::RangeIndex { place, .. }
                if place.readonly_field_term(self.context.nominals)
                    == Some(SubscriptedTerm::Represented) =>
            {
                Some(ResolvedPlace::from_path(
                    place.root.binding,
                    place.place_path(),
                ))
            }
            _ => None,
        }
    }

    /// [ENT-3] comparison-origin shape (a): a direct comparison call whose
    /// operands are each a term or constant. A measure is itself an [ENT-2]
    /// term, including when its place contains admitted subscripts.
    fn direct_comparison(&mut self, expression: &CheckedExpression) -> Option<Relation> {
        let CheckedExpression::IntegerOperation {
            operation,
            operand_type,
            arguments,
            ..
        } = expression
        else {
            return None;
        };
        fragment_type(*operand_type)?;
        let [left_expression, right_expression] = arguments.as_slice() else {
            return None;
        };
        let left = self
            .measure_operand(left_expression)
            .or_else(|| self.read_operand(left_expression))?;
        let right = self
            .measure_operand(right_expression)
            .or_else(|| self.read_operand(right_expression))?;
        sources::comparison_relation(*operation, left, right, 0)
    }

    /// [ENT-3] comparison origin of a match scrutinee: shape (a) directly, or
    /// shape (b), a bare `own Bool` binding whose initializer comparison is
    /// still valid on every path to this use.
    fn scrutinee_relation(
        &mut self,
        expression: &CheckedExpression,
        state: &FactState,
    ) -> Option<Relation> {
        if let Some(relation) = self.direct_comparison(expression) {
            return Some(relation);
        }
        if let CheckedExpression::Binding { binding, ty, .. } = expression
            && *ty == CheckedType::Bool
        {
            return state.origins.get(binding).cloned();
        }
        None
    }

    // ------------------------------------------------------------------
    // Finite exact opaque goals [ENT-2..ENT-4]
    // ------------------------------------------------------------------

    /// Converts one source expression to ENT-3's exact direct pure/total
    /// origin. Any excluded child excludes the whole expression.
    fn direct_goal_expression(&self, expression: &CheckedExpression) -> Option<GoalExpression> {
        self.goal_expression(expression, false)
    }

    /// Converts a value whose nested obligations have already been discharged
    /// to its exact stable proof expression. In addition to the pure/total
    /// direct subset, this admits an exact integer result or indexed element
    /// only after `judge_expression` has checked that nested partial operation.
    fn admitted_value_goal_expression(
        &self,
        expression: &CheckedExpression,
    ) -> Option<GoalExpression> {
        self.goal_expression(expression, true)
    }

    /// Replaces one occurrence-local FN-8 actual with the same admitted
    /// structural value used by the rest of ENT-2. The caller invokes this
    /// only after every obligation in every actual expression has succeeded.
    /// A projection that the admitted structural tree cannot represent keeps
    /// the occurrence-local value instead of inventing a different identity.
    fn admitted_call_goal_expression(
        &self,
        expression: &GoalExpression,
        call: &crate::NodePath,
        arguments: &[Option<GoalExpression>],
    ) -> GoalExpression {
        match expression {
            GoalExpression::Datum(
                original @ GoalDatum::EvaluatedValue {
                    occurrence:
                        EvaluatedValueOccurrence::CallArgument {
                            call: occurrence_call,
                            argument,
                        },
                    captured_type,
                    projections,
                    ty,
                    ..
                },
            ) if occurrence_call == call => {
                let Some(mut admitted) = usize::try_from(*argument)
                    .ok()
                    .and_then(|index| arguments.get(index))
                    .and_then(Option::as_ref)
                    .filter(|argument| argument.ty() == *captured_type)
                    .cloned()
                else {
                    return GoalExpression::Datum(original.clone());
                };
                for projection in projections {
                    let Some(projected) = admitted.with_projection(*projection, *ty) else {
                        return GoalExpression::Datum(original.clone());
                    };
                    admitted = projected;
                }
                if admitted.ty() == *ty {
                    admitted
                } else {
                    GoalExpression::Datum(original.clone())
                }
            }
            GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments: operands,
            } => GoalExpression::Operation {
                row: *row,
                type_arguments: type_arguments.clone(),
                const_arguments: const_arguments.clone(),
                result: *result,
                arguments: operands
                    .iter()
                    .map(|operand| self.admitted_call_goal_expression(operand, call, arguments))
                    .collect(),
            },
            GoalExpression::Datum(datum) => GoalExpression::Datum(datum.clone()),
        }
    }

    fn goal_expression(
        &self,
        expression: &CheckedExpression,
        admitted_partial: bool,
    ) -> Option<GoalExpression> {
        if let Some(value) = self.constant_storage_scalar(expression) {
            return Some(GoalExpression::Datum(GoalDatum::Literal(value.clone())));
        }
        // A non-consuming place read is admitted by its final copy value, not
        // by the mode of every holder traversed on the way there. In
        // particular, reading through an owning box must retain the box's
        // explicit Deref projection even though the box binding itself is
        // affine and cannot be a standalone goal datum.
        if self.is_copy(expression.ty())
            && let Some(path) = self.read_place_path(expression)
            && let PlaceRoot::Binding(root) = path.root
        {
            return Some(GoalExpression::Datum(GoalDatum::Place {
                root,
                projections: path
                    .path
                    .iter()
                    .map(goal_projection_of_step)
                    .collect::<Option<Vec<_>>>()?,
                ty: expression.ty(),
            }));
        }
        let build_operation = |row, type_arguments, const_arguments, result, arguments: Vec<_>| {
            Some(GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments,
            })
        };
        match expression {
            CheckedExpression::Constant(value) => {
                Some(GoalExpression::Datum(GoalDatum::Literal(value.clone())))
            }
            CheckedExpression::NamedConstant { declaration, value } => {
                Some(GoalExpression::Datum(GoalDatum::NamedConst {
                    declaration: *declaration,
                    projections: Vec::new(),
                    ty: value.ty(),
                }))
            }
            CheckedExpression::Binding { binding, ty, .. } if self.is_copy(*ty) => {
                Some(GoalExpression::Datum(GoalDatum::Place {
                    root: *binding,
                    projections: Vec::new(),
                    ty: *ty,
                }))
            }
            CheckedExpression::Project {
                binding,
                fields,
                ty,
                consume_root: false,
                ..
            } if self.is_copy(*ty) => Some(GoalExpression::Datum(GoalDatum::Place {
                root: *binding,
                projections: fields.iter().copied().map(GoalProjection::Field).collect(),
                ty: *ty,
            })),
            CheckedExpression::DerefAddressed { binding, ty, .. } if self.is_copy(*ty) => {
                Some(GoalExpression::Datum(GoalDatum::Place {
                    root: *binding,
                    projections: Vec::new(),
                    ty: *ty,
                }))
            }
            CheckedExpression::BoxDeref {
                referent, value, ..
            } if self.is_copy(*referent) => self
                .goal_expression(value, admitted_partial)?
                .with_projection(GoalProjection::Deref, *referent),
            CheckedExpression::ProjectValue {
                value, field, ty, ..
            } if self.is_copy(*ty) => self
                .goal_expression(value, admitted_partial)?
                .with_projection(GoalProjection::Field(*field), *ty),
            CheckedExpression::IntegerOperation {
                operation,
                operand_type,
                arguments,
                result,
                ..
            } if !operation.is_exact() || admitted_partial => build_operation(
                GoalOperation::Integer {
                    operation: *operation,
                    operand_type: *operand_type,
                },
                Vec::new(),
                Vec::new(),
                *result,
                arguments
                    .iter()
                    .map(|argument| self.goal_expression(argument, admitted_partial))
                    .collect::<Option<Vec<_>>>()?,
            ),
            CheckedExpression::IntegerOperation { .. } => None,
            CheckedExpression::FloatOperation {
                operation: row,
                operand_type,
                arguments,
                ..
            } => build_operation(
                GoalOperation::Float {
                    operation: *row,
                    operand_type: *operand_type,
                },
                if matches!(
                    row,
                    CheckedFloatOperation::Infinity | CheckedFloatOperation::Nan
                ) {
                    vec![*operand_type]
                } else {
                    Vec::new()
                },
                Vec::new(),
                row.result_type(*operand_type),
                arguments
                    .iter()
                    .map(|argument| self.goal_expression(argument, admitted_partial))
                    .collect::<Option<Vec<_>>>()?,
            ),
            CheckedExpression::NumericConversion {
                mode,
                source,
                destination,
                value,
                result,
                ..
            } => build_operation(
                GoalOperation::NumericConversion {
                    mode: *mode,
                    source: *source,
                    destination: *destination,
                },
                vec![source.ty(), destination.ty()],
                Vec::new(),
                *result,
                vec![self.goal_expression(value, admitted_partial)?],
            ),
            CheckedExpression::Reinterpret {
                source,
                destination,
                value,
                ..
            } => build_operation(
                GoalOperation::Reinterpret {
                    source: *source,
                    destination: *destination,
                },
                vec![source.ty(), destination.ty()],
                Vec::new(),
                destination.ty(),
                vec![self.goal_expression(value, admitted_partial)?],
            ),
            CheckedExpression::BooleanOperation {
                operation: row,
                arguments,
                ..
            } => build_operation(
                GoalOperation::Boolean(*row),
                Vec::new(),
                Vec::new(),
                CheckedType::Bool,
                arguments
                    .iter()
                    .map(|argument| self.goal_expression(argument, admitted_partial))
                    .collect::<Option<Vec<_>>>()?,
            ),
            CheckedExpression::EnumEquality {
                equal,
                operand_type,
                arguments,
                ..
            } => build_operation(
                GoalOperation::EnumEquality {
                    equal: *equal,
                    operand_type: *operand_type,
                },
                Vec::new(),
                Vec::new(),
                CheckedType::Bool,
                arguments
                    .iter()
                    .map(|argument| self.goal_expression(argument, admitted_partial))
                    .collect::<Option<Vec<_>>>()?,
            ),
            CheckedExpression::ArrayMeasure {
                measure,
                root,
                length,
            } => {
                let argument = self.goal_array_root(root)?;
                let CheckedType::Array { element, .. } = argument.ty() else {
                    return None;
                };
                build_operation(
                    GoalOperation::ArrayMeasure {
                        measure: *measure,
                        element,
                        length: *length,
                    },
                    Vec::new(),
                    Vec::new(),
                    CheckedType::Integer(IntegerType::U64),
                    vec![argument],
                )
            }
            CheckedExpression::ArrayIndex {
                root,
                element_type,
                length,
                offset,
                ..
            } if admitted_partial => {
                let collection = self.goal_array_root(root)?;
                let CheckedType::Array {
                    element,
                    length: root_length,
                } = collection.ty()
                else {
                    return None;
                };
                if root_length != *length
                    || self.context.elements.get(element.index()) != Some(element_type)
                {
                    return None;
                }
                build_operation(
                    GoalOperation::ArrayIndex {
                        element,
                        length: *length,
                    },
                    Vec::new(),
                    Vec::new(),
                    *element_type,
                    vec![collection, self.goal_expression(offset, admitted_partial)?],
                )
            }
            // [MSR-1] a measure of a storage shape, read as the same
            // quantity the reader row loads. [ENT-2] a place whose offsets
            // are not all captured terms or constants names no one element,
            // so it has no goal identity either.
            CheckedExpression::ContainerMeasure { measure, root }
                if root.subscripted_term() == Some(SubscriptedTerm::Represented) =>
            {
                let measured = root.measured()?;
                let argument = self.goal_container_place(root)?;
                build_operation(
                    GoalOperation::ContainerMeasure {
                        measure: *measure,
                        measured,
                        element: root.element(),
                        constant: root.type_constant(),
                    },
                    Vec::new(),
                    Vec::new(),
                    CheckedType::Integer(IntegerType::U64),
                    vec![argument],
                )
            }
            CheckedExpression::ReadStorage { root, .. } if admitted_partial => {
                let Some((CheckedPlaceStep::Subscript(index), prefix)) = root.path.split_last()
                else {
                    if root
                        .place_path()
                        .contains(&PlaceStep::Index(CapturedValue::unknown()))
                    {
                        return None;
                    }
                    return self.goal_container_place(root);
                };
                let base = CheckedContainerRoot {
                    root: root.root,
                    path: prefix.to_vec(),
                    ty: index.base_type,
                };
                let row = match base.ty {
                    CheckedType::Array { element, length } => {
                        GoalOperation::ArrayIndex { element, length }
                    }
                    _ => GoalOperation::RunIndex {
                        measured: base.measured()?,
                        element: base.element()?,
                        constant: base.type_constant(),
                    },
                };
                let collection = self.goal_container_place(&base)?;
                build_operation(
                    row,
                    Vec::new(),
                    Vec::new(),
                    root.ty,
                    vec![
                        collection,
                        self.goal_expression(&index.offset, admitted_partial)?,
                    ],
                )
            }
            CheckedExpression::BufferMeasure { measure, root }
                if root.subscripted_term() == Some(SubscriptedTerm::Represented) =>
            {
                let argument = self.goal_binding_place(
                    root.binding,
                    root.path.iter().map(CheckedPlaceStep::goal_projection),
                    CheckedType::Buffer {
                        element: root.element,
                    },
                );
                build_operation(
                    GoalOperation::BufferMeasure {
                        measure: *measure,
                        element: root.element,
                    },
                    Vec::new(),
                    Vec::new(),
                    CheckedType::Integer(IntegerType::U64),
                    vec![argument],
                )
            }
            // [MSR-1, REF-4] the one measure a range reference has.
            CheckedExpression::RangeMeasure { measure, root } => {
                let argument =
                    self.goal_binding_place(root.binding, Vec::new(), root.element_type);
                build_operation(
                    // Clause formation uses the ordinary measured-place
                    // row. A body read must have that same structural goal
                    // identity, including after let-origin expansion.
                    GoalOperation::ContainerMeasure {
                        measure: *measure,
                        measured: MeasuredKind::Range,
                        element: Some(root.element),
                        constant: None,
                    },
                    Vec::new(),
                    Vec::new(),
                    CheckedType::Integer(IntegerType::U64),
                    vec![argument],
                )
            }
            CheckedExpression::RangeElementMeasure { measure, place, .. }
                if place.subscripted_term() == Some(SubscriptedTerm::Represented) =>
            {
                let measured = place.measured()?;
                let argument = self.goal_binding_place(
                    place.root.binding,
                    place.goal_projections(),
                    place.ty,
                );
                build_operation(
                    GoalOperation::ContainerMeasure {
                        measure: *measure,
                        measured,
                        element: place.element(),
                        constant: place.type_constant(),
                    },
                    Vec::new(),
                    Vec::new(),
                    CheckedType::Integer(IntegerType::U64),
                    vec![argument],
                )
            }
            CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. }
                if admitted_partial && place.path.is_empty() =>
            {
                let collection = self.goal_binding_place(
                    place.root.binding,
                    Vec::new(),
                    place.root.element_type,
                );
                build_operation(
                    GoalOperation::RunIndex {
                        measured: MeasuredKind::Range,
                        element: place.root.element,
                        constant: None,
                    },
                    Vec::new(),
                    Vec::new(),
                    place.root.element_type,
                    vec![
                        collection,
                        self.goal_expression(&place.offset, admitted_partial)?,
                    ],
                )
            }
            CheckedExpression::BufferIndex { root, offset, .. } if admitted_partial => {
                let collection_type = CheckedType::Buffer {
                    element: root.element,
                };
                let collection = self.goal_binding_place(
                    root.binding,
                    root.path.iter().map(CheckedPlaceStep::goal_projection),
                    collection_type,
                );
                build_operation(
                    GoalOperation::BufferIndex {
                        element: root.element,
                    },
                    Vec::new(),
                    Vec::new(),
                    root.element_type,
                    vec![collection, self.goal_expression(offset, admitted_partial)?],
                )
            }
            CheckedExpression::Binding { .. }
            | CheckedExpression::Project { .. }
            | CheckedExpression::DerefAddressed { .. }
            | CheckedExpression::BoxDeref { .. }
            // [TYPE-9, WIN-3] an unbox consumes its owner, so the value it
            // produces is no longer a place any goal datum can name.
            | CheckedExpression::BoxTake { .. }
            | CheckedExpression::ProjectValue { .. }
            | CheckedExpression::UserCall { .. }
            | CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::RangeElementMeasure { .. }
            | CheckedExpression::ContainerMeasure { .. }
            | CheckedExpression::ReadStorage { .. }
            | CheckedExpression::ArrayIndex { .. }
            | CheckedExpression::BufferIndex { .. }
            | CheckedExpression::RangeIndex { .. }
            | CheckedExpression::BorrowRangeIndex { .. }
            | CheckedExpression::RangeOf { .. }
            | CheckedExpression::BorrowAddressed { .. }
            | CheckedExpression::ConstructStruct { .. }
            | CheckedExpression::ConstructEnum { .. } => None,
        }
    }

    /// Exact value identity for one operand of an already-reached proof
    /// obligation. Stable structural expressions are preferred so a prior
    /// source fact can name the same value. The occurrence-local fallback is
    /// reserved for a value that cannot be safely replayed from source.
    fn obligation_goal_operand(
        &mut self,
        site: &crate::NodePath,
        operand: usize,
        expression: &CheckedExpression,
        facts: &FactState,
    ) -> GoalExpression {
        self.admitted_value_goal_expression(expression)
            .map(|expression| self.expand_goal_expression(&expression, facts))
            .unwrap_or_else(|| {
                let operand =
                    u32::try_from(operand).expect("proof-obligation operand ordinal exceeds u32");
                GoalExpression::Datum(GoalDatum::EvaluatedValue {
                    function: self.function.id,
                    occurrence: EvaluatedValueOccurrence::ObligationOperand {
                        site: site.clone(),
                        operand,
                    },
                    captured_type: expression.ty(),
                    projections: Vec::new(),
                    ty: expression.ty(),
                })
            })
    }

    fn goal_binding_place(
        &self,
        binding: BindingId,
        projections: impl IntoIterator<Item = GoalProjection>,
        ty: CheckedType,
    ) -> GoalExpression {
        GoalExpression::Datum(GoalDatum::Place {
            root: binding,
            projections: projections.into_iter().collect(),
            ty,
        })
    }

    fn goal_array_root(&self, root: &CheckedArrayRoot) -> Option<GoalExpression> {
        match root {
            CheckedArrayRoot::Binding { binding, fields } => {
                let ty = self.projected_binding_type(*binding, fields)?;
                Some(self.goal_binding_place(
                    *binding,
                    fields.iter().copied().map(GoalProjection::Field),
                    ty,
                ))
            }
            CheckedArrayRoot::Constant(id) => {
                let declaration = self.context.constant_declaration(*id)?;
                let ty = self.context.constants.get(id.0 as usize)?.ty;
                Some(GoalExpression::Datum(GoalDatum::NamedConst {
                    declaration,
                    projections: Vec::new(),
                    ty,
                }))
            }
        }
    }

    fn goal_container_place(&self, root: &CheckedContainerRoot) -> Option<GoalExpression> {
        Some(match root.root {
            PlaceRoot::Binding(binding) => {
                self.goal_binding_place(binding, root.goal_projections(), root.ty)
            }
            PlaceRoot::Constant(id) => GoalExpression::Datum(GoalDatum::NamedConst {
                declaration: self.context.constant_declaration(id)?,
                projections: root.goal_projections(),
                ty: root.ty,
            }),
        })
    }

    fn projected_binding_type(&self, binding: BindingId, fields: &[u32]) -> Option<CheckedType> {
        let mut ty = self.summary(binding)?.ty?;
        for field in fields {
            let CheckedType::Nominal(nominal) = ty else {
                return None;
            };
            let CheckedNominalKind::Struct { fields } =
                &self.context.nominals.get(nominal.0 as usize)?.kind
            else {
                return None;
            };
            ty = fields.get(*field as usize)?.ty;
        }
        Some(ty)
    }

    /// Replaces every still-valid ordinary-let leaf by its one complete
    /// origin. Leaves without a valid origin remain direct, so expansion is
    /// all-or-nothing over exactly the eligible leaves.
    fn expand_goal_expression(
        &mut self,
        expression: &GoalExpression,
        state: &FactState,
    ) -> GoalExpression {
        self.expand_goal_expression_inner(expression, state, &mut HashSet::new(), false)
    }

    fn expand_goal_expression_inner(
        &mut self,
        expression: &GoalExpression,
        state: &FactState,
        expanding: &mut HashSet<BindingId>,
        preserve_normalized_leaf: bool,
    ) -> GoalExpression {
        match expression {
            GoalExpression::Datum(GoalDatum::Place {
                root,
                projections,
                ty,
            }) => {
                let Some(origin) = state.goal_origins.get(root).copied() else {
                    return expression.clone();
                };
                if !expanding.insert(*root) {
                    return expression.clone();
                }
                let origin = self.goals.expression(origin).clone();
                let mut expanded = self.expand_goal_expression_inner(
                    &origin,
                    state,
                    expanding,
                    preserve_normalized_leaf,
                );
                expanding.remove(root);
                for projection in projections {
                    let Some(result) = self.goal_projection_type(expanded.ty(), *projection) else {
                        return expression.clone();
                    };
                    let Some(next) = expanded.with_projection(*projection, result) else {
                        return expression.clone();
                    };
                    expanded = next;
                }
                if expanded.ty() == *ty {
                    expanded
                } else {
                    expression.clone()
                }
            }
            GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments,
            } => {
                // Once an operation already has an exact L0 projection or
                // domain normalization, expanding one of its place operands
                // into a non-fragment expression would erase the checker
                // fact that Contrib(P) must classify. Boolean parents still
                // expand their children, so their normalized leaf predicates
                // remain visible without sacrificing those leaf identities.
                if preserve_normalized_leaf
                    && (self.goal_projection(expression).is_some()
                        || self.goal_normalization(expression).is_some())
                {
                    return expression.clone();
                }
                GoalExpression::Operation {
                    row: *row,
                    type_arguments: type_arguments.clone(),
                    const_arguments: const_arguments.clone(),
                    result: *result,
                    arguments: arguments
                        .iter()
                        .map(|argument| {
                            self.expand_goal_expression_inner(
                                argument,
                                state,
                                expanding,
                                preserve_normalized_leaf,
                            )
                        })
                        .collect(),
                }
            }
            GoalExpression::Datum(_) => expression.clone(),
        }
    }

    fn goal_projection_type(
        &self,
        input: CheckedType,
        projection: GoalProjection,
    ) -> Option<CheckedType> {
        match projection {
            GoalProjection::Deref => match input {
                CheckedType::Nominal(nominal) => {
                    match self.context.nominals.get(nominal.0 as usize)?.kind {
                        CheckedNominalKind::Box { referent, .. } => Some(referent),
                        _ => Some(input),
                    }
                }
                // Borrow holders retain the referent type in checked form.
                _ => Some(input),
            },
            GoalProjection::Field(field) => {
                let CheckedType::Nominal(nominal) = input else {
                    return None;
                };
                let CheckedNominalKind::Struct { fields } =
                    &self.context.nominals.get(nominal.0 as usize)?.kind
                else {
                    return None;
                };
                fields.get(field as usize).map(|field| field.ty)
            }
            GoalProjection::Payload { variant, field } => {
                let CheckedType::Nominal(nominal) = input else {
                    return None;
                };
                let CheckedNominalKind::Enum { variants } =
                    &self.context.nominals.get(nominal.0 as usize)?.kind
                else {
                    return None;
                };
                variants
                    .iter()
                    .find(|candidate| candidate.tag == variant)?
                    .fields
                    .get(field as usize)
                    .map(|field| field.ty)
            }
            // [OP-4] a subscript selects the base's element type, which
            // [MSR-1] admits in a measure place and [WIN-1] gives the one
            // slot a run holds.
            GoalProjection::Subscript(_) => element_type(input, self.context.elements),
            // [REF-4, TYPE-8] a range step selects the run of T elements the
            // range names, and `&[T]` is a reference kind and not a type, so
            // that run's checked image is its element type, exactly as a
            // `&[T]` parameter's is.
            GoalProjection::Range(_) => element_type(input, self.context.elements),
            // [MSR-1] the same element selection a written subscript makes;
            // the offset is what the reader substitutes, not the type.
            GoalProjection::FormalSubscript { .. } => element_type(input, self.context.elements),
        }
    }

    fn goal_origin_set(
        &mut self,
        expression: &CheckedExpression,
        state: &FactState,
    ) -> Vec<GoalId> {
        let Some(direct) = self.admitted_value_goal_expression(expression) else {
            return Vec::new();
        };
        if direct.ty() != CheckedType::Bool {
            return Vec::new();
        }
        let complete = self.expand_goal_expression(&direct, state);
        let direct = self.intern_goal_expression(direct);
        let complete = self.intern_goal_expression(complete);
        if direct == complete {
            vec![direct]
        } else {
            vec![direct, complete]
        }
    }

    fn record_goal_origin(
        &mut self,
        binding: BindingId,
        value: &CheckedExpression,
        state: &mut FactState,
    ) {
        let Some(direct) = self.admitted_value_goal_expression(value) else {
            return;
        };
        let origin = self.intern_goal_expression(direct);
        state.goal_origins.insert(binding, origin);
        state.ambiguous_goal_origins.remove(&binding);
    }

    fn record_value_initializer_origin(&self, frame: &GiveFrame, state: &mut FactState) {
        let mut origins =
            frame
                .gives
                .iter()
                .zip(&frame.give_goal_origins)
                .filter_map(|(edge, origin)| {
                    let edge = &edge.facts;
                    (!edge.all_derivable).then_some(*origin)
                });
        let Some(first) = origins.next() else {
            return;
        };
        if origins.any(|origin| origin != first) {
            state.ambiguous_goal_origins.insert(frame.binding);
        }
    }

    fn intern_goal_expression(&mut self, expression: GoalExpression) -> GoalId {
        if let GoalExpression::Operation {
            row: GoalOperation::Boolean(_),
            arguments,
            ..
        } = &expression
        {
            for argument in arguments {
                if argument.ty() == CheckedType::Bool {
                    self.intern_goal_expression(argument.clone());
                }
            }
        }
        let projection = self.goal_projection(&expression);
        let normalization = self.goal_normalization(&expression);
        let mut support = Vec::new();
        self.collect_goal_support(&expression, None, &mut support);
        self.goals
            .intern(expression, projection, normalization, support)
    }

    /// [O11 candidate] The signed Boolean decomposition set of one
    /// established goal: `+band` and `-bor` decompose into their signed
    /// children recursively, `bnot` flips the sign, and every other root —
    /// in particular `-band` and `+bor`, whose content is genuinely
    /// disjunctive, and `bxor` on either sign — contributes nothing.
    ///
    /// Members are interned so their exact identities, projections, and
    /// supports are retained in the inventory, but nothing establishes them
    /// as facts in this version: v0.30 acceptance is untouched. Design:
    /// `research/investigations/o11-composition/DESIGN.md`.
    fn signed_boolean_decomposition(
        &mut self,
        parent: GoalId,
        sign: GoalSign,
        state: &FactState,
    ) -> Vec<(GoalId, GoalSign)> {
        let expression = self.goals.expression(parent).clone();
        let mut members = Vec::new();
        self.collect_decomposition_members(
            &expression,
            sign,
            state,
            &mut members,
            &mut HashSet::new(),
        );
        members
    }

    fn collect_decomposition_members(
        &mut self,
        expression: &GoalExpression,
        sign: GoalSign,
        state: &FactState,
        members: &mut Vec<(GoalId, GoalSign)>,
        following: &mut HashSet<BindingId>,
    ) {
        // An unprojected `own Bool` leaf carrying a still-valid ordinary-let
        // origin stands for that origin under either sign, so the Boolean root
        // is read through the leaf and the leaf contributes no member of its
        // own. Reading the origin here is what keeps a conjunct in the operand
        // form its own binding recorded: the members of a `band` written over
        // comparison bindings are those bindings, whose relations
        // [`Self::establish_boolean_decomposition`] then takes from
        // `state.origins`, so both source spellings use the same relations.
        if let GoalExpression::Datum(GoalDatum::Place {
            root,
            projections,
            ty: CheckedType::Bool,
        }) = expression
            && projections.is_empty()
        {
            let Some(origin) = state.goal_origins.get(root).copied() else {
                return;
            };
            // Only a Boolean root has anything to decompose, and the ordinary
            // guard binding holds a comparison, so this settles the common case
            // without retaining the origin.
            if !matches!(
                self.goals.expression(origin),
                GoalExpression::Operation {
                    row: GoalOperation::Boolean(_),
                    ..
                }
            ) {
                return;
            }
            if !following.insert(*root) {
                return;
            }
            let origin = self.goals.expression(origin).clone();
            self.collect_decomposition_members(&origin, sign, state, members, following);
            following.remove(root);
            return;
        }
        let GoalExpression::Operation {
            row: GoalOperation::Boolean(operation),
            arguments,
            ..
        } = expression
        else {
            return;
        };
        let child_sign = match (operation, sign) {
            (CheckedBooleanOperation::And, GoalSign::Positive)
            | (CheckedBooleanOperation::Or, GoalSign::Negative) => sign,
            (CheckedBooleanOperation::Not, GoalSign::Positive) => GoalSign::Negative,
            (CheckedBooleanOperation::Not, GoalSign::Negative) => GoalSign::Positive,
            _ => return,
        };
        for argument in arguments {
            let member = self.intern_goal_expression(argument.clone());
            if !members.contains(&(member, child_sign)) {
                members.push((member, child_sign));
            }
            self.collect_decomposition_members(argument, child_sign, state, members, following);
        }
    }

    /// [ENT-3] Establishes the signed Boolean decomposition set of one
    /// just-established signed goal, at that same point and in whatever proof
    /// view the state carries.
    ///
    /// Each member enters as its own concrete opaque goal under [FN-8]
    /// structural identity, and a member whose complete root is one admitted
    /// comparison additionally delivers its exact L0 projection under `+` and
    /// that projection's exact negation under `-`. A member that is a bare
    /// comparison binding carries no projection of its own and delivers the
    /// relation that binding recorded instead, so a conjunct proves exactly
    /// what the same comparison proves at a direct branch.
    /// Decomposition never runs upward: this establishes children of an
    /// established parent only, so no child ever establishes or derives a
    /// parent.
    pub(super) fn establish_boolean_decomposition(
        &mut self,
        parent: GoalId,
        sign: GoalSign,
        state: &mut FactState,
        event: FlowEventId,
    ) {
        for (member, member_sign) in self.signed_boolean_decomposition(parent, sign, state) {
            state.establish_goal(member, member_sign, &mut self.derivations, event);
            let Some(relation) = self
                .goals
                .projection(member)
                .cloned()
                .or_else(|| self.member_binding_relation(member, state))
            else {
                continue;
            };
            let relation = match member_sign {
                GoalSign::Positive => relation,
                GoalSign::Negative => relation.negated(),
            };
            state.establish(&relation, &mut self.derivations, event);
        }
    }

    /// The comparison one decomposition member's own binding recorded, for a
    /// member that is an unprojected `own Bool` place. This is `state.origins`,
    /// the [ENT-3] comparison-origin map [`Self::scrutinee_relation`] reads, so
    /// a conjunct and a direct comparison on the same binding deliver the
    /// same relation over the same terms.
    fn member_binding_relation(&self, member: GoalId, state: &FactState) -> Option<Relation> {
        let GoalExpression::Datum(GoalDatum::Place {
            root,
            projections,
            ty: CheckedType::Bool,
        }) = self.goals.expression(member)
        else {
            return None;
        };
        projections
            .is_empty()
            .then(|| state.origins.get(root).cloned())
            .flatten()
    }

    /// Records the O11 decomposition inventory entry for one signed-goal
    /// establishment. Entries deduplicate by parent and sign; this is
    /// retained metadata beside the facts
    /// [`Self::establish_boolean_decomposition`] establishes.
    pub(super) fn record_boolean_decomposition(
        &mut self,
        parent: GoalId,
        sign: GoalSign,
        state: &FactState,
    ) {
        if self
            .boolean_decompositions
            .iter()
            .any(|candidate| candidate.parent == parent && candidate.sign == sign)
        {
            return;
        }
        let members = self.signed_boolean_decomposition(parent, sign, state);
        if members.is_empty() {
            return;
        }
        self.boolean_decompositions
            .push(super::BooleanGoalDecomposition {
                parent,
                sign,
                members,
            });
    }

    fn collect_goal_support(
        &self,
        expression: &GoalExpression,
        measure: Option<CheckedMeasure>,
        support: &mut Vec<GoalSupport>,
    ) {
        match expression {
            GoalExpression::Datum(GoalDatum::Place {
                root, projections, ..
            }) => support.push(GoalSupport {
                root: *root,
                projections: projections.clone(),
                measure,
            }),
            GoalExpression::Datum(
                GoalDatum::Parameter { .. }
                | GoalDatum::NamedConst { .. }
                | GoalDatum::EvaluatedValue { .. }
                | GoalDatum::Literal(_),
            ) => {}
            GoalExpression::Operation { row, arguments, .. } => {
                // [MSR-2] every measure of one place has the same support,
                // P's descriptor storage; the selected measure only says that
                // this node is a measure node rather than a place node.
                let node_measure = match row {
                    GoalOperation::ArrayMeasure { measure, .. }
                    | GoalOperation::BufferMeasure { measure, .. }
                    | GoalOperation::ContainerMeasure { measure, .. } => Some(*measure),
                    _ => None,
                };
                for argument in arguments {
                    self.collect_goal_support(argument, node_measure, support);
                }
            }
        }
    }

    fn goal_projection(&mut self, expression: &GoalExpression) -> Option<Relation> {
        let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation,
                    operand_type,
                },
            arguments,
            ..
        } = expression
        else {
            return None;
        };
        fragment_type(*operand_type)?;
        let [left, right] = arguments.as_slice() else {
            return None;
        };
        // [MSR-5] each side is an affine expression, so each projects to one
        // term displaced by a constant and the two displacements fold into
        // the one constant a difference bound carries.
        let (left, left_constant) = self.goal_side(left)?;
        let (right, right_constant) = self.goal_side(right)?;
        sources::comparison_relation(
            *operation,
            left,
            right,
            right_constant.checked_sub(left_constant)?,
        )
    }

    /// One clause side as a term displaced by a constant [MSR-5].
    ///
    /// A side with no term at all is one constant and keeps the constant term
    /// [ENT-2] folds it onto; a side carrying two terms, or a term with any
    /// coefficient other than one, is outside the difference-bound fragment
    /// and projects to nothing, which only under-derives [ENT-1].
    fn goal_side(&mut self, expression: &GoalExpression) -> Option<(TermId, i128)> {
        let (term, constant) = self.goal_affine_side(expression)?;
        match term {
            Some(term) => Some((term, constant)),
            None => Some((self.terms.intern(TermKind::Constant(constant)), 0)),
        }
    }

    fn goal_affine_side(&mut self, expression: &GoalExpression) -> Option<(Option<TermId>, i128)> {
        if let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation:
                        operation @ (CheckedIntegerOperation::AddExact
                        | CheckedIntegerOperation::SubtractExact
                        | CheckedIntegerOperation::MultiplyExact),
                    ..
                },
            arguments,
            ..
        } = expression
        {
            let [left, right] = arguments.as_slice() else {
                return None;
            };
            let (left_term, left_value) = self.goal_affine_side(left)?;
            let (right_term, right_value) = self.goal_affine_side(right)?;
            return match operation {
                CheckedIntegerOperation::AddExact => {
                    if left_term.is_some() && right_term.is_some() {
                        return None;
                    }
                    Some((
                        left_term.or(right_term),
                        left_value.checked_add(right_value)?,
                    ))
                }
                CheckedIntegerOperation::SubtractExact => {
                    if right_term.is_some() {
                        return None;
                    }
                    Some((left_term, left_value.checked_sub(right_value)?))
                }
                _ => {
                    if left_term.is_some() || right_term.is_some() {
                        return None;
                    }
                    Some((None, left_value.checked_mul(right_value)?))
                }
            };
        }
        if let GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer { ty, bits })) =
            expression
        {
            return Some((None, integer_value(*ty, *bits)));
        }
        Some((Some(self.goal_operand(expression)?), 0))
    }

    fn goal_operand(&mut self, expression: &GoalExpression) -> Option<TermId> {
        match expression {
            // [MSR-6] a const generic operand is the symbolic constant term.
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::ConstGeneric {
                declaration,
                ..
            })) => Some(self.const_parameter_term(*declaration)),
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer { ty, bits })) => Some(
                self.terms
                    .intern(TermKind::Constant(integer_value(*ty, *bits))),
            ),
            GoalExpression::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty,
            }) if projections.is_empty() => {
                let CheckedValue::Integer {
                    ty: value_type,
                    bits,
                } = &self.context.constant(*declaration)?.value
                else {
                    return None;
                };
                (*ty == CheckedType::Integer(*value_type)).then(|| {
                    self.terms
                        .intern(TermKind::Constant(integer_value(*value_type, *bits)))
                })
            }
            GoalExpression::Datum(datum) => {
                let fragment = fragment_type(datum.ty())?;
                let path = self.goal_place_path(datum)?;
                Some(self.terms.intern(TermKind::Place(path, fragment)))
            }
            GoalExpression::Operation { row, arguments, .. }
                if matches!(
                    row,
                    GoalOperation::ArrayMeasure { .. }
                        | GoalOperation::BufferMeasure { .. }
                        | GoalOperation::ContainerMeasure { .. }
                ) =>
            {
                let [place] = arguments.as_slice() else {
                    return None;
                };
                let GoalExpression::Datum(datum) = place else {
                    return None;
                };
                let path = self.goal_place_path(datum)?;
                let (measure, measured, array_length) = match row {
                    GoalOperation::ArrayMeasure {
                        measure, length, ..
                    } => (*measure, MeasuredKind::ConstantArray, Some(*length)),
                    GoalOperation::BufferMeasure { measure, .. } => {
                        (*measure, MeasuredKind::RuntimeArray, None)
                    }
                    // [MSR-1]'s row for a storage shape. The written
                    // constant is what `measure_term` reads for a cell the
                    // table fixes as the type's own constant [MSR-2].
                    GoalOperation::ContainerMeasure {
                        measure,
                        measured,
                        constant,
                        ..
                    } => (*measure, *measured, *constant),
                    _ => return None,
                };
                // [REF-4, MSR-1] a range whose path ends in a range step is
                // the anonymous one an actual formed at its call: it names no
                // binding, so its `len` is the mathematical difference of the
                // endpoint values the formation captured rather than a
                // measure of the storage below the step. Where both endpoints
                // are value-determined that difference is a constant, which
                // is the pre-transfer term [MSR-3] of that measure. An
                // endpoint this relation cannot name leaves the opaque
                // measure term, which carries no fact and only under-derives.
                if measure == CheckedMeasure::Length
                    && let Some(PlaceStep::Range(range)) = path.path.last()
                    && let Some(length) = range.constant_length()
                {
                    return Some(self.terms.intern(TermKind::Constant(length)));
                }
                Some(self.measure_term(measure, path, measured, array_length))
            }
            GoalExpression::Operation { .. } => None,
        }
    }

    /// [ENT-3.S6] the equality one range formation establishes on its
    /// binder's `len`: `deref(part).len = hi - lo`, read over the exact
    /// current-value images captured where the endpoints are evaluated.
    ///
    /// L0 is a difference-bound fragment [ENT-4], so the equality is stored
    /// exactly when the mathematical difference is one term displaced by a
    /// constant: two constant endpoints give the constant length, and an
    /// endpoint pair sharing a term gives the same. A pair naming two
    /// different terms — `&p[a..b]` — is outside the fragment and keeps only
    /// the affine image, which only under-derives [ENT-1].
    fn range_length_relation(
        &mut self,
        length: TermId,
        start: &CheckedExpression,
        end: &CheckedExpression,
    ) -> Option<Relation> {
        let start_goal = self.admitted_value_goal_expression(start)?;
        let end_goal = self.admitted_value_goal_expression(end)?;
        let (start_term, start_constant) = self.goal_affine_side(&start_goal)?;
        let (end_term, end_constant) = self.goal_affine_side(&end_goal)?;
        let difference = end_constant.checked_sub(start_constant)?;
        let right = match (start_term, end_term) {
            (None, None) => self.terms.intern(TermKind::Constant(difference)),
            (None, Some(term)) => {
                return Some(Relation::Equal {
                    left: length,
                    right: term,
                    difference,
                });
            }
            (Some(start), Some(end)) if start == end => {
                self.terms.intern(TermKind::Constant(difference))
            }
            _ => return None,
        };
        Some(Relation::Equal {
            left: length,
            right,
            difference: 0,
        })
    }

    /// Files one [REF-4] formation's endpoint images under its start capture,
    /// the key every reader of the range-image table looks them up by.
    ///
    /// Only a source occurrence names one formation, and the checker gives
    /// every endpoint the occurrence that evaluated it. An image filed under
    /// any other capture would answer for every range carrying that capture
    /// [OWN-7], so nothing is filed for it and no ordering separates it.
    fn file_range_image(
        &mut self,
        carrier: &crate::NodePath,
        captured: CapturedRange,
        start: &CheckedExpression,
        end: &CheckedExpression,
        affine: &mut AffineFlowState,
    ) {
        if !matches!(captured.start.capture, CaptureId::Source(_)) {
            return;
        }
        if let Some(image) = self.range_formation_image(carrier, start, end, affine) {
            affine.ranges.insert(captured.start.capture, image);
        }
    }

    /// The immutable length image one [REF-4] formation captured.
    ///
    /// The range-image table is keyed by the formation's original capture
    /// occurrence. Reading it here keeps a later rebind from reconstructing
    /// the range through endpoint bindings whose current values may differ
    /// from the values the formation evaluated.
    fn captured_range_length_image(
        captured: CapturedRange,
        state: &AffineFlowState,
    ) -> Option<AffineForm> {
        let image = state.ranges.get(&captured.start.capture)?;
        image
            .end
            .subtract(&image.start, &mut AffineCheckState::new())
            .ok()
    }

    /// Installs one captured range image on the place that now names it.
    fn establish_captured_range_length(
        &mut self,
        destination: ResolvedPlace,
        captured: CapturedRange,
        state: &mut AffineFlowState,
    ) -> Option<TermId> {
        let length = Self::captured_range_length_image(captured, state)?;
        let term = self.place_measure_term(
            CheckedMeasure::Length,
            destination,
            MeasuredKind::Range,
            None,
        );
        state.measure_atoms.borrow_mut().insert(term, length);
        Some(term)
    }

    fn goal_place_path(&self, datum: &GoalDatum) -> Option<ResolvedPlace> {
        let (root, projections) = match datum {
            GoalDatum::Place {
                root, projections, ..
            } => (PlaceRoot::Binding(*root), projections),
            GoalDatum::NamedConst {
                declaration,
                projections,
                ..
            } => (
                PlaceRoot::Constant(*self.context.constant_ids.get(declaration)?),
                projections,
            ),
            GoalDatum::Parameter { .. }
            | GoalDatum::EvaluatedValue { .. }
            | GoalDatum::Literal(_) => return None,
        };
        Some(ResolvedPlace {
            root,
            path: projections
                .iter()
                .map(|projection| projection.place_step())
                .collect(),
        })
    }

    fn body_requirement_goal(&self, requirement: &CheckedRequirement) -> Option<GoalExpression> {
        self.body_goal_expression(&requirement.template.root)
    }

    fn body_goal_expression(&self, expression: &GoalExpression) -> Option<GoalExpression> {
        match expression {
            GoalExpression::Datum(GoalDatum::Parameter {
                ordinal,
                projections,
                ty,
            }) => {
                let parameter = self.function.parameters.get(*ordinal as usize)?;
                let binding = parameter.binding;
                // [MSR-1, ENT-2] a formal-valued subscript names a value
                // parameter of this same callable, so inside the body it is
                // that parameter's own binding: `deref(rows)[i].len` written
                // in the clause and written in the body are one term because
                // their canonical spellings are byte-identical there.
                let projections = self
                    .body_projections(PlaceRoot::Binding(binding), projections)
                    .iter()
                    .map(|projection| match projection {
                        GoalProjection::FormalSubscript { ordinal } => self
                            .function
                            .parameters
                            .get(*ordinal as usize)
                            .map(|offset| {
                                GoalProjection::Subscript(
                                    CapturedValue::new(
                                        CaptureId::source(u32::MAX),
                                        CapturedTerm::Binding(offset.binding),
                                    )
                                    .goal_identity(),
                                )
                            }),
                        other => Some(*other),
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(GoalExpression::Datum(GoalDatum::Place {
                    root: binding,
                    projections,
                    ty: *ty,
                }))
            }
            GoalExpression::Datum(GoalDatum::EvaluatedValue { .. }) => None,
            GoalExpression::Datum(datum) => Some(GoalExpression::Datum(datum.clone())),
            GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments,
            } => Some(GoalExpression::Operation {
                row: *row,
                type_arguments: type_arguments.clone(),
                const_arguments: const_arguments.clone(),
                result: *result,
                arguments: arguments
                    .iter()
                    .map(|argument| self.body_goal_expression(argument))
                    .collect::<Option<Vec<_>>>()?,
            }),
        }
    }

    // ------------------------------------------------------------------
    // Kill collection from expressions
    // ------------------------------------------------------------------

    /// [EFF-5] one declared `epsuffix*` as resolved-path steps, with each
    /// index and range position substituted by its own argument's value.
    ///
    /// A signature never contains an index expression: an index position
    /// names a value parameter of the same callable [EFF-1], and [EFF-5]
    /// substitutes that parameter's actual at the call, evaluated once.
    /// `offsets` carries the value each such parameter's argument holds. A
    /// position whose argument is no value a place relation can name stays
    /// the unknown offset, which no admitted family separates and which
    /// therefore reaches every element of the indexed base. That is the
    /// conservative direction for a kill.
    fn substituted_steps(
        steps: &[super::super::model::CheckedEffectStep],
        offsets: &HashMap<crate::DeclarationId, CapturedValue>,
    ) -> Vec<PlaceStep> {
        use super::super::model::CheckedEffectStep as Step;
        let offset = |declaration: &crate::DeclarationId| {
            offsets
                .get(declaration)
                .copied()
                .unwrap_or_else(CapturedValue::unknown)
        };
        steps
            .iter()
            .map(|step| match step {
                Step::Field(field) => PlaceStep::Field(*field),
                Step::Deref => PlaceStep::Deref,
                Step::Payload { variant, field } => PlaceStep::Payload {
                    variant: *variant,
                    field: *field,
                },
                Step::Index(declaration) => PlaceStep::Index(offset(declaration)),
                Step::Range { start, end } => PlaceStep::Range(CapturedRange {
                    start: offset(start),
                    end: offset(end),
                }),
                Step::Part(part) => PlaceStep::Part(*part),
                Step::Measure(measure) => PlaceStep::Measure(*measure),
            })
            .collect()
    }

    /// The value one call's argument supplies to an [EFF-5] substituted row
    /// position, for every value parameter whose argument names one.
    ///
    /// [REF-1] names a captured value by the occurrence it was evaluated at,
    /// and a substituted position is evaluated at its argument rather than at
    /// a `psuffix` of its own, so every position of one call shares the one
    /// reserved identity below. Two positions holding two different written
    /// literals are still separated by [OWN-7], which reads the value beside
    /// the identity; a pair the identity alone leaves unseparated is treated
    /// as one storage, which is the conservative direction for a kill.
    fn substituted_offsets(
        callee: Option<&super::EntailmentCallee>,
        captures: &[CapturedValue],
    ) -> HashMap<crate::DeclarationId, CapturedValue> {
        let mut offsets = HashMap::new();
        let Some(callee) = callee else {
            return offsets;
        };
        for (declaration, capture) in callee.parameter_declarations.iter().zip(captures) {
            if !matches!(capture.term, CapturedTerm::Opaque) {
                offsets.insert(*declaration, *capture);
            }
        }
        offsets
    }

    /// [OWN-1] whether a value of this type is read without being consumed,
    /// so a place of it is an ordinary goal datum. A type parameter standing
    /// for itself answers false here: the flow state keeps no fact about a
    /// value of a type it cannot see.
    fn is_copy(&self, ty: CheckedType) -> bool {
        crate::semantic::model::type_has_copy_capability(
            ty,
            self.context.nominals,
            self.context.elements,
            &|_| Some(false),
        )
        .unwrap_or(false)
    }

    /// The [REF-1] resolved place one argument expression names, and whether
    /// it reaches that place through a reference variable rather than through
    /// a `borrow_expr` written here.
    ///
    /// [EFF-5] substitutes the actual's path into the callee's row, so what a
    /// call writes through parameter i is this place extended by that row's
    /// `epsuffix*`. An argument that is not a place names none.
    ///
    /// A range formed at the call, `&x[lo..hi]` or `&deref(part)[a..b]`, is a
    /// `borrow_expr` written here exactly as `&x[i]` is: it names its source's
    /// resolved places extended by the formation's own range step [REF-4,
    /// OWN-7], the same path a bound range reference would name. That step
    /// against an index step of the same base is a pair no admitted family
    /// separates, so a write through it reaches every element fact of that
    /// base unless the paths separate earlier.
    fn argument_referents(&self, argument: &CheckedExpression) -> Vec<(ResolvedPlace, bool)> {
        let Some(named) = named_place(argument) else {
            return Vec::new();
        };
        let through_reference = match named.form {
            NamingForm::Binding(binding) if self.places.is_reference(binding) => true,
            NamingForm::Borrow | NamingForm::Range => false,
            NamingForm::Binding(_) | NamingForm::Read => return Vec::new(),
        };
        named
            .resolve(&self.places, true)
            .into_iter()
            .map(|place| (place, through_reference))
            .collect()
    }

    /// The [ENT-5] kills one call's write through a viewed range projects
    /// [CALL-3].
    ///
    /// The write reaches the viewed range's element storage and no measure
    /// term over the origin place itself, nor over the view. The event is the
    /// actual's range place with one element step of unknown offset below it,
    /// so `len_of(origin)` and `len_of(view)` both survive it [MSR-2], while
    /// a fact over any element the range may contain dies with that element's
    /// storage: a field of it, or, for an element type with descriptor
    /// storage of its own, its measures [CALL-3]. Whether the actual is a
    /// bound view or a range formed at the call, `argument_referents` names
    /// the same range place.
    fn collect_view_write_kills(
        &self,
        argument: &CheckedExpression,
        call: &crate::NodePath,
        events: &mut Vec<KillEvent>,
    ) {
        for (place, entry_image_only) in self.argument_referents(argument) {
            let place = element_write_place(place, CapturedValue::unknown());
            if entry_image_only {
                events.push(KillEvent::EntryImageHolderWrite {
                    place,
                    element: true,
                    source: call.clone(),
                });
            } else {
                events.push(KillEvent::Write {
                    place,
                    element: true,
                    source: call.clone(),
                });
            }
        }
    }

    /// Collects [ENT-5] kill events (b) and (c) from one expression tree.
    fn collect_expression_kills(
        &self,
        expression: &CheckedExpression,
        events: &mut Vec<KillEvent>,
    ) {
        match expression {
            CheckedExpression::Binding {
                carrier,
                binding,
                consume_root,
                ty,
                ..
            } => {
                if self.is_holder(*binding) {
                    if *consume_root {
                        events.push(KillEvent::EntryImageHolderConsume {
                            binding: *binding,
                            source: carrier.clone(),
                        });
                    }
                } else if !self.is_copy(*ty)
                    // [VIEW-1, OWN-5] a borrow of a view is not a consume of
                    // it. A view binding occurs in a `borrow_expr` as itself,
                    // because the descriptor is what a borrow of one carries,
                    // and that occurrence carries `consume_root: false`; the
                    // exclusive view is affine, so without this the ordinary
                    // affine kill would end every fact about a view the
                    // moment it is handed to a call that only borrows it.
                    && *consume_root
                {
                    events.push(KillEvent::Consume {
                        binding: *binding,
                        source: carrier.clone(),
                    });
                }
            }
            CheckedExpression::Project {
                carrier,
                binding,
                consume_root,
                ..
            } => {
                if *consume_root {
                    events.push(KillEvent::Consume {
                        binding: *binding,
                        source: carrier.clone(),
                    });
                }
            }
            CheckedExpression::BoxTake {
                carrier, binding, ..
            } => {
                // [TYPE-9, WIN-3, ENT-5] the selected content leaves through
                // this expression, but the complete owning root ceases to
                // exist. The checked take no longer embeds a synthetic
                // Binding child, so its root consume is explicit here.
                events.push(KillEvent::Consume {
                    binding: *binding,
                    source: carrier.clone(),
                });
            }
            // These wrappers are checked reads of one place. Their nested
            // expression preserves source spelling and lowering structure;
            // it is not a second consuming evaluation of an affine holder.
            CheckedExpression::BoxDeref { .. } | CheckedExpression::ProjectValue { .. } => {}
            CheckedExpression::UserCall {
                function,
                call,
                arguments,
                actual_captures,
                formal_effects,
                ..
            } => {
                let callee = self.context.callee(*function);
                for argument in arguments {
                    self.collect_expression_kills(argument, events);
                }
                let offsets = Self::substituted_offsets(callee, actual_captures);
                for (index, argument) in arguments.iter().enumerate() {
                    let boundary_writes = formal_effects.as_ref().and_then(|effects| {
                        let declaration = callee?.parameter_declarations.get(index)?;
                        Some(
                            effects
                                .writes
                                .iter()
                                .filter(|path| path.root == *declaration)
                                .map(|path| path.steps.clone())
                                .collect::<Vec<_>>(),
                        )
                    });
                    let Some(writes) = boundary_writes
                        .as_ref()
                        .or_else(|| callee.and_then(|callee| callee.parameter_writes.get(index)))
                    else {
                        continue;
                    };
                    // [CALL-5] the transport is the declared parameter's and
                    // never the actual's spelling: a shared borrow is a kill
                    // event for nothing [CALL-1], a view confines the write to
                    // the range's element storage [CALL-3], and every other
                    // parameter kills conservatively.
                    let transport = callee
                        .and_then(|callee| callee.parameter_transports.get(index).copied())
                        .unwrap_or_default();
                    if transport == CallTransport::ReadOnlyReference {
                        continue;
                    }
                    let element = transport.writes_element_storage();
                    if !writes.is_empty() && element {
                        self.collect_view_write_kills(argument, call, events);
                        continue;
                    }
                    for (place, entry_image_only) in self.argument_referents(argument) {
                        for steps in writes {
                            let mut written = place.clone();
                            written
                                .path
                                .extend(Self::substituted_steps(steps, &offsets));
                            if entry_image_only {
                                events.push(KillEvent::EntryImageHolderWrite {
                                    place: written,
                                    element,
                                    source: call.clone(),
                                });
                            } else {
                                events.push(KillEvent::Write {
                                    place: written,
                                    element,
                                    source: call.clone(),
                                });
                            }
                        }
                    }
                }
            }
            // [BLK-0, EFF-1] a row's declared effect row is a callee effect
            // like any other: the place its `writes` names is written by the
            // call, so [ENT-5] kills every fact whose support that place
            // reaches. Without this the store's pre-call measure facts and
            // the pre-transfer call-datum equality survive beside the row's
            // own post-state relations, and the two together are a
            // contradiction the row introduces.
            _ => {
                for child in expression_children(expression) {
                    self.collect_expression_kills(child, events);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Obligations [ENT-6]
    // ------------------------------------------------------------------

    /// Judges every bounds obligation inside one expression against the
    /// state at this point, inner offsets before the sites they feed.
    ///
    /// A parent operation is reached only after every partial operation in
    /// its already-evaluated children succeeds.  The acceptance-dark test
    /// hook keeps a failed child outcome for inspection, but must not then
    /// manufacture an admitted exact/index value or a later obligation for
    /// the unreachable parent.
    fn judge_children_reach_parent<'expression>(
        &mut self,
        children: impl IntoIterator<Item = &'expression CheckedExpression>,
        states: &mut ProofFlowState,
    ) -> bool {
        let mut reached = true;
        for child in children {
            reached &= self.judge_expression(child, states).reached;
        }
        reached
    }

    fn obligations_since_discharged(&self, obligation_start: usize) -> bool {
        self.obligations[obligation_start..].iter().all(|outcome| {
            outcome.discharged
                || matches!(outcome.family, ObligationFamily::ReferencePreservation(_))
        })
    }

    fn judge_expression(
        &mut self,
        expression: &CheckedExpression,
        states: &mut ProofFlowState,
    ) -> ExpressionJudgment {
        let mut judgment = match expression {
            CheckedExpression::UserCall {
                function,
                call,
                arguments,
                actual_captures,
                goal_arguments,
                requirements,
                formal_contract,
                allocation,
                ..
            } => {
                let obligation_start = self.obligations.len();
                let mut actuals_reached = true;
                for argument in arguments {
                    actuals_reached &= self.judge_expression(argument, states).reached;
                }
                if actuals_reached {
                    let required_captures = self
                        .function
                        .call_separations
                        .iter()
                        .filter(|separation| separation.site == *call)
                        .flat_map(|separation| separation.positions.iter())
                        .flat_map(|positions| match *positions {
                            super::super::model::CheckedCallSeparationPositions::Indices(
                                left,
                                right,
                            ) => [left.capture, right.capture],
                            super::super::model::CheckedCallSeparationPositions::Ranges(
                                left,
                                right,
                            ) => [left.start.capture, right.start.capture],
                            super::super::model::CheckedCallSeparationPositions::Live(index) => {
                                [index.capture, index.capture]
                            }
                        })
                        .collect::<HashSet<_>>();
                    for (argument, captured) in arguments.iter().zip(actual_captures) {
                        if required_captures.contains(&captured.capture) {
                            self.establish_index_capture(*captured, argument, states);
                        }
                    }
                }
                // [OP-9] a runtime-capacity construction [OP-13] and `grow`
                // [OP-10] carry the static allocation-size obligation over
                // their own stored type and count. It is judged at the call,
                // after the count expression's own obligations, and before
                // the callee's written requirements: an unproved count is an
                // OP-9 rejection of the allocation and not an FN-8 report
                // about a clause the row happens to write.
                if let Some(allocation) = allocation
                    && actuals_reached
                    && let Some(length) = arguments.get(allocation.count)
                {
                    self.judge_allocation_fit(
                        allocation.element,
                        allocation.layout_ceiling.stride.allocation_limit(),
                        length,
                        call.clone(),
                        states,
                    );
                    actuals_reached &= self.obligations_since_discharged(obligation_start);
                }
                let actual_parents = self.obligations[obligation_start..]
                    .iter()
                    .filter(|outcome| {
                        !matches!(outcome.family, ObligationFamily::ReferencePreservation(_))
                    })
                    .map(|outcome| outcome.discharged.then_some(outcome.derivation).flatten())
                    .collect::<Option<Vec<_>>>();
                let mut goal_parents = Vec::with_capacity(requirements.len());
                let mut goals_ok = actuals_reached;
                let admitted_arguments = actuals_reached.then(|| {
                    arguments
                        .iter()
                        .zip(goal_arguments)
                        .map(|(argument, captured)| {
                            matches!(
                                captured,
                                GoalExpression::Datum(GoalDatum::EvaluatedValue {
                                    occurrence: EvaluatedValueOccurrence::CallArgument {
                                        call: occurrence_call,
                                        ..
                                    },
                                    ..
                                }) if occurrence_call == call
                            )
                            .then(|| self.admitted_value_goal_expression(argument))
                            .flatten()
                        })
                        .collect::<Vec<_>>()
                });
                // FN-8 begins only after every actual-expression obligation
                // succeeds. A failed OP-4 actual therefore publishes no call
                // judgment for diagnostic selection to reorder.
                for requirement in requirements {
                    if actuals_reached {
                        let goal = ConcreteGoal::new(
                            self.admitted_call_goal_expression(
                                &requirement.goal.root,
                                call,
                                admitted_arguments
                                    .as_deref()
                                    .expect("reached actuals have admitted argument slots"),
                            ),
                        );
                        let (disposition, derivation) = self.judge_call_goal(
                            *function,
                            call,
                            requirement.requires_clause.clone(),
                            goal,
                            arguments.len(),
                            (ProofContext::new(&states.facts, &states.affine), &*states),
                        );
                        goals_ok &= disposition == CallGoalDisposition::Discharged;
                        if let Some(derivation) = derivation {
                            goal_parents.push(derivation);
                        }
                    }
                }
                let reached = actuals_reached && goals_ok;
                let contract_parents = if reached {
                    formal_contract.as_deref().and_then(|contract| {
                        self.retain_contract_call_authorities(call, contract, &goal_parents)
                    })
                } else {
                    None
                };
                let reached = reached
                    && formal_contract
                        .as_ref()
                        .is_none_or(|_| contract_parents.as_ref().is_some());
                let postconditions =
                    self.available_call_postconditions(*function, formal_contract.as_deref());
                let prepared_call = (|| {
                    let mut parents = actual_parents?;
                    if !reached || goal_parents.len() != requirements.len() {
                        return None;
                    }
                    parents.extend(goal_parents);
                    parents.extend(contract_parents.unwrap_or_default());
                    // A direct call needs an earlier-component verified FN-9
                    // summary. A bound call needs either those actual
                    // premises plus its retained FN-4 implication, or a
                    // zero-premise formal implication. Calls with no
                    // authorized relation retain the pre-S12 kill path.
                    if postconditions.is_empty() {
                        return None;
                    }
                    Some(PreparedCall {
                        callee: *function,
                        postconditions,
                        call: call.clone(),
                        parents,
                        transfer_events: Vec::new(),
                        kills: Vec::new(),
                        live: LiveIndices::new(),
                    })
                })();
                // [ENT-3.S13, MSR-3] the call datums are minted here, at the
                // pre-transfer point [ENT-5] fixes, and not at the later
                // establishment: instantiating at the call is what lets a
                // relation over an `own` operand outlive the consume the same
                // statement performs.
                if let Some(prepared) = &prepared_call {
                    self.establish_call_datums(
                        *function,
                        call,
                        goal_arguments,
                        &prepared.postconditions,
                        states,
                    );
                }
                ExpressionJudgment {
                    prepared_call,
                    reached,
                }
            }
            CheckedExpression::ArrayIndex {
                root,
                length,
                offset,
                obligation,
                ..
            } => {
                let reaches_index =
                    self.judge_children_reach_parent(std::iter::once(offset.as_ref()), states);
                let obligation_start = self.obligations.len();
                if reaches_index {
                    let base = self.array_root_place(root);
                    self.judge_obligation(
                        base,
                        MeasuredKind::ConstantArray,
                        Some(*length),
                        offset,
                        obligation.clone(),
                        states,
                    );
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reaches_index && self.obligations_since_discharged(obligation_start),
                }
            }
            CheckedExpression::BufferIndex {
                root,
                offset,
                obligation,
                ..
            } => {
                let reaches_index =
                    self.judge_children_reach_parent(std::iter::once(offset.as_ref()), states);
                let obligation_start = self.obligations.len();
                if reaches_index {
                    let base = ResolvedPlace::from_path(root.binding, root.place_path());
                    self.judge_obligation(
                        base,
                        MeasuredKind::RuntimeArray,
                        None,
                        offset,
                        obligation.clone(),
                        states,
                    );
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reaches_index && self.obligations_since_discharged(obligation_start),
                }
            }
            CheckedExpression::RangeElementMeasure { place, .. } => ExpressionJudgment {
                prepared_call: None,
                reached: self.judge_range_element_place(place, states),
            },
            // [OP-4, REF-4] one element of the run a range names owes
            // `i < deref(p).len`, the range's one measure [MSR-1].
            CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => ExpressionJudgment {
                prepared_call: None,
                reached: self.judge_range_element_place(place, states),
            },
            // [REF-4] the formation's two conjuncts, `lo <= hi` and
            // `hi <= x.len`, over the source's own one measure.
            CheckedExpression::RangeOf {
                carrier,
                source,
                start,
                end,
                obligation,
                captured,
                ..
            } => {
                let reaches_endpoints =
                    self.judge_children_reach_parent([start.as_ref(), end.as_ref()], states);
                // [OWN-7] the formation's two endpoint images, attached to
                // the occurrence that evaluated them. A range step names its
                // formation by that occurrence [REF-1], so a separation
                // submitted at a later call reads exactly these two forms and
                // never re-reads a spelling whose bindings may have moved on.
                self.file_range_image(carrier, *captured, start, end, &mut states.affine);
                let obligation_start = self.obligations.len();
                let source_subscripts = match source {
                    CheckedRangeSource::Storage(root) => self.judge_place_subscripts(root, states),
                    CheckedRangeSource::Range(_) => true,
                };
                if reaches_endpoints && source_subscripts {
                    let length = match source {
                        CheckedRangeSource::Storage(root) => CheckedExpression::ContainerMeasure {
                            measure: CheckedMeasure::Length,
                            root: root.clone(),
                        },
                        CheckedRangeSource::Range(root) => CheckedExpression::RangeMeasure {
                            measure: CheckedMeasure::Length,
                            root: root.clone(),
                        },
                    };
                    let formation_start = self.obligations.len();
                    self.judge_view_range(obligation, start, end, &length, states);
                    // [PAR-2] retain the existing range-image proof only
                    // after both endpoint-domain obligations succeeded.
                    // Permission consumes these proofs; it cannot infer a
                    // partition merely from the shape of a range argument.
                    if self.obligations.len() == formation_start + 2
                        && self.obligations_since_discharged(formation_start)
                        && let Some(image) =
                            states.affine.ranges.get(&captured.start.capture).cloned()
                    {
                        let partitions = self.proved_range_partitions(
                            captured.start.capture,
                            &image.start,
                            &image.end,
                            states,
                        );
                        let outcome = formation_start + 1;
                        for (index, partition) in partitions.iter().enumerate() {
                            for (base, parent) in [
                                (false, partition.stride_nonnegative),
                                (true, partition.base_nonnegative),
                            ] {
                                self.derivations.add_root(
                                    DerivationRootKind::RangePartition {
                                        obligation: u32::try_from(outcome)
                                            .expect("ENT obligation ordinal exceeds u32"),
                                        partition: u32::try_from(index)
                                            .expect("range partition ordinal exceeds u32"),
                                        base,
                                    },
                                    parent,
                                );
                            }
                        }
                        self.obligations[outcome].range_partitions = partitions;
                    }
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reaches_endpoints
                        && source_subscripts
                        && self.obligations_since_discharged(obligation_start),
                }
            }
            // that place's own subscripts are discharged [OP-4].
            CheckedExpression::ContainerMeasure { root, .. }
            | CheckedExpression::BorrowAddressed { root, .. } => {
                let obligation_start = self.obligations.len();
                let reached = self.judge_place_subscripts(root, states);
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reached && self.obligations_since_discharged(obligation_start),
                }
            }
            // [OP-4, WIN-1] a run's subscript owes `i < len_of(v)` wherever it
            // is written: the offset is a logical one and the window's length
            // bounds it, so the measured kind is the run's own and the written
            // capacity is not the bound. A read owes exactly what the
            // element-position target below owes, and is judged here.
            CheckedExpression::ReadStorage { root, .. } => {
                let reached = self.judge_place_subscripts(root, states);
                ExpressionJudgment {
                    prepared_call: None,
                    reached,
                }
            }
            CheckedExpression::IntegerOperation {
                carrier,
                operation,
                operand_type,
                arguments,
                ..
            } => {
                let reaches_operation = self.judge_children_reach_parent(arguments, states);
                let obligation_start = self.obligations.len();
                if operation.is_exact() && reaches_operation {
                    self.judge_integer_domain_obligation(
                        *operation,
                        *operand_type,
                        arguments,
                        carrier,
                        states,
                    );
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reaches_operation
                        && self.obligations_since_discharged(obligation_start),
                }
            }
            CheckedExpression::NumericConversion {
                carrier,
                mode: CheckedConversionMode::Exact,
                source,
                destination,
                value,
                ..
            } => {
                let reaches_operation = self.judge_children_reach_parent([value.as_ref()], states);
                let obligation_start = self.obligations.len();
                if reaches_operation {
                    self.judge_conversion_domain_obligation(
                        *source,
                        *destination,
                        value,
                        carrier,
                        states,
                    );
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reaches_operation
                        && self.obligations_since_discharged(obligation_start),
                }
            }
            _ => {
                let reached =
                    self.judge_children_reach_parent(expression_children(expression), states);
                ExpressionJudgment {
                    prepared_call: None,
                    reached,
                }
            }
        };
        if let Some(carrier) = expression.carrier() {
            judgment.reached &= self.judge_call_separations(carrier, states);
        }
        judgment
    }

    fn judge_call_goal(
        &mut self,
        callee: super::super::model::FunctionId,
        node_path: &crate::NodePath,
        requires_clause: crate::NodePath,
        goal: ConcreteGoal,
        argument_count: usize,
        (context, written): (ProofContext<'_>, &ProofFlowState),
    ) -> (CallGoalDisposition, Option<DerivationId>) {
        let (disposition, evidence, derivation) = self.call_goal_disposition(&goal, context);
        let ordinal = u32::try_from(self.call_goals.len())
            .expect("ENT call-root ordinal exceeds the u32 identity space");
        if let Some(root) = derivation {
            self.derivations
                .add_root(DerivationRootKind::CallGoal(ordinal), root);
        }
        let rendered_goal = self.render_concrete_goal(&goal.root);
        self.call_goals.push(CallGoalOutcome {
            node_path: node_path.clone(),
            callee,
            requires_clause,
            goal,
            rendered_goal,
            argument_count: u32::try_from(argument_count)
                .expect("ENT call argument count exceeds the u32 identity space"),
            disposition,
            evidence,
            derivation,
            written_before: written.written_before(disposition == CallGoalDisposition::Discharged),
        });
        (disposition, derivation)
    }

    /// Retains the external FN-4 authority that turns proofs of the formal
    /// requirements into permission to execute the selected actual [FN-5].
    /// The referenced query has its own proof arena; this caller-local node
    /// contains only the exact query ID and the caller proofs of its formal
    /// premises.
    fn retain_contract_call_authorities(
        &mut self,
        call: &crate::NodePath,
        contract: &super::super::model::CheckedCallContract,
        parents: &[DerivationId],
    ) -> Option<Vec<DerivationId>> {
        let premise_paths = contract
            .requirements
            .iter()
            .map(|requirement| &requirement.clause)
            .collect::<Vec<_>>();
        if premise_paths.len() != parents.len() {
            return None;
        }
        let mut retained = Vec::with_capacity(contract.requirement_queries.len());
        for query_id in &contract.requirement_queries {
            let query = self.context.contract_query(*query_id)?;
            let [outcome] = query.proof.contract_goals.as_slice() else {
                return None;
            };
            if query.instance != Some(self.function.id)
                || outcome.disposition != CallGoalDisposition::Discharged
                || outcome.derivation.is_none()
                || query.premises.iter().ne(premise_paths.iter().copied())
            {
                return None;
            }
            let node = self
                .derivations
                .intern(super::state::DerivationNode::ContractCall {
                    call: call.clone(),
                    query: *query_id,
                    parents: parents.to_vec(),
                });
            let occurrence = self.contract_call_roots;
            self.contract_call_roots = self
                .contract_call_roots
                .checked_add(1)
                .expect("FN-4 call roots exceed the u32 identity space");
            self.derivations
                .add_root(DerivationRootKind::CallContract(occurrence), node);
            retained.push(node);
        }
        Some(retained)
    }

    fn call_goal_disposition(
        &mut self,
        goal: &ConcreteGoal,
        context: ProofContext<'_>,
    ) -> (
        CallGoalDisposition,
        Vec<CallGoalEvidence>,
        Option<DerivationId>,
    ) {
        let affine_target = self.affine_goal_ordering_target(&goal.root, context.affine);
        let result = self.prove(
            context,
            ProofGoal::Signed {
                expression: &goal.root,
                affine: affine_target.as_ref(),
            },
        );
        let disposition = match result.disposition {
            ProofDisposition::Proved => CallGoalDisposition::Discharged,
            ProofDisposition::Refuted => CallGoalDisposition::Refuted,
            ProofDisposition::Unknown => CallGoalDisposition::Unproved,
        };
        let evidence = match (result.disposition, result.route) {
            (ProofDisposition::Proved, Some(ProofRoute::Contradiction)) => {
                vec![CallGoalEvidence::AllDerivable]
            }
            (
                sign @ (ProofDisposition::Proved | ProofDisposition::Refuted),
                Some(ProofRoute::SignedOrdinary {
                    opaque,
                    projection,
                    normalization,
                    introduction,
                }),
            ) => {
                let mut evidence = Vec::with_capacity(4);
                match sign {
                    ProofDisposition::Proved => {
                        if opaque {
                            evidence.push(CallGoalEvidence::OpaquePositive);
                        }
                        if projection {
                            evidence.push(CallGoalEvidence::ExactL0Projection);
                        }
                        if normalization {
                            evidence.push(CallGoalEvidence::NormalizationPositive);
                        }
                        if introduction {
                            evidence.push(CallGoalEvidence::BooleanIntroductionPositive);
                        }
                    }
                    ProofDisposition::Refuted => {
                        if opaque {
                            evidence.push(CallGoalEvidence::OpaqueNegative);
                        }
                        if projection {
                            evidence.push(CallGoalEvidence::NegatedL0Projection);
                        }
                        if normalization {
                            evidence.push(CallGoalEvidence::NormalizationNegative);
                        }
                        if introduction {
                            evidence.push(CallGoalEvidence::BooleanIntroductionNegative);
                        }
                    }
                    ProofDisposition::Unknown => unreachable!(),
                }
                evidence
            }
            (ProofDisposition::Proved, Some(ProofRoute::Affine)) => {
                vec![CallGoalEvidence::AffinePositive]
            }
            (ProofDisposition::Unknown, None) => Vec::new(),
            _ => unreachable!("a signed proof returned an incompatible route"),
        };
        (disposition, evidence, result.derivation)
    }

    /// Unified deterministic entry for numeric/logical entailment.  The
    /// consumer supplies one normalized proposition; this function tries the
    /// fixed ordinary closure before the fixed affine rule and constructs the
    /// selected derivation during that same query.
    fn prove(&mut self, context: ProofContext<'_>, goal: ProofGoal<'_>) -> ProofResult {
        match goal {
            ProofGoal::Affine { inequality, right } => {
                self.prove_affine(context, inequality, right)
            }
            ProofGoal::AutomaticAffine { inequality } => {
                self.prove_affine(context, inequality, None)
            }
            ProofGoal::Signed { expression, affine } => {
                self.prove_signed(context, expression, affine)
            }
            ProofGoal::Ordering { relation, affine } => {
                self.prove_ordering(context, relation, affine)
            }
            ProofGoal::IntegerDomain(goal) => self.prove_integer_domain(context, goal),
            ProofGoal::ConversionDomain {
                canonical,
                operand,
                image,
            } => self.prove_conversion_domain(context, canonical, operand, image),
            ProofGoal::BoundedRelation(goal) => self.prove_bounded_relation(context, goal),
            ProofGoal::NormalizedOrdering {
                goal,
                relation,
                affine,
                right,
                upper_bound,
            } => {
                let proof = self.prove_normalized_ordering(&context, goal, relation, affine, right);
                self.project_numeric_upper_bound(&context, proof, upper_bound)
            }
        }
    }

    fn prove_affine(
        &mut self,
        context: ProofContext<'_>,
        inequality: &AffineInequality,
        right: Option<TermId>,
    ) -> ProofResult {
        let closed = context.close(&self.terms, &self.goals, &mut self.derivations);
        if closed.contradictory() {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: closed.contradiction_proof(),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }
        let Some(proof) = self.numeric_affine_proof(inequality, right, context) else {
            return ProofResult {
                disposition: ProofDisposition::Unknown,
                route: None,
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        };
        let derivation = self.derivations.intern(DerivationNode::AffineConsequence {
            relation: None,
            premises: proof.premises.into_boxed_slice(),
            parents: proof.parents,
        });
        ProofResult {
            disposition: ProofDisposition::Proved,
            route: Some(ProofRoute::Affine),
            derivation: Some(derivation),
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    fn prove_signed(
        &mut self,
        context: ProofContext<'_>,
        expression: &GoalExpression,
        affine_target: Option<&AffineInequality>,
    ) -> ProofResult {
        let goal = self.intern_goal_expression(expression.clone());
        let closed = close(
            context.facts,
            &self.terms,
            &self.goals,
            &mut self.derivations,
        );
        if closed.contradictory() {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: closed.contradiction_proof(),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        // Conversion domains give an established negative identity priority
        // over the independent sufficient-range normalization.
        if matches!(
            expression,
            GoalExpression::Operation {
                row: GoalOperation::NumericConversion {
                    mode: CheckedConversionMode::Defined,
                    ..
                },
                ..
            }
        ) && closed.holds_opaque(goal, GoalSign::Negative)
            && !closed.holds_opaque(goal, GoalSign::Positive)
        {
            return ProofResult {
                disposition: ProofDisposition::Refuted,
                route: Some(ProofRoute::SignedOrdinary {
                    opaque: true,
                    projection: false,
                    normalization: false,
                    introduction: false,
                }),
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        let positive_opaque = closed.holds_opaque(goal, GoalSign::Positive);
        let positive_projection = self
            .goals
            .projection(goal)
            .is_some_and(|relation| closed.derives(relation));
        let positive_normalization =
            closed.derives_normalized_goal(goal, GoalSign::Positive, &self.goals);
        let positive_introduction = !positive_opaque
            && !positive_projection
            && !positive_normalization
            && closed.derives_goal(goal, GoalSign::Positive, &self.goals);
        if positive_opaque || positive_projection || positive_normalization || positive_introduction
        {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::SignedOrdinary {
                    opaque: positive_opaque,
                    projection: positive_projection,
                    normalization: positive_normalization,
                    introduction: positive_introduction,
                }),
                derivation: closed.goal_proof(
                    goal,
                    GoalSign::Positive,
                    &self.goals,
                    &mut self.derivations,
                ),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        let negative_opaque = closed.holds_opaque(goal, GoalSign::Negative);
        let negative_projection = self
            .goals
            .projection(goal)
            .is_some_and(|relation| closed.derives(&relation.negated()));
        let negative_normalization =
            closed.derives_normalized_goal(goal, GoalSign::Negative, &self.goals);
        let negative_introduction = !negative_opaque
            && !negative_projection
            && !negative_normalization
            && closed.derives_goal(goal, GoalSign::Negative, &self.goals);
        if negative_opaque || negative_projection || negative_normalization || negative_introduction
        {
            return ProofResult {
                disposition: ProofDisposition::Refuted,
                route: Some(ProofRoute::SignedOrdinary {
                    opaque: negative_opaque,
                    projection: negative_projection,
                    normalization: negative_normalization,
                    introduction: negative_introduction,
                }),
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        // The affine target is this goal's own comparison, normalized. Proving
        // it proves the goal, so the L0 projection is what the evidence names,
        // not what the route needs: a goal that carries a coefficient has no
        // two-term projection to name and instead retains the exact signed
        // goal above its affine consequence.
        if let Some(target) = affine_target {
            let projection = self.goals.projection(goal).cloned();
            let right = self.signed_goal_right_term(expression, GoalSign::Positive);
            if let Some(proof) = self.numeric_affine_proof(target, right, context) {
                let consequence = self.derivations.intern(DerivationNode::AffineConsequence {
                    relation: projection.clone().map(Box::new),
                    premises: proof.premises.into_boxed_slice(),
                    parents: proof.parents,
                });
                let derivation = match projection {
                    Some(relation) => self.derivations.intern(DerivationNode::GoalProjection {
                        goal,
                        sign: GoalSign::Positive,
                        relation,
                        parent: consequence,
                    }),
                    None => self
                        .derivations
                        .intern(DerivationNode::GoalAffineConsequence {
                            goal,
                            sign: GoalSign::Positive,
                            parent: consequence,
                        }),
                };
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::Affine),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        let derivation = self.signed_goal_affine_proof(
            context,
            expression,
            GoalSign::Positive,
            &closed,
            &mut HashSet::new(),
        );
        ProofResult {
            disposition: if derivation.is_some() {
                ProofDisposition::Proved
            } else {
                ProofDisposition::Unknown
            },
            route: derivation.map(|_| ProofRoute::Affine),
            derivation,
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    /// Proves one signed ordering leaf through the ordinary affine entry and
    /// projects the result back to the exact interned Goal. This is the leaf
    /// case used by the fixed Boolean recursion below.
    fn affine_signed_goal_leaf_proof(
        &mut self,
        context: ProofContext<'_>,
        expression: &GoalExpression,
        goal: GoalId,
        sign: GoalSign,
    ) -> Option<DerivationId> {
        let target = self.affine_signed_goal_ordering_target(expression, context.affine, sign)?;
        let mut relation = self.goals.projection(goal).cloned();
        if sign == GoalSign::Negative {
            relation = relation.map(|relation| relation.negated());
        }
        let right = self.signed_goal_right_term(expression, sign);
        let proof = self.numeric_affine_proof(&target, right, context)?;
        let consequence = self.derivations.intern(DerivationNode::AffineConsequence {
            relation: relation.clone().map(Box::new),
            premises: proof.premises.into_boxed_slice(),
            parents: proof.parents,
        });
        Some(self.derivations.intern(match relation {
            Some(relation) => DerivationNode::GoalProjection {
                goal,
                sign,
                relation,
                parent: consequence,
            },
            None => DerivationNode::GoalAffineConsequence {
                goal,
                sign,
                parent: consequence,
            },
        }))
    }

    /// Extends the existing finite Boolean introduction rule with affine
    /// ordering leaves. The recursion follows the closed truth table exactly:
    /// conjunction requires every positive child, disjunction every negative
    /// child, the opposite signs require one witness, and `not` flips sign.
    /// It performs no premise, coefficient, or path search.
    fn signed_goal_affine_proof(
        &mut self,
        context: ProofContext<'_>,
        expression: &GoalExpression,
        sign: GoalSign,
        closed: &ClosedState,
        visiting: &mut HashSet<(GoalId, GoalSign)>,
    ) -> Option<DerivationId> {
        let goal = self.intern_goal_expression(expression.clone());
        if let Some(proof) = closed.goal_proof(goal, sign, &self.goals, &mut self.derivations) {
            return Some(proof);
        }
        if !visiting.insert((goal, sign)) {
            return None;
        }

        let proof = match expression {
            GoalExpression::Operation {
                row:
                    GoalOperation::NumericConversion {
                        mode: CheckedConversionMode::Defined,
                        ..
                    },
                ..
            } if sign == GoalSign::Positive => {
                self.conversion_goal_bound_proof(context, expression, goal)
            }
            GoalExpression::Operation {
                row: GoalOperation::Boolean(operation),
                arguments,
                ..
            } => {
                let child_sign = match (operation, sign) {
                    (CheckedBooleanOperation::And, GoalSign::Positive)
                    | (CheckedBooleanOperation::Or, GoalSign::Positive) => GoalSign::Positive,
                    (CheckedBooleanOperation::And, GoalSign::Negative)
                    | (CheckedBooleanOperation::Or, GoalSign::Negative) => GoalSign::Negative,
                    (CheckedBooleanOperation::Not, GoalSign::Positive) => GoalSign::Negative,
                    (CheckedBooleanOperation::Not, GoalSign::Negative) => GoalSign::Positive,
                    (CheckedBooleanOperation::ExclusiveOr, _) => {
                        visiting.remove(&(goal, sign));
                        return None;
                    }
                };
                let requires_all = matches!(
                    (operation, sign),
                    (CheckedBooleanOperation::And, GoalSign::Positive)
                        | (CheckedBooleanOperation::Or, GoalSign::Negative)
                        | (CheckedBooleanOperation::Not, _)
                );
                let parents = if requires_all {
                    let mut parents = Vec::with_capacity(arguments.len());
                    let mut complete = true;
                    for argument in arguments {
                        let Some(parent) = self.signed_goal_affine_proof(
                            context, argument, child_sign, closed, visiting,
                        ) else {
                            complete = false;
                            break;
                        };
                        parents.push(parent);
                    }
                    complete.then_some(parents)
                } else {
                    let mut best = None;
                    for argument in arguments {
                        let Some(candidate) = self.signed_goal_affine_proof(
                            context, argument, child_sign, closed, visiting,
                        ) else {
                            continue;
                        };
                        // Existential Boolean introductions use the first
                        // successful child in source order. Later witnesses
                        // cannot change acceptance, only diagnostics.
                        if best.is_none() {
                            best = Some(candidate);
                        }
                    }
                    best.map(|parent| vec![parent])
                };
                parents.map(|parents| {
                    self.derivations
                        .intern(DerivationNode::BooleanIntroduction {
                            goal,
                            sign,
                            parents,
                        })
                })
            }
            GoalExpression::Operation { .. } => {
                self.affine_signed_goal_leaf_proof(context, expression, goal, sign)
            }
            GoalExpression::Datum(_) => None,
        };
        visiting.remove(&(goal, sign));
        proof
    }

    fn prove_ordering(
        &mut self,
        context: ProofContext<'_>,
        relation: &Relation,
        affine_target: Option<&[AffineInequality]>,
    ) -> ProofResult {
        let closed = context.close(&self.terms, &self.goals, &mut self.derivations);
        if closed.contradictory() {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: closed.contradiction_proof(),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }
        if closed.derives(relation) {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::L0),
                derivation: Some(
                    closed
                        .relation_proof(relation, &mut self.derivations)
                        .expect("a proved L0 relation must retain its local derivation"),
                ),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }
        if closed.derives(&relation.negated()) {
            return ProofResult {
                disposition: ProofDisposition::Refuted,
                route: Some(ProofRoute::L0),
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        let Some(targets) = affine_target else {
            return ProofResult {
                disposition: ProofDisposition::Unknown,
                route: None,
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        };
        let mut premises = Vec::new();
        let mut parents = Vec::new();
        for (ordinal, target) in targets.iter().enumerate() {
            let right = match relation {
                Relation::Bound { right, .. } if ordinal == 0 => Some(*right),
                Relation::Equal { left, right, .. } => match ordinal {
                    0 => Some(*right),
                    1 => Some(*left),
                    _ => None,
                },
                _ => None,
            };
            let Some(proof) = self.numeric_affine_proof(target, right, context) else {
                return ProofResult {
                    disposition: ProofDisposition::Unknown,
                    route: None,
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            };
            premises.extend(proof.premises);
            parents.extend(proof.parents);
        }
        let derivation = self.derivations.intern(DerivationNode::AffineConsequence {
            relation: None,
            premises: premises.into_boxed_slice(),
            parents,
        });
        ProofResult {
            disposition: ProofDisposition::Proved,
            route: Some(ProofRoute::Affine),
            derivation: Some(derivation),
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    fn prove_bounded_relation(
        &mut self,
        context: ProofContext<'_>,
        goal: BoundedRelationGoal<'_>,
    ) -> ProofResult {
        let canonical = goal
            .canonical
            .map(|expression| self.intern_goal_expression(expression.clone()));
        let closed = close(
            context.facts,
            &self.terms,
            &self.goals,
            &mut self.derivations,
        );
        if closed.contradictory() {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: closed.contradiction_proof(),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }
        if let Some(canonical) = canonical {
            if closed.holds_opaque(canonical, GoalSign::Positive) {
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: closed.opaque_proof(canonical, GoalSign::Positive),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
            if closed.holds_opaque(canonical, GoalSign::Negative) {
                return ProofResult {
                    disposition: ProofDisposition::Refuted,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        let relation = goal.request.as_ref().and_then(request_relation);
        if let Some(relation) = relation.as_ref() {
            if closed.derives(relation) {
                let parent = closed
                    .relation_proof(relation, &mut self.derivations)
                    .expect("a proved L0 relation must retain its local derivation");
                let derivation = self.goal_numeric_derivation(canonical, Some(relation), parent);
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::L0),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
            if closed.derives(&relation.negated()) {
                return ProofResult {
                    disposition: ProofDisposition::Refuted,
                    route: Some(ProofRoute::L0),
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        if let Some(target) = goal.direct_affine {
            let assumptions = Self::affine_facts(context.affine);
            if let Some(proof) = self.affine_target_proof(target, &assumptions, context) {
                let parent = self.derivations.intern(DerivationNode::AffineConsequence {
                    relation: relation.clone().map(Box::new),
                    premises: proof.premises.into_boxed_slice(),
                    parents: proof.parents,
                });
                let derivation = self.goal_numeric_derivation(canonical, relation.as_ref(), parent);
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::Affine),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        if let Some(request) = goal.request
            && let Some(left) = request.left
        {
            if let Some(derivation) = goal.fixed_affine_bridge.and_then(|bridge| {
                self.fixed_affine_bound_derivation(
                    bridge,
                    left,
                    request.right,
                    request.bound,
                    context.affine,
                    context.facts,
                )
            }) {
                let derivation =
                    self.goal_numeric_derivation(canonical, relation.as_ref(), derivation);
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::Affine),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }

            if let Some(derivation) = goal.affine_left.and_then(|affine_left| {
                self.affine_bound_via_l0_right(
                    affine_left,
                    left,
                    request.right,
                    request.bound,
                    context.affine,
                    context.facts,
                )
            }) {
                let derivation =
                    self.goal_numeric_derivation(canonical, relation.as_ref(), derivation);
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::Affine),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        ProofResult {
            disposition: ProofDisposition::Unknown,
            route: None,
            derivation: None,
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    fn goal_numeric_derivation(
        &mut self,
        goal: Option<GoalId>,
        relation: Option<&Relation>,
        parent: DerivationId,
    ) -> DerivationId {
        let Some(goal) = goal else {
            return parent;
        };
        if let Some(relation) = relation
            && self.goals.projection(goal) == Some(relation)
        {
            return self.derivations.intern(DerivationNode::GoalProjection {
                goal,
                sign: GoalSign::Positive,
                relation: relation.clone(),
                parent,
            });
        }
        if let Some(relation) = relation
            && self.goals.normalization(goal).is_some_and(|normalization| {
                normalization.clause_is_single_relation(GoalSign::Positive, 0, relation)
            })
        {
            return self.derivations.intern(DerivationNode::GoalNormalization {
                goal,
                sign: GoalSign::Positive,
                clause: 0,
                parents: vec![parent],
            });
        }
        // A bounded obligation may have an exact complete goal whose source
        // occurrence is represented in L0 only through an evaluated alias.
        // The relation remains the obligation's direct proof root; recording
        // it as this globally interned goal's normalization would attach
        // occurrence-local data to a shared identity.
        if relation.is_some() {
            return parent;
        }
        self.derivations
            .intern(DerivationNode::GoalAffineConsequence {
                goal,
                sign: GoalSign::Positive,
                parent,
            })
    }

    /// Proves one canonical ordering that may also be represented by a finite
    /// normalized goal. The goal identity has ordinary priority when present;
    /// the bare L0 relation is the fallback only for source operands outside
    /// that goal fragment. The affine route proves the same written relation
    /// and, when needed, concludes the exact goal normalization in the same
    /// call.
    fn prove_normalized_ordering(
        &mut self,
        context: &ProofContext<'_>,
        goal: Option<GoalId>,
        relation: Option<&Relation>,
        affine_target: Option<&AffineInequality>,
        right: Option<TermId>,
    ) -> ProofResult {
        let closed = close(
            context.facts,
            &self.terms,
            &self.goals,
            &mut self.derivations,
        );
        if closed.contradictory() {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: closed.contradiction_proof(),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }
        if let Some(goal) = goal {
            if closed.holds_opaque(goal, GoalSign::Positive) {
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: closed.opaque_proof(goal, GoalSign::Positive),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
            if closed.holds_opaque(goal, GoalSign::Negative) {
                return ProofResult {
                    disposition: ProofDisposition::Refuted,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }
        if let Some(relation) = relation {
            if closed.derives(relation) {
                let parent = closed
                    .relation_proof(relation, &mut self.derivations)
                    .expect("a proved L0 relation must retain its local derivation");
                let derivation = self.goal_numeric_derivation(goal, Some(relation), parent);
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::L0),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
            if closed.derives(&relation.negated()) {
                return ProofResult {
                    disposition: ProofDisposition::Refuted,
                    route: Some(ProofRoute::L0),
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        let Some(target) = affine_target else {
            return ProofResult {
                disposition: ProofDisposition::Unknown,
                route: None,
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        };
        let Some(proof) = self.numeric_affine_proof(target, right, *context) else {
            return ProofResult {
                disposition: ProofDisposition::Unknown,
                route: None,
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        };
        let consequence = self.derivations.intern(DerivationNode::AffineConsequence {
            relation: relation.cloned().map(Box::new),
            premises: proof.premises.into_boxed_slice(),
            parents: proof.parents,
        });
        let derivation = self.goal_numeric_derivation(goal, relation, consequence);
        ProofResult {
            disposition: ProofDisposition::Proved,
            route: Some(ProofRoute::Affine),
            derivation: Some(derivation),
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    /// Projects the tightest numeric ceiling available from the same proof
    /// context after, and only after, the normalized ordering was proved.
    /// This is not another admission query: the selected `ProofResult` remains
    /// the sole acceptance authority. The projection merely chooses between
    /// the ordering's own admitted ceiling, the ordinary closure, and the
    /// fixed affine interval rule, retaining the derivation for the chosen
    /// number.
    fn project_numeric_upper_bound(
        &mut self,
        context: &ProofContext<'_>,
        mut proof: ProofResult,
        request: Option<NumericUpperBoundRequest<'_>>,
    ) -> ProofResult {
        let Some(request) = request else {
            return proof;
        };
        if proof.disposition != ProofDisposition::Proved {
            return proof;
        }
        let Some(admission_derivation) = proof.derivation else {
            return proof;
        };
        if proof.route == Some(ProofRoute::Contradiction) {
            proof.numeric_upper_bound = Some(ProvedNumericUpperBound {
                value: 0,
                derivation: admission_derivation,
            });
            return proof;
        }

        let mut selected = ProvedNumericUpperBound {
            value: request.admitted,
            derivation: admission_derivation,
        };

        if let Some(term) = request.term {
            let closed = close(
                context.facts,
                &self.terms,
                &self.goals,
                &mut self.derivations,
            );
            if let Some(candidate) = closed.tight_bound(term, ZERO)
                && candidate < selected.value
                && let Some(derivation) =
                    closed.bound_proof(term, ZERO, candidate, &mut self.derivations)
            {
                selected = ProvedNumericUpperBound {
                    value: candidate,
                    derivation,
                };
            }
        }

        if let Some(form) = request.affine {
            let assumptions = Self::affine_facts(context.affine);
            if let Some(endpoint) = self
                .affine_closed_interval_proof(form, &assumptions, context.affine, context.facts)
                .map(|interval| interval.maximum)
                && endpoint.value < selected.value
            {
                let relation = request.term.map(|left| {
                    Box::new(Relation::Bound {
                        left,
                        right: ZERO,
                        bound: endpoint.value,
                    })
                });
                let derivation = self.derivations.intern(DerivationNode::AffineConsequence {
                    relation,
                    premises: endpoint.consequence.premises.into_boxed_slice(),
                    parents: endpoint.consequence.parents,
                });
                selected = ProvedNumericUpperBound {
                    value: endpoint.value,
                    derivation,
                };
            }
        }

        proof.numeric_upper_bound = Some(selected);
        proof
    }

    fn array_root_place(&self, root: &CheckedArrayRoot) -> ResolvedPlace {
        match root {
            CheckedArrayRoot::Binding { binding, fields } => ResolvedPlace::spelled(
                PlaceRoot::Binding(*binding),
                self.is_holder(*binding),
                fields.clone(),
            ),
            CheckedArrayRoot::Constant(id) => {
                ResolvedPlace::spelled(PlaceRoot::Constant(*id), false, Vec::new())
            }
        }
    }

    /// Interns one measure term over a place spelled in the compact
    /// [`ResolvedPlace`] form, with [MSR-2]'s standing facts.
    fn place_measure_term(
        &mut self,
        measure: CheckedMeasure,
        base: ResolvedPlace,
        measured: MeasuredKind,
        array_length: Option<CheckedConst>,
    ) -> TermId {
        self.measure_term(measure, base, measured, array_length)
    }

    /// [OP-4, MSR-4, INV-1] the same obligation, over the measure places one
    /// written affine relation names.
    ///
    /// An invariant evaluates nothing and reads no storage, but
    /// `len_of(table[i])` is a term there on exactly the terms it is one at a
    /// measure former the program executes: a measure over a place whose
    /// subscripts are not all discharged is no term, so the relation names a
    /// slot the run has or it names nothing. The judgment is made once, at
    /// the point the relation is written — a `loop`'s header invariant in its
    /// entering context, a local `invariant` at its own statement.
    fn judge_affine_relation_subscripts(
        &mut self,
        relation: &CheckedAffineRelation,
        states: &mut ProofFlowState,
    ) {
        for side in [&relation.left, &relation.right] {
            self.judge_affine_expression_subscripts(side, states);
        }
    }

    /// [ENT-2, FN-8] the subscripts of the clause (b) places one requirement
    /// forms, each owing [OP-4] against the base it indexes in the body-entry
    /// state holding the requirements written before it. A clause evaluates
    /// nothing, so only the obligations are judged, exactly as the same
    /// place's read or measure would judge them in the body.
    fn judge_clause_places(&mut self, places: &[CheckedExpression], states: &mut ProofFlowState) {
        for place in places {
            match place {
                CheckedExpression::ContainerMeasure { root, .. }
                | CheckedExpression::ReadStorage { root, .. } => {
                    self.judge_place_subscripts(root, states);
                }
                CheckedExpression::RangeElementMeasure { place, .. }
                | CheckedExpression::RangeIndex { place, .. } => {
                    self.judge_range_element_place(place, states);
                }
                _ => {}
            }
        }
    }

    fn judge_affine_expression_subscripts(
        &mut self,
        expression: &CheckedAffineExpression,
        states: &mut ProofFlowState,
    ) {
        for expression in expression.postorder() {
            if let CheckedAffineExpressionKind::Measure(measure) = &expression.kind {
                match measure.as_ref() {
                    CheckedExpression::ContainerMeasure { root, .. } => {
                        self.judge_place_subscripts(root, states);
                    }
                    CheckedExpression::RangeElementMeasure { place, .. } => {
                        self.judge_range_element_place(place, states);
                    }
                    _ => {}
                }
            }
        }
    }

    /// [OP-4, MSR-4] the obligation each subscript occurring *inside* a
    /// measured place owes, judged where the place is formed.
    ///
    /// `len_of(table[i])` names a place whose own subscript is an ordinary
    /// [OP-4] occurrence: it is discharged against `len_of(table)`, over the
    /// prefix of the path that reaches its base, and the measure term itself
    /// exists only where every one of them is discharged.
    /// Snapshot one binding-valued index at the point its place/call actual is
    /// evaluated. The immutable term lets pre-kill closure preserve exactly
    /// what was known about that occurrence without later rereading binding
    /// storage.
    fn establish_index_capture(
        &mut self,
        captured: CapturedValue,
        expression: &CheckedExpression,
        state: &mut ProofFlowState,
    ) {
        if !matches!(captured.capture, CaptureId::Source(_))
            || !matches!(captured.term, CapturedTerm::Binding(_))
        {
            return;
        }
        let kind = TermKind::IndexCapture {
            capture: captured.capture,
        };
        if self.terms.interned(&kind).is_some() {
            return;
        }
        let image = self.affine_expression_form(expression, &mut state.affine);
        let Some(source) = self.read_operand(expression) else {
            return;
        };
        let datum = self.terms.intern(kind);
        if let Some(image) = image {
            state.affine.indices.insert(captured.capture, image);
        }
        let event = self.proof_event(FlowEventKind::S13, expression.carrier());
        state.facts.establish(
            &Relation::Equal {
                left: datum,
                right: source,
                difference: 0,
            },
            &mut self.derivations,
            event,
        );
    }

    fn judge_place_subscripts(
        &mut self,
        root: &CheckedContainerRoot,
        states: &mut ProofFlowState,
    ) -> bool {
        let mut projections = Vec::new();
        if root
            .binding()
            .is_some_and(|binding| self.is_holder(binding))
        {
            projections.push(PlaceStep::Deref);
        }
        let mut reached = true;
        for step in &root.path {
            match step {
                CheckedPlaceStep::Field(field) => {
                    projections.push(PlaceStep::Field(*field));
                }
                CheckedPlaceStep::BoxReferent(_) => {
                    projections.push(PlaceStep::Deref);
                }
                CheckedPlaceStep::Subscript(subscript) => {
                    let Some(measured) = measured_kind(subscript.base_type) else {
                        return false;
                    };
                    let base = ResolvedPlace {
                        root: root.root,
                        path: projections.clone(),
                    };
                    let reaches_offset = self
                        .judge_children_reach_parent(std::iter::once(&subscript.offset), states);
                    let obligation_start = self.obligations.len();
                    if reaches_offset {
                        self.establish_index_capture(subscript.captured, &subscript.offset, states);
                        self.judge_obligation(
                            base,
                            measured,
                            type_constant(subscript.base_type),
                            &subscript.offset,
                            subscript.obligation.clone(),
                            states,
                        );
                    }
                    reached = reached
                        && reaches_offset
                        && self.obligations_since_discharged(obligation_start);
                    projections.push(PlaceStep::Index(subscript.captured));
                }
            }
        }
        reached
    }

    /// [OP-4, MSR-4] the outer range subscript and every nested subscript in
    /// one measured element place, in source order. The proof base remains
    /// the range holder until ordinary unique-origin resolution substitutes
    /// it; a joined holder is never reduced to one candidate here [ENT-5].
    fn judge_range_element_place(
        &mut self,
        place: &super::super::model::CheckedRangeElementPlace,
        states: &mut ProofFlowState,
    ) -> bool {
        let mut base = ResolvedPlace::spelled(
            PlaceRoot::Binding(place.root.binding),
            self.is_holder(place.root.binding),
            Vec::new(),
        );
        let reaches_offset =
            self.judge_children_reach_parent(std::iter::once(&place.offset), states);
        let obligation_start = self.obligations.len();
        if reaches_offset {
            self.establish_index_capture(place.captured, &place.offset, states);
            self.judge_obligation(
                base.clone(),
                MeasuredKind::Range,
                None,
                &place.offset,
                place.obligation.clone(),
                states,
            );
        }
        let mut reached = reaches_offset && self.obligations_since_discharged(obligation_start);
        if !reached {
            return false;
        }
        base.path.push(PlaceStep::Index(place.captured));
        for step in &place.path {
            match step {
                CheckedPlaceStep::Field(field) => base.path.push(PlaceStep::Field(*field)),
                CheckedPlaceStep::BoxReferent(_) => base.path.push(PlaceStep::Deref),
                CheckedPlaceStep::Subscript(subscript) => {
                    let Some(measured) = measured_kind(subscript.base_type) else {
                        return false;
                    };
                    let reaches_offset = self
                        .judge_children_reach_parent(std::iter::once(&subscript.offset), states);
                    let obligation_start = self.obligations.len();
                    if reaches_offset {
                        self.establish_index_capture(subscript.captured, &subscript.offset, states);
                        self.judge_obligation(
                            base.clone(),
                            measured,
                            type_constant(subscript.base_type),
                            &subscript.offset,
                            subscript.obligation.clone(),
                            states,
                        );
                    }
                    reached = reached
                        && reaches_offset
                        && self.obligations_since_discharged(obligation_start);
                    if !reached {
                        return false;
                    }
                    base.path.push(PlaceStep::Index(subscript.captured));
                }
            }
        }
        reached
    }

    /// The exact place one measured or subscripted root names [MSR-2].
    ///
    /// A run's path may carry subscripts of its own — `len_of(table[i])` is a
    /// term [MSR-1] — so it is a source-order projection path and never a
    /// field list.
    fn container_root_path(&self, root: &CheckedContainerRoot) -> ResolvedPlace {
        let mut place = ResolvedPlace {
            root: root.root,
            path: Vec::new(),
        };
        place.path.extend(root.place_path());
        place
    }

    /// The place one [SET-1] commit writes, as every measure term over it is
    /// stated [MSR-1]: a plain place, or one element position of a run.
    ///
    /// An element position is a place only where its offset is one a place
    /// relation can name [ENT-2] — a written literal, a live `own` integer
    /// binding, or an in-scope const generic. An offset of any other form is
    /// provably distinct from nothing, itself included, so a measure over it
    /// would relate two elements as one term [OWN-7] and there is no place to
    /// carry a measure to.
    fn set_target_place(&self, target: &CheckedSetTarget) -> Option<ResolvedPlace> {
        match target {
            CheckedSetTarget::Place(place) => Some(ResolvedPlace::spelled(
                PlaceRoot::Binding(place.binding),
                self.is_holder(place.binding),
                place.fields.clone(),
            )),
            CheckedSetTarget::Storage(target) => {
                if target
                    .place_path()
                    .contains(&PlaceStep::Index(CapturedValue::unknown()))
                {
                    return None;
                }
                Some(self.container_root_path(target))
            }
            CheckedSetTarget::RangeIndex(target) => {
                let path = target.place_path();
                if path.contains(&PlaceStep::Index(CapturedValue::unknown())) {
                    return None;
                }
                let mut place = ResolvedPlace::spelled(
                    PlaceRoot::Binding(target.root.binding),
                    self.is_holder(target.root.binding),
                    Vec::new(),
                );
                place.path.extend(path);
                Some(place)
            }
        }
    }

    /// [MSR-3] the datums one [SET-1] commit carries, minted before the
    /// statement's own kills.
    ///
    /// The right-hand side is a bare use of a measured place, which is the
    /// same shape the `let` rebind placement admits: the value keeps every
    /// measure it had and only the name it is reached by changes. Every other
    /// right-hand side mints none, and the ordinary sources establish
    /// whatever that expression publishes.
    fn mint_commit_placement(
        &mut self,
        node_path: &crate::NodePath,
        ordinal: u32,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) -> Option<MeasureCarry> {
        let source = self.placement_source_place(value)?;
        let destination = self.set_target_place(target)?;
        let placement = if matches!(destination.path.last(), Some(PlaceStep::Index(_))) {
            MeasurePlacement::Element
        } else {
            MeasurePlacement::Rebind
        };
        self.mint_measure_datums(node_path, ordinal, placement, source, value.ty(), state)
    }

    /// [MSR-3] the construct placement: the datums one `construct`'s field
    /// operands carry into the fields of the value they fill.
    ///
    /// A field whose operand is a bare use of a measured place carries that
    /// place's measures into the field, which is the one event at which a
    /// measured value enters a nominal it did not previously belong to. The
    /// operand shape admitted is the shape every other placement admits: the
    /// value keeps every measure it had and only the place it is reached by
    /// changes.
    fn mint_construct_placements(
        &mut self,
        node_path: &crate::NodePath,
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) -> Vec<(PlaceStep, MeasureCarry)> {
        let (fields, payload) = match value {
            CheckedExpression::ConstructStruct { fields, .. } => (fields, None),
            // [MSR-3] every enum variant has its own payload step. The
            // constructor selects the exact variant now, so its field
            // destination is `.Variant.field`, even when another variant
            // also carries fields.
            CheckedExpression::ConstructEnum {
                nominal,
                variant,
                fields,
                ..
            } => {
                let Some(nominal) = self.context.nominals.get(nominal.0 as usize) else {
                    return Vec::new();
                };
                let CheckedNominalKind::Enum { variants } = &nominal.kind else {
                    return Vec::new();
                };
                let Some(selected) = variants.get(*variant as usize) else {
                    return Vec::new();
                };
                let tag = selected.tag;
                (fields, Some(tag))
            }
            _ => return Vec::new(),
        };
        let mut carried = Vec::new();
        for (ordinal, field) in fields.iter().enumerate() {
            let Some(source) = self.placement_source_place(field) else {
                continue;
            };
            let ordinal = u32::try_from(ordinal).unwrap_or(u32::MAX);
            if let Some(carry) = self.mint_measure_datums(
                node_path,
                ordinal,
                MeasurePlacement::Construct,
                source,
                field.ty(),
                state,
            ) {
                let destination =
                    payload.map_or(PlaceStep::Field(ordinal), |variant| PlaceStep::Payload {
                        variant,
                        field: ordinal,
                    });
                carried.push((destination, carry));
            }
        }
        carried
    }

    /// [MSR-3] the construct placement's second half: after the statement's
    /// own kills, field i of the constructed value has the measures its
    /// operand had.
    fn establish_construct_placements(
        &mut self,
        node_path: &crate::NodePath,
        base: &ResolvedPlace,
        carried: &[(PlaceStep, MeasureCarry)],
        state: &mut FactState,
    ) {
        for (step, carry) in carried {
            let mut destination = base.clone();
            destination.path.push(*step);
            let destination = destination;
            self.establish_measure_datums(node_path, destination, carry, state);
        }
    }

    /// [MSR-3] the payload placement: the datums a `match` over an own enum
    /// place carries out of that place's payload.
    ///
    /// The `match` consumes the scrutinee, so a measure of the payload dies
    /// with it; the datum minted here is the value that measure had
    /// immediately before the consume, and the arm binder that names the
    /// payload receives it on its own arm.
    fn mint_payload_placements(
        &mut self,
        scrutinee: &CheckedExpression,
        enum_type: CheckedEnumType,
        state: &mut ProofFlowState,
    ) -> Vec<PayloadPlacement> {
        let Some(base) = self.placement_source_place(scrutinee) else {
            return Vec::new();
        };
        let Some(node_path) = scrutinee.carrier() else {
            return Vec::new();
        };
        let CheckedEnumType::Nominal(nominal) = enum_type else {
            return Vec::new();
        };
        let Some(nominal) = self.context.nominals.get(nominal.0 as usize) else {
            return Vec::new();
        };
        let CheckedNominalKind::Enum { variants } = &nominal.kind else {
            return Vec::new();
        };
        // Clone declaration data before minting terms through `self`.
        let variants = variants.clone();
        let mut placements = Vec::new();
        // [MSR-3] `ordinal within that statement` counts payload binders
        // across the whole match, not separately inside each variant. Two
        // variants' field zero are different naming events and must mint
        // different immutable datums.
        let mut placement_ordinal = 0u32;
        for variant in variants {
            let mut carried = Vec::new();
            for (ordinal, field) in variant.fields.iter().enumerate() {
                let field_ordinal = u32::try_from(ordinal).unwrap_or(u32::MAX);
                let mut source = base.clone();
                source.path.push(PlaceStep::Payload {
                    variant: variant.tag,
                    field: field_ordinal,
                });
                if let Some(carry) = self.mint_measure_datums(
                    node_path,
                    placement_ordinal,
                    MeasurePlacement::Payload,
                    source,
                    field.ty,
                    state,
                ) {
                    carried.push((field_ordinal, carry));
                }
                placement_ordinal = placement_ordinal
                    .checked_add(1)
                    .expect("payload placement ordinal exceeds u32");
            }
            if !carried.is_empty() {
                placements.push(PayloadPlacement {
                    tag: variant.tag,
                    carried,
                });
            }
        }
        placements
    }

    /// [MSR-3] the destructuring placement: the datums a destructuring
    /// consume carries out of the fields it takes apart.
    ///
    /// The operand is a bare use of a measured nominal place — `let N(f: a)
    /// = move v;` — and binder i takes the measures of `v`'s field i. A
    /// `let (a, b) = f(...)` binder list has no such operand and mints
    /// nothing; its ordinals are [CALL-4] destinations instead.
    fn mint_destructuring_placements(
        &mut self,
        node_path: &crate::NodePath,
        bindings: &[(BindingId, CheckedType, u32)],
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) -> Vec<(u32, MeasureCarry)> {
        let Some(base) = self.placement_source_place(value) else {
            return Vec::new();
        };
        let mut carried = Vec::new();
        // [MSR-3] the destructuring placement's source is the field the
        // binder names, which the rest marker makes a written ordinal rather
        // than the binder's own position.
        for (position, (_, ty, field)) in bindings.iter().enumerate() {
            let ordinal = u32::try_from(position).unwrap_or(u32::MAX);
            let mut source = base.clone();
            source.path.push(PlaceStep::Field(*field));
            if let Some(carry) = self.mint_measure_datums(
                node_path,
                ordinal,
                MeasurePlacement::Destructuring,
                source,
                *ty,
                state,
            ) {
                carried.push((ordinal, carry));
            }
        }
        carried
    }

    /// The [OWN-5] resolved place that root names.
    fn container_root_place(&self, root: &CheckedContainerRoot) -> ResolvedPlace {
        let path = self.container_root_path(root);
        self.resolve(&path)
    }

    /// [ENT-6]: the bounds obligation `i < len_of(P)`, normalized
    /// `i - len_of(P) <= -1`, discharged exactly when the closed fact state at
    /// the node derives it.
    fn judge_obligation(
        &mut self,
        base: ResolvedPlace,
        measured: MeasuredKind,
        array_length: Option<CheckedConst>,
        offset: &CheckedExpression,
        node_path: crate::NodePath,
        states: &ProofFlowState,
    ) {
        // [OP-4] the obligation is against `len_of(p)` in logical coordinates
        // [MSR-1], never against `cap_of(p)`.
        let length_term =
            self.place_measure_term(CheckedMeasure::Length, base.clone(), measured, array_length);
        let offset_term = self.read_operand(offset);
        let affine_offset = self
            .direct_goal_expression(offset)
            .and_then(|offset| self.affine_goal_value(&offset, &states.affine));
        // [MSR-4] the subscript submits its own normalized target to the one
        // disposition, so steps 4 and 5 range over the measure's own affine
        // atom instead of being reachable only through the L0-right bridge.
        let direct_affine = affine_offset.as_ref().and_then(|offset| {
            let length = self.measure_atom(length_term, &states.affine);
            let mut check = AffineCheckState::new();
            AffineInequality::from_bounded_forms(offset, &length, -1, &mut check).ok()
        });
        let fixed_array_affine =
            self.affine_fixed_array_index_target(offset, array_length, &states.affine);
        let rendered_residual = format!(
            "{} < {}.len",
            self.render_expression(offset),
            self.render_place(&base)
        );
        let request = BoundsRequest {
            left: offset_term,
            right: length_term,
            bound: -1,
            distinct: false,
        };
        let fixed_array_middle = match array_length {
            Some(CheckedConst::Value(length)) => {
                Some(self.terms.intern(TermKind::Constant(i128::from(length))))
            }
            Some(CheckedConst::Parameter(_) | CheckedConst::Derived(_)) | None => None,
        };
        let fixed_affine_bridge =
            fixed_array_affine
                .as_ref()
                .zip(fixed_array_middle)
                .map(|(target, middle)| FixedAffineBoundBridge {
                    target,
                    middle,
                    left_to_middle_bound: -1,
                });
        let proof = self.prove(
            ProofContext::new(&states.facts, &states.affine),
            ProofGoal::BoundedRelation(BoundedRelationGoal {
                canonical: None,
                request: Some(request),
                direct_affine: direct_affine.as_ref(),
                fixed_affine_bridge,
                affine_left: affine_offset.as_ref(),
            }),
        );
        let discharged = proof.disposition == ProofDisposition::Proved;
        let refuted = proof.disposition == ProofDisposition::Refuted;
        let contradictory = proof.route == Some(ProofRoute::Contradiction);
        let derivation = proof.derivation;
        let residual = (!discharged).then(|| rendered_residual.clone());
        let ordinal = u32::try_from(self.obligations.len())
            .expect("ENT obligation-root ordinal exceeds the u32 identity space");
        if let Some(root) = derivation {
            self.derivations
                .add_root(DerivationRootKind::BoundsObligation(ordinal), root);
        }
        self.obligations.push(ObligationOutcome {
            node_path: node_path.clone(),
            family: ObligationFamily::Bounds,
            conjunct: 0,
            canonical_goal: None,
            components: vec![request],
            discharged,
            refuted,
            contradictory,
            residual,
            overlap_targets: None,
            derivation,
            allocation_length_upper_bound: None,
            allocation_length_upper_bound_derivation: None,
            affine_index_maps: if discharged {
                self.proved_affine_index_maps(affine_offset.as_ref(), states)
            } else {
                Vec::new()
            },
            range_partitions: Vec::new(),
            written_before: states.written_before(discharged),
        });
    }

    /// Projects one already-computed exact offset value onto each active
    /// counted binder it depends on alone. The affine form is canonical, so
    /// the single-term test is complete for the deliberately small rule and
    /// its coefficient is nonzero by construction.
    fn proved_affine_index_maps(
        &self,
        offset: Option<&AffineForm>,
        states: &ProofFlowState,
    ) -> Vec<super::ProvedAffineIndexMap> {
        let Some(offset) = offset else {
            return Vec::new();
        };
        let [coefficient] = offset.terms() else {
            return Vec::new();
        };
        self.loops
            .iter()
            .filter_map(|frame| {
                let binder = frame.counted_binder?;
                let binder_term = states.affine.values.get(&binder)?.unit_term()?;
                (coefficient.term() == binder_term).then_some(super::ProvedAffineIndexMap {
                    loop_id: frame.id,
                    coefficient: coefficient.coefficient(),
                    constant: offset.constant_value(),
                })
            })
            .collect()
    }

    /// Retains PAR-2's adjacent-range family after both [REF-4] formation
    /// domain goals succeeded. Each active counted loop is considered once, and both sign
    /// goals run to completion for every matching exact image.
    fn proved_range_partitions(
        &mut self,
        range: CaptureId,
        start: &AffineForm,
        end: &AffineForm,
        states: &ProofFlowState,
    ) -> Vec<super::ProvedRangePartition> {
        let candidates = self
            .loops
            .iter()
            .filter_map(|frame| {
                let binder = states
                    .affine
                    .values
                    .get(&frame.counted_binder?)?
                    .unit_term()?;
                let mut visiting = HashSet::new();
                let start =
                    self.counted_value_image(start, binder, &frame.invariant_atoms, &mut visiting)?;
                let end =
                    self.counted_value_image(end, binder, &frame.invariant_atoms, &mut visiting)?;
                let after = start
                    .base
                    .add(&start.stride, &mut AffineCheckState::new())
                    .ok()?;
                (start.stride == end.stride && after == end.base).then_some((frame.id, start))
            })
            .collect::<Vec<_>>();
        let mut partitions = Vec::new();
        for (loop_id, image) in candidates {
            let Some(stride_target) =
                Self::affine_less_equal(&AffineForm::constant(0), &image.stride)
            else {
                continue;
            };
            let Some(base_target) = Self::affine_less_equal(&AffineForm::constant(0), &image.base)
            else {
                continue;
            };
            let stride = self.prove(
                ProofContext::new(&states.facts, &states.affine),
                ProofGoal::Affine {
                    inequality: &stride_target,
                    right: None,
                },
            );
            let base = self.prove(
                ProofContext::new(&states.facts, &states.affine),
                ProofGoal::Affine {
                    inequality: &base_target,
                    right: None,
                },
            );
            if stride.disposition == ProofDisposition::Proved
                && base.disposition == ProofDisposition::Proved
            {
                partitions.push(super::ProvedRangePartition {
                    loop_id,
                    range,
                    stride: image.stride,
                    base: image.base,
                    stride_nonnegative: stride
                        .derivation
                        .expect("a proved stride has a derivation"),
                    base_nonnegative: base.derivation.expect("a proved base has a derivation"),
                });
            }
        }
        partitions
    }

    /// Decomposes a finite checked value graph, expanding handles and exact
    /// product records only. Multiplication allows one binder-dependent
    /// operand and one invariant operand; its two resulting components must
    /// stay affine. Unknown body values and nonlinear binder uses fail closed.
    fn counted_value_image(
        &self,
        form: &AffineForm,
        binder: AffineTermId,
        invariant: &HashSet<AffineTermId>,
        visiting: &mut HashSet<AffineTermId>,
    ) -> Option<CountedValueImage> {
        let mut image = CountedValueImage {
            stride: AffineForm::constant(0),
            base: AffineForm::constant(form.constant_value()),
        };
        let mut check = AffineCheckState::new();
        for coefficient in form.terms() {
            let term = coefficient.term();
            if !visiting.insert(term) {
                return None;
            }
            let term_image = self.counted_atom_image(term, binder, invariant, visiting)?;
            visiting.remove(&term);
            image.stride = image
                .stride
                .add(
                    &term_image
                        .stride
                        .scale(coefficient.coefficient(), &mut check)
                        .ok()?,
                    &mut check,
                )
                .ok()?;
            image.base = image
                .base
                .add(
                    &term_image
                        .base
                        .scale(coefficient.coefficient(), &mut check)
                        .ok()?,
                    &mut check,
                )
                .ok()?;
        }
        Some(image)
    }

    fn counted_atom_image(
        &self,
        term: AffineTermId,
        binder: AffineTermId,
        invariant: &HashSet<AffineTermId>,
        visiting: &mut HashSet<AffineTermId>,
    ) -> Option<CountedValueImage> {
        if term == binder {
            return Some(CountedValueImage {
                stride: AffineForm::constant(1),
                base: AffineForm::constant(0),
            });
        }
        // A preheader product can mint a handle for a transparent sum. The
        // handle and its image name the same value, so expand it before the
        // invariant-atom case: otherwise the product's stride and an endpoint
        // that reads the sum directly would have different canonical images.
        if let Some(image) = self.handle_images.get(&term) {
            return self.counted_value_image(image, binder, invariant, visiting);
        }
        if invariant.contains(&term) {
            return Some(CountedValueImage {
                stride: AffineForm::constant(0),
                base: AffineForm::term(term),
            });
        }
        let (left, right) = *self.product_atoms.get(&term)?;
        let left =
            self.counted_value_image(&AffineForm::term(left), binder, invariant, visiting)?;
        let right =
            self.counted_value_image(&AffineForm::term(right), binder, invariant, visiting)?;
        let zero = AffineForm::constant(0);
        let (varying, fixed) = if left.stride == zero && right.stride == zero {
            // A pure exact product of fixed values is itself fixed, even
            // when the source computes it in the body. Keep its exact atom.
            return Some(CountedValueImage {
                stride: zero,
                base: AffineForm::term(term),
            });
        } else if right.stride == zero {
            (left, right.base)
        } else if left.stride == zero {
            (right, left.base)
        } else {
            return None;
        };
        let multiply = |left: &AffineForm, right: &AffineForm| {
            if left.terms().is_empty() {
                right
                    .scale(left.constant_value(), &mut AffineCheckState::new())
                    .ok()
            } else if right.terms().is_empty() {
                left.scale(right.constant_value(), &mut AffineCheckState::new())
                    .ok()
            } else {
                None
            }
        };
        Some(CountedValueImage {
            stride: multiply(&varying.stride, &fixed)?,
            base: multiply(&varying.base, &fixed)?,
        })
    }

    /// Forms the exact `offset <= N - 1` target for a fixed-size array.
    /// Dynamic buffer and slice lengths remain on the ordinary L0 route until
    /// their length term is connected to an affine value by a fixed rule.
    fn affine_fixed_array_index_target(
        &mut self,
        offset: &CheckedExpression,
        array_length: Option<CheckedConst>,
        state: &AffineFlowState,
    ) -> Option<AffineInequality> {
        let CheckedConst::Value(length) = array_length? else {
            return None;
        };
        let offset = self.direct_goal_expression(offset)?;
        let left = self.affine_goal_value(&offset, state)?;
        let right = AffineForm::constant(i128::from(length).checked_sub(1)?);
        AffineInequality::from_forms(&left, &right, &mut AffineCheckState::new()).ok()
    }

    fn affine_consequence_derivation(
        &mut self,
        target: &AffineInequality,
        relation: Option<Relation>,
        affine: &AffineFlowState,
        facts: &FactState,
    ) -> Option<DerivationId> {
        let assumptions = Self::affine_facts(affine);
        let proof =
            self.affine_target_proof(target, &assumptions, ProofContext::new(facts, affine))?;
        Some(self.derivations.intern(DerivationNode::AffineConsequence {
            relation: relation.map(Box::new),
            premises: proof.premises.into_boxed_slice(),
            parents: proof.parents,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn fixed_affine_bound_derivation(
        &mut self,
        bridge: FixedAffineBoundBridge<'_>,
        left: TermId,
        right: TermId,
        requested: i128,
        affine: &AffineFlowState,
        facts: &FactState,
    ) -> Option<DerivationId> {
        let remaining = requested.checked_sub(bridge.left_to_middle_bound)?;
        let first_relation = Relation::Bound {
            left,
            right: bridge.middle,
            bound: bridge.left_to_middle_bound,
        };
        let first =
            self.affine_consequence_derivation(bridge.target, Some(first_relation), affine, facts)?;
        let closed = close(facts, &self.terms, &self.goals, &mut self.derivations);
        let second = closed.bound_proof(bridge.middle, right, remaining, &mut self.derivations)?;
        Some(self.derivations.intern(DerivationNode::TransitiveBound {
            left,
            middle: bridge.middle,
            right,
            bound: requested,
            first,
            second,
        }))
    }

    /// The complete Step 6 inventory. Querying it must not recreate a measure
    /// image removed by an intervening write. Fixed measures and aliases use
    /// the same immutable anchor as ordinary measure reads.
    fn affine_right_bridge_candidates(
        &mut self,
        affine: &AffineFlowState,
    ) -> Vec<AffineL0Candidate> {
        let mut candidates = Vec::new();
        for term in self.measure_terms() {
            let mut anchor = term;
            let mut fixed = None;
            for _ in 0..4 {
                match self.terms.measure_bound(anchor) {
                    Some(MeasureBound::Constant(value)) => {
                        fixed = Some(AffineForm::constant(value));
                        break;
                    }
                    Some(MeasureBound::Equal(other)) => anchor = other,
                    None => break,
                }
            }
            if let Some(value) =
                fixed.or_else(|| affine.measure_atoms.borrow().get(&anchor).cloned())
            {
                candidates.push(AffineL0Candidate { term, value });
            }
        }
        let mut bindings = affine.values.keys().copied().collect::<Vec<_>>();
        bindings.sort_by_key(|binding| binding.0);
        for binding in bindings {
            let Some(ty) = self.affine_binding_type(binding) else {
                continue;
            };
            let term = self.terms.intern(TermKind::Place(
                ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, Vec::new()),
                ty,
            ));
            candidates.push(AffineL0Candidate {
                term,
                value: affine.values[&binding].clone(),
            });
        }
        candidates
    }

    /// The shared affine portion of MSR-4. AUTO remains nonrecursive: after
    /// it fails, Step 6 subtracts exactly one closed `m-r <= c` image and
    /// submits exactly that residual to AUTO. The normalization supplies r;
    /// a normalized coefficient vector never chooses a new right operand.
    fn numeric_affine_proof(
        &mut self,
        target: &AffineInequality,
        right: Option<TermId>,
        context: ProofContext<'_>,
    ) -> Option<AffineConsequenceProof> {
        let assumptions = Self::affine_facts(context.affine);
        if let Some(proof) = self.affine_target_proof(target, &assumptions, context) {
            return Some(proof);
        }
        let right = right?;
        let right_value = self.affine_term_value(right, context.affine)?;
        let candidates = self.affine_right_bridge_candidates(context.affine);
        let closed = context.close(&self.terms, &self.goals, &mut self.derivations);
        for candidate in candidates {
            let Some(bound) = closed.tight_bound(candidate.term, right) else {
                continue;
            };
            let mut check = AffineCheckState::new();
            let Ok(image) = AffineInequality::from_bounded_forms(
                &candidate.value,
                &right_value,
                bound,
                &mut check,
            ) else {
                continue;
            };
            let Ok(residual) = AffineInequality::residual_after(target, &image, &mut check) else {
                continue;
            };
            let Some(mut proof) = self.affine_target_proof(&residual, &assumptions, context) else {
                continue;
            };
            let Some(bridge) =
                closed.bound_proof(candidate.term, right, bound, &mut self.derivations)
            else {
                continue;
            };
            proof.parents.push(bridge);
            return Some(proof);
        }
        None
    }

    /// Combines one affine left-hand value with one already-live L0 bridge to
    /// the requested right-hand term. For a candidate middle term `m`, L0
    /// fixes `m - right <= c`; the single affine target is therefore
    /// `left - m <= requested - c`. It uses the same complete Step 6 inventory
    /// as the general numeric entry and retains an exact transitive L0 node.
    #[allow(clippy::too_many_arguments)]
    fn affine_bound_via_l0_right(
        &mut self,
        left: &AffineForm,
        left_term: TermId,
        right_term: TermId,
        requested: i128,
        affine: &AffineFlowState,
        facts: &FactState,
    ) -> Option<DerivationId> {
        let candidates = self.affine_right_bridge_candidates(affine);
        let closed = close(facts, &self.terms, &self.goals, &mut self.derivations);
        if closed.contradictory() {
            return closed.contradiction_proof();
        }
        for AffineL0Candidate {
            term: middle,
            value: middle_value,
        } in candidates
        {
            let Some(bridge) = closed.tight_bound(middle, right_term) else {
                continue;
            };
            let Some(affine_bound) = requested.checked_sub(bridge) else {
                continue;
            };
            let Ok(right) = middle_value.add(
                &AffineForm::constant(affine_bound),
                &mut AffineCheckState::new(),
            ) else {
                continue;
            };
            let Ok(target) =
                AffineInequality::from_forms(left, &right, &mut AffineCheckState::new())
            else {
                continue;
            };
            let relation = Relation::Bound {
                left: left_term,
                right: middle,
                bound: affine_bound,
            };
            let Some(first) =
                self.affine_consequence_derivation(&target, Some(relation), affine, facts)
            else {
                continue;
            };
            let Some(second) =
                closed.bound_proof(middle, right_term, bridge, &mut self.derivations)
            else {
                continue;
            };
            return Some(self.derivations.intern(DerivationNode::TransitiveBound {
                left: left_term,
                middle,
                right: right_term,
                bound: requested,
                first,
                second,
            }));
        }
        None
    }

    /// Judges OP-9 through either the exact total `buffer_fits::<T>(n)` goal
    /// or its one canonical L0 component. The component is used only in this
    /// direction: proving the comparison authorizes the allocation, while a
    /// predicate fact does not publish an ambient comparison fact.
    fn judge_allocation_fit(
        &mut self,
        element: CheckedType,
        maximum_length: u64,
        length: &CheckedExpression,
        node_path: crate::NodePath,
        states: &ProofFlowState,
    ) {
        let length_goal = self.obligation_goal_operand(&node_path, 0, length, &states.facts);
        let canonical_goal = GoalExpression::Operation {
            row: GoalOperation::BufferFits {
                element,
                maximum_length,
            },
            type_arguments: vec![element],
            const_arguments: Vec::new(),
            result: CheckedType::Bool,
            arguments: vec![length_goal],
        };
        let goal = Some(self.intern_goal_expression(canonical_goal.clone()));
        let length_term = self.read_operand(length);
        let threshold_term = self
            .terms
            .intern(TermKind::Constant(i128::from(maximum_length)));
        let ordering_relation = length_term.map(|length| Relation::Bound {
            left: length,
            right: threshold_term,
            bound: 0,
        });
        let affine_length = self
            .admitted_value_goal_expression(length)
            .and_then(|length| self.affine_goal_value(&length, &states.affine));
        let affine_target = affine_length.as_ref().and_then(|length| {
            AffineInequality::from_forms(
                length,
                &AffineForm::constant(i128::from(maximum_length)),
                &mut AffineCheckState::new(),
            )
            .ok()
        });
        // [OP-9] the predicate "has no writer-callable spelling", so the
        // residual is the defining comparison itself: the count this
        // operation was handed against the largest one its stored type
        // admits.
        let rendered = format!("{} <= {maximum_length}_u64", self.render_expression(length));

        let proof = self.prove(
            ProofContext::new(&states.facts, &states.affine),
            ProofGoal::NormalizedOrdering {
                goal,
                relation: ordering_relation.as_ref(),
                affine: affine_target.as_ref(),
                right: Some(threshold_term),
                upper_bound: Some(NumericUpperBoundRequest {
                    term: length_term,
                    affine: affine_length.as_ref(),
                    admitted: i128::from(maximum_length),
                }),
            },
        );
        let discharged = proof.disposition == ProofDisposition::Proved;
        let refuted = proof.disposition == ProofDisposition::Refuted;
        let contradictory = proof.route == Some(ProofRoute::Contradiction);
        let derivation = proof.derivation;
        let allocation_length_upper_bound = proof
            .numeric_upper_bound
            .and_then(|bound| u64::try_from(bound.value).ok());
        let allocation_length_upper_bound_derivation =
            proof.numeric_upper_bound.map(|bound| bound.derivation);
        let ordinal = u32::try_from(self.obligations.len())
            .expect("ENT obligation-root ordinal exceeds the u32 identity space");
        if let Some(root) = derivation {
            self.derivations
                .add_root(DerivationRootKind::BoundsObligation(ordinal), root);
        }
        if let Some(root) = allocation_length_upper_bound_derivation
            && Some(root) != derivation
        {
            self.derivations
                .add_root(DerivationRootKind::AllocationUpperBound(ordinal), root);
        }
        self.obligations.push(ObligationOutcome {
            node_path: node_path.clone(),
            family: ObligationFamily::AllocationFit,
            conjunct: 0,
            canonical_goal: Some(canonical_goal),
            components: vec![BoundsRequest {
                left: length_term,
                right: threshold_term,
                bound: 0,
                distinct: false,
            }],
            discharged,
            refuted,
            contradictory,
            residual: (!discharged).then(|| rendered.clone()),
            overlap_targets: None,
            derivation,
            allocation_length_upper_bound,
            allocation_length_upper_bound_derivation,
            affine_index_maps: Vec::new(),
            range_partitions: Vec::new(),
            written_before: states.written_before(discharged),
        });
    }

    /// [OWN-7] the separations at this call or set commit: mandatory effect
    /// questions and preservation questions demanded by later reference uses.
    ///
    /// The checker owns the comparison — clauses 2 and 3 need the actual
    /// argument spellings and the live reference state — and hands over the
    /// pairs whose separation is a proof rather than syntax [OWN-7]. Each
    /// pair is discharged here by the fixed [ENT-6] families under [MSR-4]'s
    /// disposition, and a discharged pair is recorded on the current edge so
    /// that later dominated [OWN-7] questions can read it. A set reaches this
    /// judgment after RHS effects, using target captures evaluated before it.
    fn judge_call_separations(
        &mut self,
        site: &crate::NodePath,
        state: &mut ProofFlowState,
    ) -> bool {
        let pending: Vec<_> = self
            .function
            .call_separations
            .iter()
            .enumerate()
            .filter(|(query, _)| !self.judged_separations.contains(query))
            .filter(|(_, separation)| separation.site.components().starts_with(site.components()))
            .map(|(query, separation)| (query, separation.clone()))
            .collect();
        let mut all_discharged = true;
        for (query, separation) in pending {
            self.judged_separations.insert(query);
            let discharged = self.judge_one_separation(query, &separation, state);
            // A preservation query is a later reference-use obligation. It
            // does not make this event unreachable or suppress its effects.
            all_discharged &= discharged || separation.reference_use.is_some();
        }
        all_discharged
    }

    /// One pair, by an ordered unresolved index candidate or by the four
    /// non-strict orderings [OWN-7] names for two range steps. Either empty-
    /// range ordering suffices because formation already proved start no
    /// greater than end [REF-4].
    fn judge_one_separation(
        &mut self,
        query: usize,
        separation: &super::super::model::CheckedCallSeparation,
        state: &mut ProofFlowState,
    ) -> bool {
        use super::super::model::CheckedCallSeparationPositions;
        let proved = separation.positions.iter().copied().find_map(|positions| {
            let proof = match positions {
                CheckedCallSeparationPositions::Indices(left, right) => {
                    self.prove_index_separation(left, right, state)
                }
                CheckedCallSeparationPositions::Ranges(left, right) => {
                    self.prove_range_separation(left, right, &state.facts, &state.affine)
                }
                CheckedCallSeparationPositions::Live(index) => separation
                    .window
                    .as_ref()
                    .and_then(|window| self.index_live_proof(window, index, state)),
            };
            proof.map(|proof| (positions, proof))
        });
        let proof = proved.as_ref().map(|(_, proof)| proof);
        let discharged = proof.is_some();
        if let Some((positions, _)) = &proved {
            match *positions {
                CheckedCallSeparationPositions::Indices(left, right) => {
                    state.separations.record_indices_distinct(left, right);
                }
                CheckedCallSeparationPositions::Ranges(left, right) => {
                    state.separations.record_ranges_disjoint(left, right);
                }
                // Liveness holds at this call's entry alone and is never
                // carried along the edge [WIN-2].
                CheckedCallSeparationPositions::Live(_) => {}
            }
        }
        let derivation = proof.and_then(|proof| proof.derivation);
        let ordinal =
            u32::try_from(self.obligations.len()).expect("ENT obligation ordinal exceeds u32");
        if let Some(root) = derivation {
            self.derivations
                .add_root(DerivationRootKind::BoundsObligation(ordinal), root);
        }
        self.obligations.push(ObligationOutcome {
            node_path: separation.reference_use.as_ref()
                .map_or_else(|| separation.site.clone(), |use_site| use_site.site.clone()),
            family: if separation.reference_use.is_some() {
                ObligationFamily::ReferencePreservation(
                    u32::try_from(query).expect("reference-preservation queries exceed u32"),
                )
            } else if separation.exchange {
                ObligationFamily::ExchangeSeparation
            } else {
                ObligationFamily::CallSeparation(
                    u32::try_from(query).expect("call-separation queries exceed u32"),
                )
            },
            conjunct: 0,
            canonical_goal: None,
            components: Vec::new(),
            discharged,
            refuted: false,
            contradictory: proof.is_some_and(|proof| proof.route == Some(ProofRoute::Contradiction)),
            residual: (!discharged).then(|| match separation.positions.first() {
                Some(CheckedCallSeparationPositions::Indices(..)) => format!(
                    "{} and {} require their captured indices to be distinct",
                    separation.left_spelling, separation.right_spelling
                ),
                Some(CheckedCallSeparationPositions::Ranges(..)) => format!(
                    "{} and {} select different storage (one ends before the other starts, or one is empty)",
                    separation.left_spelling, separation.right_spelling
                ),
                Some(CheckedCallSeparationPositions::Live(..)) => format!(
                    "{} and {} select different storage (the index is below the window's length)",
                    separation.left_spelling, separation.right_spelling
                ),
                None => unreachable!("checker hands off at least one position candidate"),
            }),
            overlap_targets: Some((
                separation.left_spelling.clone(),
                separation.right_spelling.clone(),
            )),
            derivation,
            allocation_length_upper_bound: None,
            allocation_length_upper_bound_derivation: None,
            affine_index_maps: Vec::new(),
            range_partitions: Vec::new(),
            written_before: state.written_before(discharged),
        });
        discharged
    }

    fn captured_index_term(&mut self, value: CapturedValue) -> Option<TermId> {
        match value.term {
            CapturedTerm::Literal(value) => {
                Some(self.terms.intern(TermKind::Constant(i128::from(value))))
            }
            CapturedTerm::Const(declaration) => Some(self.const_parameter_term(declaration)),
            CapturedTerm::Binding(_) if matches!(value.capture, CaptureId::Source(_)) => {
                self.terms.interned(&TermKind::IndexCapture {
                    capture: value.capture,
                })
            }
            CapturedTerm::Binding(_) | CapturedTerm::Opaque => None,
        }
    }

    fn prove_index_separation(
        &mut self,
        left: CapturedValue,
        right: CapturedValue,
        state: &ProofFlowState,
    ) -> Option<ProofResult> {
        let left_term = self.captured_index_term(left)?;
        let right_term = self.captured_index_term(right)?;
        let relation = if left_term <= right_term {
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
        let left_image = state
            .affine
            .indices
            .get(&left.capture)
            .cloned()
            .or_else(|| self.affine_term_value(left_term, &state.affine));
        let right_image = state
            .affine
            .indices
            .get(&right.capture)
            .cloned()
            .or_else(|| self.affine_term_value(right_term, &state.affine));
        let mut inequalities = Vec::new();
        if let Some((left_image, right_image)) = left_image.as_ref().zip(right_image.as_ref()) {
            for (lower, upper) in [(left_image, right_image), (right_image, left_image)] {
                if let Ok(inequality) = AffineInequality::from_bounded_forms(
                    lower,
                    upper,
                    -1,
                    &mut AffineCheckState::new(),
                ) {
                    inequalities.push(inequality);
                }
            }
        }
        let mut substitution = None;
        let mut proof = self.prove(
            ProofContext::new(&state.facts, &state.affine),
            ProofGoal::Ordering {
                relation: &relation,
                affine: None,
            },
        );
        if proof.disposition != ProofDisposition::Proved
            && let (CapturedTerm::Binding(left_binding), CapturedTerm::Binding(right_binding)) =
                (left.term, right.term)
        {
            let source_left = self.terms.intern(TermKind::Place(
                ResolvedPlace::spelled(PlaceRoot::Binding(left_binding), false, Vec::new()),
                super::super::model::IntegerType::U64,
            ));
            let source_right = self.terms.intern(TermKind::Place(
                ResolvedPlace::spelled(PlaceRoot::Binding(right_binding), false, Vec::new()),
                super::super::model::IntegerType::U64,
            ));
            let source_relation = if source_left <= source_right {
                Relation::Distinct {
                    left: source_left,
                    right: source_right,
                    difference: 0,
                }
            } else {
                Relation::Distinct {
                    left: source_right,
                    right: source_left,
                    difference: 0,
                }
            };
            let left_identity = Relation::Equal {
                left: left_term,
                right: source_left,
                difference: 0,
            };
            let right_identity = Relation::Equal {
                left: right_term,
                right: source_right,
                difference: 0,
            };
            let closed = ProofContext::new(&state.facts, &state.affine).close(
                &self.terms,
                &self.goals,
                &mut self.derivations,
            );
            if closed.derives(&source_relation)
                && closed.derives(&left_identity)
                && closed.derives(&right_identity)
            {
                let parent = closed
                    .relation_proof(&source_relation, &mut self.derivations)
                    .expect("a proved source disequality retains its proof");
                let left_identity = closed
                    .relation_proof(&left_identity, &mut self.derivations)
                    .expect("a live left capture identity retains its proof");
                let right_identity = closed
                    .relation_proof(&right_identity, &mut self.derivations)
                    .expect("a live right capture identity retains its proof");
                substitution = Some(Box::new(IndexCaptureSubstitution {
                    source_left,
                    source_right,
                    left_identity,
                    right_identity,
                }));
                proof = ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::L0),
                    derivation: Some(parent),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }
        let mut affine_target = None;
        let mut affine_images = None;
        if proof.disposition != ProofDisposition::Proved {
            for inequality in &inequalities {
                proof = self.prove(
                    ProofContext::new(&state.facts, &state.affine),
                    ProofGoal::Ordering {
                        relation: &relation,
                        affine: Some(std::slice::from_ref(inequality)),
                    },
                );
                if proof.disposition == ProofDisposition::Proved {
                    if proof.route == Some(ProofRoute::Affine) {
                        affine_target = Some(Box::new(inequality.clone()));
                        affine_images = Some(Box::new((
                            left_image
                                .clone()
                                .expect("an affine candidate has a left image"),
                            right_image
                                .clone()
                                .expect("an affine candidate has a right image"),
                        )));
                    }
                    break;
                }
            }
        }
        if proof.disposition != ProofDisposition::Proved {
            return None;
        }
        let parent = proof
            .derivation
            .expect("a proved index separation retains its L0 or affine parent");
        proof.derivation = Some(self.derivations.intern(DerivationNode::IndexSeparation {
            detail: Box::new(IndexSeparationDetail {
                left,
                right,
                parent,
                affine_target,
                affine_images,
                substitution,
            }),
        }));
        Some(proof)
    }

    fn prove_range_separation(
        &mut self,
        left: CapturedRange,
        right: CapturedRange,
        facts: &FactState,
        affine: &AffineFlowState,
    ) -> Option<ProofResult> {
        let left_image = affine.ranges.get(&left.start.capture)?.clone();
        let right_image = affine.ranges.get(&right.start.capture)?.clone();
        for (ordering, end, start) in [
            (
                RangeSeparationOrdering::LeftBeforeRight,
                &left_image.end,
                &right_image.start,
            ),
            (
                RangeSeparationOrdering::RightBeforeLeft,
                &right_image.end,
                &left_image.start,
            ),
            (
                RangeSeparationOrdering::LeftEmpty,
                &left_image.end,
                &left_image.start,
            ),
            (
                RangeSeparationOrdering::RightEmpty,
                &right_image.end,
                &right_image.start,
            ),
        ] {
            let Ok(inequality) =
                AffineInequality::from_forms(end, start, &mut AffineCheckState::new())
            else {
                continue;
            };
            let mut proof = self.prove(
                ProofContext::new(facts, affine),
                ProofGoal::Affine {
                    inequality: &inequality,
                    right: None,
                },
            );
            if proof.disposition == ProofDisposition::Proved {
                let parent = proof
                    .derivation
                    .expect("a proved range ordering retains its affine or contradiction parent");
                proof.derivation = Some(self.derivations.intern(DerivationNode::RangeSeparation {
                    detail: Box::new(RangeSeparationDetail {
                        left,
                        right,
                        ordering,
                        parent,
                    }),
                }));
                return Some(proof);
            }
        }
        None
    }

    /// Judges every optional [PAR-1] query whose first statement is this
    /// exact site. The incoming flow state is still the state before the
    /// statement: none of its effects, postconditions, or later branch facts
    /// have executed. Answers remain outside the flow separation ledger so a
    /// permission proof can never authorize an EFF-5 or kill judgment.
    fn judge_permission_separations(&mut self, site: &crate::NodePath, state: &ProofFlowState) {
        let pending = self
            .permission_separations
            .iter()
            .enumerate()
            .filter_map(|(index, attempt)| (attempt.query.first == *site).then_some(index))
            .collect::<Vec<_>>();
        for index in pending {
            let query = self.permission_separations[index].query.clone();
            let derivation = self
                .prove_permission_separation(&query, state)
                .and_then(|proof| proof.derivation);
            let attempt = &mut self.permission_separations[index];
            attempt.attempted = true;
            attempt.discharged &= derivation.is_some();
            if let Some(derivation) = derivation {
                attempt.derivations.push(derivation);
            }
        }
    }

    /// One [PAR-1] range question in the state before its first statement.
    ///
    /// A range bound before that statement has the image its formation
    /// published. A range one of the pair's calls forms as an actual has
    /// none yet, because its formation runs with its call; it names the same
    /// storage as that range bound immediately before the first statement
    /// [REF-4], so its endpoints are evaluated here, in a copy of this state,
    /// into exactly the image such a binding would publish. The planner lists
    /// a later statement's formation only when nothing before it writes what
    /// its endpoints read, so these are the values that formation evaluates.
    /// The copy keeps the atoms this evaluation mints out of the ordinary
    /// walk and replaces any image an earlier evaluation left under the same
    /// capture.
    fn prove_permission_separation(
        &mut self,
        query: &PermissionSeparationQuery,
        state: &ProofFlowState,
    ) -> Option<ProofResult> {
        if query.formations.is_empty() {
            return self.prove_range_separation(
                query.left,
                query.right,
                &state.facts,
                &state.affine,
            );
        }
        let mut affine = state.affine.clone();
        for formation in &query.formations {
            let capture = formation.captured.start.capture;
            match self.range_formation_image(
                &formation.carrier,
                &formation.start,
                &formation.end,
                &mut affine,
            ) {
                Some(image) => affine.ranges.insert(capture, image),
                None => affine.ranges.remove(&capture),
            };
        }
        self.prove_range_separation(query.left, query.right, &state.facts, &affine)
    }

    /// The image one [REF-4] formation publishes: its two endpoints' affine
    /// forms in `affine`, attached to the formation node.
    fn range_formation_image(
        &mut self,
        carrier: &crate::NodePath,
        start: &CheckedExpression,
        end: &CheckedExpression,
        affine: &mut AffineFlowState,
    ) -> Option<AffineRangeImage> {
        self.affine_expression_form(start, affine)
            .zip(self.affine_expression_form(end, affine))
            .map(|(start, end)| AffineRangeImage {
                source: carrier.clone(),
                start,
                end,
            })
    }

    fn finalize_permission_separations(&mut self) -> Vec<PermissionSeparationProof> {
        let mut retained = Vec::with_capacity(self.permission_separations.len());
        let attempts = std::mem::take(&mut self.permission_separations);
        for (query, attempt) in attempts.into_iter().enumerate() {
            let PermissionSeparationAttempt {
                query: identity,
                attempted,
                discharged,
                mut derivations,
            } = attempt;
            let discharged = attempted && discharged;
            if discharged {
                for (occurrence, root) in derivations.iter().copied().enumerate() {
                    self.derivations.add_root(
                        DerivationRootKind::PermissionSeparation {
                            query: u32::try_from(query)
                                .expect("PAR-1 range queries exceed the u32 identity space"),
                            occurrence: u32::try_from(occurrence)
                                .expect("PAR-1 range-query visits exceed the u32 identity space"),
                        },
                        root,
                    );
                }
            } else {
                derivations.clear();
            }
            retained.push(PermissionSeparationProof {
                query: identity,
                discharged,
                derivations,
            });
        }
        retained
    }

    /// Every pair the checker handed over must be judged once, so a pair no
    /// walked call reached is recorded undischarged rather than dropped.
    fn reject_unjudged_separations(&mut self) {
        let missing: Vec<_> = self
            .function
            .call_separations
            .iter()
            .enumerate()
            .filter(|(query, _)| !self.judged_separations.contains(query))
            .map(|(query, separation)| (query, separation.clone()))
            .collect();
        for (query, separation) in missing {
            self.judged_separations.insert(query);
            let mut state = ProofFlowState::default();
            self.judge_one_separation(query, &separation, &mut state);
        }
    }

    fn judge_exact_relation_obligation(
        &mut self,
        (family, conjunct): (ObligationFamily, u8),
        node_path: crate::NodePath,
        root: GoalExpression,
        (left, right): (Option<TermId>, Option<TermId>),
        residual: String,
        states: &ProofFlowState,
    ) {
        let canonical_goal = root.clone();
        let request = left.zip(right).map(|(left, right)| BoundsRequest {
            left: Some(left),
            right,
            bound: 0,
            distinct: false,
        });
        let affine_left = left.and_then(|term| self.affine_term_value(term, &states.affine));
        // [MSR-4] the relation submits its own normalized target so the
        // affine route ranges over each side's own atom, exactly as a
        // subscript's bound does, rather than only over the goal expression
        // the two operands render to.
        let direct_affine = self
            .affine_goal_ordering_target(&root, &states.affine)
            .or_else(|| {
                let left = affine_left.clone()?;
                let right = right.and_then(|term| self.affine_term_value(term, &states.affine))?;
                let mut check = AffineCheckState::new();
                AffineInequality::from_bounded_forms(&left, &right, 0, &mut check).ok()
            });
        let proof = self.prove(
            ProofContext::new(&states.facts, &states.affine),
            ProofGoal::BoundedRelation(BoundedRelationGoal {
                canonical: Some(&root),
                request,
                direct_affine: direct_affine.as_ref(),
                fixed_affine_bridge: None,
                affine_left: affine_left.as_ref(),
            }),
        );
        let discharged = proof.disposition == ProofDisposition::Proved;
        let refuted = proof.disposition == ProofDisposition::Refuted;
        let contradictory = proof.route == Some(ProofRoute::Contradiction);
        let derivation = proof.derivation;
        let ordinal = u32::try_from(self.obligations.len())
            .expect("ENT obligation-root ordinal exceeds the u32 identity space");
        if let Some(root) = derivation {
            self.derivations
                .add_root(DerivationRootKind::BoundsObligation(ordinal), root);
        }
        self.obligations.push(ObligationOutcome {
            node_path: node_path.clone(),
            family,
            conjunct,
            canonical_goal: Some(canonical_goal),
            components: request.into_iter().collect(),
            discharged,
            refuted,
            contradictory,
            residual: (!discharged).then(|| residual.clone()),
            overlap_targets: None,
            derivation,
            allocation_length_upper_bound: None,
            allocation_length_upper_bound_derivation: None,
            affine_index_maps: Vec::new(),
            range_partitions: Vec::new(),
            written_before: states.written_before(discharged),
        });
    }

    fn judge_view_range(
        &mut self,
        node_path: &crate::NodePath,
        start: &CheckedExpression,
        end: &CheckedExpression,
        length: &CheckedExpression,
        states: &ProofFlowState,
    ) {
        for (conjunct, (left, right)) in [(start, end), (end, length)].into_iter().enumerate() {
            let left_goal =
                self.obligation_goal_operand(node_path, conjunct + 1, left, &states.facts);
            let right_goal =
                self.obligation_goal_operand(node_path, conjunct + 2, right, &states.facts);
            let left_term = self.read_operand(left);
            let right_term = self
                .read_operand(right)
                .or_else(|| self.measure_operand(right));
            let goal = GoalExpression::Operation {
                row: GoalOperation::Integer {
                    operation: CheckedIntegerOperation::LessEqual,
                    operand_type: CheckedType::Integer(IntegerType::U64),
                },
                type_arguments: Vec::new(),
                const_arguments: Vec::new(),
                result: CheckedType::Bool,
                arguments: vec![left_goal, right_goal],
            };
            self.judge_exact_relation_obligation(
                (ObligationFamily::RangeFormation, conjunct as u8),
                node_path.clone(),
                goal,
                (left_term, right_term),
                format!(
                    "{} <= {}",
                    self.render_expression(left),
                    self.render_expression(right)
                ),
                states,
            );
        }
    }

    fn affine_term_value(&mut self, term: TermId, state: &AffineFlowState) -> Option<AffineForm> {
        match self.terms.kind(term).clone() {
            TermKind::Zero => Some(AffineForm::constant(0)),
            TermKind::Constant(value) => Some(AffineForm::constant(value)),
            TermKind::Place(place, _) if place.path.is_empty() => match place.root {
                PlaceRoot::Binding(binding) => state.values.get(&binding).cloned(),
                _ => None,
            },
            // [MSR-4] a measure term's image is its own compiler-owned atom,
            // [MSR-3] a measure datum inherits the atom of the term it
            // denotes, and [MSR-6] gives one immutable image to a symbolic
            // const parameter throughout the generic body.
            TermKind::ConstParameter(..)
            | TermKind::Measure(..)
            | TermKind::EntryDatum { .. }
            | TermKind::MeasureDatum { .. }
            | TermKind::CallDatum {
                measure: Some(_), ..
            } => Some(self.measure_atom(term, state)),
            TermKind::Place(_, _)
            | TermKind::CountedCapture { .. }
            | TermKind::IndexCapture { .. }
            | TermKind::ResultPayload(_)
            | TermKind::CommitValue { .. }
            | TermKind::CallDatum { .. } => None,
        }
    }

    /// [ENT-6] judges one proof-required exact integer operation. The source
    /// occurrence owns one canonical `.defined` goal and one obligation
    /// identity. Fixed L0 components are alternate derivations of that goal;
    /// they are never independent source obligations.
    fn judge_integer_domain_obligation(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
        node_path: &crate::NodePath,
        states: &mut ProofFlowState,
    ) {
        let canonical_goal =
            self.integer_domain_goal(operation, operand_type, arguments, node_path, &states.facts);
        let goal = Some(self.intern_goal_expression(canonical_goal.clone()));
        let components = self.integer_domain_components(operation, operand_type, arguments);
        let residual = self.render_integer_domain_goal(operation, arguments);
        // Goal preparation may need to install a missing binding value image
        // before it can spell the fixed affine alternatives. Keep that work
        // local until the one proof query selects an affine route (or leaves
        // the goal unknown, matching the prior attempted-route state change).
        let candidate_atom_start = self.affine_atoms.len();
        let mut prepared_affine = states.affine.clone();
        let affine_clauses = self.affine_integer_domain_clauses(
            operation,
            operand_type,
            arguments,
            &mut prepared_affine,
        );
        let affine_product =
            self.affine_integer_product(operation, operand_type, arguments, &mut prepared_affine);

        let outcome = self.prove(
            ProofContext::new(&states.facts, &prepared_affine),
            ProofGoal::IntegerDomain(IntegerDomainGoal {
                canonical: goal,
                operation,
                operand_type,
                components: &components,
                affine_clauses: affine_clauses.as_deref(),
                affine_product: affine_product.as_ref(),
            }),
        );
        if outcome.route == Some(ProofRoute::Affine) || outcome.route.is_none() {
            states.affine = prepared_affine;
        } else {
            self.affine_atoms.truncate(candidate_atom_start);
        }
        let discharged = outcome.disposition == ProofDisposition::Proved;
        let refuted = outcome.disposition == ProofDisposition::Refuted;
        let contradictory = outcome.route == Some(ProofRoute::Contradiction);
        // Both records below describe this walk of this operation. A loop body
        // is walked more than once and the same node then carries different
        // operand values each time, so the previous walk's measurement is
        // dropped before this one decides: a judgment that does not discharge
        // must leave nothing behind for the binding to read.
        self.product_intervals.remove(node_path);
        self.product_operands.remove(node_path);
        // [ENT-3.S7]'s multiplication row reads the interval-product rule's
        // intervals only when that rule discharged the domain, so the
        // interval is retained exactly when this obligation discharged
        // through the interval-product route.
        if discharged && let Some(interval) = outcome.product_interval.clone() {
            self.product_intervals
                .insert(node_path.clone(), (interval, outcome.derivation));
        }
        // That the exact multiplication's domain held, for [PRF-1] to fold a
        // term-scaled premise against. Recorded only when the domain
        // discharged through an affine route, which is what committed
        // `prepared_affine` and so fixed the images the judgment read.
        let ordinal = u32::try_from(self.obligations.len())
            .expect("ENT obligation-root ordinal exceeds the u32 identity space");
        if discharged
            && outcome.route == Some(ProofRoute::Affine)
            && operation == CheckedIntegerOperation::MultiplyExact
            && let Some(parent) = outcome.derivation
        {
            self.product_operands
                .insert(node_path.clone(), (parent, ordinal));
        }
        if let Some(root) = outcome.derivation {
            self.derivations
                .add_root(DerivationRootKind::IntegerDomainObligation(ordinal), root);
        }
        self.obligations.push(ObligationOutcome {
            node_path: node_path.clone(),
            family: ObligationFamily::IntegerDomain,
            conjunct: 0,
            canonical_goal: Some(canonical_goal),
            components,
            discharged,
            refuted,
            contradictory,
            residual: (!discharged).then_some(residual),
            overlap_targets: None,
            derivation: outcome.derivation,
            allocation_length_upper_bound: None,
            allocation_length_upper_bound_derivation: None,
            affine_index_maps: Vec::new(),
            range_partitions: Vec::new(),
            written_before: states.written_before(discharged),
        });
    }

    fn integer_domain_goal(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
        node_path: &crate::NodePath,
        facts: &FactState,
    ) -> GoalExpression {
        GoalExpression::Operation {
            row: GoalOperation::Integer {
                operation: operation
                    .defined_query()
                    .expect("every proof-required exact row has one total domain query"),
                operand_type,
            },
            type_arguments: Vec::new(),
            const_arguments: Vec::new(),
            result: CheckedType::Bool,
            arguments: arguments
                .iter()
                .enumerate()
                .map(|(ordinal, argument)| {
                    self.obligation_goal_operand(node_path, ordinal, argument, facts)
                })
                .collect(),
        }
    }

    fn integer_domain_components(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
    ) -> Vec<BoundsRequest> {
        let operands = arguments
            .iter()
            .map(|argument| IntegerDomainOperand {
                term: self.read_operand(argument),
                constant: checked_integer_constant(argument),
            })
            .collect::<Vec<_>>();
        self.integer_domain_plan(operation, operand_type, &operands)
            .map_or_else(Vec::new, |plan| plan.components)
    }

    /// The one normalization authority attached to any goal family that has
    /// a fixed L0 interpretation. Integer domains may use a small DNF;
    /// AllocationFit is one conjunction containing its ceiling comparison.
    fn goal_normalization(&mut self, expression: &GoalExpression) -> Option<GoalNormalization> {
        if let Some(normalization) = self.conversion_goal_normalization(expression) {
            return Some(normalization);
        }
        if let Some(plan) = self.goal_integer_domain_plan(expression) {
            return Some(plan.normalization());
        }
        let GoalExpression::Operation {
            row: GoalOperation::BufferFits { maximum_length, .. },
            arguments,
            result: CheckedType::Bool,
            ..
        } = expression
        else {
            return None;
        };
        let [length] = arguments.as_slice() else {
            return None;
        };
        let threshold = self
            .terms
            .intern(TermKind::Constant(i128::from(*maximum_length)));
        Some(GoalNormalization::conjunction(vec![
            self.goal_operand(length).map(|length| Relation::Bound {
                left: length,
                right: threshold,
                bound: 0,
            }),
        ]))
    }

    fn goal_integer_domain_plan(
        &mut self,
        expression: &GoalExpression,
    ) -> Option<IntegerDomainPlan> {
        let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation,
                    operand_type,
                },
            arguments,
            result: CheckedType::Bool,
            ..
        } = expression
        else {
            return None;
        };
        if !operation.is_defined_query() {
            return None;
        }
        let mut operands = Vec::with_capacity(arguments.len());
        for argument in arguments {
            operands.push(IntegerDomainOperand {
                term: self.goal_operand(argument),
                constant: self.goal_integer_constant(argument),
            });
        }
        self.integer_domain_plan(*operation, *operand_type, &operands)
    }

    fn goal_integer_constant(&self, expression: &GoalExpression) -> Option<i128> {
        match expression {
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer { ty, bits })) => {
                Some(integer_value(*ty, *bits))
            }
            GoalExpression::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty,
            }) if projections.is_empty() => {
                let CheckedValue::Integer {
                    ty: value_type,
                    bits,
                } = &self.context.constant(*declaration)?.value
                else {
                    return None;
                };
                (*ty == CheckedType::Integer(*value_type))
                    .then(|| integer_value(*value_type, *bits))
            }
            _ => None,
        }
    }

    fn integer_domain_plan(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        operands: &[IntegerDomainOperand],
    ) -> Option<IntegerDomainPlan> {
        let fragment = fragment_type(operand_type)?;
        if matches!(
            operation,
            CheckedIntegerOperation::AddExact
                | CheckedIntegerOperation::AddDefined
                | CheckedIntegerOperation::SubtractExact
                | CheckedIntegerOperation::SubtractDefined
                | CheckedIntegerOperation::MultiplyExact
                | CheckedIntegerOperation::MultiplyDefined
        ) {
            let [left, right] = operands else {
                return None;
            };
            let conjuncts =
                overflow_conjuncts_for_values(operation, left.constant, right.constant, fragment)?;
            let operand = if conjuncts.ground {
                Some(ZERO)
            } else if left.constant.is_some() {
                right.term
            } else {
                left.term
            };
            return Some(IntegerDomainPlan {
                components: vec![
                    BoundsRequest {
                        left: conjuncts.ground.then_some(ZERO).or(operand),
                        right: ZERO,
                        bound: conjuncts.upper,
                        distinct: false,
                    },
                    BoundsRequest {
                        left: (conjuncts.ground || operand.is_some()).then_some(ZERO),
                        right: if conjuncts.ground {
                            ZERO
                        } else {
                            operand.unwrap_or(ZERO)
                        },
                        bound: conjuncts.lower,
                        distinct: false,
                    },
                ],
                kind: IntegerDomainPlanKind::Conjunction,
            });
        }

        if matches!(
            operation,
            CheckedIntegerOperation::DivideExact
                | CheckedIntegerOperation::DivideDefined
                | CheckedIntegerOperation::RemainderExact
                | CheckedIntegerOperation::RemainderDefined
        ) {
            let [dividend, divisor] = operands else {
                return None;
            };
            let mut components = vec![BoundsRequest {
                left: divisor.term,
                right: ZERO,
                bound: 0,
                distinct: true,
            }];
            let kind = if fragment.signed() {
                components.push(BoundsRequest {
                    left: dividend.term,
                    right: self
                        .terms
                        .intern(TermKind::Constant(type_range(fragment).0)),
                    bound: 0,
                    distinct: true,
                });
                components.push(BoundsRequest {
                    left: divisor.term,
                    right: self.terms.intern(TermKind::Constant(-1)),
                    bound: 0,
                    distinct: true,
                });
                IntegerDomainPlanKind::SignedDivision
            } else {
                components.push(BoundsRequest {
                    left: Some(ZERO),
                    right: ZERO,
                    bound: 0,
                    distinct: false,
                });
                IntegerDomainPlanKind::Conjunction
            };
            normalize_distinct_requests(&mut components);
            return Some(IntegerDomainPlan { components, kind });
        }

        if matches!(
            operation,
            CheckedIntegerOperation::AbsoluteExact
                | CheckedIntegerOperation::AbsoluteDefined
                | CheckedIntegerOperation::NegateExact
                | CheckedIntegerOperation::NegateDefined
        ) {
            let [operand] = operands else {
                return None;
            };
            let mut components = vec![BoundsRequest {
                left: operand.term,
                right: self
                    .terms
                    .intern(TermKind::Constant(type_range(fragment).0)),
                bound: 0,
                distinct: true,
            }];
            normalize_distinct_requests(&mut components);
            return Some(IntegerDomainPlan {
                components,
                kind: IntegerDomainPlanKind::Conjunction,
            });
        }

        if matches!(
            operation,
            CheckedIntegerOperation::ShiftLeftExact
                | CheckedIntegerOperation::ShiftLeftDefined
                | CheckedIntegerOperation::ShiftRightExact
                | CheckedIntegerOperation::ShiftRightDefined
        ) {
            let [_, amount] = operands else {
                return None;
            };
            return Some(IntegerDomainPlan {
                components: vec![BoundsRequest {
                    left: amount.term,
                    right: ZERO,
                    bound: i128::from(fragment.width()) - 1,
                    distinct: false,
                }],
                kind: IntegerDomainPlanKind::Conjunction,
            });
        }

        None
    }

    fn prove_integer_domain(
        &mut self,
        context: ProofContext<'_>,
        goal: IntegerDomainGoal<'_>,
    ) -> ProofResult {
        let finite = self.prove_integer_domain_finite(context, &goal);
        if finite.disposition != ProofDisposition::Unknown {
            return finite;
        }

        if let Some(derivation) = goal.affine_clauses.and_then(|clauses| {
            self.affine_integer_domain_derivation(
                clauses,
                context.affine,
                context.facts,
                goal.canonical,
            )
        }) {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Affine),
                derivation: Some(derivation),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        if let Some((derivation, interval)) = goal.affine_product.and_then(|product| {
            self.affine_integer_product_derivation(
                product,
                context.affine,
                context.facts,
                goal.canonical,
            )
        }) {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Affine),
                derivation: Some(derivation),
                numeric_upper_bound: None,
                // [ENT-3.S7]'s multiplication row establishes this interval on
                // whatever value the multiplication binds. It travels with the
                // judgment because only this route proved it: a domain
                // discharged by the finite L0 or affine-clause route leaves
                // that row to read the closed operand intervals instead.
                product_interval: Some(interval),
            };
        }

        ProofResult {
            disposition: ProofDisposition::Unknown,
            route: None,
            derivation: None,
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    fn prove_integer_domain_finite(
        &mut self,
        context: ProofContext<'_>,
        goal: &IntegerDomainGoal<'_>,
    ) -> ProofResult {
        let closed = close(
            context.facts,
            &self.terms,
            &self.goals,
            &mut self.derivations,
        );
        let contradictory = closed.contradictory();
        let signed_division = matches!(
            goal.operation,
            CheckedIntegerOperation::DivideExact | CheckedIntegerOperation::RemainderExact
        ) && fragment_type(goal.operand_type)
            .is_some_and(IntegerType::signed);
        let component_proof =
            |index: usize, derivations: &mut DerivationLedger| -> Option<DerivationId> {
                request_relation(goal.components.get(index)?)
                    .and_then(|relation| closed.relation_proof(&relation, derivations))
            };
        if contradictory {
            let parents = closed.contradiction_proof().map(|proof| vec![proof]);
            let Some(parents) = parents else {
                unreachable!("a contradictory closure retains its proof");
            };
            let derivation = self.derivations.intern(DerivationNode::IntegerDomain {
                goal: goal.canonical,
                parents,
            });
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: Some(derivation),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        if let Some(canonical) = goal.canonical {
            if closed.holds_opaque(canonical, GoalSign::Positive) {
                let parent = closed
                    .opaque_proof(canonical, GoalSign::Positive)
                    .expect("an opaque goal fact retains its proof");
                let derivation = self.derivations.intern(DerivationNode::IntegerDomain {
                    goal: Some(canonical),
                    parents: vec![parent],
                });
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
            if closed.holds_opaque(canonical, GoalSign::Negative) {
                return ProofResult {
                    disposition: ProofDisposition::Refuted,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        let normalization_parents = if signed_division && goal.components.len() == 3 {
            component_proof(0, &mut self.derivations).and_then(|nonzero| {
                component_proof(1, &mut self.derivations)
                    .or_else(|| component_proof(2, &mut self.derivations))
                    .map(|witness| vec![nonzero, witness])
            })
        } else if !goal.components.is_empty() {
            goal.components
                .iter()
                .map(|request| {
                    request_relation(request).and_then(|relation| {
                        closed.relation_proof(&relation, &mut self.derivations)
                    })
                })
                .collect::<Option<Vec<_>>>()
        } else {
            None
        };
        if let Some(parents) = normalization_parents {
            let parents = if let Some(canonical) = goal.canonical {
                if let Some(normalization) = closed.normalization_proof(
                    canonical,
                    GoalSign::Positive,
                    &self.goals,
                    &mut self.derivations,
                ) {
                    vec![normalization]
                } else {
                    // A complete admitted Goal may expand an ordinary-let
                    // operand into an exact operation that is not an L0 term.
                    // Its source occurrence can still have fixed L0
                    // components over the already evaluated alias. Those
                    // occurrence-local parents prove this IntegerDomain
                    // judgment directly; they must not become a normalization
                    // on the globally interned complete Goal identity.
                    parents
                }
            } else {
                parents
            };
            let derivation = self.derivations.intern(DerivationNode::IntegerDomain {
                goal: goal.canonical,
                parents,
            });
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::L0),
                derivation: Some(derivation),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        let component_false = |index: usize| {
            goal.components
                .get(index)
                .and_then(request_relation)
                .is_some_and(|relation| closed.derives(&relation.negated()))
        };
        let normalization_refuted = if signed_division && goal.components.len() == 3 {
            component_false(0) || (component_false(1) && component_false(2))
        } else {
            goal.components
                .iter()
                .filter_map(request_relation)
                .any(|relation| closed.derives(&relation.negated()))
        };
        if normalization_refuted {
            return ProofResult {
                disposition: ProofDisposition::Refuted,
                route: Some(ProofRoute::L0),
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        ProofResult {
            disposition: ProofDisposition::Unknown,
            route: None,
            derivation: None,
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    /// Builds and proves the fixed affine range normalization of one exact
    /// integer operation from its already-evaluated operands.  This function
    /// never asks for the current operation's result image: using that image's
    /// result type here would circularly assume the domain being checked.
    fn affine_integer_domain_derivation(
        &mut self,
        clauses: &[Vec<NumericAffineTarget>],
        affine: &AffineFlowState,
        facts: &FactState,
        goal: Option<GoalId>,
    ) -> Option<DerivationId> {
        for clause in clauses {
            let mut consequences = Vec::with_capacity(clause.len());
            let mut proved = true;
            for target in clause {
                let Some(proof) = self.numeric_affine_proof(
                    &target.inequality,
                    target.right,
                    ProofContext::new(facts, affine),
                ) else {
                    proved = false;
                    break;
                };
                consequences.push(self.derivations.intern(DerivationNode::AffineConsequence {
                    relation: None,
                    premises: proof.premises.into_boxed_slice(),
                    parents: proof.parents,
                }));
            }
            if proved {
                return Some(self.derivations.intern(DerivationNode::IntegerDomain {
                    goal,
                    parents: consequences,
                }));
            }
        }
        None
    }

    /// Selects the one nonlinear integer-domain rule. Both operands must be
    /// genuine affine values: constant multiplication stays on the ordinary
    /// affine path above, while every other nonlinear expression remains
    /// unavailable to this checker.
    fn affine_integer_product(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
        state: &mut AffineFlowState,
    ) -> Option<AffineIntegerProduct> {
        if !matches!(
            operation,
            CheckedIntegerOperation::MultiplyExact | CheckedIntegerOperation::MultiplyDefined
        ) {
            return None;
        }
        let CheckedType::Integer(ty) = operand_type else {
            return None;
        };
        let [left, right] = arguments else {
            return None;
        };
        let left = self.affine_pre_domain_form(left, state)?;
        let right = self.affine_pre_domain_form(right, state)?;
        if left.terms().is_empty() || right.terms().is_empty() {
            return None;
        }
        Some(AffineIntegerProduct { left, right, ty })
    }

    /// Applies the fixed interval-product rule. Once independent inclusive
    /// intervals are proved for the two affine operands, the product's extrema
    /// occur among exactly four endpoint pairs. All four products are formed
    /// with checked `i128` arithmetic before any range decision is made.
    fn affine_integer_product_derivation(
        &mut self,
        product: &AffineIntegerProduct,
        affine: &AffineFlowState,
        facts: &FactState,
        goal: Option<GoalId>,
    ) -> Option<(DerivationId, AffineProductInterval)> {
        let interval = self.affine_integer_product_interval(product, affine, facts)?;
        let derivation = self.derivations.intern(DerivationNode::IntegerDomain {
            goal,
            parents: interval.consequences.to_vec(),
        });
        Some((derivation, interval))
    }

    /// The one measurement the fixed interval-product rule performs. The four
    /// endpoint products decide [ENT-6]'s domain admission and bound the
    /// interval [ENT-3.S7]'s multiplication row publishes, so both read this
    /// result rather than proving the same endpoints twice: the admitted
    /// range and the published bound then cannot disagree by construction.
    fn affine_integer_product_interval(
        &mut self,
        product: &AffineIntegerProduct,
        affine: &AffineFlowState,
        facts: &FactState,
    ) -> Option<AffineProductInterval> {
        let assumptions = Self::affine_facts(affine);
        let left = self.affine_closed_interval_proof(&product.left, &assumptions, affine, facts)?;
        let right =
            self.affine_closed_interval_proof(&product.right, &assumptions, affine, facts)?;

        let products = [
            left.minimum.value.checked_mul(right.minimum.value)?,
            left.minimum.value.checked_mul(right.maximum.value)?,
            left.maximum.value.checked_mul(right.minimum.value)?,
            left.maximum.value.checked_mul(right.maximum.value)?,
        ];
        let (type_minimum, type_maximum) = type_range(product.ty);
        if products
            .iter()
            .any(|value| *value < type_minimum || *value > type_maximum)
        {
            return None;
        }
        // The extrema of a product over two inclusive intervals occur among
        // exactly these four pairs, so the tightest interval the rule can
        // state is their own minimum and maximum.
        let minimum = *products.iter().min()?;
        let maximum = *products.iter().max()?;

        let consequences: Vec<DerivationId> = [
            left.minimum.consequence,
            left.maximum.consequence,
            right.minimum.consequence,
            right.maximum.consequence,
        ]
        .into_iter()
        .map(|proof| {
            self.derivations.intern(DerivationNode::AffineConsequence {
                relation: None,
                premises: proof.premises.into_boxed_slice(),
                parents: proof.parents,
            })
        })
        .collect();
        Some(AffineProductInterval {
            minimum,
            maximum,
            consequences: consequences.into_boxed_slice(),
        })
    }

    /// Computes the tightest endpoint found by the existing coefficient-one
    /// rule: first the L0/type interval alone, then each source invariant once
    /// in deterministic order. The final endpoint is reproved through
    /// `affine_target_proof`, so the retained consequence names the actual
    /// invariant premise and every selected L0 endpoint.
    fn affine_closed_interval_proof(
        &mut self,
        form: &AffineForm,
        assumptions: &[ActiveAffineFact],
        values: &AffineFlowState,
        facts: &FactState,
    ) -> Option<AffineClosedIntervalProof> {
        let zero = AffineForm::constant(0);
        let upper_zero = Self::affine_less_equal(form, &zero)?;
        let lower_zero = Self::affine_less_equal(&zero, form)?;
        let constant = form.constant_value();

        let mut maximum = self
            .affine_lhs_maximum(&upper_zero, values, facts, &mut AffineCheckState::new())
            .ok()
            .flatten()
            .and_then(|terms| constant.checked_add(terms));
        let mut minimum = self
            .affine_lhs_maximum(&lower_zero, values, facts, &mut AffineCheckState::new())
            .ok()
            .flatten()
            .and_then(|terms| constant.checked_sub(terms));

        for assumption in Self::canonical_affine_facts(assumptions) {
            if let Ok(residual) = AffineInequality::residual_after(
                &upper_zero,
                &assumption.inequality,
                &mut AffineCheckState::new(),
            ) && let Some(candidate) = self
                .affine_lhs_maximum(&residual, values, facts, &mut AffineCheckState::new())
                .ok()
                .flatten()
                .and_then(|residual_maximum| {
                    constant
                        .checked_add(assumption.inequality.upper())?
                        .checked_add(residual_maximum)
                })
                && maximum.is_none_or(|current| candidate < current)
            {
                maximum = Some(candidate);
            }

            if let Ok(residual) = AffineInequality::residual_after(
                &lower_zero,
                &assumption.inequality,
                &mut AffineCheckState::new(),
            ) && let Some(candidate) = self
                .affine_lhs_maximum(&residual, values, facts, &mut AffineCheckState::new())
                .ok()
                .flatten()
                .and_then(|residual_maximum| {
                    constant
                        .checked_sub(assumption.inequality.upper())?
                        .checked_sub(residual_maximum)
                })
                && minimum.is_none_or(|current| candidate > current)
            {
                minimum = Some(candidate);
            }
        }

        let minimum = minimum?;
        let maximum = maximum?;
        if minimum > maximum {
            return None;
        }
        let minimum_target = Self::affine_less_equal(&AffineForm::constant(minimum), form)?;
        let maximum_target = Self::affine_less_equal(form, &AffineForm::constant(maximum))?;
        let minimum_proof = self.affine_target_proof(
            &minimum_target,
            assumptions,
            ProofContext::new(facts, values),
        )?;
        let maximum_proof = self.affine_target_proof(
            &maximum_target,
            assumptions,
            ProofContext::new(facts, values),
        )?;
        Some(AffineClosedIntervalProof {
            minimum: AffineIntervalEndpointProof {
                value: minimum,
                consequence: minimum_proof,
            },
            maximum: AffineIntervalEndpointProof {
                value: maximum,
                consequence: maximum_proof,
            },
        })
    }

    fn affine_integer_domain_clauses(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
        state: &mut AffineFlowState,
    ) -> Option<Vec<Vec<NumericAffineTarget>>> {
        let CheckedType::Integer(ty) = operand_type else {
            return None;
        };
        if matches!(
            operation,
            CheckedIntegerOperation::ShiftLeftExact
                | CheckedIntegerOperation::ShiftLeftDefined
                | CheckedIntegerOperation::ShiftRightExact
                | CheckedIntegerOperation::ShiftRightDefined
        ) {
            let [_, amount] = arguments else {
                return None;
            };
            let amount = self.affine_pre_domain_form(amount, state)?;
            let target = Self::affine_less_equal(
                &amount,
                &AffineForm::constant(i128::from(ty.width()) - 1),
            )?;
            let right = self
                .terms
                .intern(TermKind::Constant(i128::from(ty.width()) - 1));
            return Some(vec![vec![NumericAffineTarget {
                inequality: target,
                right: Some(right),
            }]]);
        }
        if matches!(
            operation,
            CheckedIntegerOperation::AbsoluteExact | CheckedIntegerOperation::AbsoluteDefined
        ) {
            let [value] = arguments else {
                return None;
            };
            let right = self
                .read_operand(value)
                .or_else(|| self.measure_operand(value));
            let value = self.affine_pre_domain_form(value, state)?;
            let minimum = type_range(ty).0;
            let target =
                Self::affine_less_equal(&AffineForm::constant(minimum.checked_add(1)?), &value)?;
            return Some(vec![vec![NumericAffineTarget {
                inequality: target,
                right,
            }]]);
        }
        if matches!(
            operation,
            CheckedIntegerOperation::DivideExact
                | CheckedIntegerOperation::DivideDefined
                | CheckedIntegerOperation::RemainderExact
                | CheckedIntegerOperation::RemainderDefined
        ) {
            let [dividend, divisor] = arguments else {
                return None;
            };
            let dividend_right = self
                .read_operand(dividend)
                .or_else(|| self.measure_operand(dividend));
            let divisor_right = self
                .read_operand(divisor)
                .or_else(|| self.measure_operand(divisor));
            let dividend = self.affine_pre_domain_form(dividend, state)?;
            let divisor = self.affine_pre_domain_form(divisor, state)?;
            let positive = NumericAffineTarget {
                inequality: Self::affine_less_equal(&AffineForm::constant(1), &divisor)?,
                right: divisor_right,
            };
            if !ty.signed() {
                return Some(vec![vec![positive]]);
            }
            let minus_one = self.terms.intern(TermKind::Constant(-1));
            let minus_two = self.terms.intern(TermKind::Constant(-2));
            let negative = NumericAffineTarget {
                inequality: Self::affine_less_equal(&divisor, &AffineForm::constant(-1))?,
                right: Some(minus_one),
            };
            let dividend_not_min = NumericAffineTarget {
                inequality: Self::affine_less_equal(
                    &AffineForm::constant(type_range(ty).0.checked_add(1)?),
                    &dividend,
                )?,
                right: dividend_right,
            };
            let divisor_below_minus_one = NumericAffineTarget {
                inequality: Self::affine_less_equal(&divisor, &AffineForm::constant(-2))?,
                right: Some(minus_two),
            };
            let divisor_above_minus_one = NumericAffineTarget {
                inequality: Self::affine_less_equal(&AffineForm::constant(0), &divisor)?,
                right: divisor_right,
            };
            let nonzero = [negative, positive];
            let overflow_safe = [
                dividend_not_min,
                divisor_below_minus_one,
                divisor_above_minus_one,
            ];
            let mut clauses = Vec::with_capacity(nonzero.len() * overflow_safe.len());
            for nonzero in &nonzero {
                for overflow_safe in &overflow_safe {
                    clauses.push(vec![nonzero.clone(), overflow_safe.clone()]);
                }
            }
            return Some(clauses);
        }
        let result = match operation {
            CheckedIntegerOperation::AddExact | CheckedIntegerOperation::AddDefined => {
                let [left, right] = arguments else {
                    return None;
                };
                let left = self.affine_pre_domain_form(left, state)?;
                let right = self.affine_pre_domain_form(right, state)?;
                left.add(&right, &mut AffineCheckState::new()).ok()?
            }
            CheckedIntegerOperation::SubtractExact | CheckedIntegerOperation::SubtractDefined => {
                let [left, right] = arguments else {
                    return None;
                };
                let left = self.affine_pre_domain_form(left, state)?;
                let right = self.affine_pre_domain_form(right, state)?;
                left.subtract(&right, &mut AffineCheckState::new()).ok()?
            }
            CheckedIntegerOperation::MultiplyExact | CheckedIntegerOperation::MultiplyDefined => {
                let [left, right] = arguments else {
                    return None;
                };
                let left = self.affine_pre_domain_form(left, state)?;
                let right = self.affine_pre_domain_form(right, state)?;
                if left.terms().is_empty() {
                    right
                        .scale(left.constant_value(), &mut AffineCheckState::new())
                        .ok()?
                } else if right.terms().is_empty() {
                    left.scale(right.constant_value(), &mut AffineCheckState::new())
                        .ok()?
                } else {
                    return None;
                }
            }
            CheckedIntegerOperation::NegateExact | CheckedIntegerOperation::NegateDefined => {
                let [value] = arguments else {
                    return None;
                };
                self.affine_pre_domain_form(value, state)?
                    .scale(-1, &mut AffineCheckState::new())
                    .ok()?
            }
            _ => return None,
        };
        let (minimum, maximum) = type_range(ty);
        let mut check = AffineCheckState::new();
        let maximum_term = self.terms.intern(TermKind::Constant(maximum));
        Some(vec![vec![
            NumericAffineTarget {
                inequality: AffineInequality::from_forms(
                    &result,
                    &AffineForm::constant(maximum),
                    &mut check,
                )
                .ok()?,
                right: Some(maximum_term),
            },
            NumericAffineTarget {
                inequality: AffineInequality::from_forms(
                    &AffineForm::constant(minimum),
                    &result,
                    &mut check,
                )
                .ok()?,
                right: None,
            },
        ]])
    }

    fn affine_less_equal(left: &AffineForm, right: &AffineForm) -> Option<AffineInequality> {
        AffineInequality::from_forms(left, right, &mut AffineCheckState::new()).ok()
    }

    /// Exact pre-domain value construction.  Unlike ordinary value flow it
    /// has no fresh-result fallback: failure to reconstruct the mathematical
    /// operands simply makes the affine proof route unavailable.
    fn affine_pre_domain_form(
        &mut self,
        expression: &CheckedExpression,
        state: &mut AffineFlowState,
    ) -> Option<AffineForm> {
        if let Some(value) = self.constant_storage_scalar(expression).cloned() {
            return self.affine_pre_domain_form(&CheckedExpression::Constant(value), state);
        }
        let mut events = Vec::new();
        self.collect_expression_kills(expression, &mut events);
        if !events.is_empty() {
            return None;
        }
        match expression {
            CheckedExpression::ArrayMeasure { .. }
            | CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::RangeMeasure { .. }
            | CheckedExpression::RangeElementMeasure { .. }
            | CheckedExpression::ContainerMeasure { .. } => self
                .checked_measure_term(expression)
                .map(|term| self.measure_atom(term, state)),
            CheckedExpression::Constant(CheckedValue::Integer { ty, bits })
            | CheckedExpression::NamedConstant {
                value: CheckedValue::Integer { ty, bits },
                ..
            } => Some(AffineForm::constant(integer_value(*ty, *bits))),
            // [MSR-6] a symbolic const generic is one declaration-anchored
            // value throughout the generic body. Counted-range endpoint
            // capture must use that same image as an invariant or contract
            // spelling of the parameter; treating the endpoint as an
            // unrelated unknown loses the exhaustion fact at the return.
            CheckedExpression::Constant(CheckedValue::ConstGeneric { declaration, .. }) => {
                let term = self.const_parameter_term(*declaration);
                Some(self.measure_atom(term, state))
            }
            CheckedExpression::Binding { binding, ty, .. } => {
                let CheckedType::Integer(integer) = *ty else {
                    return None;
                };
                if self.affine_binding_type(*binding) != Some(integer) {
                    return None;
                }
                if let Some(value) = state.values.get(binding) {
                    Some(value.clone())
                } else {
                    let value = self.new_affine_atom(integer);
                    state.values.insert(*binding, value.clone());
                    Some(value)
                }
            }
            CheckedExpression::NumericConversion {
                mode: CheckedConversionMode::Exact,
                source: CheckedNumericType::Integer(_),
                destination: CheckedNumericType::Integer(_),
                value,
                ..
            } => self.affine_pre_domain_form(value, state),
            CheckedExpression::IntegerOperation {
                operation,
                arguments,
                result: CheckedType::Integer(_),
                ..
            } => match operation {
                CheckedIntegerOperation::AddExact | CheckedIntegerOperation::AddDefined => {
                    let [left, right] = arguments.as_slice() else {
                        return None;
                    };
                    self.affine_pre_domain_form(left, state)?
                        .add(
                            &self.affine_pre_domain_form(right, state)?,
                            &mut AffineCheckState::new(),
                        )
                        .ok()
                }
                CheckedIntegerOperation::SubtractExact
                | CheckedIntegerOperation::SubtractDefined => {
                    let [left, right] = arguments.as_slice() else {
                        return None;
                    };
                    self.affine_pre_domain_form(left, state)?
                        .subtract(
                            &self.affine_pre_domain_form(right, state)?,
                            &mut AffineCheckState::new(),
                        )
                        .ok()
                }
                CheckedIntegerOperation::MultiplyExact
                | CheckedIntegerOperation::MultiplyDefined => {
                    let [left, right] = arguments.as_slice() else {
                        return None;
                    };
                    let left = self.affine_pre_domain_form(left, state)?;
                    let right = self.affine_pre_domain_form(right, state)?;
                    if left.terms().is_empty() {
                        right
                            .scale(left.constant_value(), &mut AffineCheckState::new())
                            .ok()
                    } else if right.terms().is_empty() {
                        left.scale(right.constant_value(), &mut AffineCheckState::new())
                            .ok()
                    } else {
                        None
                    }
                }
                CheckedIntegerOperation::NegateExact | CheckedIntegerOperation::NegateDefined => {
                    let [value] = arguments.as_slice() else {
                        return None;
                    };
                    self.affine_pre_domain_form(value, state)?
                        .scale(-1, &mut AffineCheckState::new())
                        .ok()
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn render_integer_domain_goal(
        &self,
        operation: CheckedIntegerOperation,
        arguments: &[CheckedExpression],
    ) -> String {
        let rendered = arguments
            .iter()
            .map(|argument| self.render_expression(argument))
            .collect::<Vec<_>>();
        match (operation, rendered.as_slice()) {
            (CheckedIntegerOperation::AddExact, [left, right]) => {
                format!("{left} +defined {right}")
            }
            (CheckedIntegerOperation::SubtractExact, [left, right]) => {
                format!("{left} -defined {right}")
            }
            (CheckedIntegerOperation::MultiplyExact, [left, right]) => {
                format!("{left} *defined {right}")
            }
            (CheckedIntegerOperation::DivideExact, [left, right]) => {
                format!("{left} /defined {right}")
            }
            (CheckedIntegerOperation::RemainderExact, [left, right]) => {
                format!("{left} %defined {right}")
            }
            (CheckedIntegerOperation::AbsoluteExact, [value]) => {
                format!("iabs.defined({value})")
            }
            (CheckedIntegerOperation::NegateExact, [value]) => {
                format!("ineg.defined({value})")
            }
            (CheckedIntegerOperation::ShiftLeftExact, [value, amount]) => {
                format!("ishl.defined({value}, {amount})")
            }
            (CheckedIntegerOperation::ShiftRightExact, [value, amount]) => {
                format!("ishr.defined({value}, {amount})")
            }
            _ => "<invalid integer-domain goal>".to_owned(),
        }
    }

    fn judge_set_target(&mut self, target: &CheckedSetTarget, states: &mut ProofFlowState) -> bool {
        match target {
            CheckedSetTarget::Place(_) => true,
            // [OP-4, REF-4] judge the outer range position and every nested
            // subscript in source order before the commit may execute.
            CheckedSetTarget::RangeIndex(target) => self.judge_range_element_place(target, states),
            // [OP-4, WIN-1] the run's own obligation is `i < len_of(v)`: the
            // offset is a logical one and the window's length bounds it, so
            // the measured kind is the run's and the written capacity is not
            // the bound.
            CheckedSetTarget::Storage(target) => self.judge_place_subscripts(target, states),
        }
    }

    // ------------------------------------------------------------------
    // Statement walk
    // ------------------------------------------------------------------

    /// Walks one block in its own lexical scope. Returns the fall-through:
    /// `true` when control continues past the block with `state` holding the
    /// post-scope-exit facts.
    fn affine_binding_type(&self, binding: BindingId) -> Option<IntegerType> {
        match self.summary(binding)?.ty? {
            CheckedType::Integer(ty) if !self.is_holder(binding) => Some(ty),
            _ => None,
        }
    }

    fn new_affine_atom(&mut self, ty: IntegerType) -> AffineForm {
        let (minimum, maximum) = type_range(ty);
        self.new_affine_atom_with_interval(ty, minimum, maximum, false)
    }

    fn new_affine_atom_with_interval(
        &mut self,
        ty: IntegerType,
        minimum: i128,
        maximum: i128,
        join_delta: bool,
    ) -> AffineForm {
        let index = u32::try_from(self.affine_atoms.len())
            .expect("affine value atoms exceed the u32 identity space");
        self.affine_atoms.push(AffineAtom {
            ty,
            minimum,
            maximum,
            join_delta,
        });
        AffineForm::term(AffineTermId::from_index(index))
    }

    /// Folds every delta atom an earlier join minted back into the constant
    /// interval it stands for [ENT-6].
    ///
    /// Without this, a delta atom counts as an ordinary nonconstant term at
    /// the next join, so two joins in sequence lose an image one join over the
    /// same branches keeps: acceptance would depend on whether the writer
    /// spelled the branch set as nested conditionals or as one flat match.
    fn fold_join_deltas(&self, value: &AffineForm) -> Option<FoldedJoinImage> {
        let mut form = value.nonconstant_part();
        let mut minimum = value.constant_value();
        let mut maximum = minimum;
        let mut check = AffineCheckState::new();
        for coefficient in value.terms() {
            let atom = self.affine_atoms.get(coefficient.term().index() as usize)?;
            if !atom.join_delta {
                continue;
            }
            let low = coefficient.coefficient().checked_mul(atom.minimum)?;
            let high = coefficient.coefficient().checked_mul(atom.maximum)?;
            minimum = minimum.checked_add(low.min(high))?;
            maximum = maximum.checked_add(low.max(high))?;
            let folded = AffineForm::term(coefficient.term())
                .scale(coefficient.coefficient(), &mut check)
                .ok()?;
            form = form.subtract(&folded, &mut check).ok()?;
        }
        Some(FoldedJoinImage {
            form,
            minimum,
            maximum,
        })
    }

    fn new_affine_binding_atom(&mut self, binding: BindingId) -> Option<AffineForm> {
        let ty = self.affine_binding_type(binding)?;
        Some(self.new_affine_atom(ty))
    }

    /// The one atom that stands for a binding's whole value, for the length of
    /// one certificate.
    ///
    /// A binding whose image is already a single atom is its own handle and
    /// mints nothing. Otherwise a fresh atom is minted and the image it stands
    /// for is remembered, so the handle can be unfolded again before anything
    /// is proved.
    ///
    /// [PRF-1]'s fold needs this because a local's image is transparent by
    /// design. `let stride = width + padding; let base = stride * row;` gives
    /// the product the operands `width + padding` and `row`, so a certificate
    /// scaling by `stride` distributes into pieces no admitted multiplication
    /// matches. Naming the binding on both sides — the product it forms and
    /// the multiplicity that scales by it — is what makes the two agree, and
    /// it is the rule [PRF-1] already states one sentence away for a named
    /// premise: resolve by the declaration the writer wrote, not by whatever
    /// that declaration currently expands to.
    ///
    /// The handle is deliberately not published as a fact and does not replace
    /// the binding's image. Both were tried and both cost more than they
    /// bought: a published equality is invisible to the residual, which is the
    /// direct L0 route by rule, and replacing the image makes every ordinary
    /// premise about the binding need that equality to prove. Keeping it a
    /// name that exists between the fold and the residual leaves the rest of
    /// the checker reading exactly what it read before.
    fn affine_opaque_handle(
        &mut self,
        binding: BindingId,
        state: &mut AffineFlowState,
    ) -> Option<AffineForm> {
        if let Some(image) = state.values.get(&binding)
            && image.unit_term().is_some()
        {
            return Some(image.clone());
        }
        if let Some(handle) = state.opaque_values.get(&binding) {
            return Some(handle.clone());
        }
        let handle = self.new_affine_binding_atom(binding)?;
        let Some(image) = state.values.get(&binding).cloned() else {
            state.values.insert(binding, handle.clone());
            state.opaque_values.insert(binding, handle.clone());
            return Some(handle);
        };
        let atom = handle.unit_term()?;
        self.handle_images.insert(atom, image);
        state.opaque_values.insert(binding, handle.clone());
        Some(handle)
    }

    /// Intersects every published affine fact by canonical numeric content,
    /// then records representative predecessor evidence for diagnostics.
    ///
    /// The first phase deliberately does not inspect evidence or the source
    /// category that published a fact: an inequality survives exactly when
    /// every predecessor contains the same relation over the same immutable
    /// value images. Loop dependencies are then unioned conservatively, so a
    /// cross-category match cannot carry an assumption beyond its loop.
    fn join_affine_facts(&mut self, states: &[ProofFlowState]) -> Vec<ActiveAffineFact> {
        let Some(first) = states.first() else {
            return Vec::new();
        };

        // This is deliberately an ordered intersection, not a set iteration:
        // the first contributing structural predecessor fixes the retained
        // order, and the first canonical occurrence in that predecessor fixes
        // each fact's position. Evidence category never participates.
        let mut common_inequalities = Vec::new();
        for candidate in &first.affine.facts {
            if common_inequalities.contains(&candidate.inequality) {
                continue;
            }
            if states.iter().skip(1).all(|state| {
                state
                    .affine
                    .facts
                    .iter()
                    .any(|fact| fact.inequality == candidate.inequality)
            }) {
                common_inequalities.push(candidate.inequality.clone());
            }
        }

        common_inequalities
            .into_iter()
            .map(|inequality| {
                let witnesses = states
                    .iter()
                    .map(|state| {
                        state
                            .affine
                            .facts
                            .iter()
                            .filter(|fact| fact.inequality == inequality)
                            // A stable witness is strictly preferable to an
                            // active-loop assumption for the same canonical
                            // fact. Remaining ties keep deterministic fact
                            // insertion order and do not affect acceptance.
                            .min_by_key(|fact| fact.active_loops.len())
                            .expect("a common affine inequality has one witness")
                    })
                    .collect::<Vec<_>>();

                let mut active_loops = witnesses
                    .iter()
                    .flat_map(|fact| fact.active_loops.iter().copied())
                    .collect::<Vec<_>>();
                active_loops.sort_unstable_by_key(|loop_id| loop_id.0);
                active_loops.dedup();

                let first_evidence = witnesses[0].evidence;
                let evidence = if witnesses.iter().all(|fact| fact.evidence == first_evidence) {
                    first_evidence
                } else if witnesses
                    .iter()
                    .all(|fact| matches!(fact.evidence, AffineFactEvidence::Source(_)))
                {
                    let predecessors = witnesses
                        .iter()
                        .map(|fact| match fact.evidence {
                            AffineFactEvidence::Source(source) => source,
                            AffineFactEvidence::Derivation(_) => unreachable!(),
                        })
                        .collect::<Vec<_>>()
                        .into_boxed_slice();
                    let join_ordinal = u32::try_from(self.joined_source_proofs.len())
                        .expect("joined affine fact count exceeds the u32 identity space");
                    self.joined_source_proofs
                        .push(JoinedSourceProofProvenance { predecessors });
                    AffineFactEvidence::Source(SourceAffineFactRef::JoinedSourceProof {
                        join_ordinal,
                    })
                } else {
                    // Derivation and source identities are explanation only.
                    // The canonical all-predecessor intersection above is the
                    // sole authority for retaining this conclusion.
                    first_evidence
                };
                ActiveAffineFact {
                    inequality,
                    evidence,
                    active_loops,
                }
            })
            .collect()
    }

    fn join_affine_states(&mut self, states: &[ProofFlowState]) -> AffineFlowState {
        let Some(first) = states.first() else {
            return AffineFlowState::default();
        };
        let mut bindings = first.affine.values.keys().copied().collect::<Vec<_>>();
        bindings.sort_by_key(|binding| binding.0);
        let mut values = WordHashMap::default();
        for binding in bindings {
            let Some(first_value) = first.affine.values.get(&binding) else {
                continue;
            };
            if !states
                .iter()
                .skip(1)
                .all(|state| state.affine.values.contains_key(&binding))
            {
                continue;
            }
            let value = if states.iter().skip(1).all(|state| {
                state
                    .affine
                    .values
                    .get(&binding)
                    .is_some_and(|value| value == first_value)
            }) {
                first_value.clone()
            } else if let Some(ty) = self.affine_binding_type(binding) {
                // [ENT-6] every input is normalized before the comparison: a
                // delta atom an earlier join minted folds back into the
                // constant interval it stands for, so what is compared is the
                // part of the image no join invented. Nested joins therefore
                // reach exactly the image one flat join over the same branches
                // reaches, and acceptance stops depending on the join shape.
                let folded = states
                    .iter()
                    .map(|state| {
                        state
                            .affine
                            .values
                            .get(&binding)
                            .and_then(|value| self.fold_join_deltas(value))
                    })
                    .collect::<Option<Vec<_>>>();
                match folded {
                    Some(folded)
                        if folded
                            .iter()
                            .skip(1)
                            .all(|image| image.form == folded[0].form) =>
                    {
                        let minimum = folded
                            .iter()
                            .map(|image| image.minimum)
                            .min()
                            .expect("one join input exists");
                        let maximum = folded
                            .iter()
                            .map(|image| image.maximum)
                            .max()
                            .expect("one join input exists");
                        let atom_start = self.affine_atoms.len();
                        let delta = self.new_affine_atom_with_interval(ty, minimum, maximum, true);
                        match folded[0].form.add(&delta, &mut AffineCheckState::new()) {
                            Ok(value) => value,
                            Err(_) => {
                                self.affine_atoms.truncate(atom_start);
                                self.new_affine_atom(ty)
                            }
                        }
                    }
                    _ => self.new_affine_atom(ty),
                }
            } else {
                continue;
            };
            values.insert(binding, value);
        }
        // An opaque handle is a convenience for one certificate, not a fact,
        // so a join keeps none: the next demand re-mints against whatever the
        // joined image is.
        let opaque_values = WordHashMap::default();

        // A measure keeps its current image only when every predecessor
        // carries that exact image. Otherwise a later query mints a fresh
        // image; no branch-local equality is promoted through this join.
        let mut measure_atoms = first.affine.measure_atoms.borrow().clone();
        measure_atoms.retain(|term, image| {
            states
                .iter()
                .skip(1)
                .all(|state| state.affine.measure_atoms.borrow().get(term) == Some(image))
        });

        AffineFlowState {
            values,
            measure_atoms: RefCell::new(measure_atoms),
            opaque_values,
            ranges: first
                .affine
                .ranges
                .iter()
                .filter(|(id, range)| {
                    states
                        .iter()
                        .skip(1)
                        .all(|state| state.affine.ranges.get(id) == Some(*range))
                })
                .map(|(id, range)| (*id, range.clone()))
                .collect(),
            indices: first
                .affine
                .indices
                .iter()
                .filter(|(id, image)| {
                    states
                        .iter()
                        .skip(1)
                        .all(|state| state.affine.indices.get(id) == Some(*image))
                })
                .map(|(id, image)| (*id, image.clone()))
                .collect(),
            facts: self.join_affine_facts(states),
            published_invariants: first
                .affine
                .published_invariants
                .iter()
                .filter(|(declaration, inequality)| {
                    states.iter().skip(1).all(|state| {
                        state.affine.published_invariants.get(declaration) == Some(*inequality)
                    })
                })
                .map(|(declaration, inequality)| (*declaration, inequality.clone()))
                .collect(),
        }
    }

    fn join_flows(&mut self, states: &[ProofFlowState]) -> ProofFlowState {
        // Close L0 contradiction before any structural intersection. An
        // unreachable predecessor is neutral: it cannot erase a live affine
        // value, published invariant name, or canonical fact. Keeping all
        // promoted L0 states in `join_at` still records their contradiction
        // proofs as join parents. When every predecessor is contradictory,
        // the affine component is deliberately empty because L0 proves every
        // downstream goal.
        let mut promoted = states.to_vec();
        for state in &mut promoted {
            self.promote_flow_contradiction(state);
        }
        let contributing = promoted
            .iter()
            .filter(|state| !state.facts.all_derivable)
            .cloned()
            .collect::<Vec<_>>();
        let facts = promoted
            .iter()
            .map(|states| states.facts.clone())
            .collect::<Vec<_>>();
        let event = self.derivations.event(FlowEventKind::Join, None);
        let entry_images = (0..self.entry_images.len())
            .map(|index| {
                contributing
                    .iter()
                    .filter_map(|state| state.entry_images[index])
                    .min()
            })
            .collect();
        let results = self.join_result_evidence(&contributing);
        let mut continuing = Vec::new();
        for state in states {
            record_continuing(&mut continuing, &state.continuing);
        }
        ProofFlowState {
            results,
            facts: join_at(
                &facts,
                &self.terms,
                &self.goals,
                &mut self.derivations,
                event,
            ),
            entry_images,
            separations: SeparationLedger::intersection(
                contributing.iter().map(|state| &state.separations),
            ),
            affine: self.join_affine_states(&contributing),
            written: contributing
                .iter()
                .flat_map(|state| state.written.iter().copied())
                .collect(),
            continuing,
        }
    }

    fn eligible_delivery_terms(
        &mut self,
        value: &CheckedExpression,
        receiver_type: CheckedType,
    ) -> Option<(BindingId, TermId, IntegerType)> {
        let CheckedExpression::Binding {
            binding,
            ty,
            consume_root: false,
            ..
        } = value
        else {
            return None;
        };
        if *ty != receiver_type {
            return None;
        }
        let fragment = fragment_type(*ty)?;
        let carrier = self.terms.intern(TermKind::Place(
            ResolvedPlace::spelled(PlaceRoot::Binding(*binding), false, Vec::new()),
            fragment,
        ));
        Some((*binding, carrier, fragment))
    }

    fn substitute_delivery_relation(
        relation: &Relation,
        carrier: TermId,
        receiver: TermId,
    ) -> Relation {
        let replace = |term| if term == carrier { receiver } else { term };
        match relation {
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
                // Ordering the pair reverses the difference with it.
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
                        difference: -difference,
                    }
                }
            }
        }
    }

    fn delivery_edge_state(
        &mut self,
        closed: ClosedState,
        context: &DeliveryEdgeContext<'_>,
    ) -> FactState {
        if closed.contradictory() {
            return FactState::contradictory(
                closed
                    .contradiction_proof()
                    .expect("contradictory delivery edge has one exact proof"),
            );
        }
        let mut image = FactState::new();
        let mut explicit = HashMap::new();
        for (source_relation, parent) in closed.delivery_relations() {
            if !source_relation.terms().contains(&context.carrier)
                || !self
                    .derivations
                    .depends_on_explicit_relation(parent, &mut explicit)
            {
                continue;
            }
            let relation = Self::substitute_delivery_relation(
                &source_relation,
                context.carrier,
                context.receiver,
            );
            let proof = self.derivations.intern(DerivationNode::PostconditionGive {
                statement: context.statement.clone(),
                carrier: context.carrier_binding,
                receiver: context.receiver_binding,
                relation: Box::new(relation.clone()),
                event: context.event,
                parent,
            });
            image.establish_from_proof(&relation, proof, &self.derivations);
        }
        image
    }

    fn retain_delivery_give_parents(&mut self, parents: &[JoinParent]) {
        for parent in parents {
            if !matches!(
                self.derivations.nodes[parent.parent.0 as usize],
                DerivationNode::PostconditionGive { .. }
            ) {
                continue;
            }
            let occurrence = u32::try_from(self.delivery_give_roots.len())
                .expect("value-if give roots exceed the u32 identity space");
            // A full and an ordinary delivery join can share an edge parent.
            // The ledger retains one required root per Give node.
            if !self.delivery_give_roots.insert(parent.parent) {
                continue;
            }
            self.derivations.add_root(
                DerivationRootKind::PostconditionGive { occurrence },
                parent.parent,
            );
        }
    }

    fn value_delivery_image(
        &mut self,
        value: &CheckedExpression,
        source: &ProofFlowState,
        context: DeliveryImageContext<'_>,
    ) -> ProofFlowState {
        let Some((carrier_binding, carrier, fragment)) =
            self.eligible_delivery_terms(value, context.receiver_type)
        else {
            return ProofFlowState::default();
        };
        let receiver = self.terms.intern(TermKind::Place(
            ResolvedPlace::spelled(
                PlaceRoot::Binding(context.receiver_binding),
                false,
                Vec::new(),
            ),
            fragment,
        ));
        // Every edge explicitly withholds the fresh receiver, including
        // edges visited after an earlier give interned the same stable term.
        // No implicit fact on x may participate in selecting d -> x.
        let facts = close_excluding_term(
            &source.facts,
            &self.terms,
            &self.goals,
            &mut self.derivations,
            receiver,
        );
        let event = self.proof_event(FlowEventKind::PostconditionGive, Some(context.statement));
        let edge = DeliveryEdgeContext {
            statement: context.statement,
            carrier_binding,
            receiver_binding: context.receiver_binding,
            carrier,
            receiver,
            event,
        };
        let mut delivered = self.delivery_edge_state(facts, &edge);
        if delivered.may_hold_postcondition_candidates() {
            let mut ordinary = source.facts.clone();
            ordinary.retain_non_postcondition_candidates(&self.derivations);
            let ordinary = close_excluding_term(
                &ordinary,
                &self.terms,
                &self.goals,
                &mut self.derivations,
                receiver,
            );
            let fallback = self.delivery_edge_state(ordinary, &edge);
            delivered.merge_relation_candidates_from(&fallback, &self.derivations);
        }
        let mut image = ProofFlowState {
            facts: delivered,
            results: BTreeMap::new(),
            entry_images: Vec::new(),
            separations: source.separations.clone(),
            // Delivery-image construction currently exists only to retain
            // postcondition relations.  Withholding an affine image is
            // conservative; the normal value-initializer join installs the
            // receiver's value separately.
            affine: AffineFlowState::default(),
            written: source.written.clone(),
            continuing: Vec::new(),
        };
        // The forward substitution happens above before the ordinary edge
        // kills, so the carrier's own branch scope cannot delete the image.
        self.kill_scopes_to(&mut image, context.scope_depth);
        self.exit_counted_loops_from(&mut image, context.loop_depth);
        image
    }

    fn establish_delivery_join(
        &mut self,
        images: &[FactState],
        context: &DeliveryJoinContext<'_>,
        target: &mut FactState,
    ) {
        self.establish_delivery_join_once(images, context, target, false);
        if !images
            .iter()
            .any(FactState::may_hold_postcondition_candidates)
        {
            return;
        }
        // Substitution preserves both proof layers. The join must do so too:
        // selecting only the strongest edge proof here would lose an ordinary
        // fallback when a later holder event removes call-dependent proofs.
        let mut ordinary = images.to_vec();
        for image in &mut ordinary {
            image.retain_non_postcondition_candidates(&self.derivations);
        }
        self.establish_delivery_join_once(&ordinary, context, target, true);
    }

    fn establish_delivery_join_once(
        &mut self,
        images: &[FactState],
        context: &DeliveryJoinContext<'_>,
        target: &mut FactState,
        ordinary_only: bool,
    ) {
        assert!(images.iter().all(|image| {
            image.all_derivable
                || image.live_l0_relations().iter().all(|(_, proof)| {
                    matches!(
                        self.derivations.nodes[proof.0 as usize],
                        DerivationNode::PostconditionGive { .. }
                    )
                })
        }));
        let contributing = images
            .iter()
            .enumerate()
            .filter_map(|(index, image)| (!image.all_derivable).then_some(index))
            .collect::<Vec<_>>();
        let Some((&first_index, rest)) = contributing.split_first() else {
            return;
        };
        let first = &images[first_index];
        let bound_pairs = first
            .bounds
            .cells()
            .map(|(left, right, bound, _)| ((left, right), bound))
            .collect::<Vec<_>>();
        for (pair, first_bound) in bound_pairs {
            if pair.0 != context.receiver && pair.1 != context.receiver {
                continue;
            }
            let mut weakest = first_bound;
            if !rest.iter().all(|index| {
                images[*index]
                    .bounds
                    .get(pair.0, pair.1)
                    .is_some_and(|(bound, _)| {
                        weakest = weakest.max(bound);
                        true
                    })
            }) {
                continue;
            }
            if ordinary_only
                && target
                    .bounds
                    .get(pair.0, pair.1)
                    .is_some_and(|(bound, proof)| {
                        bound <= weakest && !self.derivations.depends_on_postcondition_call(proof)
                    })
            {
                continue;
            }
            let parents = images
                .iter()
                .enumerate()
                .map(|(ordinal, image)| JoinParent {
                    ordinal: u32::try_from(ordinal)
                        .expect("delivery predecessor ordinal exceeds the u32 identity space"),
                    parent: if image.all_derivable {
                        image
                            .contradiction
                            .expect("contradictory delivery image has one proof")
                    } else {
                        image
                            .bounds
                            .get(pair.0, pair.1)
                            .map(|(_, proof)| proof)
                            .expect("every contributing delivery image holds the pair")
                    },
                })
                .collect::<Vec<_>>();
            let relation = Relation::Bound {
                left: pair.0,
                right: pair.1,
                bound: weakest,
            };
            let proof = self
                .derivations
                .intern(DerivationNode::PostconditionDeliveryJoin {
                    detail: Box::new(super::state::PostconditionDeliveryJoinDetail {
                        statement: context.statement.clone(),
                        receiver: context.receiver_binding,
                        relation: relation.clone(),
                        event: context.event,
                        parents,
                    }),
                });
            let DerivationNode::PostconditionDeliveryJoin { detail } =
                &self.derivations.nodes[proof.0 as usize]
            else {
                unreachable!("just interned one delivery join")
            };
            let parents = detail.parents.clone();
            self.retain_delivery_give_parents(&parents);
            let occurrence = self.delivery_join_roots;
            self.delivery_join_roots = self
                .delivery_join_roots
                .checked_add(1)
                .expect("value-if delivery join roots exceed the u32 identity space");
            self.derivations.add_root(
                DerivationRootKind::PostconditionDeliveryJoin { occurrence },
                proof,
            );
            target.establish_from_proof(&relation, proof, &self.derivations);
        }

        let mut distinct = first.distinct.iter().copied().collect::<Vec<_>>();
        distinct.sort_unstable();
        for pair in distinct {
            if (pair.0 != context.receiver && pair.1 != context.receiver)
                || !rest
                    .iter()
                    .all(|index| images[*index].distinct.contains(&pair))
            {
                continue;
            }
            if ordinary_only
                && target
                    .distinct_proofs
                    .get(&pair)
                    .is_some_and(|proof| !self.derivations.depends_on_postcondition_call(*proof))
            {
                continue;
            }
            let parents = images
                .iter()
                .enumerate()
                .map(|(ordinal, image)| JoinParent {
                    ordinal: u32::try_from(ordinal)
                        .expect("delivery predecessor ordinal exceeds the u32 identity space"),
                    parent: if image.all_derivable {
                        image
                            .contradiction
                            .expect("contradictory delivery image has one proof")
                    } else {
                        image.distinct_proofs[&pair]
                    },
                })
                .collect::<Vec<_>>();
            let relation = Relation::Distinct {
                left: pair.0,
                right: pair.1,
                difference: 0,
            };
            let proof = self
                .derivations
                .intern(DerivationNode::PostconditionDeliveryJoin {
                    detail: Box::new(super::state::PostconditionDeliveryJoinDetail {
                        statement: context.statement.clone(),
                        receiver: context.receiver_binding,
                        relation: relation.clone(),
                        event: context.event,
                        parents,
                    }),
                });
            let DerivationNode::PostconditionDeliveryJoin { detail } =
                &self.derivations.nodes[proof.0 as usize]
            else {
                unreachable!("just interned one delivery join")
            };
            let parents = detail.parents.clone();
            self.retain_delivery_give_parents(&parents);
            let occurrence = self.delivery_join_roots;
            self.delivery_join_roots = self
                .delivery_join_roots
                .checked_add(1)
                .expect("value-if delivery join roots exceed the u32 identity space");
            self.derivations.add_root(
                DerivationRootKind::PostconditionDeliveryJoin { occurrence },
                proof,
            );
            target.establish_from_proof(&relation, proof, &self.derivations);
        }
    }

    fn establish_value_delivery_join(&mut self, frame: &GiveFrame, target: &mut ProofFlowState) {
        assert_eq!(frame.delivery_images.len(), frame.gives.len());
        assert_eq!(frame.delivery_edges.len(), frame.delivery_images.len());
        assert!(
            frame
                .delivery_edges
                .windows(2)
                .all(|pair| { pair[0].components().cmp(pair[1].components()).is_lt() })
        );
        let Some(fragment) = fragment_type(frame.result_type) else {
            return;
        };
        let receiver = self.terms.intern(TermKind::Place(
            ResolvedPlace::spelled(PlaceRoot::Binding(frame.binding), false, Vec::new()),
            fragment,
        ));
        let event = self.proof_event(
            FlowEventKind::PostconditionDeliveryJoin,
            Some(&frame.node_path),
        );
        let context = DeliveryJoinContext {
            statement: &frame.node_path,
            receiver_binding: frame.binding,
            receiver,
            event,
        };
        let facts = frame
            .delivery_images
            .iter()
            .map(|image| image.facts.clone())
            .collect::<Vec<_>>();
        self.establish_delivery_join(&facts, &context, &mut target.facts);
    }

    fn walk_block(&mut self, statements: &[CheckedStatement], state: &mut ProofFlowState) -> bool {
        self.scopes.push(Vec::new());
        let mut continues = true;
        for statement in statements {
            if !continues {
                break;
            }
            continues = self.walk_statement(statement, state);
        }
        if continues {
            let depth = self.scopes.len() - 1;
            self.exit_scopes_to(state, depth);
        }
        self.scopes.pop();
        continues
    }

    fn declare(&mut self, binding: BindingId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(binding);
        }
    }

    fn affine_unknown_integer(&mut self, ty: CheckedType) -> Option<AffineForm> {
        let CheckedType::Integer(ty) = ty else {
            return None;
        };
        Some(self.new_affine_atom(ty))
    }

    fn affine_expression_form(
        &mut self,
        expression: &CheckedExpression,
        state: &mut AffineFlowState,
    ) -> Option<AffineForm> {
        let mut events = Vec::new();
        self.collect_expression_kills(expression, &mut events);
        if !events.is_empty() {
            return self.affine_unknown_integer(expression.ty());
        }
        self.affine_pure_expression_form(expression, state)
    }

    /// The operand value images of one admitted S7 unsigned division, read
    /// where it was evaluated. A `set` commit reads both before its target
    /// kill, since an operand naming that place has a new image afterwards.
    fn unsigned_division_operand_forms(
        &mut self,
        value: &CheckedExpression,
        state: &mut AffineFlowState,
    ) -> Option<(AffineForm, AffineForm)> {
        let CheckedExpression::IntegerOperation { arguments, .. } = value else {
            return None;
        };
        let [dividend, divisor] = arguments.as_slice() else {
            return None;
        };
        Some((
            self.affine_pre_domain_form(dividend, state)?,
            self.affine_pre_domain_form(divisor, state)?,
        ))
    }

    /// Records what one admitted exact multiplication's bound value equals.
    ///
    /// The domain judgment already measured the operands where the product was
    /// formed; this pairs that measurement with the atom the binding took, so
    /// [PRF-1] can recognize `n*p` in a certificate sum as the value `base`
    /// already holds. A product whose result image is not one atom — a
    /// conversion, a further operation — records nothing, because there is
    /// then no single value the monomial equals.
    fn record_product_atom(
        &mut self,
        binding: BindingId,
        value: &CheckedExpression,
        state: &mut AffineFlowState,
    ) {
        let CheckedExpression::IntegerOperation {
            carrier, arguments, ..
        } = value
        else {
            return;
        };
        if !self.product_operands.contains_key(carrier) {
            return;
        }
        let [left, right] = arguments.as_slice() else {
            return;
        };
        let Some(product) = state.values.get(&binding).and_then(AffineForm::unit_term) else {
            return;
        };
        // Constant-scaled products already have a transparent affine image
        // and need no opaque operand handles for a nonlinear certificate fold.
        let nonconstant =
            |image: Option<AffineForm>| image.is_some_and(|image| !image.terms().is_empty());
        if !nonconstant(self.affine_pre_domain_form(left, state))
            || !nonconstant(self.affine_pre_domain_form(right, state))
        {
            return;
        }
        // What proved the domain and what the fold names are two questions.
        // The domain judgment reads the transparent images, whose intervals are
        // what admit the multiply at all; the record names the bindings, so a
        // certificate scaling by one of them meets the same value here. Reading
        // handles at the domain site instead was tried and costs the interval:
        // an opaque operand is only bounded by its type, and the four endpoint
        // products then leave the range.
        let (Some(left), Some(right)) = (
            self.affine_operand_handle(left, state),
            self.affine_operand_handle(right, state),
        ) else {
            return;
        };
        self.product_atoms
            .insert(product, (left.min(right), left.max(right)));
    }

    /// The atom a multiplication's operand contributes to the fold: the
    /// binding's opaque handle when the operand is a plain read of one, and
    /// nothing otherwise.
    fn affine_operand_handle(
        &mut self,
        operand: &CheckedExpression,
        state: &mut AffineFlowState,
    ) -> Option<AffineTermId> {
        let CheckedExpression::Binding { binding, ty, .. } = operand else {
            return None;
        };
        let CheckedType::Integer(integer) = *ty else {
            return None;
        };
        if self.affine_binding_type(*binding) != Some(integer) {
            return None;
        }
        self.affine_opaque_handle(*binding, state)?.unit_term()
    }

    /// Captures one unsigned division for the fixed product consequence and
    /// publishes the existing literal-divisor scaled image. Later writes get
    /// new atoms and cannot retarget either consequence.
    fn establish_unsigned_division_image(
        &mut self,
        quotient: &AffineForm,
        dividend: &AffineForm,
        divisor: &AffineForm,
        established: sources::EstablishedUnsignedDivision,
        state: &mut AffineFlowState,
    ) {
        self.unsigned_divisions.push(CapturedUnsignedDivision {
            quotient: quotient.clone(),
            dividend: dividend.clone(),
            divisor: divisor.clone(),
            parent: established.parent,
        });
        let Some(scale) = established.literal_divisor else {
            return;
        };
        let Ok(scaled_quotient) = quotient.scale(scale, &mut AffineCheckState::new()) else {
            return;
        };
        let Some(inequality) = Self::affine_less_equal(&scaled_quotient, dividend) else {
            return;
        };
        state.facts.push(ActiveAffineFact {
            inequality,
            evidence: AffineFactEvidence::Derivation(established.parent),
            active_loops: Vec::new(),
        });
    }

    /// [ENT-3.S7] A checked product of a captured unsigned quotient and
    /// divisor is no greater than the captured dividend. Matching reads
    /// immutable value images, so a source binding's later assignment cannot
    /// retarget the relation. Its own domain was proved before this transfer.
    fn establish_unsigned_division_product(
        &mut self,
        product: &AffineForm,
        value: &CheckedExpression,
        state: &mut AffineFlowState,
    ) {
        let CheckedExpression::IntegerOperation {
            carrier, arguments, ..
        } = value
        else {
            return;
        };
        let Some(&(domain, ordinal)) = self.product_operands.get(carrier) else {
            return;
        };
        let [left, right] = arguments.as_slice() else {
            return;
        };
        let (Some(left), Some(right)) = (
            self.affine_pre_domain_form(left, state),
            self.affine_pre_domain_form(right, state),
        ) else {
            return;
        };
        let Some(division) = self.unsigned_divisions.iter().find(|division| {
            (division.quotient == left && division.divisor == right)
                || (division.quotient == right && division.divisor == left)
        }) else {
            return;
        };
        let Some(inequality) = Self::affine_less_equal(product, &division.dividend) else {
            return;
        };
        let parent = self
            .derivations
            .intern(DerivationNode::UnsignedDivisionProduct {
                product: carrier.clone(),
                division: division.parent,
                domain,
            });
        self.derivations
            .add_root(DerivationRootKind::UnsignedDivisionProduct(ordinal), parent);
        state.facts.push(ActiveAffineFact {
            inequality,
            evidence: AffineFactEvidence::Derivation(parent),
            active_loops: Vec::new(),
        });
    }

    fn affine_pure_expression_form(
        &mut self,
        expression: &CheckedExpression,
        state: &mut AffineFlowState,
    ) -> Option<AffineForm> {
        if let Some(value) = self.constant_storage_scalar(expression).cloned() {
            return self.affine_pure_expression_form(&CheckedExpression::Constant(value), state);
        }
        let formed = match expression {
            CheckedExpression::ArrayMeasure { .. }
            | CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::RangeMeasure { .. }
            | CheckedExpression::RangeElementMeasure { .. }
            | CheckedExpression::ContainerMeasure { .. } => self
                .checked_measure_term(expression)
                .map(|term| self.measure_atom(term, state)),
            CheckedExpression::Constant(CheckedValue::Integer { ty, bits })
            | CheckedExpression::NamedConstant {
                value: CheckedValue::Integer { ty, bits },
                ..
            } => Some(AffineForm::constant(integer_value(*ty, *bits))),
            // [MSR-6] endpoint and initializer value flow must use the same
            // declaration-anchored image as an invariant or contract
            // spelling of the symbolic const parameter.
            CheckedExpression::Constant(CheckedValue::ConstGeneric { declaration, .. }) => {
                let term = self.const_parameter_term(*declaration);
                Some(self.measure_atom(term, state))
            }
            CheckedExpression::Binding { binding, ty, .. } => {
                let CheckedType::Integer(integer) = *ty else {
                    return None;
                };
                if self.affine_binding_type(*binding) != Some(integer) {
                    Some(self.new_affine_atom(integer))
                } else if let Some(value) = state.values.get(binding) {
                    Some(value.clone())
                } else {
                    let value = self.new_affine_atom(integer);
                    state.values.insert(*binding, value.clone());
                    Some(value)
                }
            }
            CheckedExpression::NumericConversion {
                mode: CheckedConversionMode::Exact,
                source: CheckedNumericType::Integer(_),
                destination: CheckedNumericType::Integer(_),
                value,
                ..
            } => self.affine_pure_expression_form(value, state),
            CheckedExpression::IntegerOperation {
                operation,
                arguments,
                result: CheckedType::Integer(_),
                ..
            } => {
                let [left, right] = arguments.as_slice() else {
                    return self.affine_unknown_integer(expression.ty());
                };
                let left = self.affine_pure_expression_form(left, state)?;
                let right = self.affine_pure_expression_form(right, state)?;
                let mut check = AffineCheckState::new();
                match operation {
                    CheckedIntegerOperation::AddExact | CheckedIntegerOperation::AddDefined => {
                        left.add(&right, &mut check).ok()
                    }
                    CheckedIntegerOperation::SubtractExact
                    | CheckedIntegerOperation::SubtractDefined => {
                        left.subtract(&right, &mut check).ok()
                    }
                    CheckedIntegerOperation::MultiplyExact
                    | CheckedIntegerOperation::MultiplyDefined => {
                        if left.terms().is_empty() {
                            right.scale(left.constant_value(), &mut check).ok()
                        } else if right.terms().is_empty() {
                            left.scale(right.constant_value(), &mut check).ok()
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        formed.or_else(|| self.affine_unknown_integer(expression.ty()))
    }

    fn checked_affine_form(
        &mut self,
        expression: &CheckedAffineExpression,
        state: &mut AffineFlowState,
        check: &mut AffineCheckState,
    ) -> Result<AffineForm, AffineCheckError> {
        enum Pending<'expression> {
            Visit(&'expression CheckedAffineExpression),
            Add,
            Subtract,
            Scale(i128),
        }

        let mut pending = vec![Pending::Visit(expression)];
        let mut values = Vec::new();
        while let Some(next) = pending.pop() {
            match next {
                Pending::Visit(expression) => match &expression.kind {
                    CheckedAffineExpressionKind::Constant { value, .. } => {
                        values.push(AffineForm::constant(*value));
                    }
                    CheckedAffineExpressionKind::Local { binding, .. } => {
                        let value = if let Some(value) = state.values.get(binding) {
                            value.clone()
                        } else {
                            let value = self
                                .new_affine_binding_atom(*binding)
                                .ok_or(AffineCheckError::CoefficientMismatch)?;
                            state.values.insert(*binding, value.clone());
                            value
                        };
                        values.push(value);
                    }
                    // [INV-1, MSR-2] a measure factor's image is the one this
                    // program point holds for that term. It is retargeted by
                    // exactly the events that kill the term, so a relation
                    // proved before a write says nothing after it.
                    CheckedAffineExpressionKind::Measure(measure) => {
                        let term = self
                            .checked_measure_term(measure)
                            .ok_or(AffineCheckError::CoefficientMismatch)?;
                        values.push(self.measure_atom(term, state));
                    }
                    // [INV-1, MSR-6, ENT-2] a const generic at the symbolic
                    // instance is the declaration-anchored constant term, and
                    // no [ENT-5] event kills it, so its image is one
                    // immutable atom for the whole walk.
                    CheckedAffineExpressionKind::ConstGeneric { declaration, .. } => {
                        let term = self.const_parameter_term(*declaration);
                        values.push(self.measure_atom(term, state));
                    }
                    CheckedAffineExpressionKind::Add(left, right) => {
                        pending.push(Pending::Add);
                        pending.push(Pending::Visit(right));
                        pending.push(Pending::Visit(left));
                    }
                    CheckedAffineExpressionKind::Subtract(left, right) => {
                        pending.push(Pending::Subtract);
                        pending.push(Pending::Visit(right));
                        pending.push(Pending::Visit(left));
                    }
                    CheckedAffineExpressionKind::MultiplyByConstant {
                        constant, value, ..
                    } => {
                        pending.push(Pending::Scale(*constant));
                        pending.push(Pending::Visit(value));
                    }
                },
                Pending::Add => {
                    let right = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                    let left = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                    values.push(left.add(&right, check)?);
                }
                Pending::Subtract => {
                    let right = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                    let left = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                    values.push(left.subtract(&right, check)?);
                }
                Pending::Scale(constant) => {
                    let value = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                    values.push(value.scale(constant, check)?);
                }
            }
        }
        let result = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
        if values.is_empty() {
            Ok(result)
        } else {
            Err(AffineCheckError::CoefficientMismatch)
        }
    }

    fn checked_loop_invariant_inequality(
        &mut self,
        invariant: &CheckedLoopInvariant,
        state: &mut AffineFlowState,
        check: &mut AffineCheckState,
    ) -> Option<AffineInequality> {
        self.checked_affine_relation_inequality(&invariant.relation, state, check)
            .ok()
    }

    /// [INV-1] the second member of an `==` target, `b-a <= 0` beside the
    /// `a-b <= 0` the record carries. A relation written with one of the four
    /// ordered symbols has no second member and answers `None`.
    fn checked_affine_relation_partner(
        &mut self,
        relation: &CheckedAffineRelation,
        state: &mut AffineFlowState,
        check: &mut AffineCheckState,
    ) -> Option<Result<AffineInequality, AffineCheckError>> {
        if !relation.equality {
            return None;
        }
        let left = match self.checked_affine_form(&relation.left, state, check) {
            Ok(form) => form,
            Err(error) => return Some(Err(error)),
        };
        let right = match self.checked_affine_form(&relation.right, state, check) {
            Ok(form) => form,
            Err(error) => return Some(Err(error)),
        };
        Some(AffineInequality::from_bounded_forms(
            &right, &left, 0, check,
        ))
    }

    /// The disposition of one written invariant's batch in this state
    /// [INV-1]: one inequality, or both bounds of an equality.
    fn prove_affine_relation_batch(
        &mut self,
        relation: &CheckedAffineRelation,
        state: &mut ProofFlowState,
    ) -> TargetDisposition {
        let target = self
            .checked_affine_relation_inequality(
                relation,
                &mut state.affine,
                &mut AffineCheckState::new(),
            )
            .ok();
        let partner = self
            .checked_affine_relation_partner(
                relation,
                &mut state.affine,
                &mut AffineCheckState::new(),
            )
            .map(|partner| partner.ok());
        let right = self.checked_affine_right_term(&relation.right);
        let left = self.checked_affine_right_term(&relation.left);
        let mut members = vec![(target, right, left)];
        if let Some(partner) = partner {
            members.push((partner, left, right));
        }
        self.affine_target_disposition(&members, &state.facts, &state.affine)
    }

    /// [MSR-4] the disposition of one [INV-1] target's bounds in one state:
    /// proved when every bound is, refuted when the state derives the
    /// negation of one bound, and unproved otherwise. Each member carries its
    /// bound, that bound's own right-hand term, and the opposite side's term,
    /// which is the right-hand term of the bound's negation.
    fn affine_target_disposition(
        &mut self,
        members: &[(Option<AffineInequality>, Option<TermId>, Option<TermId>)],
        facts: &FactState,
        affine: &AffineFlowState,
    ) -> TargetDisposition {
        let proved = members.iter().all(|(member, right, _)| {
            member.as_ref().is_some_and(|inequality| {
                self.prove(
                    ProofContext::new(facts, affine),
                    ProofGoal::Affine {
                        inequality,
                        right: *right,
                    },
                )
                .disposition
                    == ProofDisposition::Proved
            })
        });
        if proved {
            return TargetDisposition::Proved;
        }
        // A contradictory state proves every bound, so no negation below is
        // proved from a contradiction.
        let refuted = members.iter().any(|(member, _, opposite)| {
            member
                .as_ref()
                .and_then(|inequality| inequality.negated(&mut AffineCheckState::new()).ok())
                .is_some_and(|negation| {
                    self.prove(
                        ProofContext::new(facts, affine),
                        ProofGoal::Affine {
                            inequality: &negation,
                            right: *opposite,
                        },
                    )
                    .disposition
                        == ProofDisposition::Proved
                })
        });
        if refuted {
            TargetDisposition::Refuted
        } else {
            TargetDisposition::Unproved
        }
    }

    /// INV-1 base is a simultaneous batch: every target is checked against
    /// the same preheader state before any invariant from the batch becomes an
    /// assumption.
    fn prove_loop_invariant_bases(
        &mut self,
        invariants: &[CheckedLoopInvariant],
        state: &mut ProofFlowState,
    ) -> Vec<TargetDisposition> {
        invariants
            .iter()
            .map(|invariant| self.prove_affine_relation_batch(&invariant.relation, state))
            .collect()
    }

    /// Installs the complete invariant batch at a generic loop header only
    /// after every base judgment succeeded. No source-order prefix can lend
    /// authority to a later base case.
    fn activate_loop_invariant_batch(
        &mut self,
        loop_id: CheckedLoopId,
        invariants: &[CheckedLoopInvariant],
        base_batch: bool,
        state: &mut AffineFlowState,
    ) {
        for (source_ordinal, invariant) in invariants.iter().enumerate() {
            let target = self.checked_affine_relation_inequality(
                &invariant.relation,
                state,
                &mut AffineCheckState::new(),
            );
            let partner = self
                .checked_affine_relation_partner(
                    &invariant.relation,
                    state,
                    &mut AffineCheckState::new(),
                )
                .and_then(Result::ok);
            self.invariant_targets
                .insert(invariant.declaration, target.clone());
            if base_batch && let Ok(inequality) = target {
                state
                    .published_invariants
                    .insert(invariant.declaration, inequality.clone());
                // [INV-1] an `==` target is one batch of two bounds, and both
                // become assumptions together once the base batch succeeded.
                for inequality in std::iter::once(inequality).chain(partner) {
                    state.facts.push(ActiveAffineFact {
                        inequality,
                        evidence: AffineFactEvidence::Source(SourceAffineFactRef::LoopInvariant(
                            SourceLoopInvariantRef {
                                loop_id,
                                source_ordinal: u32::try_from(source_ordinal)
                                    .expect("loop invariant ordinal exceeds u32"),
                            },
                        )),
                        active_loops: vec![loop_id],
                    });
                }
            }
        }
    }

    fn record_loop_invariant_outcomes(
        &mut self,
        loop_id: CheckedLoopId,
        invariants: &[CheckedLoopInvariant],
        base: &[TargetDisposition],
        step: &[Option<TargetDisposition>],
        counted_binder: Option<BindingId>,
    ) {
        for (index, invariant) in invariants.iter().enumerate() {
            self.loop_invariants.push(LoopInvariantOutcome {
                node_path: invariant.relation.node_path.clone(),
                loop_id,
                source_ordinal: u32::try_from(index).expect("loop invariant ordinal exceeds u32"),
                name: invariant.name.clone(),
                base_target: self.render_checked_invariant_relation(&invariant.relation, None),
                backedge_target: self
                    .render_checked_invariant_relation(&invariant.relation, counted_binder),
                proof: LoopInvariantProof {
                    base: base[index] == TargetDisposition::Proved,
                    step: step[index].map(|step| step == TargetDisposition::Proved),
                    base_refuted: base[index] == TargetDisposition::Refuted,
                    step_refuted: step[index] == Some(TargetDisposition::Refuted),
                },
            });
        }
    }

    /// Renders one INV-1 incoming-edge target using only source spellings.
    ///
    /// The checked relation contains immutable binding identities, which are
    /// appropriate for proof but useless in a source diagnostic. For a
    /// counted backedge the only compiler-written value transition is the
    /// hidden unit update, so occurrences of that binder are rendered as the
    /// exact source expression the writer must preserve. Ordinary loops pass
    /// no binder and therefore render the header relation unchanged.
    fn render_checked_invariant_relation(
        &self,
        relation: &CheckedAffineRelation,
        counted_next_binder: Option<BindingId>,
    ) -> String {
        let left = self.render_checked_affine_expression(&relation.left, counted_next_binder);
        let right = self.render_checked_affine_expression(&relation.right, counted_next_binder);
        match relation.bound {
            0 => format!("{left} <= {right}"),
            -1 => format!("{left} < {right}"),
            // INV-1 formation currently admits only strict and non-strict
            // ordered roots. Keep a source-level fallback so an internal
            // inconsistency never leaks an affine term identity.
            bound => format!("({left} - {right}) <= {bound}_i128"),
        }
    }

    fn render_checked_affine_expression(
        &self,
        expression: &CheckedAffineExpression,
        counted_next_binder: Option<BindingId>,
    ) -> String {
        let mut values = Vec::new();
        for expression in expression.postorder() {
            let rendered = match &expression.kind {
                CheckedAffineExpressionKind::Constant { value, ty } => {
                    format!("{value}_{}", integer_type_name(*ty))
                }
                CheckedAffineExpressionKind::Local { binding, .. } => {
                    let name = self.binding_name(*binding);
                    if counted_next_binder == Some(*binding) {
                        format!("({name} + 1_u64)")
                    } else {
                        name
                    }
                }
                // [INV-1] a measure factor renders as the writer wrote it: the
                // former over the place, never an internal term identity.
                CheckedAffineExpressionKind::Measure(measure) => self
                    .render_affine_measure(measure)
                    .unwrap_or_else(|| "?".to_owned()),
                CheckedAffineExpressionKind::ConstGeneric { name, .. } => name.clone(),
                CheckedAffineExpressionKind::Add(_, _)
                | CheckedAffineExpressionKind::Subtract(_, _) => {
                    let right = values.pop().expect("postorder retains the right child");
                    let left = values.pop().expect("postorder retains the left child");
                    let operator =
                        if matches!(expression.kind, CheckedAffineExpressionKind::Add(_, _)) {
                            '+'
                        } else {
                            '-'
                        };
                    format!("({left} {operator} {right})")
                }
                CheckedAffineExpressionKind::MultiplyByConstant {
                    constant,
                    constant_ty,
                    ..
                } => {
                    let value = values.pop().expect("postorder retains the scaled child");
                    format!("({constant}_{} * {value})", integer_type_name(*constant_ty))
                }
            };
            values.push(rendered);
        }
        values.pop().expect("postorder visits the root")
    }

    /// The writer's own spelling of one [INV-1] affine measure factor.
    fn render_affine_measure(&self, expression: &CheckedExpression) -> Option<String> {
        let (measure, binding, fields) = match expression {
            CheckedExpression::ArrayMeasure {
                measure,
                root: CheckedArrayRoot::Binding { binding, fields },
                ..
            } => (*measure, *binding, fields.clone()),
            CheckedExpression::BufferMeasure { measure, root } => {
                let place =
                    self.render_place(&ResolvedPlace::from_path(root.binding, root.place_path()));
                return Some(format!("{place}.{}", measure.spelling()));
            }
            CheckedExpression::RangeMeasure { measure, root } => {
                (*measure, root.binding, Vec::new())
            }
            CheckedExpression::RangeElementMeasure { measure, place, .. } => {
                let mut resolved = ResolvedPlace::spelled(
                    PlaceRoot::Binding(place.root.binding),
                    self.is_holder(place.root.binding),
                    Vec::new(),
                );
                resolved.path.extend(place.place_path());
                return Some(format!(
                    "{}.{}",
                    self.render_place(&resolved),
                    measure.spelling()
                ));
            }
            // [MSR-1] a measured place may carry a subscript, so this one is
            // rendered from the same source-order path every other consumer
            // reads rather than from a field list.
            CheckedExpression::ContainerMeasure { measure, root } => {
                let mut path = self.container_root_path(root);
                path.path
                    .retain(|projection| !matches!(projection, PlaceStep::Deref));
                let place = self.render_place(&ResolvedPlace {
                    root: root.root,
                    path: path.path,
                });
                return Some(format!("{place}.{}", measure.spelling()));
            }
            _ => return None,
        };
        let place = self.render_place(&ResolvedPlace::spelled(
            PlaceRoot::Binding(binding),
            false,
            fields,
        ));
        Some(format!("{place}.{}", measure.spelling()))
    }

    fn checked_affine_relation_inequality(
        &mut self,
        relation: &CheckedAffineRelation,
        state: &mut AffineFlowState,
        check: &mut AffineCheckState,
    ) -> Result<AffineInequality, AffineCheckError> {
        let left = self.checked_affine_form(&relation.left, state, check)?;
        let right = self.checked_affine_form(&relation.right, state, check)?;
        AffineInequality::from_bounded_forms(&left, &right, relation.bound, check)
    }

    /// Recognizes the source right side's one L0 term plus displacement,
    /// using the existing source normalizer rather than the current value's
    /// coefficient vector. The displacement already belongs to the target
    /// inequality, so only the term is needed by Step 6.
    fn checked_affine_right_term(
        &mut self,
        expression: &CheckedAffineExpression,
    ) -> Option<TermId> {
        let source = CheckedAffineRelation {
            node_path: expression.node_path.clone(),
            left: CheckedAffineExpression {
                node_path: expression.node_path.clone(),
                kind: CheckedAffineExpressionKind::Constant {
                    value: 0,
                    ty: IntegerType::U64,
                },
            },
            right: expression.clone(),
            bound: 0,
            equality: false,
        };
        let Relation::Bound {
            left: ZERO,
            right,
            bound,
        } = self.checked_affine_relation_l0(&source)?
        else {
            return None;
        };
        if right == ZERO {
            Some(self.terms.intern(TermKind::Constant(bound)))
        } else {
            Some(right)
        }
    }

    /// Projects the exact source relation into L0 when its normalized binding
    /// coefficients have one of the fixed difference-bound shapes. This does
    /// no discovery: it only recognizes `x - y <= c`, `x <= c`, `c <= x`, or
    /// a constant proposition after the source-written affine arithmetic has
    /// been normalized.
    fn checked_affine_relation_l0(&mut self, relation: &CheckedAffineRelation) -> Option<Relation> {
        /// One leaf of the written relation, in the order the walk reaches it.
        enum SourceLeaf {
            Local(BindingId),
            /// [INV-1] one measure factor, already interned as its [ENT-2]
            /// term by the pre-pass below.
            Measure(TermId),
        }

        fn source_form(
            expression: &CheckedAffineExpression,
            leaves: &mut Vec<SourceLeaf>,
            measures: &[TermId],
            visited: &mut usize,
            check: &mut AffineCheckState,
        ) -> Option<AffineForm> {
            let mut values: Vec<AffineForm> = Vec::new();
            for expression in expression.postorder() {
                let value = match &expression.kind {
                    CheckedAffineExpressionKind::Constant { value, .. } => {
                        AffineForm::constant(*value)
                    }
                    CheckedAffineExpressionKind::Local { binding, .. } => {
                        let index = leaves
                            .iter()
                            .position(|candidate| {
                                matches!(candidate, SourceLeaf::Local(other) if other == binding)
                            })
                            .unwrap_or_else(|| {
                                leaves.push(SourceLeaf::Local(*binding));
                                leaves.len() - 1
                            });
                        let index = u32::try_from(index).ok()?;
                        AffineForm::term(AffineTermId::from_index(index))
                    }
                    CheckedAffineExpressionKind::Measure(_)
                    | CheckedAffineExpressionKind::ConstGeneric { .. } => {
                        let term = *measures.get(*visited)?;
                        *visited = visited.checked_add(1)?;
                        let index = leaves
                            .iter()
                            .position(|candidate| {
                                matches!(candidate, SourceLeaf::Measure(other) if *other == term)
                            })
                            .unwrap_or_else(|| {
                                leaves.push(SourceLeaf::Measure(term));
                                leaves.len() - 1
                            });
                        let index = u32::try_from(index).ok()?;
                        AffineForm::term(AffineTermId::from_index(index))
                    }
                    CheckedAffineExpressionKind::Add(_, _) => {
                        let right = values.pop()?;
                        let left = values.pop()?;
                        left.add(&right, check).ok()?
                    }
                    CheckedAffineExpressionKind::Subtract(_, _) => {
                        let right = values.pop()?;
                        let left = values.pop()?;
                        left.subtract(&right, check).ok()?
                    }
                    CheckedAffineExpressionKind::MultiplyByConstant { constant, .. } => {
                        values.pop()?.scale(*constant, check).ok()?
                    }
                };
                values.push(value);
            }
            values.pop()
        }

        // Interning needs `&mut self`, and the walk above does not have it, so
        // the measure terms are resolved first in exactly the order that walk
        // reaches them.
        let mut measures = Vec::new();
        self.collect_affine_measure_terms(&relation.left, &mut measures)?;
        self.collect_affine_measure_terms(&relation.right, &mut measures)?;
        let mut leaves = Vec::new();
        let mut visited = 0;
        let mut check = AffineCheckState::new();
        let left = source_form(
            &relation.left,
            &mut leaves,
            &measures,
            &mut visited,
            &mut check,
        )?;
        let right = source_form(
            &relation.right,
            &mut leaves,
            &measures,
            &mut visited,
            &mut check,
        )?;
        let inequality =
            AffineInequality::from_bounded_forms(&left, &right, relation.bound, &mut check).ok()?;
        let mut term = |coefficient: super::affine::AffineCoefficient| match leaves
            .get(coefficient.term().index() as usize)?
        {
            SourceLeaf::Measure(term) => Some(*term),
            SourceLeaf::Local(binding) => {
                let binding = *binding;
                let fragment =
                    fragment_type(CheckedType::Integer(self.affine_binding_type(binding)?))?;
                Some(self.terms.intern(TermKind::Place(
                    ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, Vec::new()),
                    fragment,
                )))
            }
        };
        let (left, right) = match inequality.terms() {
            [] => (ZERO, ZERO),
            [coefficient] if coefficient.coefficient() == 1 => (term(*coefficient)?, ZERO),
            [coefficient] if coefficient.coefficient() == -1 => (ZERO, term(*coefficient)?),
            [first, second] => match (first.coefficient(), second.coefficient()) {
                (1, -1) => (term(*first)?, term(*second)?),
                (-1, 1) => (term(*second)?, term(*first)?),
                _ => return None,
            },
            _ => return None,
        };
        Some(Relation::Bound {
            left,
            right,
            bound: inequality.upper(),
        })
    }

    fn source_proof_formation_failure(error: AffineCheckError) -> SourceProofCertificateFailure {
        match error {
            AffineCheckError::ArithmeticOverflow => {
                SourceProofCertificateFailure::ArithmeticOverflow
            }
            AffineCheckError::LimitExceeded(_) => SourceProofCertificateFailure::FormationCapacity,
            AffineCheckError::CoefficientMismatch | AffineCheckError::InvalidCertificateFactor => {
                unreachable!("a checked affine source has inconsistent internal structure")
            }
        }
    }

    fn source_proof_premise_results(
        &mut self,
        premises: &[Option<AffineInequality>],
        named_premises: &[bool],
        published_premises: &[bool],
        values: &AffineFlowState,
        facts: &FactState,
    ) -> Vec<bool> {
        let mut closed: Option<ProofClosure> = None;
        premises
            .iter()
            .zip(named_premises)
            .zip(published_premises)
            .map(|((premise, named), published)| {
                // A bare invariant name means that exact declaration's
                // published theorem, not merely any proposition with the same
                // normalized inequality. Only a relation-form use asks AUTO
                // to prove its written source from the entering context.
                if *named {
                    return *published;
                }
                let Some(premise) = premise.as_ref() else {
                    return false;
                };
                let goal = ProofGoal::AutomaticAffine {
                    inequality: premise,
                };
                // Each relation source reads these same immutable facts. A
                // newly registered term or goal changes the closure universe,
                // so only the unchanged view is reused; no proof is memoized.
                if closed
                    .as_ref()
                    .is_none_or(|view| !view.matches(&self.terms, &self.goals))
                {
                    closed = Some(ProofClosure::new(
                        facts,
                        &self.terms,
                        &self.goals,
                        &mut self.derivations,
                    ));
                }
                self.prove(
                    ProofContext {
                        facts,
                        affine: values,
                        closed: closed.as_ref(),
                    },
                    goal,
                )
                .disposition
                    == ProofDisposition::Proved
            })
            .collect()
    }

    /// Forms the one weighted premise sum the source writer selected.
    ///
    /// The written premises are multiplied and summed exactly in source order.
    /// This phase depends only on the formed source propositions and written
    /// factors. It deliberately runs before premise availability is judged.
    fn source_proof_sum(
        &self,
        premises: &[(AffineInequality, CertificateMultiplicity)],
    ) -> Result<CertificateSum, (SourceProofCertificateFailure, u32)> {
        let actual = u32::try_from(premises.len()).unwrap_or(u32::MAX);
        if premises.len() > MAX_CERTIFICATE_PREMISES {
            let maximum =
                u32::try_from(MAX_CERTIFICATE_PREMISES).expect("certificate capacity fits u32");
            return Err((
                SourceProofCertificateFailure::UseCapacity { maximum, actual },
                maximum,
            ));
        }

        let mut first_by_premise = HashMap::new();
        for (index, (premise, multiplicity)) in premises.iter().enumerate() {
            let index = u32::try_from(index).expect("certificate capacity fits u32");
            // A term multiplicity is unsigned by [PRF-1], so only the written
            // decimal can be degenerate. A runtime zero drops its premise and
            // the sum stays sound, which is why nothing rejects it here.
            if matches!(multiplicity, CertificateMultiplicity::Literal(factor) if *factor <= 0) {
                return Err((
                    SourceProofCertificateFailure::InvalidFactor { use_index: index },
                    index,
                ));
            }
            if let Some(first) = first_by_premise.insert(premise.clone(), index) {
                return Err((
                    SourceProofCertificateFailure::RepeatedUse {
                        first,
                        repeated: index,
                    },
                    index,
                ));
            }
        }

        // Build the written sum one source entry at a time. Besides preserving
        // source order, this records the exact entry whose scale or addition
        // first exceeds the proof arithmetic or affine formation domain.
        //
        // The accumulator starts affine and becomes a degree-two polynomial at
        // the first term multiplicity, if there is one; a certificate written
        // entirely with bare decimals therefore never leaves the affine arm
        // and forms exactly the inequality it always did.
        let mut sum = CertificateSum::Empty;
        for (index, (inequality, multiplicity)) in premises.iter().enumerate() {
            let index = u32::try_from(index).expect("certificate capacity fits u32");
            sum =
                Self::extend_certificate_sum(sum, inequality, multiplicity).map_err(|failure| {
                    (
                        Self::certificate_step_failure(failure, index, actual),
                        index,
                    )
                })?;
        }
        match sum {
            CertificateSum::Empty => Err((SourceProofCertificateFailure::FormationCapacity, 0)),
            formed => Ok(formed),
        }
    }

    /// Adds one written entry to the accumulated certificate sum.
    fn extend_certificate_sum(
        sum: CertificateSum,
        inequality: &AffineInequality,
        multiplicity: &CertificateMultiplicity,
    ) -> Result<CertificateSum, CertificateStepFailure> {
        if let CertificateMultiplicity::Literal(factor) = *multiplicity {
            match sum {
                CertificateSum::Empty => {
                    let mut check = AffineCheckState::new();
                    return Ok(CertificateSum::Affine(sum_explicit_scaled_inequalities(
                        &[ScaledAffinePremise { inequality, factor }],
                        &mut check,
                    )?));
                }
                CertificateSum::Affine(previous) => {
                    let mut check = AffineCheckState::new();
                    return Ok(CertificateSum::Affine(sum_explicit_scaled_inequalities(
                        &[
                            ScaledAffinePremise {
                                inequality: &previous,
                                factor: 1,
                            },
                            ScaledAffinePremise { inequality, factor },
                        ],
                        &mut check,
                    )?));
                }
                CertificateSum::Nonlinear(previous) => {
                    let scaled =
                        CertificatePolynomial::from_inequality(inequality)?.scale(factor)?;
                    return Ok(CertificateSum::Nonlinear(previous.add(&scaled)?));
                }
            }
        }
        let CertificateMultiplicity::Value(value) = multiplicity else {
            unreachable!("the literal arm returned above");
        };
        let scaled = CertificatePolynomial::from_inequality(inequality)?
            .multiply(&CertificatePolynomial::from_form(value)?)?;
        let previous = match sum {
            CertificateSum::Empty => CertificatePolynomial::zero(),
            CertificateSum::Affine(previous) => CertificatePolynomial::from_inequality(&previous)?,
            CertificateSum::Nonlinear(previous) => previous,
        };
        Ok(CertificateSum::Nonlinear(previous.add(&scaled)?))
    }

    fn certificate_step_failure(
        failure: CertificateStepFailure,
        index: u32,
        actual: u32,
    ) -> SourceProofCertificateFailure {
        match failure {
            CertificateStepFailure::Overflow => SourceProofCertificateFailure::ArithmeticOverflow,
            CertificateStepFailure::UseCapacity => SourceProofCertificateFailure::UseCapacity {
                maximum: u32::try_from(MAX_CERTIFICATE_PREMISES)
                    .expect("certificate capacity fits u32"),
                actual,
            },
            CertificateStepFailure::Formation => SourceProofCertificateFailure::FormationCapacity,
            CertificateStepFailure::InvalidFactor => {
                SourceProofCertificateFailure::InvalidFactor { use_index: index }
            }
        }
    }

    /// Resolves one written multiplicity where the certificate is checked.
    ///
    /// A named multiplicity reads the value image its binding holds in the
    /// entering context, minting the atom if this is the first read of it, so
    /// the scaling step is over the same immutable value identity every other
    /// affine premise names.
    fn certificate_multiplicity(
        &mut self,
        multiplicity: CheckedProofMultiplicity,
        state: &mut AffineFlowState,
    ) -> Option<CertificateMultiplicity> {
        match multiplicity {
            CheckedProofMultiplicity::Literal(factor) => {
                Some(CertificateMultiplicity::Literal(factor))
            }
            CheckedProofMultiplicity::Value { binding, .. } => Some(
                CertificateMultiplicity::Value(self.affine_opaque_handle(binding, state)?),
            ),
        }
    }

    /// Brings the accumulated certificate sum back to one affine inequality
    /// and checks the writer-selected residual against it.
    ///
    /// A nonlinear accumulation folds first: each degree-two monomial must be
    /// the value image of an admitted exact product, which is the only way a
    /// term-scaled premise can meet an affine target. Once folded, the residual
    /// is the same one a bare-decimal certificate reaches, proved by the same
    /// route; a monomial with no such product is a refusal, not a weaker check.
    fn source_proof_certificate_residual(
        &mut self,
        target: &AffineInequality,
        sum: &CertificateSum,
        values: &AffineFlowState,
        facts: &FactState,
    ) -> Result<bool, SourceProofCertificateFailure> {
        let folded;
        let sum = match sum {
            CertificateSum::Empty => {
                return Err(SourceProofCertificateFailure::FormationCapacity);
            }
            CertificateSum::Affine(sum) => sum,
            CertificateSum::Nonlinear(polynomial) => {
                folded = self.folded_certificate_sum(polynomial, target)?;
                &folded
            }
        };
        self.source_proof_residual(target, sum, values, facts)
    }

    /// Folds a nonlinear certificate sum to the affine inequality it equals.
    fn folded_certificate_sum(
        &self,
        polynomial: &CertificatePolynomial,
        target: &AffineInequality,
    ) -> Result<AffineInequality, SourceProofCertificateFailure> {
        // Several bindings can hold the same product, and they are equal
        // values, so any of them folds soundly. The target's own text picks
        // among them: a monomial folded to the value the target already names
        // cancels against it, while one folded to an equal value under
        // another name does not. Failing that, the least atom is a canonical
        // choice. Neither is a search — one pass, one winner per operand pair.
        let named_by_target = target
            .terms()
            .iter()
            .map(|coefficient| coefficient.term())
            .collect::<HashSet<_>>();
        let mut products = std::collections::BTreeMap::new();
        for (product, operands) in &self.product_atoms {
            products
                .entry(*operands)
                .and_modify(|chosen: &mut AffineTermId| {
                    let better = match (
                        named_by_target.contains(chosen),
                        named_by_target.contains(product),
                    ) {
                        (false, true) => true,
                        (true, false) => false,
                        _ => *product < *chosen,
                    };
                    if better {
                        *chosen = *product;
                    }
                })
                .or_insert(*product);
        }
        let folded = polynomial
            .fold_products(&products)
            .map_err(Self::certificate_fold_failure)?;
        let mut images = std::collections::BTreeMap::new();
        for (handle, image) in &self.handle_images {
            let mut weights = image
                .terms()
                .iter()
                .map(|coefficient| (Some(coefficient.term()), coefficient.coefficient()))
                .collect::<Vec<_>>();
            weights.push((None, image.constant_value()));
            images.insert(*handle, weights);
        }
        let folded = folded
            .unfold_handles(&images)
            .map_err(Self::certificate_fold_failure)?;
        let mut check = AffineCheckState::new();
        match folded.into_inequality(&mut check) {
            Some(formed) => formed.map_err(Self::certificate_fold_failure),
            None => Err(SourceProofCertificateFailure::NonlinearResidual),
        }
    }

    fn certificate_fold_failure(error: PolynomialError) -> SourceProofCertificateFailure {
        match error {
            PolynomialError::ArithmeticOverflow => {
                SourceProofCertificateFailure::ArithmeticOverflow
            }
            PolynomialError::DegreeExceeded | PolynomialError::LimitExceeded => {
                SourceProofCertificateFailure::FormationCapacity
            }
        }
    }

    /// Checks the final writer-selected residual after every source proposition
    /// and its scaled sum have formed.
    ///
    /// `target - sum` may be discharged only by the existing direct L0 closure
    /// or fixed interval rule at the entering program point, applied to the
    /// written sum and then to its integer tightenings. This route never
    /// selects another affine premise, derives a multiplier, or retries a
    /// subset.
    fn source_proof_residual(
        &mut self,
        target: &AffineInequality,
        sum: &AffineInequality,
        values: &AffineFlowState,
        facts: &FactState,
    ) -> Result<bool, SourceProofCertificateFailure> {
        let mut check = AffineCheckState::new();
        // The untightened residual forms first so an arithmetic or capacity
        // failure of the written sum keeps its exact PRF-1 diagnostic.
        match AffineInequality::residual_after(target, sum, &mut check) {
            Ok(_) => {}
            Err(AffineCheckError::ArithmeticOverflow) => {
                return Err(SourceProofCertificateFailure::ArithmeticOverflow);
            }
            Err(AffineCheckError::LimitExceeded(_)) => {
                return Err(SourceProofCertificateFailure::FormationCapacity);
            }
            Err(
                AffineCheckError::CoefficientMismatch | AffineCheckError::InvalidCertificateFactor,
            ) => return Ok(false),
        }
        let candidates = self.affine_l0_candidates(values);
        let closed = close(facts, &self.terms, &self.goals, &mut self.derivations);
        let l0 = self.affine_l0_index(&candidates, &closed, &mut check);
        let mut query = AffineDirectQuery::new(&l0, values, &closed);
        Ok(self
            .affine_candidate_residual_proof(target, sum, &mut query, &mut check)
            .is_some())
    }

    /// Exact maximum of one affine left-hand side under the independently
    /// known L0/type interval of each atom. This is numeric discovery only;
    /// callers must subsequently prove any selected endpoint with
    /// `affine_target_proof` before it can discharge a source obligation.
    fn affine_lhs_maximum(
        &mut self,
        inequality: &AffineInequality,
        values: &AffineFlowState,
        facts: &FactState,
        check: &mut AffineCheckState,
    ) -> Result<Option<i128>, AffineCheckError> {
        let mut requested = inequality
            .terms()
            .iter()
            .map(|coefficient| coefficient.term())
            .collect::<Vec<_>>();
        requested.sort_unstable();
        requested.dedup();

        let measure_terms_by_atom = self.measure_terms_by_atom(values);
        let mut term_intervals = HashMap::new();
        for atom_id in requested {
            let atom = *self
                .affine_atoms
                .get(atom_id.index() as usize)
                .ok_or(AffineCheckError::CoefficientMismatch)?;
            let (minimum, maximum) = (atom.minimum, atom.maximum);
            let mut bindings = values
                .values
                .iter()
                .filter_map(|(binding, value)| {
                    (value.unit_term() == Some(atom_id)).then_some(*binding)
                })
                .collect::<Vec<_>>();
            bindings.sort_by_key(|binding| binding.0);
            let mut terms = bindings
                .into_iter()
                .filter_map(|binding| {
                    if self.affine_binding_type(binding) != Some(atom.ty) {
                        return None;
                    }
                    Some(self.terms.intern(TermKind::Place(
                        ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, Vec::new()),
                        atom.ty,
                    )))
                })
                .collect::<Vec<_>>();
            if let Some(measures) = measure_terms_by_atom.get(&atom_id) {
                terms.extend(measures.iter().copied());
            }
            term_intervals.insert(atom_id, (minimum, maximum, terms));
        }

        let closed = close(facts, &self.terms, &self.goals, &mut self.derivations);
        if closed.contradictory() {
            return Ok(None);
        }
        let intervals = term_intervals
            .into_iter()
            .map(|(atom, (mut minimum, mut maximum, terms))| {
                for term in terms {
                    if let Some(upper) = closed.tight_bound(term, ZERO) {
                        maximum = maximum.min(upper);
                    }
                    if let Some(negative_lower) = closed.tight_bound(ZERO, term)
                        && let Some(lower) = negative_lower.checked_neg()
                    {
                        minimum = minimum.max(lower);
                    }
                }
                (atom, (minimum, maximum))
            })
            .collect::<HashMap<_, _>>();
        interval_maximum(
            inequality.terms(),
            |term| intervals.get(&term).copied(),
            check,
        )
    }

    /// Builds the fixed L0 vocabulary before closure. Each live integer
    /// binding contributes its ordinary term and its exact current affine
    /// value; Z is the fixed zero candidate. Later matching never invents a
    /// term after the closed state was formed.
    /// The image of one measure term [MSR-4].
    ///
    /// A measure whose table cell [MSR-1] fixes its value is a standing fact
    /// [MSR-2], and its image is that fact rather than a free atom: a cell
    /// with a constant value has that constant, and a cell the table equates
    /// to another term shares that term's image. Every other measure gets one
    /// compiler-owned immutable atom, minted on first use and stable for the
    /// rest of the function walk.
    /// The binding one measure term's place is rooted in, where it has one.
    fn measure_term_root(&self, term: TermId) -> Option<BindingId> {
        let root = match self.terms.kind(term) {
            TermKind::Measure(_, place) => place.root,
            _ => return None,
        };
        match root {
            PlaceRoot::Binding(binding) => Some(binding),
            PlaceRoot::Constant(_) => None,
        }
    }

    /// Every [INV-1] measure factor of one written affine expression, in the
    /// order a left-to-right walk reaches it, interned as its [ENT-2] term.
    fn collect_affine_measure_terms(
        &mut self,
        expression: &CheckedAffineExpression,
        out: &mut Vec<TermId>,
    ) -> Option<()> {
        for expression in expression.postorder() {
            match &expression.kind {
                CheckedAffineExpressionKind::Constant { .. }
                | CheckedAffineExpressionKind::Local { .. }
                | CheckedAffineExpressionKind::Add(_, _)
                | CheckedAffineExpressionKind::Subtract(_, _)
                | CheckedAffineExpressionKind::MultiplyByConstant { .. } => {}
                CheckedAffineExpressionKind::Measure(measure) => {
                    out.push(self.checked_measure_term(measure)?);
                }
                CheckedAffineExpressionKind::ConstGeneric { declaration, .. } => {
                    out.push(self.const_parameter_term(*declaration));
                }
            }
        }
        Some(())
    }

    /// The [ENT-2] measure term one [INV-1] affine measure factor names.
    fn checked_measure_term(&mut self, expression: &CheckedExpression) -> Option<TermId> {
        let goal = self.goal_expression(expression, false)?;
        self.goal_operand(&goal)
    }

    /// The image this program point holds for one measure or const-generic term.
    ///
    /// [MSR-4]'s automatic derivation reads the current edge's immutable
    /// value image. A written invariant captures that image in its theorem;
    /// a later kill removes only the affected edge's current mapping, so a
    /// new image cannot reuse that theorem without a surviving relation.
    /// A measure the table fixes reads as its standing [MSR-2] fact.
    fn measure_atom(&mut self, term: TermId, state: &AffineFlowState) -> AffineForm {
        let mut anchor = term;
        // The table relates a cell to a constant or to one other term, and
        // this version's rows chain at most once. The bound keeps a future
        // row from looping.
        for _ in 0..4 {
            match self.terms.measure_bound(anchor) {
                Some(MeasureBound::Constant(value)) => return AffineForm::constant(value),
                Some(MeasureBound::Equal(other)) => anchor = other,
                None => break,
            }
        }
        if let Some(atom) = state.measure_atoms.borrow().get(&anchor) {
            return atom.clone();
        }
        // [REF-4, MSR-1] an anonymous range actual has no binding on which
        // S6 can install its length image. Its measure term retains the exact
        // range step, whose original capture occurrence selects the image
        // published when the formation evaluated its endpoints.
        let captured = match self.terms.kind(anchor) {
            TermKind::Measure(CheckedMeasure::Length, place) => {
                place.path.last().and_then(|step| match step {
                    PlaceStep::Range(range) => Some(*range),
                    _ => None,
                })
            }
            _ => None,
        };
        if let Some(atom) =
            captured.and_then(|captured| Self::captured_range_length_image(captured, state))
        {
            state
                .measure_atoms
                .borrow_mut()
                .insert(anchor, atom.clone());
            return atom;
        }
        // [MSR-6] a symbolic const parameter has its declaration's integer
        // type; measures and their immutable datums instead have type u64.
        // Sharing the image path cannot give a signed parameter the unsigned
        // nonnegativity bound or widen a narrower parameter's range.
        let ty = match self.terms.kind(anchor) {
            TermKind::ConstParameter(_, ty) => *ty,
            _ => IntegerType::U64,
        };
        let atom = self.new_affine_atom(ty);
        state
            .measure_atoms
            .borrow_mut()
            .insert(anchor, atom.clone());
        atom
    }

    /// [MSR-3] one measure datum inherits the atom the term it is established
    /// equal to holds at that point.
    ///
    /// The datum denotes that value, so it is that value in the affine domain
    /// too. Because nothing kills a datum, its atom outlives the write that
    /// retargets the term's: a header conclusion published over the old atom
    /// stays anchored to a live term, which is what lets one published
    /// relation preserve an invariant across a [SET-1] commit.
    fn adopt_measure_atom(&mut self, datum: TermId, live: TermId, state: &AffineFlowState) {
        if state.measure_atoms.borrow().contains_key(&datum) {
            return;
        }
        let atom = self.measure_atom(live, state);
        // A constant image is still the exact immutable value this datum
        // captures. Retaining it is what carries an affine exact measure
        // across the transfer that kills the live place.
        state.measure_atoms.borrow_mut().insert(datum, atom);
    }

    /// Every registered measure term, in term order.
    ///
    /// The registry only grows during the forward walk, so this scans just
    /// the terms interned since the last call and keeps the answer. Every
    /// numeric goal queries it, and rescanning the whole registry per query
    /// made that quadratic in the size of the function.
    fn measure_terms(&mut self) -> Vec<TermId> {
        let registered = self.terms.ids().count();
        for index in self.measure_terms_scanned..registered {
            let id = TermId(
                u32::try_from(index).expect("ENT term inventory exceeds the u32 identity space"),
            );
            // [MSR-3] a measure datum is a measure of the affine domain's
            // kind: it denotes one measure's value at a point, it is of
            // fragment type u64, and nothing kills it. It participates in
            // step 6's bridge exactly as a live measure term does, which is
            // what carries a header conclusion across the write that kills
            // the term the conclusion was published over.
            if matches!(
                self.terms.kind(id),
                TermKind::Measure(..)
                    | TermKind::CallDatum {
                        measure: Some(_),
                        ..
                    }
                    | TermKind::EntryDatum { .. }
                    | TermKind::MeasureDatum { .. }
            ) {
                self.measure_terms_seen.push(id);
            }
        }
        self.measure_terms_scanned = registered;
        self.measure_terms_seen.clone()
    }

    /// Every live measure term, grouped by the affine atom it images.
    ///
    /// [MSR-4]'s interval step starts from each atom's *direct closed L0*
    /// interval, so it has to be able to name the term whose value the atom
    /// stands for. A local's atom is named through the binding that denotes
    /// it; a measure atom's own name is its measure term [MSR-2], and without
    /// this map a measure entered every interval substitution at its complete
    /// `u64` range however tightly the closed state had already bounded it.
    fn measure_terms_by_atom(
        &mut self,
        state: &AffineFlowState,
    ) -> WordHashMap<AffineTermId, Vec<TermId>> {
        let mut grouped: WordHashMap<AffineTermId, Vec<TermId>> = WordHashMap::default();
        for term in self.measure_terms() {
            if let Some(atom) = self.measure_atom(term, state).unit_term() {
                grouped.entry(atom).or_default().push(term);
            }
        }
        for terms in grouped.values_mut() {
            terms.sort_unstable_by_key(|term| term.0);
            terms.dedup();
        }
        grouped
    }

    fn affine_l0_candidates(&mut self, values: &AffineFlowState) -> Vec<AffineL0Candidate> {
        let mut candidates = vec![AffineL0Candidate {
            term: ZERO,
            value: AffineForm::constant(0),
        }];
        // [MSR-4] step 6 ranges over every live measure term as well as every
        // own integer binding with an image, so a measure participates in the
        // affine domain through its own atom.
        for term in self.measure_terms() {
            let value = self.measure_atom(term, values);
            // A measure whose image is a constant is Z displaced by that
            // constant, and Z is already the fixed zero candidate, so its
            // index entries would duplicate Z's under one coefficient vector.
            if value.terms().is_empty() {
                continue;
            }
            candidates.push(AffineL0Candidate { term, value });
        }
        let mut bindings = values.values.keys().copied().collect::<Vec<_>>();
        bindings.sort_by_key(|binding| binding.0);
        for binding in bindings {
            let Some(ty) = self.affine_binding_type(binding) else {
                continue;
            };
            let term = self.terms.intern(TermKind::Place(
                ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, Vec::new()),
                ty,
            ));
            candidates.push(AffineL0Candidate {
                term,
                value: values.values[&binding].clone(),
            });
        }
        candidates
    }

    /// Builds the goal-query index for ordinary difference bounds.
    ///
    /// This is an ephemeral view over the already-closed L0 state, not a copy
    /// of `FactState::bounds` in the affine premise set. For each canonical
    /// affine coefficient vector it retains the strongest live L0 image. A
    /// target or residual can therefore query exactly its own vector without
    /// making every L0 edge participate in affine premise enumeration.
    fn affine_l0_index(
        &self,
        candidates: &[AffineL0Candidate],
        closed: &ClosedState,
        check: &mut AffineCheckState,
    ) -> AffineL0Index {
        let mut index = AffineL0Index::default();
        for left in candidates {
            for right in candidates {
                let Some(bound) = closed.tight_bound(left.term, right.term) else {
                    continue;
                };
                let Ok(inequality) =
                    AffineInequality::from_bounded_forms(&left.value, &right.value, bound, check)
                else {
                    // This L0 image is outside the affine i128 vocabulary.
                    // It cannot suppress another representable image.
                    continue;
                };
                let key: Box<[AffineCoefficient]> = inequality.terms().into();
                if let Some(existing) = index.by_terms.get(&key).copied() {
                    if inequality.upper() < index.entries[existing].inequality.upper() {
                        index.entries[existing] = AffineL0Entry {
                            inequality,
                            left: left.term,
                            right: right.term,
                            bound,
                        };
                    }
                    continue;
                }
                let entry = index.entries.len();
                index.by_terms.insert(key, entry);
                index.entries.push(AffineL0Entry {
                    inequality,
                    left: left.term,
                    right: right.term,
                    bound,
                });
            }
        }
        index
    }

    /// Collects only explicit source-affine facts and automatic value images.
    /// Ordinary difference bounds remain in L0 and are queried through
    /// [`Self::affine_l0_index`] for the concrete target or residual.
    /// x1 retires the capacity identity. [MSR-2] used to make
    /// `P.len + P.room = P.cap` a standing fact of every window and [ENT-6]
    /// appended it here as two inequalities over the place's three measure
    /// atoms. The `room` measure is gone, so the identity has no third term
    /// to relate and the fact system carries only the orderings
    /// `Z <= P.len`, `Z <= P.head`, `P.len <= P.cap` and `P.head <= P.cap`
    /// that [`Self::measure_term`] and the implicit bounds publish. Nothing
    /// else was appended by that route, so the sequence now starts empty.
    fn automatic_affine_premises(
        &mut self,
        facts: &[ActiveAffineFact],
        check: &mut AffineCheckState,
    ) -> Result<Vec<AutomaticAffinePremise>, AffineCheckError> {
        let mut premises = Vec::new();
        for fact in Self::canonical_affine_facts(facts) {
            check.charge(1)?;
            let (source, parent) = match fact.evidence {
                AffineFactEvidence::Source(source) => (Some(source), None),
                AffineFactEvidence::Derivation(parent) => (None, Some(parent)),
            };
            premises.push(AutomaticAffinePremise {
                inequality: fact.inequality.clone(),
                source,
                parent,
            });
        }
        Ok(premises)
    }

    fn affine_consequence_from_residual(
        selected: &[(usize, i128)],
        automatic: &[AutomaticAffinePremise],
        mut parents: Vec<DerivationId>,
    ) -> AffineConsequenceProof {
        let mut premises = Vec::new();
        for &(index, factor) in selected {
            let premise = &automatic[index];
            if let Some(source) = premise.source {
                premises.push(AffinePremiseUse { source, factor });
            }
            if let Some(parent) = premise.parent {
                parents.push(parent);
            }
        }
        parents.sort_unstable_by_key(|parent| parent.0);
        parents.dedup();
        AffineConsequenceProof { premises, parents }
    }

    /// Queries the strongest closed L0 image with exactly this affine vector.
    fn affine_l0_proof(
        &mut self,
        inequality: &AffineInequality,
        index: &AffineL0Index,
        closed: &ClosedState,
    ) -> Result<Option<Vec<DerivationId>>, AffineCheckError> {
        let Some(entry) = index.entry(inequality.terms()) else {
            return Ok(None);
        };
        if entry.inequality.upper() > inequality.upper() {
            return Ok(None);
        }
        let parent = closed
            .bound_proof(entry.left, entry.right, entry.bound, &mut self.derivations)
            .ok_or(AffineCheckError::CoefficientMismatch)?;
        Ok(Some(vec![parent]))
    }

    fn affine_interval_proof(
        &mut self,
        inequality: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Result<Option<Vec<DerivationId>>, AffineCheckError> {
        let mut requested = inequality
            .terms()
            .iter()
            .map(|coefficient| coefficient.term())
            .collect::<Vec<_>>();
        requested.sort_unstable();
        requested.dedup();

        if query.measures.is_none() {
            query.measures = Some(self.measure_terms_by_atom(query.values));
        }
        let measures = query
            .measures
            .as_ref()
            .expect("measure index prepared above");
        for atom_id in requested {
            if query.intervals.contains_key(&atom_id) {
                continue;
            }
            let atom = *self
                .affine_atoms
                .get(atom_id.index() as usize)
                .ok_or(AffineCheckError::CoefficientMismatch)?;
            let mut interval = AffineAtomInterval {
                minimum: atom.minimum,
                maximum: atom.maximum,
                minimum_parent: None,
                maximum_parent: None,
            };
            let mut bindings = query
                .values
                .values
                .iter()
                .filter_map(|(binding, value)| {
                    (value.unit_term() == Some(atom_id)).then_some(*binding)
                })
                .collect::<Vec<_>>();
            bindings.sort_by_key(|binding| binding.0);
            let mut terms = bindings
                .into_iter()
                .filter_map(|binding| {
                    if self.affine_binding_type(binding) != Some(atom.ty) {
                        return None;
                    }
                    Some(self.terms.intern(TermKind::Place(
                        ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, Vec::new()),
                        atom.ty,
                    )))
                })
                .collect::<Vec<_>>();
            if let Some(measures) = measures.get(&atom_id) {
                terms.extend(measures.iter().copied());
            }
            for term in terms {
                if let Some(upper) = query.closed.tight_bound(term, ZERO)
                    && upper < interval.maximum
                {
                    interval.maximum = upper;
                    interval.maximum_parent = Some((term, ZERO, upper));
                }
                if let Some(negative_lower) = query.closed.tight_bound(ZERO, term)
                    && let Some(lower) = negative_lower.checked_neg()
                    && lower > interval.minimum
                {
                    interval.minimum = lower;
                    interval.minimum_parent = Some((ZERO, term, negative_lower));
                }
            }
            query.intervals.insert(atom_id, interval);
        }

        if query.closed.contradictory() {
            return Ok(query.closed.contradiction_proof().map(|proof| vec![proof]));
        }
        let proved = interval_proves(
            inequality,
            |term| {
                query
                    .intervals
                    .get(&term)
                    .map(|interval| (interval.minimum, interval.maximum))
            },
            check,
        )?;
        if !proved {
            return Ok(None);
        }
        let mut parents = Vec::new();
        for coefficient in inequality.terms() {
            let interval = query
                .intervals
                .get(&coefficient.term())
                .ok_or(AffineCheckError::CoefficientMismatch)?;
            let selected = if coefficient.coefficient() > 0 {
                interval.maximum_parent
            } else {
                interval.minimum_parent
            };
            if let Some((left, right, bound)) = selected {
                let parent = query
                    .closed
                    .bound_proof(left, right, bound, &mut self.derivations)
                    .ok_or(AffineCheckError::CoefficientMismatch)?;
                parents.push(parent);
            }
        }
        parents.sort_unstable_by_key(|parent| parent.0);
        parents.dedup();
        Ok(Some(parents))
    }

    fn affine_residual_proof(
        &mut self,
        inequality: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Result<Option<Vec<DerivationId>>, AffineCheckError> {
        if query.closed.contradictory() {
            return Ok(query.closed.contradiction_proof().map(|proof| vec![proof]));
        }
        if let Some(parents) = self.affine_l0_proof(inequality, query.l0, query.closed)? {
            return Ok(Some(parents));
        }
        self.affine_interval_proof(inequality, query, check)
    }

    /// Checks the fixed `DIRECT(T - S)` residual of one accumulated candidate
    /// `S`, and then the same residual against each integer tightening of `S`.
    ///
    /// Every affine atom denotes a mathematical integer, so an accumulated
    /// `k * v <= u` with a positive integer `k` dividing every coefficient
    /// also proves `v <= floor(u / k)`. The tightening factors are functions
    /// of the candidate and the target alone: this step selects no additional
    /// premise, guesses no multiplier, and leaves the candidate families
    /// exactly as fixed by the specification. Each tightening is formed on its
    /// own: an unrepresentable one is skipped and removes neither the other
    /// tightening nor the untightened candidate.
    fn affine_candidate_residual_proof(
        &mut self,
        target: &AffineInequality,
        candidate: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Option<Vec<DerivationId>> {
        let tightenings = integer_tightenings(candidate, target, check);
        for accumulated in std::iter::once(candidate).chain(tightenings.iter()) {
            let Ok(residual) = AffineInequality::residual_after(target, accumulated, check) else {
                continue;
            };
            if let Ok(Some(parents)) = self.affine_residual_proof(&residual, query, check) {
                return Some(parents);
            }
        }
        None
    }

    /// Exhausts one coefficient-one L0 premise followed by the direct
    /// L0/interval residual rule. The L0 index contains one strongest entry
    /// per coefficient vector, so strengthening ordinary facts can only make
    /// a residual easier and never removes an earlier witness.
    fn affine_l0_then_direct_proof(
        &mut self,
        target: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Option<Vec<DerivationId>> {
        for entry in &query.l0.entries {
            let Some(mut parents) =
                self.affine_candidate_residual_proof(target, &entry.inequality, query, check)
            else {
                continue;
            };
            let Some(parent) = query.closed.bound_proof(
                entry.left,
                entry.right,
                entry.bound,
                &mut self.derivations,
            ) else {
                continue;
            };
            parents.push(parent);
            parents.sort_unstable_by_key(|parent| parent.0);
            parents.dedup();
            return Some(parents);
        }
        None
    }

    fn affine_target_proof(
        &mut self,
        target: &AffineInequality,
        assumptions: &[ActiveAffineFact],
        context: ProofContext<'_>,
    ) -> Option<AffineConsequenceProof> {
        let values = context.affine;
        let mut check = AffineCheckState::new();
        let candidates = self.affine_l0_candidates(values);
        let closed = context.close(&self.terms, &self.goals, &mut self.derivations);
        // Every relation-form use in a certificate sees the same entering
        // facts and value images. Its target and residual still run through
        // all ordinary rules; only the unchanged ordered query index is
        // shared. Candidate formation precedes the revision check because it
        // may register a previously unseen term.
        let l0 = context
            .closed
            .and_then(|view| view.affine_index(&self.terms, &self.goals))
            .unwrap_or_else(|| {
                let index = Rc::new(self.affine_l0_index(&candidates, &closed, &mut check));
                if let Some(view) = context
                    .closed
                    .filter(|view| view.matches(&self.terms, &self.goals))
                {
                    *view.affine_index.borrow_mut() = Some(Rc::clone(&index));
                }
                index
            });
        let mut query = AffineDirectQuery::new(&l0, values, &closed);
        if let Ok(Some(parents)) = self.affine_residual_proof(target, &mut query, &mut check) {
            return Some(AffineConsequenceProof {
                premises: Vec::new(),
                parents,
            });
        }
        let automatic = self
            .automatic_affine_premises(assumptions, &mut check)
            .ok()?;

        // Preserve the complete coefficient-one single-premise route. Every
        // premise is tried independently; an arithmetic error in one candidate
        // cannot suppress a later source or value-image fact.
        for (index, assumption) in automatic.iter().enumerate() {
            // A candidate that cannot participate in an i128 residual grants
            // no authority, but it must not hide a later independently
            // representable source fact in the same deterministic order.
            if let Some(parents) = self.affine_candidate_residual_proof(
                target,
                &assumption.inequality,
                &mut query,
                &mut check,
            ) {
                return Some(Self::affine_consequence_from_residual(
                    &[(index, 1)],
                    &automatic,
                    parents,
                ));
            }
        }

        // R2 exhausts the source-shaped set of unordered coefficient-one
        // pairs, including one premise used twice. There is no greedy state,
        // backtracking cutoff, or cumulative work budget: fact order changes
        // only which successful derivation is retained, never acceptance.
        if let Some((first, second, parents)) =
            first_two_premise_candidate(&automatic, &mut check, |sum, check| {
                self.affine_candidate_residual_proof(target, sum, &mut query, check)
            })
        {
            let selected = if first == second {
                vec![(first, 2)]
            } else {
                vec![(first, 1), (second, 1)]
            };
            return Some(Self::affine_consequence_from_residual(
                &selected, &automatic, parents,
            ));
        }

        // Ordinary L0 relations remain outside the affine premise set. This
        // is the specification's final `DIRECT(T - R)` family: subtract each
        // strongest indexed L0 image once, then run the ordinary DIRECT check
        // on the residual. DIRECT may itself close an exact L0 image, but the
        // route never publishes or recursively saturates either relation.
        self.affine_l0_then_direct_proof(target, &mut query, &mut check)
            .map(|parents| AffineConsequenceProof {
                premises: Vec::new(),
                parents,
            })
    }

    fn affine_event_kills_binding(
        &self,
        separations: &dyn SeparationOracle,
        binding: BindingId,
        event: &KillEvent,
    ) -> bool {
        match event {
            KillEvent::Write { place, .. } | KillEvent::EntryImageHolderWrite { place, .. } => {
                self.resolved_places_overlap(separations, &ResolvedPlace::binding(binding), place)
            }
            KillEvent::Consume {
                binding: consumed, ..
            }
            | KillEvent::EntryImageHolderConsume {
                binding: consumed, ..
            } => binding == *consumed,
        }
    }

    fn apply_affine_kills(
        &mut self,
        separations: &dyn SeparationOracle,
        state: &mut AffineFlowState,
        events: &[KillEvent],
    ) {
        state.values.retain(|binding, _| {
            !events
                .iter()
                .any(|event| self.affine_event_kills_binding(separations, *binding, event))
        });
        // [MSR-2] a measure atom is retargeted on exactly the events that
        // kill its term, exactly as a local's image is retargeted by a write
        // to that local: the next occurrence of the term mints a fresh atom,
        // so no conclusion published over the old one reaches past the write.
        let stale: Vec<TermId> = state
            .measure_atoms
            .borrow()
            .keys()
            .copied()
            .filter(|term| {
                events
                    .iter()
                    .any(|event| self.event_kills_term(separations, *term, event))
            })
            .collect();
        for term in stale {
            state.measure_atoms.get_mut().remove(&term);
        }
        // An opaque handle names one binding's value at the point the fold
        // took it, so it dies with a write to that binding exactly as the
        // binding's own image does [ENT-5].
        state.opaque_values.retain(|binding, _| {
            !events
                .iter()
                .any(|event| self.affine_event_kills_binding(separations, *binding, event))
        });
        // Published facts name immutable AffineTermId value identities, not
        // mutable bindings. Removing the map above prevents a replacement
        // value from matching an old image; retaining each theorem preserves
        // valid aliases to the old value.
    }

    fn expression_effects(
        &mut self,
        expression: &CheckedExpression,
        state: &mut ProofFlowState,
    ) -> ExpressionJudgment {
        let mut judgment = self.judge_expression(expression, state);
        let mut events = Vec::new();
        self.collect_expression_kills(expression, &mut events);
        if let Some(prepared) = &mut judgment.prepared_call {
            let transfer_events = &mut prepared.transfer_events;
            let live = self.apply_kills_each(state, &events, |analyzer, event| {
                let kind = match event {
                    KillEvent::Consume { .. } | KillEvent::EntryImageHolderConsume { .. } => {
                        FlowEventKind::PostconditionCallConsume
                    }
                    KillEvent::Write { .. } | KillEvent::EntryImageHolderWrite { .. } => {
                        FlowEventKind::PostconditionCallWrite
                    }
                };
                let proof_event = analyzer.proof_event(kind, Some(event.source()));
                transfer_events.push(proof_event);
                proof_event
            });
            prepared.kills = events;
            prepared.live = live;
        } else {
            self.apply_kills(state, &events);
        }
        if let Some(prepared) = &judgment.prepared_call {
            self.establish_call_state(expression, prepared, state);
        }
        judgment
    }

    /// A relation over exclusive exit state needs no result destination.
    /// It is established after the call's effects; enclosing commits and
    /// scope exits kill its ordinary place support in the normal flow.
    fn establish_call_state(
        &mut self,
        expression: &CheckedExpression,
        prepared: &PreparedCall,
        state: &mut ProofFlowState,
    ) {
        let CheckedExpression::UserCall {
            function,
            call,
            arguments,
            goal_arguments,
            ..
        } = expression
        else {
            return;
        };
        for available in prepared.postconditions.iter().cloned() {
            if available.variant.is_some()
                || available
                    .relation
                    .operands
                    .iter()
                    .any(|term| term.contains_result())
            {
                continue;
            }
            let Some(instantiated) = self.instantiate_call_postcondition_relation(
                *function,
                call,
                &available.relation,
                arguments,
                goal_arguments,
                &[],
                &[],
            ) else {
                continue;
            };
            if !self.s12_substitutions_survive(
                &prepared.entry_separations(&state.separations),
                &instantiated.substitutions,
                &prepared.kills,
                true,
            ) {
                continue;
            }
            let Some(proof) = self.retain_postcondition_call(&instantiated, &available, prepared)
            else {
                continue;
            };
            let occurrence = self.s12_roots;
            self.s12_roots = self
                .s12_roots
                .checked_add(1)
                .expect("S12 roots exceed the u32 identity space");
            self.derivations
                .add_root(DerivationRootKind::PostconditionState { occurrence }, proof);
            state
                .facts
                .establish_from_proof(&instantiated.relation, proof, &self.derivations);
        }
    }

    /// The [ENT-5] commit kill of one `set` target. The statement walk and
    /// the loop summary both form it here, so the event a loop body applies
    /// is the event its head subtracts.
    fn commit_kill(&self, node_path: &crate::NodePath, target: &CheckedSetTarget) -> KillEvent {
        match target {
            CheckedSetTarget::Place(place) => {
                let spelled = ResolvedPlace::spelled(
                    PlaceRoot::Binding(place.binding),
                    self.is_holder(place.binding),
                    place.fields.clone(),
                );
                KillEvent::Write {
                    place: self.resolve(&spelled),
                    element: false,
                    source: node_path.clone(),
                }
            }
            CheckedSetTarget::RangeIndex(target) => {
                let mut spelled = ResolvedPlace::spelled(
                    PlaceRoot::Binding(target.root.binding),
                    self.is_holder(target.root.binding),
                    Vec::new(),
                );
                spelled.path.extend(target.place_path());
                KillEvent::Write {
                    place: self.resolve(&spelled),
                    element: true,
                    source: node_path.clone(),
                }
            }
            // [MSR-2] an element store into a run overlaps the descriptor
            // storage of `v[i]` and none of `v`'s own, so it kills the
            // measures of the element and none of the run's.
            CheckedSetTarget::Storage(target) => KillEvent::Write {
                place: self.container_root_place(target),
                element: true,
                source: node_path.clone(),
            },
        }
    }

    /// The commit kill of one `set` target, and the goal-origin state a
    /// whole-place commit invalidates. One target list's commits are
    /// exactly this event per target, on the same edge.
    fn collect_target_kill(
        &self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        state: &mut ProofFlowState,
        target_kills: &mut Vec<KillEvent>,
    ) {
        target_kills.push(self.commit_kill(node_path, target));
        if let CheckedSetTarget::Place(place) = target
            && place.fields.is_empty()
        {
            state.facts.origins.remove(&place.binding);
        }
    }

    fn walk_set(
        &mut self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) {
        // [MSR-3] the [LIV-2] `set`-target placement is minted before the
        // statement's own kills, because the datum it forms is the value the
        // transferred place had immediately before them. The destination is
        // the place this commit writes, which is a plain place or one element
        // position of a run.
        let placement = self.mint_commit_placement(node_path, 0, target, value, state);
        let constructed = if self.set_target_place(target).is_some() {
            self.mint_construct_placements(node_path, value, state)
        } else {
            Vec::new()
        };
        // [SET-1]: the target's base and offset are evaluated before the
        // right-hand side; both are judged at this point, then the commit
        // kill applies.
        let target_reached = self.judge_set_target(target, state);
        let mut result = self.capture_result(node_path, value, state);
        let affine_value = self.affine_expression_form(value, &mut state.affine);
        let judgment = self.expression_effects(value, state);
        self.finish_result(value, &judgment, &mut result, state);
        let ExpressionJudgment {
            prepared_call: prepared,
            reached: value_reached,
        } = judgment;
        let ranges_reached = self.judge_call_separations(node_path, state);
        let commit_reached = target_reached && value_reached && ranges_reached;
        let receiver_route = commit_reached
            .then(|| {
                prepared.as_ref().and_then(|prepared| {
                    self.direct_receiver_route(&state.separations, target, value, prepared)
                })
            })
            .flatten();
        invalidate_goal_origin_for_set(&mut state.facts, target);
        // [SET-1, ENT-3.S5]: the right-hand side is now evaluated, so its
        // image is established here, on this occurrence's own commit value,
        // under exactly the rules a `let` initializer uses. Establishing it
        // before the target kill is what lets [ENT-5]'s pre-kill closure
        // carry the surviving consequences of that value past the write,
        // instead of losing them with the old target value.
        //
        // Only a direct fragment place can receive that image, so only such a
        // commit forms a value term: with no destination to carry it to, the
        // image would relate nothing to the program's later state.
        let commit_carries_image = self.commit_target_term(target).is_some();
        let mut commit_event = None;
        let commit_division = (commit_reached && commit_carries_image)
            .then(|| {
                self.establish_value_image(
                    node_path,
                    ValueImage::Commit(node_path),
                    value,
                    &mut state.facts,
                    &mut commit_event,
                )
            })
            .flatten();
        let commit_operands = commit_division
            .as_ref()
            .and_then(|_| self.unsigned_division_operand_forms(value, &mut state.affine));
        if commit_reached && let Some(product) = &affine_value {
            self.establish_unsigned_division_product(product, value, &mut state.affine);
        }
        let mut target_kills = Vec::new();
        self.collect_target_kill(node_path, target, state, &mut target_kills);
        let receivers =
            if let (Some(prepared), Some(receiver_route)) = (prepared.as_ref(), receiver_route) {
                self.prepare_direct_receiver(
                    &state.separations,
                    receiver_route,
                    value,
                    prepared,
                    &target_kills,
                )
            } else {
                Vec::new()
            };
        let target_event = (!receivers.is_empty())
            .then(|| self.proof_event(FlowEventKind::PostconditionReceiverWrite, Some(node_path)));
        if let Some(result) = &mut result {
            let live = self.event_live_indices(state, &target_kills);
            let separations = EventSeparations {
                ledger: &state.separations,
                live: &live,
            };
            self.apply_kills_one(&separations, &mut result.facts, &target_kills);
        }
        if let Some(target_event) = target_event {
            self.apply_kills_each(state, &target_kills, |_, _| target_event);
        } else {
            self.apply_kills(state, &target_kills);
        }
        // [MSR-3] the placement's second half, after the target's own kills:
        // the committed place's measures are the datums minted before them.
        if commit_reached
            && let Some(carry) = &placement
            && let Some(destination) = self.set_target_place(target)
        {
            self.establish_measure_datums(node_path, destination, carry, &mut state.facts);
        }
        // [REF-4, MSR-1] rebinding a range holder installs the new
        // formation's captured length only after the old holder facts have
        // been killed. Read the formation image by capture occurrence; the
        // endpoint bindings may no longer denote the evaluated values.
        if commit_reached
            && let CheckedSetTarget::Place(place) = target
            && place.fields.is_empty()
            && let CheckedExpression::RangeOf { captured, .. } = value
        {
            let destination = self.bound_place(place.binding);
            let _ = self.establish_captured_range_length(destination, *captured, &mut state.affine);
        }
        if commit_reached
            && !constructed.is_empty()
            && let Some(base) = self.set_target_place(target)
        {
            self.establish_construct_placements(node_path, &base, &constructed, &mut state.facts);
        }
        // [CALL-4] a `set` target is an S12 destination, and [CALL-6] puts
        // the establishment after the call's own transfer, consumes and the
        // target's commit and kills — which is exactly this point. The
        // destination list is one entry long because a single-target `set`
        // takes result ordinal zero, and the route is the same one a `let`
        // binder and a `set` target list take. `PreparedCall` already holds
        // the exact direct or formal-boundary publication surface, with the
        // target's own kills the events every substitution must survive.
        if commit_reached
            && let Some(prepared) = prepared.as_ref()
            && let CheckedSetTarget::Place(place) = target
        {
            let destinations = vec![Some((
                place.binding,
                place
                    .fields
                    .iter()
                    .map(|field| GoalProjection::Field(*field))
                    .collect::<Vec<_>>(),
                place.ty,
            ))];
            self.establish_result_list_destinations(
                node_path,
                &destinations,
                value,
                prepared,
                &target_kills,
                state,
            );
        }
        // [ENT-3.S5, ENT-5]: the committed value exists only after the old
        // target facts have died. The equality names the commit value formed
        // above; when no source recognized the right-hand side, no commit
        // value was interned and this commit contributes no fact either.
        let mut set_image_event = None;
        if commit_reached {
            if let Some(commit) = self.interned_commit_value_term(node_path, value) {
                self.establish_commit_copy_fact(
                    node_path,
                    target,
                    commit,
                    &mut state.facts,
                    &mut set_image_event,
                );
            }
            let mut committed_affine = None;
            if let CheckedSetTarget::Place(place) = target
                && place.fields.is_empty()
                && self.affine_binding_type(place.binding).is_some()
                && let Some(value) = affine_value
            {
                committed_affine = Some(value.clone());
                state.affine.values.insert(place.binding, value);
            }
            // The scaled quotient image binds the committed value to the
            // dividend image read before the kill [ENT-3.S7].
            if let (Some(established), Some((dividend, divisor)), Some(quotient)) =
                (commit_division, commit_operands, committed_affine)
            {
                self.establish_unsigned_division_image(
                    &quotient,
                    &dividend,
                    &divisor,
                    established,
                    &mut state.affine,
                );
            }
        }
        if let (Some(prepared), Some(target_event)) = (&prepared, target_event) {
            for receiver in &receivers {
                self.establish_direct_receiver(node_path, receiver, prepared, target_event, state);
            }
        }
        if commit_reached
            && let Some(result) = result
            && let CheckedSetTarget::Place(place) = target
            && place.fields.is_empty()
            && !self.is_holder(place.binding)
        {
            state.results.insert(place.binding, result);
        }
    }

    fn walk_statement(&mut self, statement: &CheckedStatement, state: &mut ProofFlowState) -> bool {
        let permission_site = match statement {
            CheckedStatement::Proof(proof) => Some(&proof.node_path),
            CheckedStatement::Let { node_path, .. }
            | CheckedStatement::Set { node_path, .. }
            | CheckedStatement::Evaluate { node_path, .. }
            | CheckedStatement::DropExpression { node_path, .. } => Some(node_path),
            CheckedStatement::Match {
                scrutinee: CheckedExpression::UserCall { call, .. },
                ..
            } => Some(call),
            _ => None,
        };
        if let Some(site) = permission_site {
            self.judge_permission_separations(site, state);
        }
        match statement {
            CheckedStatement::Let {
                node_path,
                binding,
                value,
            } => {
                let affine_value = self.affine_expression_form(value, &mut state.affine);
                // [MSR-3] the rebind placement is minted before the
                // initializer's own kills, because the datum it forms is the
                // value the transferred place had immediately before them.
                let rebind = self.mint_rebind_datums(node_path, 0, value, state);
                // [MSR-3] the construct placement is minted at the same
                // point and for the same reason: a field operand is consumed
                // by the construct that fills the field with it.
                let constructed = self.mint_construct_placements(node_path, value, state);
                let mut result = self.capture_result(node_path, value, state);
                let judgment = self.expression_effects(value, state);
                self.finish_result(value, &judgment, &mut result, state);
                if let Some(result) = result {
                    state.results.insert(*binding, result);
                }
                self.declare(*binding);
                if judgment.reached
                    && self.affine_binding_type(*binding).is_some()
                    && let Some(value) = affine_value
                {
                    state.affine.values.insert(*binding, value);
                }
                if let Some(prepared) = &judgment.prepared_call {
                    self.establish_direct_result(node_path, *binding, value, prepared, state);
                }
                if judgment.reached
                    && value.ty() == CheckedType::Bool
                    && let Some(relation) = self.direct_comparison(value)
                {
                    state.facts.origins.insert(*binding, relation);
                }
                if judgment.reached {
                    self.record_goal_origin(*binding, value, &mut state.facts);
                }
                // Sources S5, S6, S7, and S9 establish at the binding, after
                // the initializer's own kills [ENT-3, ENT-5].
                let mut event = None;
                let unsigned_division = if judgment.reached {
                    self.establish_value_image(
                        node_path,
                        ValueImage::Binding(*binding),
                        value,
                        &mut state.facts,
                        &mut event,
                    )
                } else {
                    None
                };
                if judgment.reached
                    && let Some(rebind) = &rebind
                {
                    self.establish_rebind_datums(node_path, *binding, rebind, &mut state.facts);
                }
                if judgment.reached && !constructed.is_empty() {
                    let base = self.bound_place(*binding);
                    self.establish_construct_placements(
                        node_path,
                        &base,
                        &constructed,
                        &mut state.facts,
                    );
                }
                if let Some(established) = unsigned_division
                    && let Some(quotient) = state.affine.values.get(binding).cloned()
                    && let Some((dividend, divisor)) =
                        self.unsigned_division_operand_forms(value, &mut state.affine)
                {
                    self.establish_unsigned_division_image(
                        &quotient,
                        &dividend,
                        &divisor,
                        established,
                        &mut state.affine,
                    );
                }
                if judgment.reached {
                    self.record_product_atom(*binding, value, &mut state.affine);
                    if let Some(product) = state.affine.values.get(binding).cloned() {
                        self.establish_unsigned_division_product(
                            &product,
                            value,
                            &mut state.affine,
                        );
                    }
                    // [REF-4, MSR-1] a range reference's one measure is
                    // `len`, equal to the immutable endpoint images the
                    // formation recorded while evaluating this initializer.
                    if let CheckedExpression::RangeOf {
                        start,
                        end,
                        captured,
                        ..
                    } = value
                    {
                        let destination = self.bound_place(*binding);
                        let term = self.establish_captured_range_length(
                            destination,
                            *captured,
                            &mut state.affine,
                        );
                        // [ENT-3.S6] the same formation establishes
                        // `deref(part).len = hi - lo` as an ordinary fact, so
                        // a requirement stated over the range's length is
                        // judged against the length the range has and not
                        // merely against an affine premise.
                        if let Some(relation) = term.and_then(|term| {
                            Self::captured_range_length_image(*captured, &state.affine)
                                .filter(|image| image.terms().is_empty())
                                .map(|image| Relation::Equal {
                                    left: term,
                                    right: self
                                        .terms
                                        .intern(TermKind::Constant(image.constant_value())),
                                    difference: 0,
                                })
                                .or_else(|| self.range_length_relation(term, start, end))
                        }) {
                            let formation = self.proof_event(FlowEventKind::S6, Some(node_path));
                            state
                                .facts
                                .establish(&relation, &mut self.derivations, formation);
                        }
                    }
                }
                true
            }
            // [GRAM-4, CALL-4] `let (a, b) = f(...);`. The call is judged once;
            // each binder is declared and receives the published relations
            // naming its own result ordinal [ENT-3.S12].
            CheckedStatement::DestructuringLet {
                node_path,
                bindings,
                value,
                ..
            } => {
                // [MSR-3] the destructuring placement is minted before the
                // consume the statement performs, because the datums it
                // forms are the measures the taken-apart value's fields had
                // immediately before it.
                let taken = self.mint_destructuring_placements(node_path, bindings, value, state);
                let judgment = self.expression_effects(value, state);
                let mut destinations = Vec::with_capacity(bindings.len());
                for (binding, ty, _) in bindings {
                    self.declare(*binding);
                    // A call's result list has no single expression image to
                    // copy into each destination. Each fragment result still
                    // receives its own current-value atom, so S12's ordinary
                    // result relation can bridge published affine facts to
                    // that ordinal without conflating sibling results.
                    if judgment.reached
                        && let Some(value) = self.affine_unknown_integer(*ty)
                    {
                        state.affine.values.insert(*binding, value);
                    }
                    destinations.push(Some((*binding, Vec::new(), *ty)));
                }
                if judgment.reached {
                    for (ordinal, carry) in &taken {
                        let Some((binding, _, _)) = bindings.get(*ordinal as usize) else {
                            continue;
                        };
                        let destination = self.bound_place(*binding);
                        self.establish_measure_datums(
                            node_path,
                            destination,
                            carry,
                            &mut state.facts,
                        );
                    }
                }
                if let Some(prepared) = &judgment.prepared_call
                    && judgment.reached
                {
                    self.establish_result_list_destinations(
                        node_path,
                        &destinations,
                        value,
                        prepared,
                        &[],
                        state,
                    );
                }
                true
            }
            CheckedStatement::PropagateLet {
                node_path,
                binding,
                scrutinee,
                ok_type,
                ..
            } => {
                let mut result = self.capture_result(node_path, scrutinee, state);
                let judgment = self.expression_effects(scrutinee, state);
                self.finish_result(scrutinee, &judgment, &mut result, state);
                self.declare(*binding);
                if let Some(result) = result {
                    self.select_result(node_path, &result, *binding, *ok_type, state);
                }
                if self.affine_binding_type(*binding).is_some()
                    && let Some(value) = self.affine_unknown_integer(*ok_type)
                {
                    state.affine.values.insert(*binding, value);
                }
                true
            }
            CheckedStatement::Set {
                node_path,
                target,
                value,
                ..
            } => {
                self.walk_set(node_path, target, value, state);
                true
            }
            CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. } => {
                let _ = self.expression_effects(value, state);
                true
            }
            CheckedStatement::Proof(proof) => {
                self.judge_affine_relation_subscripts(&proof.target, state);
                for written_use in &proof.uses {
                    if let CheckedProofUseSource::Relation(relation) = &written_use.source {
                        self.judge_affine_relation_subscripts(relation, state);
                    }
                }
                let source_ordinal = u32::try_from(self.source_proofs.len())
                    .expect("local invariant count exceeds the u32 identity space");
                let target_result = self.checked_affine_relation_inequality(
                    &proof.target,
                    &mut state.affine,
                    &mut AffineCheckState::new(),
                );
                let target_failure = target_result
                    .as_ref()
                    .err()
                    .copied()
                    .map(Self::source_proof_formation_failure);
                let target = target_result.as_ref().ok().cloned();
                // [INV-1] an `==` statement target is one batch of two
                // bounds: both are proved here, and both are published.
                let partner_result = self.checked_affine_relation_partner(
                    &proof.target,
                    &mut state.affine,
                    &mut AffineCheckState::new(),
                );
                let partner_failure = partner_result
                    .as_ref()
                    .and_then(|partner| partner.as_ref().err().copied())
                    .map(Self::source_proof_formation_failure);
                let partner = partner_result
                    .as_ref()
                    .and_then(|partner| partner.as_ref().ok().cloned());
                let partner_written = partner_result.is_some();
                let target_failure = target_failure.or(partner_failure);
                self.invariant_targets
                    .insert(proof.declaration, target_result);
                let formed_premises = proof
                    .uses
                    .iter()
                    .map(|written_use| match &written_use.source {
                        CheckedProofUseSource::Named(declaration) => self
                            .invariant_targets
                            .get(declaration)
                            .cloned()
                            .unwrap_or(Err(AffineCheckError::CoefficientMismatch)),
                        CheckedProofUseSource::Relation(relation) => self
                            .checked_affine_relation_inequality(
                                relation,
                                &mut state.affine,
                                &mut AffineCheckState::new(),
                            ),
                    })
                    .collect::<Vec<_>>();
                let source_failure_use_index = formed_premises
                    .iter()
                    .position(|premise| premise.is_err())
                    .map(|index| {
                        u32::try_from(index).expect("source-proof use index fits the u32 identity")
                    });
                let source_failure = source_failure_use_index.map(|index| {
                    let error = formed_premises
                        [usize::try_from(index).expect("source-proof use index fits usize")]
                    .as_ref()
                    .expect_err("source failure index names a failed source")
                    .to_owned();
                    Self::source_proof_formation_failure(error)
                });
                let premises = formed_premises
                    .iter()
                    .map(|premise| premise.as_ref().ok().cloned())
                    .collect::<Vec<_>>();
                let published_premises = proof
                    .uses
                    .iter()
                    .map(|written_use| match &written_use.source {
                        CheckedProofUseSource::Named(declaration) => self
                            .invariant_targets
                            .get(declaration)
                            .and_then(|formed| formed.as_ref().ok())
                            .zip(state.affine.published_invariants.get(declaration))
                            .is_some_and(|(declared, published)| declared == published),
                        CheckedProofUseSource::Relation(_) => false,
                    })
                    .collect::<Vec<_>>();
                let named_premises = proof
                    .uses
                    .iter()
                    .map(|written_use| {
                        matches!(&written_use.source, CheckedProofUseSource::Named(_))
                    })
                    .collect::<Vec<_>>();
                let multiplicities = proof
                    .uses
                    .iter()
                    .map(|written_use| {
                        self.certificate_multiplicity(written_use.multiplicity, &mut state.affine)
                    })
                    .collect::<Vec<_>>();
                let certificate_premises = premises
                    .iter()
                    .zip(&multiplicities)
                    .map(|(premise, multiplicity)| premise.clone().zip(multiplicity.clone()))
                    .collect::<Option<Vec<_>>>();

                // [INV-1] a blockless target receives the complete MSR-4
                // disposition. [PRF-1] instead judges a written certificate's
                // redundancy by AUTO alone: Step 6 may prove a target without
                // making its explicitly written certificate redundant.
                let (target_right, partner_right) = if proof.uses.is_empty() {
                    (
                        self.checked_affine_right_term(&proof.target.right),
                        self.checked_affine_right_term(&proof.target.left),
                    )
                } else {
                    (None, None)
                };
                let target_goal = |inequality, right| {
                    if proof.uses.is_empty() {
                        ProofGoal::Affine { inequality, right }
                    } else {
                        ProofGoal::AutomaticAffine { inequality }
                    }
                };
                let target_proved = target.as_ref().is_some_and(|target| {
                    self.prove(
                        ProofContext::new(&state.facts, &state.affine),
                        target_goal(target, target_right),
                    )
                    .disposition
                        == ProofDisposition::Proved
                }) && (!partner_written
                    || partner.as_ref().is_some_and(|partner| {
                        self.prove(
                            ProofContext::new(&state.facts, &state.affine),
                            target_goal(partner, partner_right),
                        )
                        .disposition
                            == ProofDisposition::Proved
                    }));
                let redundant = !proof.uses.is_empty() && target_proved;
                // [MSR-4] a blockless target no step discharged is refuted
                // when the entering context derives the negation of one of
                // its bounds.
                let target_refuted =
                    proof.uses.is_empty() && !target_proved && target_failure.is_none() && {
                        let mut members = vec![(target.clone(), target_right, partner_right)];
                        if partner_written {
                            members.push((partner.clone(), partner_right, target_right));
                        }
                        self.affine_target_disposition(&members, &state.facts, &state.affine)
                            == TargetDisposition::Refuted
                    };
                let certificate_sum = if proof.uses.is_empty() || source_failure.is_some() {
                    None
                } else {
                    Some(certificate_premises.as_deref().map_or_else(
                        || Err((SourceProofCertificateFailure::FormationCapacity, 0)),
                        |premises| self.source_proof_sum(premises),
                    ))
                };

                // Every written `use` is proved against the same pre-proof
                // program point. No premise established by this statement can
                // help another premise in the same statement.
                let premise_results = self.source_proof_premise_results(
                    &premises,
                    &named_premises,
                    &published_premises,
                    &state.affine,
                    &state.facts,
                );
                let certificate_failure = certificate_sum
                    .as_ref()
                    .and_then(|sum| sum.as_ref().err().copied());
                let certificate_failure_kind = certificate_failure.map(|(failure, _)| failure);
                let certificate_failure_use_index =
                    certificate_failure.map(|(_, use_index)| use_index);
                let first_unproved_premise =
                    premise_results
                        .iter()
                        .position(|proved| !proved)
                        .map(|index| {
                            u32::try_from(index)
                                .expect("source-proof use index fits the u32 identity")
                        });
                let residual = if proof.uses.is_empty() {
                    Ok(target_proved)
                } else if target_failure.is_some()
                    || source_failure.is_some()
                    || certificate_failure_kind.is_some()
                    || first_unproved_premise.is_some()
                {
                    Ok(false)
                } else {
                    match (
                        target.as_ref(),
                        certificate_sum.as_ref().and_then(|sum| sum.as_ref().ok()),
                    ) {
                        (Some(target), Some(sum)) => {
                            // [INV-1] every member of the batch is closed by
                            // the one written certificate.
                            let forward = self.source_proof_certificate_residual(
                                target,
                                sum,
                                &state.affine,
                                &state.facts,
                            );
                            match (&forward, partner.as_ref()) {
                                (Ok(true), Some(partner)) => self
                                    .source_proof_certificate_residual(
                                        partner,
                                        sum,
                                        &state.affine,
                                        &state.facts,
                                    ),
                                _ => forward,
                            }
                        }
                        _ => Err(SourceProofCertificateFailure::FormationCapacity),
                    }
                };
                let residual_failure = residual.as_ref().err().copied();
                let check = SourceProofCheck {
                    premises: premise_results,
                    first_unproved_premise,
                    combination: residual.unwrap_or(false),
                    target_failure,
                    source_failure,
                    source_failure_use_index,
                    certificate_failure: certificate_failure_kind,
                    certificate_failure_use_index,
                    residual_failure,
                    redundant,
                    target_refuted,
                };

                if let Some(target) = target
                    && check.discharged()
                {
                    for inequality in std::iter::once(target.clone()).chain(partner) {
                        state.affine.facts.push(ActiveAffineFact {
                            inequality,
                            evidence: AffineFactEvidence::Source(
                                SourceAffineFactRef::SourceProof { source_ordinal },
                            ),
                            active_loops: Vec::new(),
                        });
                    }
                    state
                        .affine
                        .published_invariants
                        .insert(proof.declaration, target);
                }
                self.source_proofs.push(SourceProofOutcome {
                    node_path: proof.node_path.clone(),
                    use_node_paths: proof
                        .uses
                        .iter()
                        .map(|written_use| written_use.node_path.clone())
                        .collect(),
                    source_ordinal,
                    name: proof.name.clone(),
                    certificate_written: !proof.uses.is_empty(),
                    check,
                });
                true
            }
            CheckedStatement::Return {
                node_path,
                value,
                drops: _,
            } => {
                let multiple = self.function.postconditions.iter().any(|clause| {
                    clause.selected_returns.iter().any(|selected| {
                        selected.statement == *node_path && selected.values.len() > 1
                    })
                });
                let returned = match value {
                    CheckedExpression::ConstructStruct { fields, .. } if multiple => {
                        fields.iter().collect::<Vec<_>>()
                    }
                    _ => vec![value],
                };
                let mut results = returned
                    .iter()
                    .map(|value| self.capture_result(node_path, value, state))
                    .collect::<Vec<_>>();

                let affine_result = self.affine_pure_expression_form(value, &mut state.affine);
                // [FN-9] the relation is queried "immediately before return
                // transfer and edge cleanup": the returned value's own
                // consume is that transfer, so it has not happened at the
                // query point and its kills are applied after. Nothing reads
                // the state between the two, because a return has no normal
                // continuation.
                //
                // [MSR-3] a call in return position is not that transfer. The
                // exit-state measure of a written reference parameter
                // "evaluates over that parameter's resolved referent
                // immediately before each selected return, after the return's
                // ordinary effects and kills", and a call's projected writes
                // and its published exit relation are exactly those effects
                // [CALL-6]. Judging the clause before them read the referent's
                // entry state at the exit, which made `deref(p).len ==
                // deref(entry(p)).len` hold over a callee that had just
                // changed it.
                let judgment = if matches!(value, CheckedExpression::UserCall { .. }) {
                    self.expression_effects(value, state)
                } else {
                    self.judge_expression(value, state)
                };
                let mut events = Vec::new();
                if !matches!(value, CheckedExpression::UserCall { .. }) {
                    self.collect_expression_kills(value, &mut events);
                }
                for result in &mut results {
                    // Every ordinal sees the complete return expression's
                    // effects. A clause selects only its own Result context.
                    self.finish_result(value, &judgment, result, state);
                }

                self.judge_postcondition_return(
                    node_path,
                    state,
                    affine_result.as_ref(),
                    judgment.reached,
                    &results,
                );
                self.apply_kills(state, &events);
                false
            }
            CheckedStatement::Give {
                node_path,
                value,
                drops: _,
            } => {
                let mut result = self.capture_result(node_path, value, state);
                let judgment = self.expression_effects(value, state);
                self.finish_result(value, &judgment, &mut result, state);
                if let Some((scope_depth, loop_depth, binding, result_type)) =
                    self.gives.last().map(|frame| {
                        (
                            frame.scope_depth,
                            frame.loop_depth,
                            frame.binding,
                            frame.result_type,
                        )
                    })
                {
                    let give_goal_origin = if judgment.reached && result_type == CheckedType::Bool {
                        self.admitted_value_goal_expression(value)
                            .map(|origin| self.intern_goal_expression(origin))
                    } else {
                        None
                    };
                    let delivery = Some({
                        self.value_delivery_image(
                            value,
                            state,
                            DeliveryImageContext {
                                statement: node_path,
                                receiver_binding: binding,
                                receiver_type: result_type,
                                scope_depth,
                                loop_depth,
                            },
                        )
                    });
                    let mut exit = state.clone();
                    if let Some(result) = result {
                        exit.results.insert(binding, result);
                    }
                    self.exit_scopes_to(&mut exit, scope_depth);
                    self.exit_counted_loops_from(&mut exit, loop_depth);
                    if let Some(frame) = self.gives.last_mut() {
                        frame.gives.push(exit);
                        frame.give_goal_origins.push(give_goal_origin);
                        if let Some(delivery) = delivery {
                            frame.delivery_images.push(delivery);
                            frame.delivery_edges.push(node_path.clone());
                        }
                    }
                }
                false
            }
            CheckedStatement::Break { target, drops: _ } => {
                if let Some(position) = self.loops.iter().rposition(|frame| frame.id == *target) {
                    let depth = self.loops[position].scope_depth;
                    let mut exit = state.clone();
                    self.exit_scopes_to(&mut exit, depth);
                    self.exit_counted_loops_from(&mut exit, position);
                    self.loops[position].breaks.push(exit);
                }
                false
            }
            CheckedStatement::Match {
                scrutinee,
                enum_type,
                arms,
                ..
            } => {
                // [MSR-3] the payload placement is minted before the `match`
                // consumes its scrutinee, because the datums it forms are the
                // measures the payload had immediately before that consume.
                let payload = self.mint_payload_placements(scrutinee, *enum_type, state);
                let mut result = Self::expression_node_path(scrutinee)
                    .cloned()
                    .and_then(|site| self.capture_result(&site, scrutinee, state));
                let judgment = self.expression_effects(scrutinee, state);
                self.finish_result(scrutinee, &judgment, &mut result, state);
                let facts = if judgment.reached {
                    self.arm_facts(scrutinee, *enum_type, &state.facts)
                } else {
                    ArmFacts::default()
                };
                let mut exits = Vec::new();
                for arm in arms {
                    let payload = payload.iter().find(|payload| payload.tag == arm.tag);
                    if let Some(exit) = self.walk_arm(arm, state, &facts, payload, result.as_ref())
                    {
                        exits.push(exit);
                    }
                }
                if exits.is_empty() {
                    false
                } else {
                    *state = self.join_flows(&exits);
                    true
                }
            }
            CheckedStatement::ValueMatchLet {
                node_path,
                binding,
                result_type,
                scrutinee,
                enum_type,
                arms,
                ..
            } => {
                // [MSR-3] the payload placement is minted before the `match`
                // consumes its scrutinee, because the datums it forms are the
                // measures the payload had immediately before that consume.
                let payload = self.mint_payload_placements(scrutinee, *enum_type, state);
                let mut result = self.capture_result(node_path, scrutinee, state);
                let judgment = self.expression_effects(scrutinee, state);
                self.finish_result(scrutinee, &judgment, &mut result, state);
                let facts = if judgment.reached {
                    self.arm_facts(scrutinee, *enum_type, &state.facts)
                } else {
                    ArmFacts::default()
                };
                self.gives.push(GiveFrame {
                    scope_depth: self.scopes.len(),
                    loop_depth: self.loops.len(),
                    node_path: node_path.clone(),
                    binding: *binding,
                    result_type: *result_type,
                    gives: Vec::new(),
                    give_goal_origins: Vec::new(),
                    delivery_images: Vec::new(),
                    delivery_edges: Vec::new(),
                });
                for arm in arms {
                    // Every delivering path leaves by `give`; an arm's
                    // fall-through state contributes nothing [GIVE-1].
                    let payload = payload.iter().find(|payload| payload.tag == arm.tag);
                    let _ = self.walk_arm(arm, state, &facts, payload, result.as_ref());
                }
                let frame = self
                    .gives
                    .pop()
                    .expect("checked value initializer has one active give frame");
                self.declare(*binding);
                if frame.gives.is_empty() {
                    return false;
                }
                *state = self.join_flows(&frame.gives);
                if self.affine_binding_type(*binding).is_some()
                    && let Some(value) = self.affine_unknown_integer(*result_type)
                {
                    state.affine.values.insert(*binding, value);
                }
                self.record_value_initializer_origin(&frame, &mut state.facts);
                self.establish_value_delivery_join(&frame, state);
                true
            }
            CheckedStatement::Loop {
                id,
                invariants,
                body,
                backedge_drops: _,
            } => {
                for invariant in invariants {
                    self.judge_affine_relation_subscripts(&invariant.relation, state);
                }
                let base = self.prove_loop_invariant_bases(invariants, state);
                let base_batch = base
                    .iter()
                    .all(|disposition| *disposition == TargetDisposition::Proved);

                // The generic header starts from the preheader minus every
                // fact a continuing kill may invalidate. Invariants then add
                // precisely the author-written induction hypotheses whose
                // complete base batch succeeded.
                let mut kills = LoopKills::default();
                self.collect_continuing_loop_kills(
                    body,
                    true,
                    &mut LoopReachability::default(),
                    &mut kills,
                );
                self.apply_loop_kills(state, &kills, None);
                self.activate_loop_invariant_batch(*id, invariants, base_batch, &mut state.affine);
                let invariant_declarations = invariants
                    .iter()
                    .map(|invariant| invariant.declaration)
                    .collect::<Vec<_>>();
                let head_entry_images = state.entry_images.clone();
                self.loops.push(LoopFrame {
                    id: *id,
                    invariant_declarations: invariant_declarations.clone().into_boxed_slice(),
                    scope_depth: self.scopes.len(),
                    counted_binder: None,
                    invariant_atoms: HashSet::new(),
                    capture_path: None,
                    breaks: Vec::new(),
                });
                let mut body_state = state.clone();
                let outer_continuing = std::mem::take(&mut body_state.continuing);
                let body_falls_through = self.walk_block(body, &mut body_state);
                if body_falls_through {
                    Self::debug_assert_summarized(&body_state, &kills);
                }

                let mut step = vec![None; invariants.len()];
                if body_falls_through {
                    for (index, invariant) in invariants.iter().enumerate() {
                        step[index] = Some(
                            self.prove_affine_relation_batch(&invariant.relation, &mut body_state),
                        );
                    }
                }
                self.record_loop_invariant_outcomes(*id, invariants, &base, &step, None);

                let frame = self.loops.pop();
                let mut breaks = frame.map(|frame| frame.breaks).unwrap_or_default();
                for break_state in &mut breaks {
                    Self::remove_active_loop_invariants(
                        &mut break_state.affine,
                        *id,
                        &invariant_declarations,
                    );
                }
                let has_breaks = !breaks.is_empty();
                // The continuation is the join over the break edges; with no
                // break it is the contradictory all-derivable state, matching
                // an unreachable-in-truth continuation the conservative graph
                // keeps reachable [ENT-5].
                *state = self.join_flows(&breaks);
                if !has_breaks {
                    state.entry_images = head_entry_images;
                }
                record_continuing(&mut state.continuing, &outer_continuing);
                true
            }
            CheckedStatement::CountedRange {
                id,
                node_path,
                binder,
                lower,
                upper,
                invariants,
                body,
                backedge_drops: _,
            } => {
                let occurrence = self.encountered_counted;
                self.encountered_counted = self
                    .encountered_counted
                    .checked_add(1)
                    .expect("counted statements exceed the u32 identity space");
                // [FN-1, ENT-3 S11]: evaluate each endpoint exactly once,
                // left to right, then install the private captures and the
                // compiler-updated binder in a construct-owned fact scope.
                let lower_affine = self.affine_expression_form(lower, &mut state.affine);
                let _ = self.expression_effects(lower, state);
                // Capture the upper endpoint after lower-endpoint effects even
                // when no current proof consumes its affine image.  This
                // preserves FN-1 evaluation order and therefore deterministic
                // atom identities for every later program-point value.
                let upper_affine = self.affine_expression_form(upper, &mut state.affine);
                let _ = self.expression_effects(upper, state);
                let outer_scope_depth = self.scopes.len();
                self.scopes.push(vec![*binder]);
                let range_path = node_path.components().to_vec();
                let preheader_event = self.proof_event(FlowEventKind::S11, Some(node_path));
                let counted_terms = self.establish_counted_preheader(
                    &range_path,
                    *binder,
                    lower,
                    upper,
                    &mut state.facts,
                    preheader_event,
                );
                // S11 fixes the complete post-capture closure before
                // continuing kills are subtracted. This preserves sound
                // snapshot consequences without rereading a mutable endpoint
                // on later iterations.
                let snapshot = self.derivations.event(FlowEventKind::Snapshot, None);
                state.facts = materialize_closure_at(
                    &state.facts,
                    &self.terms,
                    &self.goals,
                    &mut self.derivations,
                    snapshot,
                );
                let counted = self.capture_counted_preheader(counted_terms, &state.facts);
                let binder_affine = lower_affine
                    .clone()
                    .or_else(|| self.new_affine_binding_atom(*binder))
                    .expect("a checked counted binder has one u64 affine value");
                state.affine.values.insert(*binder, binder_affine);

                let lower_le_upper = lower_affine.as_ref().and_then(|lower| {
                    upper_affine.as_ref().and_then(|upper| {
                        AffineInequality::from_forms(lower, upper, &mut AffineCheckState::new())
                            .ok()
                    })
                });
                let lower_le_upper = lower_le_upper.as_ref().is_some_and(|target| {
                    self.prove(
                        ProofContext::new(&state.facts, &state.affine),
                        ProofGoal::Affine {
                            inequality: target,
                            right: None,
                        },
                    )
                    .disposition
                        == ProofDisposition::Proved
                });

                for invariant in invariants {
                    self.judge_affine_relation_subscripts(&invariant.relation, state);
                }
                let base = self.prove_loop_invariant_bases(invariants, state);
                let base_batch = base
                    .iter()
                    .all(|disposition| *disposition == TargetDisposition::Proved);

                let mut kills = LoopKills::default();
                let body_reaches_head = self.collect_continuing_loop_kills(
                    body,
                    true,
                    &mut LoopReachability::default(),
                    &mut kills,
                );
                if body_reaches_head {
                    // The hidden update is a continuing write exactly when
                    // normal body fallthrough can reach it.
                    kills.push_event_group(vec![KillEvent::Write {
                        place: ResolvedPlace {
                            root: PlaceRoot::Binding(*binder),
                            path: Vec::new(),
                        },
                        element: false,
                        source: node_path.clone(),
                    }]);
                    kills.set_bindings.insert(*binder);
                }
                self.apply_loop_kills(state, &kills, Some(snapshot));

                let invariant_atoms = state
                    .affine
                    .values
                    .values()
                    .chain(state.affine.opaque_values.values())
                    .chain(state.affine.measure_atoms.borrow().values())
                    .flat_map(|form| form.terms().iter().map(|coefficient| coefficient.term()))
                    .collect();

                // Values killed by a possible continuing iteration now read
                // as fresh header atoms.  The written expressions themselves
                // are unchanged; only their program-point value images differ
                // from the base targets above.
                let header_binder = self
                    .new_affine_binding_atom(*binder)
                    .expect("a checked counted binder has one u64 affine value");
                state.affine.values.insert(*binder, header_binder);
                self.activate_loop_invariant_batch(*id, invariants, base_batch, &mut state.affine);

                let head = state.clone();
                let invariant_declarations = invariants
                    .iter()
                    .map(|invariant| invariant.declaration)
                    .collect::<Vec<_>>();
                self.loops.push(LoopFrame {
                    id: *id,
                    invariant_declarations: invariant_declarations.clone().into_boxed_slice(),
                    scope_depth: outer_scope_depth,
                    counted_binder: Some(*binder),
                    invariant_atoms,
                    capture_path: Some(range_path.clone()),
                    breaks: Vec::new(),
                });
                let mut body_state = head.clone();
                let outer_continuing = std::mem::take(&mut body_state.continuing);
                let body_event = self.proof_event(FlowEventKind::S11, Some(node_path));
                let counted = self.establish_counted_body_entry(
                    node_path,
                    counted,
                    &mut body_state.facts,
                    body_event,
                );
                self.retain_counted_derivations(occurrence, counted);
                let body_falls_through = self.walk_block(body, &mut body_state);
                if body_falls_through {
                    Self::debug_assert_summarized(&body_state, &kills);
                }

                let mut step = vec![None; invariants.len()];
                let mut hidden_update = !body_falls_through;
                // A body reaching the backedge normally should still carry the
                // header binder's affine image. Where this walk has lost it,
                // the hidden `binder + 1` update is unproved and every
                // next-header target with it: [OWN-8]'s conservative reading,
                // which withholds the exhaustion rule and the step batch and
                // never widens acceptance.
                let current_binder = body_state.affine.values.get(binder).cloned();
                if body_falls_through && current_binder.is_none() {
                    step = vec![Some(TargetDisposition::Unproved); invariants.len()];
                }
                if let (true, Some(current_binder)) = (body_falls_through, current_binder) {
                    let next_binder = current_binder
                        .add(&AffineForm::constant(1), &mut AffineCheckState::new())
                        .ok();
                    let hidden_target = next_binder.as_ref().and_then(|next| {
                        AffineInequality::from_forms(
                            next,
                            &AffineForm::constant(u64::MAX as i128),
                            &mut AffineCheckState::new(),
                        )
                        .ok()
                    });
                    let counter_limit = self.terms.intern(TermKind::Constant(u64::MAX as i128));
                    hidden_update = hidden_target.as_ref().is_some_and(|target| {
                        self.prove(
                            ProofContext::new(&body_state.facts, &body_state.affine),
                            ProofGoal::Affine {
                                inequality: target,
                                right: Some(counter_limit),
                            },
                        )
                        .disposition
                            == ProofDisposition::Proved
                    });
                    // Normalize the next-header target with `binder :=
                    // binder_head + 1`, but retain the old header binding in
                    // the proof state.  The true-header S11 relation constrains
                    // `binder_head`; replacing the live binding first would
                    // make that exact old value unreachable while proving the
                    // backedge target.
                    let mut next_affine = body_state.affine.clone();
                    if let Some(next_binder) = next_binder {
                        next_affine.values.insert(*binder, next_binder);
                    }

                    for (index, invariant) in invariants.iter().enumerate() {
                        let next_target = self.checked_loop_invariant_inequality(
                            invariant,
                            &mut next_affine,
                            &mut AffineCheckState::new(),
                        );
                        // [INV-1] both bounds of an `==` next-header target
                        // are proved, in the same substituted state.
                        let next_partner = self
                            .checked_affine_relation_partner(
                                &invariant.relation,
                                &mut next_affine,
                                &mut AffineCheckState::new(),
                            )
                            .map(|partner| partner.ok());
                        let right = self.checked_affine_right_term(&invariant.relation.right);
                        let left = self.checked_affine_right_term(&invariant.relation.left);
                        let mut members = vec![(next_target, right, left)];
                        if let Some(partner) = next_partner {
                            members.push((partner, left, right));
                        }
                        let disposition = self.affine_target_disposition(
                            &members,
                            &body_state.facts,
                            &body_state.affine,
                        );
                        // An unrepresentable hidden update fails the step
                        // without refuting the target it would reach.
                        step[index] = Some(if hidden_update {
                            disposition
                        } else {
                            TargetDisposition::Unproved
                        });
                    }
                }

                self.record_loop_invariant_outcomes(*id, invariants, &base, &step, Some(*binder));
                let step_batch = step.iter().all(|disposition| {
                    disposition.is_none_or(|disposition| disposition == TargetDisposition::Proved)
                });
                let export = lower_le_upper && base_batch && step_batch && hidden_update;
                let frame = self.loops.pop();
                let mut breaks = frame.map(|frame| frame.breaks).unwrap_or_default();
                for break_state in &mut breaks {
                    Self::remove_active_loop_invariants(
                        &mut break_state.affine,
                        *id,
                        &invariant_declarations,
                    );
                }

                // Unlike an ordinary loop, the real false-header edge always
                // contributes. Binder and captures leave scope before it or
                // a matching break reaches the continuation.
                let mut exhaustion = head;
                if let Some(upper_affine) = &upper_affine
                    && export
                {
                    let mut normalized = exhaustion.affine.clone();
                    normalized.values.insert(*binder, upper_affine.clone());
                    for (source_ordinal, invariant) in invariants.iter().enumerate() {
                        let Some(inequality) = self.checked_loop_invariant_inequality(
                            invariant,
                            &mut normalized,
                            &mut AffineCheckState::new(),
                        ) else {
                            continue;
                        };
                        let partner = self
                            .checked_affine_relation_partner(
                                &invariant.relation,
                                &mut normalized,
                                &mut AffineCheckState::new(),
                            )
                            .and_then(Result::ok);
                        for inequality in std::iter::once(inequality).chain(partner) {
                            if !self.affine_fact_uses_only_outer_values(
                                &inequality,
                                &normalized,
                                *binder,
                            ) {
                                continue;
                            }
                            let fact = ActiveAffineFact {
                                inequality,
                                evidence: AffineFactEvidence::Source(
                                    SourceAffineFactRef::LoopInvariant(SourceLoopInvariantRef {
                                        loop_id: *id,
                                        source_ordinal: u32::try_from(source_ordinal)
                                            .expect("loop invariant ordinal exceeds u32"),
                                    }),
                                ),
                                active_loops: Vec::new(),
                            };
                            exhaustion.affine.facts.push(fact);
                        }
                    }
                }
                Self::remove_active_loop_invariants(
                    &mut exhaustion.affine,
                    *id,
                    &invariant_declarations,
                );
                self.exit_scopes_to(&mut exhaustion, outer_scope_depth);
                self.exit_counted_capture_scope(&mut exhaustion, &range_path);
                let mut exits = Vec::with_capacity(1 + breaks.len());
                exits.push(exhaustion);
                exits.extend(breaks);
                self.scopes.pop();
                *state = self.join_flows(&exits);
                record_continuing(&mut state.continuing, &outer_continuing);
                true
            }
        }
    }

    /// Walks one match arm from `entry`; establishes the arm-entry facts the
    /// scrutinee admits, applies the arm's scope-exit kills on fall-through,
    /// and returns the arm-exit state when the arm reaches the continuation.
    fn walk_arm(
        &mut self,
        arm: &CheckedMatchArm,
        entry: &ProofFlowState,
        facts: &ArmFacts,
        payload: Option<&PayloadPlacement>,
        result: Option<&ResultEvidence>,
    ) -> Option<ProofFlowState> {
        let mut state = entry.clone();
        let s1_event = (!facts.goals.is_empty() || facts.comparison.is_some())
            .then(|| self.proof_event(FlowEventKind::S1, facts.node_path.as_ref()));
        self.establish_arm_entry(arm, facts, &mut state.facts, s1_event);
        // [MSR-3] the payload placement's second half: on the arm whose
        // variant carries the payload, the binder that names it has the
        // measures the payload had before the consume.
        if let Some(payload) = payload {
            for (field, carry) in &payload.carried {
                let Some(binder) = arm.binders.iter().find(|binder| binder.field == *field) else {
                    continue;
                };
                let destination = self.bound_place(binder.binding);
                self.establish_measure_datums(
                    &binder.node_path,
                    destination,
                    carry,
                    &mut state.facts,
                );
            }
        }
        if arm.tag == 0
            && let Some(result) = result
            && let Some(binder) = arm
                .binders
                .iter()
                .find(|binder| binder.field == 0 && binder.mode == CheckedMode::Own)
        {
            self.select_result(
                &binder.node_path,
                result,
                binder.binding,
                binder.ty,
                &mut state,
            );
        }
        self.scopes
            .push(arm.binders.iter().map(|b| b.binding).collect());
        for binder in &arm.binders {
            if let CheckedType::Integer(ty) = binder.ty
                && matches!(binder.mode, CheckedMode::Own)
            {
                let value = self.new_affine_atom(ty);
                state.affine.values.insert(binder.binding, value);
            }
        }
        let mut continues = true;
        for statement in &arm.body {
            if !continues {
                break;
            }
            continues = self.walk_statement(statement, &mut state);
        }
        if continues {
            let depth = self.scopes.len() - 1;
            self.exit_scopes_to(&mut state, depth);
        }
        self.scopes.pop();
        continues.then_some(state)
    }

    fn establish_arm_entry(
        &mut self,
        arm: &CheckedMatchArm,
        facts: &ArmFacts,
        state: &mut FactState,
        event: Option<FlowEventId>,
    ) {
        if let Some(relation) = &facts.comparison {
            // Bool arms: tag 1 is `True()`, tag 0 is `False()`; the False
            // arm takes the exact negation [ENT-3].
            if arm.tag == 1 {
                state.establish(
                    relation,
                    &mut self.derivations,
                    event.expect("comparison arm has an S1 proof event"),
                );
            } else if arm.tag == 0 {
                state.establish(
                    &relation.negated(),
                    &mut self.derivations,
                    event.expect("comparison arm has an S1 proof event"),
                );
            }
        }
        for goal in &facts.goals {
            if arm.tag == 1 {
                state.establish_goal(
                    *goal,
                    GoalSign::Positive,
                    &mut self.derivations,
                    event.expect("goal arm has an S1 proof event"),
                );
                // [ENT-3] Signed Boolean decomposition of the established goal.
                self.establish_boolean_decomposition(
                    *goal,
                    GoalSign::Positive,
                    state,
                    event.expect("goal arm has an S1 proof event"),
                );
                self.record_boolean_decomposition(*goal, GoalSign::Positive, state);
            } else if arm.tag == 0 {
                state.establish_goal(
                    *goal,
                    GoalSign::Negative,
                    &mut self.derivations,
                    event.expect("goal arm has an S1 proof event"),
                );
                // [ENT-3] Signed Boolean decomposition of the established goal.
                self.establish_boolean_decomposition(
                    *goal,
                    GoalSign::Negative,
                    state,
                    event.expect("goal arm has an S1 proof event"),
                );
                self.record_boolean_decomposition(*goal, GoalSign::Negative, state);
            }
        }
    }

    // ------------------------------------------------------------------
    // Loop kill summary
    // ------------------------------------------------------------------

    /// Returns whether a block entry can reach the loop head whose summary is
    /// being built. `normal_reaches` describes the containing block's normal
    /// exit. This is structural reachability over [FN-1], not an executable
    /// constant-folding judgment.
    fn loop_block_reaches(
        &self,
        statements: &[CheckedStatement],
        normal_reaches: bool,
        reachability: &mut LoopReachability,
    ) -> bool {
        let mut reaches = normal_reaches;
        for statement in statements.iter().rev() {
            reaches = self.loop_statement_reaches(statement, reaches, reachability);
        }
        reaches
    }

    fn loop_statement_reaches(
        &self,
        statement: &CheckedStatement,
        normal_reaches: bool,
        reachability: &mut LoopReachability,
    ) -> bool {
        match statement {
            CheckedStatement::Let { .. }
            | CheckedStatement::DestructuringLet { .. }
            | CheckedStatement::PropagateLet { .. }
            | CheckedStatement::Set { .. }
            | CheckedStatement::Evaluate { .. }
            | CheckedStatement::DropExpression { .. }
            | CheckedStatement::Proof(_) => normal_reaches,
            CheckedStatement::Return { .. } => false,
            CheckedStatement::Give { .. } => reachability.gives.last().copied().unwrap_or(false),
            CheckedStatement::Break { target, .. } => reachability.break_reaches(*target),
            CheckedStatement::Match { arms, .. } => {
                let mut reaches = false;
                for arm in arms {
                    reaches |= self.loop_block_reaches(&arm.body, normal_reaches, reachability);
                }
                reaches
            }
            CheckedStatement::ValueMatchLet { arms, .. } => {
                // Arm fallthrough never reaches a value initializer's
                // continuation. Its `give` edges do, and nested value
                // initializers shadow this target while they are inspected.
                reachability.gives.push(normal_reaches);
                let mut reaches = false;
                for arm in arms {
                    reaches |= self.loop_block_reaches(&arm.body, false, reachability);
                }
                reachability.gives.pop();
                reaches
            }
            CheckedStatement::Loop { id, body, .. } => {
                // A nested loop body reaches its successor through its own
                // break edges, or can escape through another visible target.
                // A backedge alone cannot create reachability, so evaluating
                // the body with a false normal exit computes the least fixed
                // point. Once the body entry reaches the target, its normal
                // exit can take another iteration and eventually use that
                // same route.
                reachability.breaks.push((*id, normal_reaches));
                let body_reaches = self.loop_block_reaches(body, false, reachability);
                reachability.breaks.pop();
                // [FN-1] also keeps a conservative direct edge from the
                // nested loop statement to its normal successor. That edge
                // carries no event from inside the body.
                normal_reaches || body_reaches
            }
            CheckedStatement::CountedRange { id, body, .. } => {
                // The false-header edge reaches the normal successor, while
                // body fallthrough updates and returns to a header that may
                // then take that same edge. A matching break also reaches the
                // successor; enclosing exits retain their visible targets.
                reachability.breaks.push((*id, normal_reaches));
                let body_reaches = self.loop_block_reaches(body, normal_reaches, reachability);
                reachability.breaks.pop();
                normal_reaches || body_reaches
            }
        }
    }

    /// Collects exactly the kill events whose carrying edge can reach this
    /// loop's next head. The return value is the same structural entry
    /// reachability computed by [`Self::loop_block_reaches`].
    fn collect_continuing_loop_kills(
        &self,
        statements: &[CheckedStatement],
        normal_reaches: bool,
        reachability: &mut LoopReachability,
        kills: &mut LoopKills,
    ) -> bool {
        let mut reaches = normal_reaches;
        for statement in statements.iter().rev() {
            reaches =
                self.collect_continuing_statement_kills(statement, reaches, reachability, kills);
        }
        reaches
    }

    fn collect_continuing_statement_kills(
        &self,
        statement: &CheckedStatement,
        normal_reaches: bool,
        reachability: &mut LoopReachability,
        kills: &mut LoopKills,
    ) -> bool {
        match statement {
            CheckedStatement::Let { value, .. }
            | CheckedStatement::DestructuringLet { value, .. }
            | CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. }
            | CheckedStatement::PropagateLet {
                scrutinee: value, ..
            } => {
                if normal_reaches {
                    self.collect_loop_expression_kills(value, kills);
                }
                normal_reaches
            }
            CheckedStatement::Set {
                node_path,
                target,
                value,
                ..
            } => {
                if normal_reaches {
                    self.collect_set_kills(node_path, target, value, kills);
                }
                normal_reaches
            }
            CheckedStatement::Return { .. } => false,
            CheckedStatement::Give { value, .. } => {
                let reaches = reachability.gives.last().copied().unwrap_or(false);
                if reaches {
                    self.collect_loop_expression_kills(value, kills);
                }
                reaches
            }
            CheckedStatement::Break { target, .. } => reachability.break_reaches(*target),
            CheckedStatement::Proof(_) => normal_reaches,
            CheckedStatement::Match {
                scrutinee, arms, ..
            } => {
                let mut reaches = false;
                for arm in arms {
                    reaches |= self.collect_continuing_loop_kills(
                        &arm.body,
                        normal_reaches,
                        reachability,
                        kills,
                    );
                }
                if reaches {
                    self.collect_loop_expression_kills(scrutinee, kills);
                }
                reaches
            }
            CheckedStatement::ValueMatchLet {
                scrutinee, arms, ..
            } => {
                reachability.gives.push(normal_reaches);
                let mut reaches = false;
                for arm in arms {
                    reaches |=
                        self.collect_continuing_loop_kills(&arm.body, false, reachability, kills);
                }
                reachability.gives.pop();
                if reaches {
                    self.collect_loop_expression_kills(scrutinee, kills);
                }
                reaches
            }
            CheckedStatement::Loop { id, body, .. } => {
                reachability.breaks.push((*id, normal_reaches));
                let body_reaches = self.loop_block_reaches(body, false, reachability);
                self.collect_continuing_loop_kills(body, body_reaches, reachability, kills);
                reachability.breaks.pop();
                normal_reaches || body_reaches
            }
            CheckedStatement::CountedRange {
                id,
                lower,
                upper,
                body,
                ..
            } => {
                reachability.breaks.push((*id, normal_reaches));
                let body_reaches =
                    self.collect_continuing_loop_kills(body, normal_reaches, reachability, kills);
                reachability.breaks.pop();
                // Both endpoint atoms execute before either the real false
                // edge or a body path. Their own effects are continuing for
                // the enclosing target exactly when this statement can reach
                // that target through one of those successors.
                let reaches = normal_reaches || body_reaches;
                if reaches {
                    let mut events = Vec::new();
                    self.collect_expression_kills(lower, &mut events);
                    self.collect_expression_kills(upper, &mut events);
                    kills.push_event_group(events);
                }
                reaches
            }
        }
    }

    fn collect_loop_expression_kills(&self, expression: &CheckedExpression, kills: &mut LoopKills) {
        let mut events = Vec::new();
        self.collect_expression_kills(expression, &mut events);
        kills.push_event_group(events);
    }

    fn collect_set_kills(
        &self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        kills: &mut LoopKills,
    ) {
        let mut events = Vec::new();
        self.collect_expression_kills(value, &mut events);
        self.push_commit_kill(node_path, target, &mut events, kills);
    }

    fn push_commit_kill(
        &self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        events: &mut Vec<KillEvent>,
        kills: &mut LoopKills,
    ) {
        kills.set_bindings.insert(target.binding());
        events.push(self.commit_kill(node_path, target));
        kills.push_event_group(std::mem::take(events));
    }

    fn apply_loop_kills_one(
        &mut self,
        separations: &dyn SeparationOracle,
        state: &mut FactState,
        kills: &LoopKills,
    ) {
        self.materialize_before_event_kill(state, &kills.events);
        state.kill(|term| {
            kills
                .events
                .iter()
                .any(|event| self.event_kills_term(separations, term, event))
        });
        for event in &kills.events {
            self.kill_s12_candidates_for_event(separations, state, event);
        }
        state.kill_goals(|goal| {
            kills
                .events
                .iter()
                .any(|event| self.event_kills_goal(separations, goal, event))
        });
        state.goal_origins.retain(|binding, _| {
            !kills
                .events
                .iter()
                .any(|event| self.event_kills_goal_origin_binding(separations, *binding, event))
        });
        state
            .origins
            .retain(|binding, _| !kills.set_bindings.contains(binding));
        state
            .goal_origins
            .retain(|binding, _| !kills.set_bindings.contains(binding));
        state
            .ambiguous_goal_origins
            .retain(|binding| !kills.set_bindings.contains(binding));
    }

    fn apply_loop_kills(
        &mut self,
        states: &mut ProofFlowState,
        kills: &LoopKills,
        event: Option<FlowEventId>,
    ) {
        self.promote_flow_contradiction(states);
        // The header kills stand for the body's events on every iteration,
        // and an index this state proves live stays live at each of them
        // [WIN-2]: `r.len` falls only at an event that writes `r.last`,
        // `r.filled` or the whole window [OP-10], each of which kills every
        // fact below `r[i]` here because no ledger records `i != r.len - 1`,
        // and a write of `i` kills the fact through its offset support
        // [ENT-5]. A fact these kills leave therefore meets no such event.
        let ledger = states.separations.clone();
        let live = self.event_live_indices(states, &kills.events);
        let separations = EventSeparations {
            ledger: &ledger,
            live: &live,
        };
        self.kill_result_evidence(states, &kills.events);
        self.apply_loop_kills_one(&separations, &mut states.facts, kills);
        self.apply_affine_kills(&separations, &mut states.affine, &kills.events);
        states.record_writes(&kills.events);
        let mut groups = kills.entry_image_groups.iter().collect::<Vec<_>>();
        groups.sort_by(|left, right| left.owner.components().cmp(right.owner.components()));
        for group in groups {
            self.invalidate_entry_images(
                states,
                &separations,
                &kills.events[group.range.clone()],
                event,
            );
        }
    }

    // ------------------------------------------------------------------
    // Canonical rendering [ENT-6]
    // ------------------------------------------------------------------

    fn binding_name(&self, binding: BindingId) -> String {
        self.context
            .binding_names
            .get(binding.0 as usize)
            .cloned()
            .unwrap_or_else(|| "?".to_owned())
    }

    /// The source spelling of one declaration, such as a generic parameter.
    fn declaration_name(&self, declaration: crate::DeclarationId) -> String {
        self.context
            .declarations
            .get(declaration.index())
            .map_or_else(|| "?".to_owned(), |record| record.spelling().to_owned())
    }

    /// One [OP-4] subscript offset, in the spelling the source wrote it in.
    fn render_offset(&self, offset: CapturedValue) -> String {
        match offset.term {
            CapturedTerm::Literal(value) => value.to_string(),
            CapturedTerm::Binding(binding) => self.binding_name(binding),
            CapturedTerm::Const(declaration) => self.declaration_name(declaration),
            CapturedTerm::Opaque => "?".to_owned(),
        }
    }

    fn render_place(&self, place: &ResolvedPlace) -> String {
        let reference_root = matches!(place.root, PlaceRoot::Binding(binding)
            if self.places.is_reference(binding));
        let (mut rendered, mut ty) = match place.root {
            PlaceRoot::Binding(binding) => (
                {
                    // [REF-1, OP-15] a reference variable names a path and is
                    // not storage of its own, so the storage it names is
                    // reached only through `deref`. A term over a reference
                    // anchors at the binding and carries no step of its own,
                    // so the spelling the writer reads puts the step back.
                    let name = self.binding_name(binding);
                    if reference_root {
                        format!("deref({name})")
                    } else {
                        name
                    }
                },
                self.summary(binding).and_then(|summary| summary.ty),
            ),
            PlaceRoot::Constant(id) => (
                self.context
                    .constants
                    .get(id.0 as usize)
                    .map(|constant| constant.name.clone())
                    .unwrap_or_else(|| "?".to_owned()),
                self.context
                    .constants
                    .get(id.0 as usize)
                    .map(|constant| constant.ty),
            ),
        };
        for projection in &place.path {
            match projection {
                PlaceStep::Descendant(target) => {
                    rendered.push_str(".**");
                    ty = Some(target.ty);
                }
                PlaceStep::Payload { variant, field } => {
                    rendered.push_str(&format!(".{variant}.{field}"));
                    ty = None;
                }
                PlaceStep::Range(range) => {
                    rendered.push_str(&format!(
                        "[{}..{}]",
                        self.render_offset(range.start),
                        self.render_offset(range.end)
                    ));
                }
                PlaceStep::Part(part) => {
                    rendered.push('.');
                    rendered.push_str(part.spelling());
                    ty = None;
                }
                PlaceStep::Measure(measure) => {
                    rendered.push('.');
                    rendered.push_str(measure.spelling());
                    ty = Some(CheckedType::Integer(IntegerType::U64));
                }
                PlaceStep::Field(field) => {
                    let name = ty
                        .and_then(|current| self.field_name(current, *field))
                        .unwrap_or(None);
                    match name {
                        Some((field_name, field_ty)) => {
                            rendered.push('.');
                            rendered.push_str(&field_name);
                            ty = Some(field_ty);
                        }
                        None => {
                            rendered.push_str(".?");
                            ty = None;
                        }
                    }
                }
                PlaceStep::Index(offset) => {
                    rendered.push_str(&format!("[{}]", self.render_offset(*offset)));
                    ty = ty.and_then(|ty| element_type(ty, self.context.elements));
                }
                PlaceStep::Deref => {
                    self.render_content_step(&mut rendered, &mut ty);
                }
            }
        }
        rendered
    }

    /// References cannot be stored in aggregates. After the root reference
    /// step is consumed, a typed Box dereference spells its `inner` member.
    fn render_content_step(&self, rendered: &mut String, ty: &mut Option<CheckedType>) {
        let boxed = ty.is_some_and(|ty| {
            matches!(ty, CheckedType::Nominal(id)
                if self.context.nominals.get(id.0 as usize)
                    .is_some_and(|nominal| matches!(nominal.kind, CheckedNominalKind::Box { .. })))
        });
        if boxed {
            rendered.push_str(".inner");
        } else {
            *rendered = format!("deref({rendered})");
        }
        *ty = ty.and_then(|current| self.deref_type(current));
    }

    fn deref_type(&self, ty: CheckedType) -> Option<CheckedType> {
        let CheckedType::Nominal(id) = ty else {
            // Borrow bindings retain the referent type in checked form.
            return Some(ty);
        };
        let nominal = self.context.nominals.get(id.0 as usize)?;
        match nominal.kind {
            CheckedNominalKind::Box { referent, .. } => Some(referent),
            _ => Some(ty),
        }
    }

    #[allow(clippy::type_complexity)]
    fn field_name(&self, ty: CheckedType, field: u32) -> Option<Option<(String, CheckedType)>> {
        let CheckedType::Nominal(id) = ty else {
            return Some(None);
        };
        let nominal = self.context.nominals.get(id.0 as usize)?;
        let CheckedNominalKind::Struct { fields } = &nominal.kind else {
            return Some(None);
        };
        let field = fields.get(field as usize)?;
        Some(Some((field.name.clone(), field.ty)))
    }

    /// Renders one normalized relation for diagnostics.
    fn render_relation(&self, relation: &Relation) -> String {
        match relation {
            Relation::Bound { left, right, bound } => format!(
                "{} - {} <= {bound}",
                self.render_term(*left),
                self.render_term(*right)
            ),
            // An undisplaced relation reads as the writer wrote it; a
            // displaced one names its difference, exactly as a bound does.
            Relation::Equal {
                left,
                right,
                difference: 0,
            } => format!("{} = {}", self.render_term(*left), self.render_term(*right)),
            Relation::Equal {
                left,
                right,
                difference,
            } => format!(
                "{} - {} = {difference}",
                self.render_term(*left),
                self.render_term(*right)
            ),
            Relation::Distinct {
                left,
                right,
                difference: 0,
            } => format!(
                "{} != {}",
                self.render_term(*left),
                self.render_term(*right)
            ),
            Relation::Distinct {
                left,
                right,
                difference,
            } => format!(
                "{} - {} != {difference}",
                self.render_term(*left),
                self.render_term(*right)
            ),
        }
    }

    fn render_term(&self, term: TermId) -> String {
        match self.terms.kind(term) {
            TermKind::Zero => "0".to_owned(),
            TermKind::Constant(value) => value.to_string(),
            TermKind::ConstParameter(..) => "<const parameter>".to_owned(),
            TermKind::Place(place, _) => self.render_place(place),
            TermKind::Measure(measure, place) => {
                format!("{}.{}", self.render_place(place), measure.spelling())
            }
            TermKind::CountedCapture { side, .. } => match side {
                CountedCaptureSide::Lower => "<counted lower capture>".to_owned(),
                CountedCaptureSide::Upper => "<counted upper capture>".to_owned(),
            },
            TermKind::IndexCapture { .. } => "<captured index>".to_owned(),
            TermKind::ResultPayload(_) => "<success payload>".to_owned(),
            TermKind::CommitValue { .. } => "<assigned value>".to_owned(),
            TermKind::CallDatum { measure, .. } => measure.map_or_else(
                || "<argument value at the call>".to_owned(),
                |measure| format!("<argument {} at the call>", measure.spelling()),
            ),
            // [MSR-3] an entry datum is what the writer wrote: a measure of
            // the parameter, at the one state an `ensures` gives it.
            TermKind::EntryDatum {
                formal,
                projections,
                measure,
            } => {
                let mut place = self.function.parameters.get(*formal as usize).map_or_else(
                    || "?".to_owned(),
                    |parameter| {
                        if matches!(parameter.mode, CheckedMode::Reference) {
                            format!("entry({})", parameter.name)
                        } else {
                            parameter.name.clone()
                        }
                    },
                );
                for projection in projections {
                    match projection {
                        PlaceStep::Descendant(_) => place.push_str(".**"),
                        PlaceStep::Deref => place = format!("deref({place})"),
                        PlaceStep::Field(field) => {
                            place = format!("{place}.{field}");
                        }
                        PlaceStep::Payload { variant, field } => {
                            place = format!("{place}.{variant}.{field}");
                        }
                        PlaceStep::Index(offset) => {
                            place = format!("{place}[{}]", self.render_offset(*offset));
                        }
                        PlaceStep::Range(range) => {
                            place = format!(
                                "{place}[{}..{}]",
                                self.render_offset(range.start),
                                self.render_offset(range.end)
                            );
                        }
                        PlaceStep::Part(part) => {
                            place = format!("{place}.{}", part.spelling());
                        }
                        PlaceStep::Measure(measure) => {
                            place = format!("{place}.{}", measure.spelling());
                        }
                    }
                }
                format!("{place}.{}", measure.spelling())
            }
            // A measure datum has no source spelling of its own: it is the
            // measure the carried value had at the event that renamed it.
            TermKind::MeasureDatum {
                measure, placement, ..
            } => {
                let event = match placement {
                    MeasurePlacement::Rebind => "the rebind",
                    MeasurePlacement::Construct => "the construct",
                    MeasurePlacement::Destructuring => "the destructuring",
                    MeasurePlacement::Element => "the element position",
                    MeasurePlacement::Payload => "the payload",
                };
                format!("<{} at {event}>", measure.spelling())
            }
        }
    }

    /// One concrete [FN-8] call goal in the terms the source wrote it in.
    ///
    /// The structural dump this replaced published `Integer { operation:
    /// LessEqual, .. }(Place { root: BindingId(6), .. })`: a writer cannot find
    /// either half in their own program, and the blind-writer trial of
    /// 2026-08-28 recorded four rounds of readers failing to. [OP-4]
    /// already publishes its residual as source terms from the
    /// renderers below, so FN-8 publishes its goal from the same ones. The
    /// operation spellings come from the compiler's own exhaustive maps, which
    /// `semantic::tests::operation_table` locks against the specification.
    pub(super) fn render_concrete_goal(&self, expression: &GoalExpression) -> String {
        match expression {
            GoalExpression::Datum(datum) => self.render_goal_datum(datum),
            GoalExpression::Operation { row, arguments, .. } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.render_concrete_goal(argument))
                    .collect::<Vec<_>>();
                render_goal_row(row, &arguments, self.context.declarations)
            }
        }
    }

    fn render_goal_datum(&self, datum: &GoalDatum) -> String {
        match datum {
            // A concrete goal has no formal left in it, but a template
            // rendered through this path names the position the formal holds.
            GoalDatum::Parameter {
                ordinal,
                projections,
                ..
            } => self.render_goal_projections(format!("parameter #{ordinal}"), None, projections),
            GoalDatum::NamedConst {
                declaration,
                projections,
                ..
            } => {
                let (name, ty) = self.context.constant(*declaration).map_or_else(
                    || ("?".to_owned(), None),
                    |constant| (constant.name.clone(), Some(constant.ty)),
                );
                self.render_goal_projections(name, ty, projections)
            }
            GoalDatum::Place {
                root, projections, ..
            } => {
                let base = self.binding_name(*root);
                let ty = self.summary(*root).and_then(|summary| summary.ty);
                // [REF-1, OP-15] a reference variable names a path and is not
                // storage of its own, so every place that goes through one is
                // written under a `deref` step: `deref(p)`, `deref(p).field`,
                // `deref(part).len`. A term rooted at a reference anchors at
                // that binding and carries no step of its own — the parameter
                // name *is* the path inside the body — so rendering puts that
                // source wrapper back while retaining every concrete step
                // below the referent.
                let reference = self.places.is_reference(*root);
                let base = if reference {
                    format!("deref({base})")
                } else {
                    base
                };
                self.render_goal_projections(base, ty, projections)
            }
            // Source cannot name this datum: render its structural source
            // role rather than inventing an expression that could reread a
            // different runtime value.
            GoalDatum::EvaluatedValue {
                occurrence,
                captured_type,
                projections,
                ..
            } => {
                let base = match occurrence {
                    // [DIAG-1] fixes this spelling for an FN-8 payload.
                    EvaluatedValueOccurrence::CallArgument { argument, .. } => {
                        format!("argument #{argument} pre-transfer value")
                    }
                    EvaluatedValueOccurrence::ObligationOperand { operand, .. } => {
                        format!("<operand #{operand} evaluated value>")
                    }
                };
                self.render_goal_projections(base, Some(*captured_type), projections)
            }
            // A literal renders as the source spelling that denotes it
            // [FORM-5]: `unit`, a `Bool` variant constructor, a suffixed
            // integer, or a float's canonical literal.
            GoalDatum::Literal(value) => match value {
                CheckedValue::Integer { ty, bits } => {
                    format!("{}_{}", integer_value(*ty, *bits), integer_type_name(*ty))
                }
                CheckedValue::Float { ty, bits } => {
                    super::super::check::floats::float_value_spelling(*ty, *bits)
                }
                CheckedValue::Unit => "unit".to_owned(),
                CheckedValue::Bool(true) => "True()".to_owned(),
                CheckedValue::Bool(false) => "False()".to_owned(),
                // [CONST-1] a const generic is named by its parameter, and a
                // generic-numeric identity is `0_T` or `1_T` [FORM-5].
                CheckedValue::ConstGeneric { declaration, .. } => {
                    self.declaration_name(*declaration)
                }
                CheckedValue::NumericIdentity {
                    ty:
                        CheckedType::GenericInt(declaration) | CheckedType::GenericFloat(declaration),
                    one,
                } => format!("{}_{}", u8::from(*one), self.declaration_name(*declaration)),
                other => format!("{other:?}"),
            },
        }
    }

    fn render_goal_projections(
        &self,
        base: String,
        root_type: Option<CheckedType>,
        projections: &[GoalProjection],
    ) -> String {
        let mut rendered = base;
        let mut ty = root_type;
        for projection in projections {
            match projection {
                GoalProjection::Deref => {
                    self.render_content_step(&mut rendered, &mut ty);
                }
                GoalProjection::Field(field) => {
                    match ty
                        .and_then(|current| self.field_name(current, *field))
                        .unwrap_or(None)
                    {
                        Some((name, field_type)) => {
                            rendered.push('.');
                            rendered.push_str(&name);
                            ty = Some(field_type);
                        }
                        None => {
                            rendered.push_str(".?");
                            ty = None;
                        }
                    }
                }
                GoalProjection::Payload { variant, field } => {
                    let selected = ty.and_then(|current| {
                        let CheckedType::Nominal(nominal) = current else {
                            return None;
                        };
                        let CheckedNominalKind::Enum { variants } =
                            &self.context.nominals.get(nominal.0 as usize)?.kind
                        else {
                            return None;
                        };
                        let variant = variants.iter().find(|item| item.tag == *variant)?;
                        let field = variant.fields.get(*field as usize)?;
                        Some((&variant.name, &field.name, field.ty))
                    });
                    if let Some((variant, field, field_type)) = selected {
                        rendered.push_str(&format!(".{variant}.{field}"));
                        ty = Some(field_type);
                    } else {
                        rendered.push_str(&format!(".{variant}.{field}"));
                        ty = None;
                    }
                }
                GoalProjection::Subscript(offset) => {
                    rendered.push_str(&format!("[{}]", self.render_offset(*offset)));
                    ty = ty.and_then(|ty| element_type(ty, self.context.elements));
                }
                // [REF-4] the range the actual formed, spelled exactly as it
                // was written: the range names no binding, so its two
                // endpoints are what identifies it to the writer.
                GoalProjection::Range(range) => {
                    rendered.push_str(&format!(
                        "[{}..{}]",
                        self.render_offset(range.start),
                        self.render_offset(range.end)
                    ));
                    ty = ty.and_then(|ty| element_type(ty, self.context.elements));
                }
                GoalProjection::FormalSubscript { ordinal } => {
                    rendered.push_str(&format!("[parameter #{ordinal}]"));
                    ty = ty.and_then(|ty| element_type(ty, self.context.elements));
                }
            }
        }
        rendered
    }

    /// Render the checked storage path without reducing subscript offsets
    /// to overlap identities: even a non-term offset retains its source
    /// expression in an obligation's residual.
    fn render_storage_place(&self, root: &CheckedContainerRoot) -> String {
        let mut rendered = self.render_place(&ResolvedPlace {
            root: root.root,
            path: Vec::new(),
        });
        let mut ty = match root.root {
            PlaceRoot::Binding(binding) => self.summary(binding).and_then(|summary| summary.ty),
            PlaceRoot::Constant(id) => self
                .context
                .constants
                .get(id.0 as usize)
                .map(|value| value.ty),
        };
        for step in &root.path {
            match step {
                CheckedPlaceStep::Field(field) => {
                    if let Some((name, selected)) =
                        ty.and_then(|ty| self.field_name(ty, *field)).flatten()
                    {
                        rendered.push('.');
                        rendered.push_str(&name);
                        ty = Some(selected);
                    } else {
                        rendered.push_str(".?");
                        ty = None;
                    }
                }
                CheckedPlaceStep::BoxReferent(nominal) => {
                    rendered.push_str(".inner");
                    ty = self.deref_type(CheckedType::Nominal(*nominal));
                }
                CheckedPlaceStep::Subscript(index) => {
                    rendered.push_str(&format!("[{}]", self.render_expression(&index.offset)));
                    ty = Some(index.element_type);
                }
            }
        }
        rendered
    }

    fn render_expression(&self, expression: &CheckedExpression) -> String {
        match expression {
            CheckedExpression::Constant(CheckedValue::ConstGeneric { .. }) => {
                "<const parameter>".to_owned()
            }
            CheckedExpression::Constant(CheckedValue::Integer { ty, bits }) => {
                format!("{}_{}", integer_value(*ty, *bits), integer_type_name(*ty))
            }
            CheckedExpression::NamedConstant {
                value: CheckedValue::Integer { ty, bits },
                ..
            } => format!("{}_{}", integer_value(*ty, *bits), integer_type_name(*ty)),
            CheckedExpression::Binding { binding, .. } => self.binding_name(*binding),
            // [OP-15, MSR-1] a measure is read as a member of the measured
            // place, so a residual naming one spells it `p.len` and never as
            // a call of a reader row [FORM-1].
            CheckedExpression::BufferMeasure { measure, root } => format!(
                "{}.{}",
                self.render_place(&ResolvedPlace::from_path(root.binding, root.place_path())),
                measure.spelling(),
            ),
            CheckedExpression::RangeMeasure { measure, root } => format!(
                "{}.{}",
                self.render_place(&ResolvedPlace::spelled(
                    PlaceRoot::Binding(root.binding),
                    self.is_holder(root.binding),
                    Vec::new()
                )),
                measure.spelling(),
            ),
            CheckedExpression::RangeElementMeasure { measure, place, .. } => {
                let mut resolved = ResolvedPlace::spelled(
                    PlaceRoot::Binding(place.root.binding),
                    self.is_holder(place.root.binding),
                    Vec::new(),
                );
                resolved.path.extend(place.place_path());
                format!("{}.{}", self.render_place(&resolved), measure.spelling())
            }
            CheckedExpression::ArrayMeasure { measure, root, .. } => format!(
                "{}.{}",
                self.render_place(&self.array_root_place(root)),
                measure.spelling(),
            ),
            CheckedExpression::ContainerMeasure { measure, root } => {
                format!("{}.{}", self.render_storage_place(root), measure.spelling(),)
            }
            CheckedExpression::Project {
                binding, fields, ..
            } => self.render_place(&ResolvedPlace::spelled(
                PlaceRoot::Binding(*binding),
                false,
                fields.clone(),
            )),
            CheckedExpression::DerefAddressed { binding, .. } => {
                format!("deref({})", self.binding_name(*binding))
            }
            CheckedExpression::BoxDeref { value, .. } => {
                format!("{}.inner", self.render_expression(value))
            }
            CheckedExpression::ProjectValue {
                value,
                nominal,
                field,
                ..
            } => {
                let field_name = self
                    .context
                    .nominals
                    .get(nominal.0 as usize)
                    .and_then(|nominal| match &nominal.kind {
                        CheckedNominalKind::Struct { fields } => {
                            fields.get(*field as usize).map(|field| field.name.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| "?".to_owned());
                format!("{}.{field_name}", self.render_expression(value))
            }
            CheckedExpression::ArrayIndex { root, offset, .. } => {
                let base = self.array_root_place(root);
                format!(
                    "{}[{}]",
                    self.render_place(&base),
                    self.render_expression(offset)
                )
            }
            CheckedExpression::ReadStorage { root, .. } => self.render_storage_place(root),
            CheckedExpression::BufferIndex { root, offset, .. } => {
                let base = ResolvedPlace::from_path(root.binding, root.place_path());
                format!(
                    "{}[{}]",
                    self.render_place(&base),
                    self.render_expression(offset)
                )
            }
            CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => {
                let mut base = ResolvedPlace::spelled(
                    PlaceRoot::Binding(place.root.binding),
                    self.is_holder(place.root.binding),
                    Vec::new(),
                );
                base.path.extend(place.place_path());
                self.render_place(&base)
            }
            _ => "?".to_owned(),
        }
    }
}

fn normalize_distinct_requests(requests: &mut [BoundsRequest]) {
    for request in requests {
        if request.distinct
            && let Some(left) = request.left
            && request.right < left
        {
            request.left = Some(request.right);
            request.right = left;
        }
    }
}

fn request_relation(request: &BoundsRequest) -> Option<Relation> {
    let left = request.left?;
    Some(if request.distinct {
        Relation::Distinct {
            left,
            right: request.right,
            difference: 0,
        }
    } else {
        Relation::Bound {
            left,
            right: request.right,
            bound: request.bound,
        }
    })
}

fn checked_integer_constant(expression: &CheckedExpression) -> Option<i128> {
    match expression {
        CheckedExpression::Constant(CheckedValue::Integer { ty, bits })
        | CheckedExpression::NamedConstant {
            value: CheckedValue::Integer { ty, bits },
            ..
        } => Some(integer_value(*ty, *bits)),
        _ => None,
    }
}

/// A let-origin expansion is valid only while the bound value has no `set`
/// target on the path to its use. The target's projection does not narrow
/// this invalidation: changing one field or element invalidates the aggregate
/// value identity even when a separately established length fact survives.
fn invalidate_goal_origin_for_set(state: &mut FactState, target: &CheckedSetTarget) {
    state.goal_origins.remove(&target.binding());
    state.ambiguous_goal_origins.remove(&target.binding());
}

/// The type one slot of an indexable base holds [OP-4, WIN-1].
fn element_type(input: CheckedType, elements: &[CheckedType]) -> Option<CheckedType> {
    match input {
        CheckedType::Buffer { element } => elements.get(element.index()).copied(),
        CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => {
            elements.get(element.0 as usize).copied()
        }
        _ => None,
    }
}

/// The place one element write names [MSR-2]: the base it selects through,
/// with the element it writes appended.
///
/// [MSR-2] states the granularity over storage: a write at an element
/// position of P overlaps the descriptor storage of `P[i]` and none of P's
/// own, so the event carries `P[i]` and the overlap relation reads it.
fn element_write_place(mut base: ResolvedPlace, offset: CapturedValue) -> ResolvedPlace {
    base.path.push(PlaceStep::Index(offset));
    base
}

/// One goal operation applied to already-rendered operands, in the spelling
/// the source uses for that row.
///
/// An operation whose [OP-1] spelling is a call name renders as a call; the
/// arithmetic rows, whose only spelling is the infix operator [GRAM-6], render
/// as the infix expression a writer would have to write.
fn render_goal_row(
    row: &GoalOperation,
    arguments: &[String],
    declarations: &[crate::DeclarationRecord],
) -> String {
    match row {
        GoalOperation::Integer { operation, .. } => {
            render_operation_spelling(operation.spelling(), arguments)
        }
        GoalOperation::Float { operation, .. } => {
            render_operation_spelling(operation.spelling(), arguments)
        }
        GoalOperation::Boolean(operation) => {
            render_operation_spelling(operation.spelling(), arguments)
        }
        GoalOperation::EnumEquality { equal, .. } => {
            render_operation_spelling(if *equal { "eeq" } else { "ene" }, arguments)
        }
        GoalOperation::NumericConversion {
            mode,
            source,
            destination,
        } => format!(
            "{}::<{}, {}>({})",
            match mode {
                CheckedConversionMode::Exact => "cvt",
                CheckedConversionMode::Checked => "cvt.checked",
                CheckedConversionMode::Defined => "cvt.defined",
            },
            numeric_type_name(*source, declarations),
            numeric_type_name(*destination, declarations),
            arguments.join(", ")
        ),
        GoalOperation::Reinterpret {
            source,
            destination,
        } => format!(
            "reinterpret::<{}, {}>({})",
            numeric_type_name(*source, declarations),
            numeric_type_name(*destination, declarations),
            arguments.join(", ")
        ),
        // [OP-15, MSR-1]: one quantity, one name, term and reader alike, and
        // the name is the member spelling `p.len` of the measured place. The
        // v0.59 `len_of(p)` former is not a v0.60 spelling.
        GoalOperation::ArrayMeasure { measure, .. }
        | GoalOperation::BufferMeasure { measure, .. }
        | GoalOperation::ContainerMeasure { measure, .. } => match arguments {
            [place] => format!("{place}.{}", measure.spelling()),
            _ => "<invalid measure goal>".to_owned(),
        },
        GoalOperation::ArrayIndex { .. }
        | GoalOperation::BufferIndex { .. }
        | GoalOperation::RunIndex { .. } => match arguments {
            [collection, offset] => format!("{collection}[{offset}]"),
            _ => "<invalid index goal>".to_owned(),
        },
        GoalOperation::BufferFits { .. } => render_operation_spelling("buffer_fits", arguments),
    }
}

/// A call spelling renders `name(a, b)`; an operator spelling renders the
/// binary infix form, which is the only form [GRAM-6] admits for those rows.
fn render_operation_spelling(spelling: &str, arguments: &[String]) -> String {
    let infix = !spelling.starts_with(|first: char| first.is_ascii_alphabetic());
    match (infix, arguments) {
        (true, [left, right]) => format!("{left} {spelling} {right}"),
        _ => format!("{spelling}({})", arguments.join(", ")),
    }
}

fn numeric_type_name(ty: CheckedNumericType, declarations: &[crate::DeclarationRecord]) -> String {
    match ty {
        CheckedNumericType::Integer(integer) => integer_type_name(integer).to_owned(),
        CheckedNumericType::Float(FloatType::F32) => "f32".to_owned(),
        CheckedNumericType::Float(FloatType::F64) => "f64".to_owned(),
        CheckedNumericType::GenericInteger(declaration)
        | CheckedNumericType::GenericFloat(declaration) => {
            declarations.get(declaration.index()).map_or_else(
                || format!("<type-parameter:{}>", declaration.index()),
                |record| record.spelling().to_owned(),
            )
        }
    }
}

const fn integer_type_name(ty: IntegerType) -> &'static str {
    match ty {
        IntegerType::I8 => "i8",
        IntegerType::I16 => "i16",
        IntegerType::I32 => "i32",
        IntegerType::I64 => "i64",
        IntegerType::U8 => "u8",
        IntegerType::U16 => "u16",
        IntegerType::U32 => "u32",
        IntegerType::U64 => "u64",
    }
}

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
            waiting: crate::semantic::model::CheckedWaiting::default(),
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
        let goal = analyzer.goals.intern(
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
                    let established = analyzer.derivations.event(FlowEventKind::S1, None);
                    facts.establish_goal(goal, sign, &mut analyzer.derivations, established);
                    analyzer.apply_kills_one(&separations, &mut facts, &[event]);
                    let closed = close(
                        &facts,
                        &analyzer.terms,
                        &analyzer.goals,
                        &mut analyzer.derivations,
                    );
                    assert_eq!(
                        closed.derives_goal(goal, sign, &analyzer.goals),
                        binding == unrelated,
                        "event must remove exactly the goals that read its binding"
                    );
                }
                let mut facts = FactState::new();
                let established = analyzer.derivations.event(FlowEventKind::S1, None);
                facts.establish_goal(goal, sign, &mut analyzer.derivations, established);
                let exited = HashSet::from([binding]);
                materialize_closure_before_kill(
                    &mut facts,
                    &analyzer.terms,
                    &analyzer.goals,
                    &mut analyzer.derivations,
                );
                facts.kill_goals(|candidate| analyzer.scope_kills_goal(candidate, &exited));
                let closed = close(
                    &facts,
                    &analyzer.terms,
                    &analyzer.goals,
                    &mut analyzer.derivations,
                );
                assert_eq!(
                    closed.derives_goal(goal, sign, &analyzer.goals),
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
            waiting: crate::semantic::model::CheckedWaiting::default(),
            entailment: FunctionEntailment::default(),
        };
        let mut analyzer = Analyzer::new(&context, &function);
        analyzer.places = PlaceMap::for_function(&function);
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
            analyzer.file_range_image(&carrier, shared, &endpoint(4), &endpoint(4), &mut affine);
            assert!(
                affine.ranges.is_empty(),
                "a capture shared by formations files no image: {:?}",
                affine.ranges
            );
            let own = range(20, 1, 3);
            analyzer.file_range_image(&carrier, own, &endpoint(1), &endpoint(3), &mut affine);
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
            analyzer.argument_referents(&direct),
            vec![(place(vec![PlaceStep::Range(inner)]), false)]
        );
        let reslice = formation(view_source(), inner, 1, 3);
        assert_eq!(
            analyzer.argument_referents(&reslice),
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
            analyzer.collect_view_write_kills(argument, &call, &mut events);
            let [event] = events.as_slice() else {
                panic!("one written range names one event: {events:?}");
            };
            let mut selected = prefix.clone();
            selected.push(PlaceStep::Index(literal(30, 1)));
            let mut own = prefix;
            own.push(PlaceStep::Range(inner));
            let [selected, origin, own] = [selected, Vec::new(), own].map(|path| {
                analyzer
                    .terms
                    .intern(TermKind::Measure(CheckedMeasure::Length, place(path)))
            });
            assert!(
                analyzer.event_kills_term(&separations, selected, event),
                "the write may replace the selected element, so its measure dies"
            );
            assert!(
                !analyzer.event_kills_term(&separations, origin, event),
                "an element write reaches no measure of the origin place [CALL-3]"
            );
            assert!(
                !analyzer.event_kills_term(&separations, own, event),
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
