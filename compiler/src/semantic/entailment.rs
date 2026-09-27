//! The combined entailment fragment [ENT-1..ENT-6]: a closed, deterministic,
//! search-free derivation system over L0 difference bounds and finite exact
//! signed goals.
//!
//! The engine is acceptance-bearing: [`analyze_function`] computes the
//! closed fact state along the [FN-1] structural graph and judges every
//! obligation it reaches, and [`answer_records`] answers each obligation
//! record the checker formed with the judgment that decided it. The checker
//! accepts a function exactly when every record is answered and discharged,
//! and retains the complete summary on the checked function [DIAG-2].
//!
//! Judgments are per function body [ENT-2]; the [ENT-3] S4 `requires`
//! relation is the one fact that enters from outside the body, and no fact
//! crosses a call boundary.
//!
//! Implemented fact sources: S1 branch and match facts with both
//! comparison-origin shapes, S4 requires facts, S5 binding and post-SET-1
//! copy/conversion equalities, S6 length facts, S7 operation facts, S9
//! const-array element ranges, S11 counted-range facts, and the S12 and S13
//! call publication sources. An absent source only under-derives, which is
//! the version-monotone direction [ENT-1].

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) mod affine;
mod flow;
mod polynomial;
mod state;
mod term;

pub(crate) use state::DerivationId;
#[cfg(test)]
pub(crate) use state::GoalId;
pub(crate) use state::SourceAffineFactRef;
use state::{DerivationInventory, DerivationLedger};
#[cfg(not(test))]
use term::TermId;

#[cfg(test)]
pub(crate) use state::{
    CountedRootAtom, DerivationNode, DerivationRootKind, FlowEvent, FlowEventId, FlowEventKind,
    GoalSign, ImplicitBoundKind, JoinParent, PostconditionCallDetail,
    PostconditionDeliveryJoinDetail, RangeSeparationOrdering, Relation,
};
#[cfg(test)]
pub(crate) use term::{
    CountedCaptureSide, MeasureBound, PlaceRoot, TermId, TermKind, ZERO, type_range,
};

use std::collections::{BTreeSet, HashMap};

use super::goal::{ConcreteGoal, GoalExpression};
use super::model::{
    BindingId, CheckedConstant, CheckedConstantId, CheckedExpression, CheckedFunction,
    CheckedIntegerOperation, CheckedLoopId, CheckedMode, CheckedNominal, CheckedSetTarget,
    CheckedStatement, CheckedType, FunctionId, IntegerType,
};
use super::obligations::{ObligationSubject, RecordAnswer};
use super::postcondition::CheckedPostcondition;
use crate::{DeclarationId, NodePath};

/// The transport one call selects for one declared parameter [CALL-1,
/// CALL-2, CALL-3, CALL-5].
///
/// [CALL-5] fixes the selector: the callee's declared parameter mode and
/// type, and its declared contract, and nothing else. The argument
/// expression's shape, the callee's name, and every summary derived from the
/// callee's body are not selectors, so this value is computed once per
/// declared parameter and is the same at every call site of it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CallTransport {
    /// [CALL-1] A reference parameter whose declared row carries no `writes`
    /// of any path rooted at it. [EFF-2]'s both-ways check then lets no
    /// `writes` occurrence project onto the actual's place, so the call is a
    /// kill event for no fact supported by it.
    ReadOnlyReference,
    /// [CALL-2] A value delivered at an `own` parameter. An affine or linear
    /// actual is a consuming use, which kills every fact whose support
    /// contains that actual's root [ENT-5](c); a copy actual is a duplicate
    /// and kills nothing. Either way the result carries exactly the callee's
    /// declared relations.
    Value,
    /// [CALL-3] A range reference parameter `&[T]` [REF-4]: a projected
    /// write reaches the range's storage, which for an element type with
    /// descriptor storage of its own includes that element's measures, and
    /// reaches no measure of the origin place itself nor of the range
    /// reference.
    ViewedRange,
    /// [CALL-5] No transport is selected, so a projected write kills
    /// conservatively: an ordinary descriptor-storage-overlapping [ENT-5]
    /// event [MSR-2].
    #[default]
    Conservative,
}

impl CallTransport {
    /// The transport a declared parameter selects, read from its declared
    /// reference kind and from whether its own declared row writes any path
    /// rooted at it [CALL-5].
    ///
    /// [CALL-1] is keyed on the row rather than on a mode, because v0.60 has
    /// one reference kind and no shared/unique spelling: a reference the row
    /// only reads keeps every fact, and a reference the row writes kills
    /// conservatively. `&[T]` is [CALL-3]'s range reference whatever its row
    /// carries, its own rule classifying how far such a write reaches.
    pub(crate) const fn of_declaration(mode: CheckedMode, row_writes_here: bool) -> Self {
        match mode {
            CheckedMode::Range => Self::ViewedRange,
            CheckedMode::Own => Self::Value,
            CheckedMode::Reference if !row_writes_here => Self::ReadOnlyReference,
            CheckedMode::Reference => Self::Conservative,
        }
    }

    /// Whether a write projected through this transport reaches element
    /// storage only, so it kills the measures of the written elements and
    /// none of the origin place's own [CALL-3, MSR-2].
    pub(crate) const fn writes_element_storage(self) -> bool {
        matches!(self, Self::ViewedRange)
    }
}

/// Kill-relevant [EFF-2] projection of one callee signature: for each
/// parameter, whether the callee's declared effect row writes the region that
/// parameter carries, so a call kills exactly the facts whose support
/// overlaps that actual's resolved place [ENT-5](b), and which transport
/// [CALL-5] selects for that parameter, which is what fixes how far such a
/// write reaches [CALL-1, CALL-2, CALL-3].
#[derive(Clone, Debug, Default)]
pub(crate) struct EntailmentCallee {
    pub(crate) parameter_declarations: Vec<crate::DeclarationId>,
    pub(crate) parameter_modes: Vec<CheckedMode>,
    /// Per parameter, the `epsuffix*` of every declared `writes` entry
    /// rooted at it [EFF-1]. The steps are complete: [EFF-5] substitutes the
    /// path and [OWN-7] compares the result, so a truncated step list would
    /// under-approximate the write.
    pub(crate) parameter_writes: Vec<Vec<Vec<super::model::CheckedEffectStep>>>,
    pub(crate) parameter_transports: Vec<CallTransport>,
}

impl EntailmentCallee {
    /// Derives the projection from one callee's parameter modes, declared
    /// types, and declared `writes` regions. A row with no `writes` kills
    /// nothing; a written region reached only through a `&uniq` actual kills
    /// through exactly that actual, and the transport that parameter's
    /// declaration selects fixes whether such a write reaches the origin's
    /// descriptor storage or only a viewed range's element storage.
    pub(crate) fn from_signature(
        parameters: impl Iterator<Item = (crate::DeclarationId, CheckedMode, CheckedType)>,
        writes: &[super::model::CheckedStatePath],
    ) -> Self {
        let parameters = parameters.collect::<Vec<_>>();
        Self {
            parameter_declarations: parameters
                .iter()
                .map(|(declaration, _, _)| *declaration)
                .collect(),
            parameter_writes: parameters
                .iter()
                .map(|(declaration, _, _)| {
                    writes
                        .iter()
                        .filter(|path| path.root == *declaration)
                        .map(|path| path.steps.clone())
                        .collect()
                })
                .collect(),
            parameter_transports: parameters
                .iter()
                .map(|(declaration, mode, _)| {
                    let writes_here = writes.iter().any(|path| path.root == *declaration);
                    CallTransport::of_declaration(*mode, writes_here)
                })
                .collect(),
            parameter_modes: parameters.into_iter().map(|(_, mode, _)| mode).collect(),
        }
    }
}

/// Program-level context the per-function analysis reads.
pub(crate) struct EntailmentContext<'check> {
    /// Existing resolved declarations, borrowed for source names in residuals.
    pub(crate) declarations: &'check [crate::DeclarationRecord],
    /// Callee projections indexed by [`FunctionId`].
    pub(crate) callees: &'check [EntailmentCallee],
    pub(crate) constants: &'check [CheckedConstant],
    /// Source declaration identity to dense checked-constant identity. Goal
    /// equality keeps the former while L0 projection reads the latter's
    /// mathematical value.
    pub(crate) constant_ids: &'check HashMap<DeclarationId, CheckedConstantId>,
    /// Written integer types of symbolic const parameters [MSR-6]. Extents
    /// and forwarded arguments carry the original declaration identity, even
    /// when their use has a different integer type.
    pub(crate) const_parameter_types: &'check HashMap<DeclarationId, super::model::IntegerType>,
    pub(crate) nominals: &'check [CheckedNominal],
    pub(crate) elements: &'check [CheckedType],
    /// Accepted instantiated FN-4 implications. Bound-call evidence refers
    /// to these records by checked-program-private index and never imports a
    /// query-local term, goal, or derivation identity.
    pub(crate) contract_queries: &'check [super::model::CheckedContractQuery],
    /// Published earlier-component FN-9 declarations and proofs, indexed by
    /// concrete [`FunctionId`]. Same-component entries remain absent until
    /// the component's atomic publication boundary.
    pub(crate) verified_postconditions: &'check [Vec<&'check CheckedPostcondition>],
    pub(crate) verified_postcondition_proofs: &'check [Vec<&'check FunctionPostconditionProof>],
    /// Binding names in dense [`super::model::BindingId`] order, for the
    /// [ENT-6] canonical residual rendering.
    pub(crate) binding_names: &'check [String],
}

impl EntailmentContext<'_> {
    pub(crate) fn callee(&self, function: FunctionId) -> Option<&EntailmentCallee> {
        self.callees.get(function.0 as usize)
    }

    pub(crate) fn constant(&self, declaration: DeclarationId) -> Option<&CheckedConstant> {
        let id = self.constant_ids.get(&declaration)?;
        self.constants.get(id.0 as usize)
    }

    pub(crate) fn contract_query(
        &self,
        query: super::model::ContractQueryId,
    ) -> Option<&super::model::CheckedContractQuery> {
        self.contract_queries.get(query.0 as usize)
    }

    pub(crate) fn constant_declaration(
        &self,
        constant: CheckedConstantId,
    ) -> Option<DeclarationId> {
        self.constant_ids
            .iter()
            .find_map(|(declaration, id)| (*id == constant).then_some(*declaration))
    }

    pub(crate) fn verified_postconditions(
        &self,
        function: FunctionId,
    ) -> Option<Vec<(&CheckedPostcondition, &FunctionPostconditionProof)>> {
        let postconditions = self.verified_postconditions.get(function.0 as usize)?;
        let proofs = self
            .verified_postcondition_proofs
            .get(function.0 as usize)?;
        if postconditions.len() != proofs.len() {
            return None;
        }
        postconditions
            .iter()
            .copied()
            .zip(proofs.iter().copied())
            .enumerate()
            .map(|(ordinal, (postcondition, proof))| {
                let summary = proof.summary.as_ref()?;
                (summary.function == function
                    && summary.relation_ordinal == u32::try_from(ordinal).ok()?)
                .then_some((postcondition, proof))
            })
            .collect()
    }

    pub(crate) fn verified_postcondition(
        &self,
        function: FunctionId,
        relation_ordinal: u32,
    ) -> Option<(&CheckedPostcondition, &FunctionPostconditionProof)> {
        self.verified_postconditions(function)?
            .into_iter()
            .find(|(_, proof)| {
                proof.summary.as_ref().is_some_and(|summary| {
                    summary.function == function && summary.relation_ordinal == relation_ordinal
                })
            })
    }
}

/// The [ENT-6] obligation family one outcome belongs to.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ObligationFamily {
    /// A subscript bounds obligation `i < P.len` [OP-4, WIN-1].
    Bounds,
    /// One canonical `.defined` goal for a proof-required exact integer
    /// operation [OP-2, ENT-6].
    IntegerDomain,
    /// One exact numeric conversion's `cvt.defined` domain [OP-6].
    ConversionDomain,
    /// A runtime-sized buffer allocation's canonical fit predicate [OP-9].
    AllocationFit,
    /// One range-reference formation goal `lo <= hi` or `hi <= x.len`,
    /// submitted under [MSR-4] exactly as every other consumer's obligation
    /// is [REF-4].
    RangeFormation,
    /// Two range steps of a compared pair of paths must be disjoint, by the
    /// four non-strict orderings [OWN-7] submits under [ENT-6] [EFF-5]. The
    /// number is the query's position in the function's call separations.
    CallSeparation(u32),
    /// Disjoint positions exclude proper ancestry at an exchange [OP-11]. The
    /// number is the query's position in the function's call separations.
    ExchangeSeparation(u32),
    /// A REF-2 use depends on this event-site separation query.
    ReferencePreservation(u32),
}

/// One exact single-binder affine image retained at a discharged OP-4 site.
///
/// At the indexed program point the offset's mathematical value is
/// `coefficient * binder + constant`. The nonzero coefficient makes this map
/// injective over the counted binder's mathematical integer values. This is
/// checked flow evidence, not a reconstruction from source syntax; PAR-2 may
/// consume it together with the enclosing bounds outcome without rerunning
/// either check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProvedAffineIndexMap {
    pub(crate) loop_id: CheckedLoopId,
    pub(crate) coefficient: i128,
    pub(crate) constant: i128,
}

/// One formation whose exact endpoint images are `[s*i+b, s*i+b+s)`.
/// The stride and base depend only on values fixed at the loop preheader.
/// The retained nonnegativity proofs complete PAR-2's adjacent-range family;
/// permission combines this evidence with the formation's VIEW-2 bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProvedRangePartition {
    pub(crate) loop_id: CheckedLoopId,
    pub(crate) range: super::places::CaptureId,
    pub(crate) stride: affine::AffineForm,
    pub(crate) base: affine::AffineForm,
    pub(crate) stride_nonnegative: DerivationId,
    pub(crate) base_nonnegative: DerivationId,
}

/// [ENT-6] disposition of one source obligation, judged at its source node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObligationOutcome {
    /// The subscript's `psuffix` node or the class call's `infix` node the
    /// obligation is attached to — one record per subscript in a chain and
    /// one per overflow conjunct [ENT-6].
    pub(crate) node_path: NodePath,
    /// The obligation family this occurrence belongs to.
    pub(crate) family: ObligationFamily,
    /// Requirement ordinal within a kernel row; zero for single-goal families.
    pub(crate) conjunct: u8,
    /// The canonical total Bool domain predicate. Bounds obligations alone
    /// carry `None`; OP-2, OP-9, and ordinary call requirements retain one exact identity,
    /// using an occurrence-local evaluated-value leaf only when no stable
    /// structural operand identity exists.
    pub(crate) canonical_goal: Option<GoalExpression>,
    /// Fixed family normalization used only as an alternate proof route.
    /// Components never receive source-obligation identities.
    pub(crate) components: Vec<BoundsRequest>,
    /// The closed fact state at the node derives the normalized relation.
    pub(crate) discharged: bool,
    /// The noncontradictory state proves the canonical goal false, or proves
    /// one fixed normalization component false.
    pub(crate) refuted: bool,
    /// The state at the node was contradictory, discharging everything.
    pub(crate) contradictory: bool,
    /// The exact residual rendering for an undischarged obligation: the
    /// offset atom's canonical source bytes, ` < len_of(`, the base place's
    /// canonical source bytes, `)`.
    pub(crate) residual: Option<String>,
    /// The two rendered substituted paths of a [EFF-5] range-separation
    /// obligation. Every other family carries none.
    pub(crate) overlap_targets: Option<(String, String)>,
    /// Exact ENT-4 derivation for an accepted obligation. Failed judgments
    /// deliberately carry no positive root.
    pub(crate) derivation: Option<DerivationId>,
    /// For a discharged AllocationFit occurrence, the source-proved numeric
    /// ceiling on its element count. Other obligation families retain None.
    /// Target qualification combines this value with the selected target's
    /// actual stride before any allocation is emitted.
    pub(crate) allocation_length_upper_bound: Option<u64>,
    /// Derivation of the exact numeric ceiling retained above. This may be
    /// tighter than the OP-9 admission derivation when ordinary or affine
    /// facts in the same proof context establish a smaller target ceiling.
    pub(crate) allocation_length_upper_bound_derivation: Option<DerivationId>,
    /// Exact injective index images available to the active counted loops at a
    /// discharged Bounds occurrence. Every other family, and every unproved
    /// bounds occurrence, retains an empty list.
    pub(crate) affine_index_maps: Vec<ProvedAffineIndexMap>,
    /// Adjacent-range images retained only at a discharged VIEW-2 formation.
    pub(crate) range_partitions: Vec<ProvedRangePartition>,
    /// For an undischarged obligation, every binding at which a kill event
    /// on some path to its node roots its place, sorted [ENT-5]. A
    /// requirement over a parameter outside this set still holds at the
    /// node, which is what its repair reads to offer one [DIAG-1].
    /// Discharged obligations retain none.
    pub(crate) written_before: Vec<BindingId>,
}

/// Exact normalized identity of one obligation query in the function-local
/// term inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BoundsRequest {
    pub(crate) left: Option<TermId>,
    pub(crate) right: TermId,
    pub(crate) bound: i128,
    /// The requested normalized form. The bounds and overflow families
    /// request the difference bound `left - right <= bound`; the division
    /// family requests the disequality `left != right`, whose `bound` cell
    /// is unused and recorded as zero [ENT-6].
    pub(crate) distinct: bool,
}

/// The arithmetic family selected by one fixed overflow normalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OverflowClassOperation {
    Add,
    Subtract,
    Multiply,
}

/// The two normalized [ENT-6] overflow conjuncts of one class call over one
/// selected fragment type: ordinal zero the upper bound `operand - Z <=
/// upper`, ordinal one the lower bound `Z - operand <= lower`. A ground
/// obligation relates Z to Z on both sides with bound 0 (in range) or -1
/// (inevitable overflow).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OverflowConjuncts {
    pub(crate) upper: i128,
    pub(crate) lower: i128,
    /// Both conjuncts relate Z to Z; no operand term participates.
    pub(crate) ground: bool,
    /// The exact decimal mathematical result of a ground obligation, for
    /// the `z outside T` residual rendering.
    pub(crate) ground_result: Option<GroundResult>,
}

/// Exact mathematical result of a two-constant class call. A multiply of
/// two 64-bit magnitudes can exceed `i128`, so magnitude and sign are kept
/// separately; every value is renderable in decimal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GroundResult {
    pub(crate) negative: bool,
    pub(crate) magnitude: u128,
}

impl GroundResult {
    fn from_value(value: i128) -> Self {
        Self {
            negative: value < 0,
            magnitude: value.unsigned_abs(),
        }
    }

    fn in_range(self, low: i128, high: i128) -> bool {
        let Ok(magnitude) = i128::try_from(self.magnitude) else {
            return false;
        };
        let value = if self.negative { -magnitude } else { magnitude };
        low <= value && value <= high
    }

    #[cfg(test)]
    pub(crate) fn render(self) -> String {
        if self.negative && self.magnitude != 0 {
            format!("-{}", self.magnitude)
        } else {
            self.magnitude.to_string()
        }
    }
}

/// Exact-quotient division rounding toward negative infinity.
const fn floor_div(dividend: i128, divisor: i128) -> i128 {
    let quotient = dividend / divisor;
    if dividend % divisor != 0 && ((dividend < 0) != (divisor < 0)) {
        quotient - 1
    } else {
        quotient
    }
}

/// Exact-quotient division rounding toward positive infinity.
const fn ceil_div(dividend: i128, divisor: i128) -> i128 {
    let quotient = dividend / divisor;
    if dividend % divisor != 0 && ((dividend < 0) == (divisor < 0)) {
        quotient + 1
    } else {
        quotient
    }
}

/// Folds one class call's overflow obligation into its two checker-computed
/// conjunct constants over the selected fragment type, exactly as [ENT-6]
/// tabulates: the fold is an equivalence over mathematical integers, never
/// an approximation. Every intermediate fits `i128` because type extrema
/// and operand constants are below `2^64` in magnitude, except a ground
/// multiply, whose exact result keeps magnitude and sign separately.
#[derive(Clone, Copy)]
enum OverflowConjunctClass {
    Folded {
        operation: OverflowClassOperation,
        constant: i128,
        constant_is_left: bool,
    },
    Ground {
        operation: OverflowClassOperation,
        left: i128,
        right: i128,
    },
}

/// The shared arithmetic fold used by exact occurrences and globally
/// interned `.defined` goals. `None` means two nonconstant operands and
/// therefore no L0 normalization route.
pub(crate) fn overflow_conjuncts_for_values(
    operation: CheckedIntegerOperation,
    left: Option<i128>,
    right: Option<i128>,
    ty: IntegerType,
) -> Option<OverflowConjuncts> {
    let operation = match operation {
        CheckedIntegerOperation::AddExact | CheckedIntegerOperation::AddDefined => {
            OverflowClassOperation::Add
        }
        CheckedIntegerOperation::SubtractExact | CheckedIntegerOperation::SubtractDefined => {
            OverflowClassOperation::Subtract
        }
        CheckedIntegerOperation::MultiplyExact | CheckedIntegerOperation::MultiplyDefined => {
            OverflowClassOperation::Multiply
        }
        _ => return None,
    };
    let class = match (left, right) {
        (Some(left), Some(right)) => OverflowConjunctClass::Ground {
            operation,
            left,
            right,
        },
        (Some(constant), None) => OverflowConjunctClass::Folded {
            operation,
            constant,
            constant_is_left: true,
        },
        (None, Some(constant)) => OverflowConjunctClass::Folded {
            operation,
            constant,
            constant_is_left: false,
        },
        (None, None) => return None,
    };
    Some(overflow_conjuncts_for_class(class, ty))
}

fn overflow_conjuncts_for_class(
    class: OverflowConjunctClass,
    ty: IntegerType,
) -> OverflowConjuncts {
    let (low, high) = term::type_range(ty);
    let folded = |upper: i128, lower: i128| OverflowConjuncts {
        upper,
        lower,
        ground: false,
        ground_result: None,
    };
    match class {
        OverflowConjunctClass::Folded {
            operation: OverflowClassOperation::Add,
            constant,
            ..
        } => folded(high - constant, constant - low),
        OverflowConjunctClass::Folded {
            operation: OverflowClassOperation::Subtract,
            constant,
            constant_is_left: false,
        } => folded(high + constant, -low - constant),
        OverflowConjunctClass::Folded {
            operation: OverflowClassOperation::Subtract,
            constant,
            constant_is_left: true,
        } => folded(constant - low, high - constant),
        OverflowConjunctClass::Folded {
            operation: OverflowClassOperation::Multiply,
            constant,
            ..
        } => {
            if constant == 0 {
                // Zero times anything is zero, in range for every type.
                OverflowConjuncts {
                    upper: 0,
                    lower: 0,
                    ground: true,
                    ground_result: Some(GroundResult::from_value(0)),
                }
            } else if constant > 0 {
                folded(floor_div(high, constant), -ceil_div(low, constant))
            } else {
                folded(floor_div(low, constant), -ceil_div(high, constant))
            }
        }
        OverflowConjunctClass::Ground {
            operation,
            left,
            right,
        } => {
            let result = match operation {
                OverflowClassOperation::Add => {
                    left.checked_add(right).map(GroundResult::from_value)
                }
                OverflowClassOperation::Subtract => {
                    left.checked_sub(right).map(GroundResult::from_value)
                }
                OverflowClassOperation::Multiply => Some(GroundResult {
                    negative: (left < 0) != (right < 0) && left != 0 && right != 0,
                    magnitude: left.unsigned_abs() * right.unsigned_abs(),
                }),
            }
            .expect("class constants are below 2^64 in magnitude, so add and subtract fit i128");
            let bound = if result.in_range(low, high) { 0 } else { -1 };
            OverflowConjuncts {
                upper: bound,
                lower: bound,
                ground: true,
                ground_result: Some(result),
            }
        }
    }
}

/// The two exact S11 proof points retained for one counted statement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CountedProofPoint {
    /// The complete post-capture closure, before continuing kills.
    PreheaderSnapshot,
    /// The executed true-header edge entering the counted body.
    BodyEntry,
}

/// One directed normalized bound and its exact parent in the sole
/// function-local derivation ledger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CountedAtomicDerivation {
    pub(crate) relation: state::Relation,
    pub(crate) proof_point: CountedProofPoint,
    pub(crate) parent: DerivationId,
}

/// One normative S11 equality and both of its directed atomic bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CountedEqualityDerivation {
    pub(crate) relation: state::Relation,
    pub(crate) forward: CountedAtomicDerivation,
    pub(crate) reverse: CountedAtomicDerivation,
}

/// One normative S11 ordering relation and its directed atomic bound.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CountedBoundDerivation {
    pub(crate) relation: state::Relation,
    pub(crate) atomic: CountedAtomicDerivation,
}

/// The complete fixed S11 root group for one concrete counted statement.
///
/// Field order is normative S11 order: the two endpoint captures, binder
/// initialization, and the two true-header bounds. The three equalities each
/// retain both directions, for exactly eight atomic ledger roots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CountedDerivationSet {
    pub(crate) counted_node_path: NodePath,
    pub(crate) lower_capture_eq_endpoint: CountedEqualityDerivation,
    pub(crate) upper_capture_eq_endpoint: CountedEqualityDerivation,
    pub(crate) binder_eq_lower_capture: CountedEqualityDerivation,
    pub(crate) lower_capture_le_binder: CountedBoundDerivation,
    pub(crate) binder_lt_upper_capture: CountedBoundDerivation,
}

/// The two induction judgments for a source-written loop invariant.
/// `step` is absent exactly when no body path reaches the hidden binder
/// update and next header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LoopInvariantProof {
    pub(crate) base: bool,
    pub(crate) step: Option<bool>,
    /// The preheader state derives the negation of one of the target's
    /// bounds, so the base judgment is refuted rather than unproved [MSR-4].
    pub(crate) base_refuted: bool,
    /// Some reachable backedge derives the negation of one of the
    /// next-header target's bounds [MSR-4].
    pub(crate) step_refuted: bool,
}

impl LoopInvariantProof {
    /// Whether source facts established the complete induction obligation. A
    /// missing step means no normal body edge reaches the next loop header.
    pub(crate) fn discharged(self) -> bool {
        self.base && self.step.unwrap_or(true)
    }
}

/// Direct result of checking one INV-1 statement in normal source order.
/// This is compiler analysis metadata for diagnostics and later fact use; it
/// is neither a portable proof object nor input to another validation pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoopInvariantOutcome {
    pub(crate) node_path: NodePath,
    pub(crate) loop_id: CheckedLoopId,
    pub(crate) source_ordinal: u32,
    pub(crate) name: String,
    /// Canonical source-language rendering of the relation required on the
    /// preheader edge. This deliberately names source bindings rather than
    /// checker-owned affine terms.
    pub(crate) base_target: String,
    /// Canonical source-language rendering of the relation required on a
    /// reachable backedge. A counted loop substitutes the hidden next binder
    /// value (`i + 1_u64`); an ordinary loop has no hidden substitution.
    pub(crate) backedge_target: String,
    /// The source fact context's induction result. This is the semantic result
    /// consumed by diagnostics and later proof queries.
    pub(crate) proof: LoopInvariantProof,
}

/// A structural failure while following one source-written local certificate.
/// These are closed, deterministic source-shape outcomes, not compiler
/// resource failures and not work-budget exhaustion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceProofCertificateFailure {
    /// Two `use` entries normalized to the same proposition. The writer must
    /// express their combined contribution with one explicit multiplier.
    RepeatedUse { first: u32, repeated: u32 },
    /// The written `use` list exceeds the fixed source-language capacity.
    UseCapacity { maximum: u32, actual: u32 },
    /// A written multiplier or source-order accumulated sum exceeds the
    /// admitted i128 proof arithmetic.
    ArithmeticOverflow,
    /// The accumulated certificate exceeds another fixed affine formation
    /// capacity, such as the number of canonical result terms.
    FormationCapacity,
    /// An invalid nonpositive factor reached the arithmetic core. Normal
    /// source checking rejects this earlier; retaining it here keeps the
    /// acceptance-bearing core total over its internal input type.
    InvalidFactor { use_index: u32 },
    /// A term-scaled premise left a degree-two monomial that is not the value
    /// image of any admitted exact product, so the certificate sum never
    /// became an affine inequality the target could be measured against.
    NonlinearResidual,
}

/// One direct result for an erased local invariant. Premises are retained in
/// source order so a rejection points to the first unproved `use`;
/// `combination` records only the deterministic written weighted sum and
/// direct residual check and grants no fact by itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceProofCheck {
    pub(crate) premises: Vec<bool>,
    /// First written premise that AUTO did not establish. Keeping the index in
    /// the entailment result makes the eventual diagnostic a direct consumer
    /// of this check rather than a second interpretation of `premises`.
    pub(crate) first_unproved_premise: Option<u32>,
    pub(crate) combination: bool,
    /// Failure while substituting current value images into the owning
    /// invariant target. This remains an INV-1 target-formation failure even
    /// when the statement also carries a PRF-1 block.
    pub(crate) target_failure: Option<SourceProofCertificateFailure>,
    /// First value-image normalization failure among the written `use`
    /// sources. Source formation precedes redundancy and every certificate
    /// arithmetic or availability judgment.
    pub(crate) source_failure: Option<SourceProofCertificateFailure>,
    /// The written `use` whose source formation first failed.
    pub(crate) source_failure_use_index: Option<u32>,
    /// Failure while forming the use list's factors, duplicate-free scaled
    /// premises, or source-order sum. PRF-1 selects this before premise
    /// availability because it depends only on the written certificate.
    pub(crate) certificate_failure: Option<SourceProofCertificateFailure>,
    /// The written `use` at which certificate formation failed. For a
    /// capacity failure this is the first entry beyond the admitted prefix.
    pub(crate) certificate_failure_use_index: Option<u32>,
    /// Failure while forming `target - sum` after every written premise has
    /// been admitted. This is selected after the first failed premise rather
    /// than allowing an unavailable premise to change certificate formation.
    pub(crate) residual_failure: Option<SourceProofCertificateFailure>,
    /// A nonempty `use` block is invalid when the specification-defined AUTO
    /// route already proves its outer target from the entering context.
    pub(crate) redundant: bool,
    /// A blockless target no step discharged whose negation, for one of its
    /// bounds, the entering context derives [MSR-4].
    pub(crate) target_refuted: bool,
}

impl SourceProofCheck {
    pub(crate) fn discharged(&self) -> bool {
        !self.redundant
            && self.target_failure.is_none()
            && self.source_failure.is_none()
            && self.certificate_failure.is_none()
            && self.residual_failure.is_none()
            && self.premises.iter().all(|proved| *proved)
            && self.combination
    }

    /// Selects the source-written `use` that owns this failed PRF-1 judgment.
    /// The order is the normative PRF-1 failure order; failures owned by the
    /// invariant target or final combination deliberately return `None`.
    pub(crate) fn failure_use_index(&self) -> Option<u32> {
        if self.target_failure.is_some() {
            None
        } else if self.source_failure.is_some() {
            self.source_failure_use_index
        } else if self.redundant {
            None
        } else if self.certificate_failure.is_some() {
            self.certificate_failure_use_index
        } else if self.first_unproved_premise.is_some() {
            self.first_unproved_premise
        } else {
            None
        }
    }
}

/// Direct result of checking one erased local invariant in source order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceProofOutcome {
    pub(crate) node_path: NodePath,
    /// Source paths for every written `use`, retained in source order so a
    /// failed premise or certificate step points at the smallest owning node.
    pub(crate) use_node_paths: Box<[NodePath]>,
    pub(crate) source_ordinal: u32,
    pub(crate) name: String,
    /// Whether the source statement carries a nonempty PRF-1 `use` block.
    /// A false value means this is an INV-1 AUTO-only local invariant.
    pub(crate) certificate_written: bool,
    /// The direct INV-1 AUTO or PRF-1 certificate result in the source fact
    /// context, selected by `certificate_written`.
    pub(crate) check: SourceProofCheck,
}

impl SourceProofOutcome {
    pub(crate) fn rejection_node_path(&self) -> &NodePath {
        let Some(index) = self.check.failure_use_index() else {
            return &self.node_path;
        };
        &self.use_node_paths[usize::try_from(index).expect("source-proof use index fits usize")]
    }
}

/// Diagnostic provenance for one source-proof fact retained across a join.
///
/// The semantic decision has already been made by intersecting the canonical
/// inequality on every predecessor. These references only preserve where the
/// independently established copies came from; they are never replayed and
/// never participate in acceptance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct JoinedSourceProofProvenance {
    pub(crate) predecessors: Box<[SourceAffineFactRef]>,
}

/// The complete and exclusive FN-9 relation-query disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PostconditionDisposition {
    Discharged,
    Refuted,
    Unproved,
}

/// One exact entry-image datum referenced by the concrete relation template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionEntryImage {
    pub(crate) parameter: u32,
    pub(crate) projections: Vec<super::goal::GoalProjection>,
    /// Which [MSR-1] measure the datum denotes, when it denotes one.
    pub(crate) measure: Option<super::model::CheckedMeasure>,
}

/// Source-value stability retained at one selected return. `None` is the
/// successful absence of an invalidating event, never a positive proof node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionEntryImageOutcome {
    pub(crate) datum: PostconditionEntryImage,
    pub(crate) invalidation: Option<state::FlowEventId>,
}

/// One source-ordered selected return and its source-context judgment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionExit {
    pub(crate) statement: NodePath,
    pub(crate) relation: state::Relation,
    pub(crate) residual: String,
    pub(crate) entry_images: Vec<PostconditionEntryImageOutcome>,
    pub(crate) disposition: PostconditionDisposition,
    pub(crate) derivation: Option<DerivationId>,
}

/// One nonempty all-exit aggregation. A failed aggregate has no derivation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionAggregate {
    pub(crate) discharged: bool,
    pub(crate) derivation: Option<DerivationId>,
}

/// The checked local FN-9 proof retained with one concrete function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FunctionPostconditionProof {
    pub(crate) block: NodePath,
    pub(crate) selector: NodePath,
    pub(crate) relation_ordinal: u32,
    /// Present only after the concrete-call SCC scheduler publishes every
    /// independently verified summary in this component atomically. This is
    /// checked-program-private identity; a caller never imports this proof's
    /// function-local derivation IDs.
    pub(crate) summary: Option<VerifiedPostconditionSummary>,
    pub(crate) exits: Vec<PostconditionExit>,
    pub(crate) aggregate: PostconditionAggregate,
}

/// One verified concrete FN-9 summary identity made referenceable by the SCC
/// schedule. The single relation ordinal is retained explicitly because the
/// occurrence identity is `(function, ensures clause, 0)`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VerifiedPostconditionSummary {
    pub(crate) function: FunctionId,
    pub(crate) block: NodePath,
    pub(crate) relation_ordinal: u32,
    pub(crate) component: u32,
}

/// Where one published relation comes from [CALL-6].
///
/// [ENT-3.S13]'s population is every source `fn_decl` whose declared relation
/// list has a verified [FN-9] summary.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RelationProvenance {
    Verified(VerifiedPostconditionSummary),
    /// One formal relation exported through a bound call. The isolated FN-4
    /// query proves the implication; every non-hypothetical premise is named
    /// by its independently verified actual FN-9 summary. No query-local DAG
    /// identity crosses into the caller.
    FormalBoundary {
        query: super::model::ContractQueryId,
        actual: FunctionId,
        premises: Vec<VerifiedPostconditionSummary>,
    },
}

impl RelationProvenance {
    /// Stable checked-program identities this provenance contributes to
    /// deterministic proof-choice ordering. The complete provenance remains
    /// part of the derivation node's equality and hash identity. These are
    /// external record identities, never caller- or query-local derivation IDs.
    pub(crate) fn identity(&self) -> Vec<u32> {
        match self {
            Self::Verified(summary) => vec![0, summary.function.0, summary.component],
            Self::FormalBoundary {
                query,
                actual,
                premises,
            } => {
                let premise_count = u32::try_from(premises.len())
                    .expect("formal-boundary premise count exceeds the u32 identity space");
                let mut identity = vec![1, query.0, actual.0, premise_count];
                for premise in premises {
                    identity.extend([
                        premise.function.0,
                        premise.relation_ordinal,
                        premise.component,
                    ]);
                }
                identity
            }
        }
    }
}

/// Caller-local reference to one authorized S12 publication. Direct
/// provenance names an earlier-component verified summary; formal-boundary
/// provenance additionally names the retained FN-4 query and its verified
/// actual premises. It carries no callee- or query-local [`DerivationId`].
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VerifiedPostconditionSummaryRef {
    pub(crate) summary: RelationProvenance,
}

/// One concrete ordinary-call SCC in deterministic callee-before-caller
/// order. Function and summary identities are both dense-function ordered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionComponent {
    pub(crate) ordinal: u32,
    pub(crate) functions: Vec<FunctionId>,
    /// Outgoing callee components in callee-before-caller order.
    pub(crate) outgoing: Vec<u32>,
    pub(crate) summaries: Vec<VerifiedPostconditionSummary>,
}

/// One checked concrete ordinary-user-call occurrence collected by the same
/// structural walk that builds the FN-9 SCC graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConcreteCallOccurrence {
    pub(crate) caller: FunctionId,
    pub(crate) node_path: NodePath,
    pub(crate) callee: FunctionId,
}

/// Program-private SCC schedule retained for the later caller-publication
/// handoff. An empty schedule is the no-postcondition fast path.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PostconditionSchedule {
    pub(crate) components: Vec<PostconditionComponent>,
    /// Dense function-to-component map, using ordered component ordinals.
    pub(crate) function_components: Vec<u32>,
    /// Stable caller-instance then call-NodePath order.
    pub(crate) calls: Vec<ConcreteCallOccurrence>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallGoalDisposition {
    Discharged,
    Refuted,
    Unproved,
}

/// Every direct derivation ground retained for one call judgment, in the
/// fixed order documented on [`CallGoalOutcome::evidence`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallGoalEvidence {
    AllDerivable,
    OpaquePositive,
    ExactL0Projection,
    NormalizationPositive,
    BooleanIntroductionPositive,
    /// The concrete caller predicate is one supported affine comparison or a
    /// fixed Boolean introduction whose ordering leaves are proved from the
    /// current source invariant facts.
    AffinePositive,
    OpaqueNegative,
    NegatedL0Projection,
    NormalizationNegative,
    BooleanIntroductionNegative,
}

/// Retained checked metadata for one ordinary call carrying a requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallGoalOutcome {
    /// Exact source `call` occurrence.
    pub(crate) node_path: NodePath,
    pub(crate) callee: FunctionId,
    /// Exact authoritative `requires_clause` occurrence: the concrete
    /// callee for a direct call and the instantiated formal for a bound call
    /// [FN-5].
    pub(crate) requires_clause: NodePath,
    pub(crate) goal: ConcreteGoal,
    /// The same goal in the terms the source wrote it in, rendered here
    /// because this is where the caller's binding names are in scope. [FN-8]
    /// publishes it as its `instantiated_goal` payload, the way [OP-4] publishes its residual.
    pub(crate) rendered_goal: String,
    /// Exact declared-order actual count at this concrete call occurrence.
    /// This remains zero for a legal zero-argument call with a requirement.
    pub(crate) argument_count: u32,
    pub(crate) disposition: CallGoalDisposition,
    /// Deterministic complete evidence. Contradictory states retain only
    /// `AllDerivable`; positive opaque and projection grounds follow in that
    /// order, followed by positive integer-domain normalization, Boolean
    /// introduction, and the fixed affine comparison route; negative opaque,
    /// negated projection, and negative normalization follow in the same
    /// order.
    pub(crate) evidence: Vec<CallGoalEvidence>,
    /// One exact positive or contradiction root for a discharged call.
    /// Refuted and unproved calls carry none.
    pub(crate) derivation: Option<DerivationId>,
    /// For a call no step discharged, every binding at which a kill event on
    /// some path to the call roots its place, sorted [ENT-5], as
    /// [`ObligationOutcome::written_before`] records it.
    pub(crate) written_before: Vec<BindingId>,
}

/// One retained declaration-only [FN-4] implication result. Its enclosing
/// [`FunctionEntailment`] is an isolated proof namespace, not a source
/// function summary and not a premise published to either callable body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContractGoalOutcome {
    pub(crate) goal: ConcreteGoal,
    pub(crate) disposition: CallGoalDisposition,
    pub(crate) evidence: Vec<CallGoalEvidence>,
    pub(crate) derivation: Option<DerivationId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BooleanGoalDecomposition {
    /// The established parent goal.
    pub(crate) parent: state::GoalId,
    /// The parent's established sign at the recording point.
    pub(crate) sign: state::GoalSign,
    /// The signed decomposition set in deterministic structural walk order.
    pub(crate) members: Vec<(state::GoalId, state::GoalSign)>,
}

/// Retained summary of one function's entailment analysis [DIAG-2].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FunctionEntailment {
    /// Checked disposition after all independent S4 sources close at body
    /// entry. An uninhabited function retains the exact contradiction root.
    pub(crate) body_disposition: super::model::CheckedBodyDisposition,
    /// Bounds obligations in deterministic source walk order.
    pub(crate) obligations: Vec<ObligationOutcome>,
    /// Ordinary call-goal judgments in deterministic checked-tree walk order.
    pub(crate) call_goals: Vec<CallGoalOutcome>,
    /// Declaration-only FN-4 queries. Ordinary function summaries leave this
    /// empty; each retained contract query owns one isolated summary with one
    /// entry here.
    pub(crate) contract_goals: Vec<ContractGoalOutcome>,
    /// One complete five-relation/eight-atomic S11 group per counted
    /// statement, in deterministic statement-walk order.
    pub(crate) counted_derivations: Vec<CountedDerivationSet>,
    /// Source-written loop invariants in statement order.
    pub(crate) loop_invariants: Vec<LoopInvariantOutcome>,
    /// Erased finite local invariants in statement order.
    pub(crate) source_proofs: Vec<SourceProofOutcome>,
    /// Diagnostic-only DAG nodes introduced when equal source-proof facts
    /// meet at structural joins. Dense ordinals are function-local.
    pub(crate) joined_source_proofs: Vec<JoinedSourceProofProvenance>,
    /// One entry per source-ordered FN-9 relation on a concrete function.
    pub(crate) postconditions: Vec<FunctionPostconditionProof>,
    /// O11 candidate decomposition sets recorded at the signed-goal
    /// establishments; never an acceptance input in this version.
    pub(crate) boolean_decompositions: Vec<BooleanGoalDecomposition>,
    /// Optional [PAR-1] range-separation proofs, keyed by the exact ordered
    /// statement pair and captured ranges whose first-point state proved
    /// them. Absence or an undischarged entry retains sequential lowering.
    pub(crate) permission_separations: Vec<super::permission::PermissionSeparationProof>,
    /// One answer per obligation record of the analyzed function, in the
    /// records' order; `None` where no judgment decided the record.
    pub(crate) answers: Vec<Option<RecordAnswer>>,
    /// The sites of judgments that answer no record. The checker and the
    /// engine then disagree about the function's obligations, and it is not
    /// accepted.
    pub(crate) unrecorded: Vec<NodePath>,
    /// Function-local, lifetime-bound derivations for mandatory DIAG-2 roots.
    pub(crate) derivations: DerivationLedger,
    /// Canonical term and goal identities moved from the analyzer so every
    /// retained dense ID remains exact and interpretable after analysis.
    pub(crate) inventory: DerivationInventory,
}

/// Answers each obligation record of `function` with the judgment that
/// decided it (`design/compiler/acceptance-records.md`).
///
/// A judgment answers a record formed at its own site for its own subject.
/// Records and judgments of one identity pair in the order the checker formed
/// and the engine made them, so the contract rests on their counts agreeing,
/// not on identities being unique. A record no judgment is left for stays
/// unanswered; a judgment no record is left for is returned by its site.
/// Either way the function's obligations and the engine's judgments disagree,
/// which acceptance refuses.
pub(crate) fn answer_records(
    function: &CheckedFunction,
    entailment: &FunctionEntailment,
) -> (Vec<Option<RecordAnswer>>, Vec<NodePath>) {
    let mut obligations = Judgments::of(entailment.obligations.iter().map(|outcome| {
        (
            (&outcome.node_path, outcome.family, outcome.conjunct),
            &outcome.node_path,
        )
    }));
    let mut call_goals = Judgments::of(entailment.call_goals.iter().map(|outcome| {
        (
            (&outcome.node_path, &outcome.requires_clause),
            &outcome.node_path,
        )
    }));
    let mut loop_invariants = Judgments::of(
        entailment
            .loop_invariants
            .iter()
            .map(|outcome| (&outcome.node_path, &outcome.node_path)),
    );
    let mut source_proofs = Judgments::of(
        entailment
            .source_proofs
            .iter()
            .map(|outcome| (&outcome.node_path, &outcome.node_path)),
    );
    // [PRE-1] the relations of a signature without a body are declaration
    // premises the analysis publishes, never obligations a record asks for.
    let mut postconditions = Judgments::of(
        entailment
            .postconditions
            .iter()
            .filter(|_| function.body.is_some())
            .map(|proof| ((&proof.selector, proof.relation_ordinal), &proof.selector)),
    );
    let uninhabited = matches!(
        entailment.body_disposition,
        super::model::CheckedBodyDisposition::Uninhabited { .. }
    );
    let answers = function
        .obligations
        .iter()
        .map(|record| {
            let site = &record.site;
            match &record.subject {
                ObligationSubject::Source { family, conjunct } => obligations
                    .take(&(site, *family, *conjunct))
                    .map(RecordAnswer::Obligation),
                ObligationSubject::CallRequirement {
                    requires_clause, ..
                } => call_goals
                    .take(&(site, requires_clause))
                    .map(RecordAnswer::CallGoal),
                ObligationSubject::LoopInvariant => {
                    loop_invariants.take(&site).map(RecordAnswer::LoopInvariant)
                }
                ObligationSubject::SourceProof => {
                    source_proofs.take(&site).map(RecordAnswer::SourceProof)
                }
                // [FN-9] a relation of a body whose requirements contradict
                // holds at every exit, since none is reachable.
                ObligationSubject::Postcondition { .. } if uninhabited => {
                    Some(RecordAnswer::Uninhabited)
                }
                ObligationSubject::Postcondition { relation_ordinal } => postconditions
                    .take(&(site, *relation_ordinal))
                    .map(RecordAnswer::Postcondition),
            }
        })
        .collect();
    let mut unrecorded = obligations
        .unanswered()
        .chain(call_goals.unanswered())
        .chain(loop_invariants.unanswered())
        .chain(source_proofs.unanswered())
        .chain(postconditions.unanswered())
        .cloned()
        .collect::<Vec<_>>();
    unrecorded.sort_by(|left, right| left.components().cmp(right.components()));
    (answers, unrecorded)
}

/// One kind's judgments by the identity a record names them by, each identity
/// holding its judgments' positions in the order they were made, with the
/// site of each.
struct Judgments<'judgment, Key> {
    by_key: HashMap<Key, std::collections::VecDeque<(usize, &'judgment NodePath)>>,
}

impl<'judgment, Key: Eq + std::hash::Hash> Judgments<'judgment, Key> {
    fn of(judgments: impl Iterator<Item = (Key, &'judgment NodePath)>) -> Self {
        let mut by_key = HashMap::<Key, std::collections::VecDeque<_>>::new();
        for (index, (key, site)) in judgments.enumerate() {
            by_key.entry(key).or_default().push_back((index, site));
        }
        Self { by_key }
    }

    /// The earliest judgment of `key` no record has taken yet.
    fn take(&mut self, key: &Key) -> Option<usize> {
        self.by_key
            .get_mut(key)?
            .pop_front()
            .map(|(index, _)| index)
    }

    /// The sites of the judgments no record took.
    fn unanswered(&self) -> impl Iterator<Item = &'judgment NodePath> + '_ {
        self.by_key
            .values()
            .flat_map(|judgments| judgments.iter().map(|(_, site)| *site))
    }
}

/// What one term of a rejected obligation reads, which is what selects the
/// routes its repair names [DIAG-1].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TermRead {
    /// A constant, a const-generic parameter, the zero term, or a measure of
    /// a named constant.
    Constant,
    /// A place, or a measure of one, rooted at this binding and reached by
    /// field selections and `deref` alone, so a clause can spell it.
    Binding(BindingId),
    /// An operand that is no term at all [ENT-2], such as an element read:
    /// no fact names its value until a `let` binds it.
    Unnamed,
    /// A captured, computed, subscripted or compiler-owned value.
    Computed,
}

impl FunctionEntailment {
    /// What each term of one obligation's normalized relations reads, in
    /// component order. A component without a left term relates an operand
    /// that is no term [ENT-2], which reads as [`TermRead::Unnamed`].
    pub(crate) fn obligation_term_reads(&self, outcome: &ObligationOutcome) -> Vec<TermRead> {
        outcome
            .components
            .iter()
            .flat_map(|component| {
                [
                    component
                        .left
                        .map_or(TermRead::Unnamed, |term| self.term_read(term)),
                    self.term_read(component.right),
                ]
            })
            .collect()
    }

    fn term_read(&self, term: TermId) -> TermRead {
        let spelled = |place: &super::places::ResolvedPlace| {
            let steps = place.path.iter().all(|step| {
                matches!(
                    step,
                    super::places::PlaceStep::Field(_) | super::places::PlaceStep::Deref
                )
            });
            match place.root {
                term::PlaceRoot::Binding(binding) if steps => TermRead::Binding(binding),
                term::PlaceRoot::Constant(_) if steps => TermRead::Constant,
                _ => TermRead::Computed,
            }
        };
        match self.inventory.terms.get(term.0 as usize) {
            Some(
                term::TermKind::Zero
                | term::TermKind::Constant(_)
                | term::TermKind::ConstParameter(..),
            ) => TermRead::Constant,
            Some(term::TermKind::Place(place, _) | term::TermKind::Measure(_, place)) => {
                spelled(place)
            }
            _ => TermRead::Computed,
        }
    }
}

/// Computes the combined entailment analysis of one checked function body.
///
/// The analysis is total: it never rejects, never reports unsupported, and
/// never fails compilation. A body shape outside the engine's current
/// vocabulary contributes no facts, which only under-derives.
pub(crate) fn analyze_function(
    function: &CheckedFunction,
    context: &EntailmentContext<'_>,
) -> FunctionEntailment {
    flow::analyze(function, context)
}

/// Computes one optimistic FN-9 function batch without pruning its shared
/// derivation ledger. The caller finalizes it after component publication.
pub(crate) fn analyze_function_candidate(
    function: &CheckedFunction,
    context: &EntailmentContext<'_>,
) -> FunctionEntailment {
    flow::analyze_candidate(function, context)
}

/// Checks one [FN-4] contract implication in a declaration-only proof
/// context. The supplied function carries only alpha-renamed contract
/// variables and the hypothetical premise set; [`flow`] submits both the
/// premises and goal through the ordinary S4/AUTO path. The caller finalizes
/// the returned ledger only after the query discharges.
pub(crate) fn contract_implies(
    function: &CheckedFunction,
    context: &EntailmentContext<'_>,
    goal: &super::goal::GoalExpression,
) -> FunctionEntailment {
    flow::contract_implies(function, context, goal)
}

/// Performs the sole root retention and dense-ID remap for one accepted
/// optimistic function batch.
pub(crate) fn finalize_function_entailment(entailment: &mut FunctionEntailment) {
    flow::finish(entailment);
}

pub(crate) fn postcondition_schedule<'function>(
    functions: impl IntoIterator<Item = &'function CheckedFunction>,
) -> Option<PostconditionSchedule> {
    let functions = functions.into_iter().collect::<Vec<_>>();
    if !functions
        .iter()
        .any(|function| !function.postconditions.is_empty())
    {
        return Some(PostconditionSchedule::default());
    }
    let mut graph = vec![Vec::new(); functions.len()];
    let mut calls = Vec::new();
    for (index, function) in functions.iter().enumerate() {
        if function.id.0 as usize != index {
            return None;
        }
        let start = calls.len();
        collect_statement_calls(
            function.id,
            function.body.as_deref().unwrap_or_default(),
            &mut calls,
        );
        calls[start..].sort_by(|left, right| {
            left.node_path
                .components()
                .cmp(right.node_path.components())
                .then_with(|| left.callee.0.cmp(&right.callee.0))
        });
        graph[index].extend(calls[start..].iter().map(|call| call.callee));
        if graph[index]
            .iter()
            .any(|callee| callee.0 as usize >= functions.len())
        {
            return None;
        }
    }
    // [FN-9] a caller's component treats an instance of another module's
    // generic callable as calling every function-kind actual supplied to it,
    // whether or not the instance's body calls them. Components only grow,
    // so no circular proof is admitted, and an edit to that body cannot
    // change which summaries the calling module's proofs may use. Within one
    // module the rule adds nothing.
    let mut conservative = Vec::new();
    for (caller, callees) in graph.iter().enumerate() {
        for callee in callees {
            let callee_function = functions.get(callee.0 as usize)?;
            if callee_function.module != functions.get(caller)?.module {
                conservative.push((callee.0 as usize, callee_function.function_actuals.clone()));
            }
        }
    }
    for (instance, actuals) in conservative {
        if actuals
            .iter()
            .any(|actual| actual.0 as usize >= functions.len())
        {
            return None;
        }
        graph[instance].extend(actuals);
    }
    for callees in &mut graph {
        callees.sort_unstable_by_key(|function| function.0);
        callees.dedup();
    }

    let graph = graph
        .into_iter()
        .map(|callees| {
            callees
                .into_iter()
                .map(|function| function.0 as usize)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut components = strongly_connected_components(&graph);
    for component in &mut components {
        component.sort_unstable();
    }
    let mut component_of = vec![usize::MAX; functions.len()];
    for (component, members) in components.iter().enumerate() {
        for member in members {
            component_of[*member] = component;
        }
    }

    // The source graph points caller -> callee. Reverse each inter-component
    // edge so Kahn's order is callee-before-caller, selecting the component
    // with the smallest dense FunctionId whenever multiple are ready.
    let mut callers = vec![BTreeSet::new(); components.len()];
    let mut incoming = vec![0usize; components.len()];
    for (caller, callees) in graph.iter().enumerate() {
        let caller_component = component_of[caller];
        for callee in callees {
            let callee_component = component_of[*callee];
            if caller_component != callee_component
                && callers[callee_component].insert(caller_component)
            {
                incoming[caller_component] += 1;
            }
        }
    }
    let mut ready = components
        .iter()
        .enumerate()
        .filter(|(component, _)| incoming[*component] == 0)
        .map(|(component, members)| (members[0], component))
        .collect::<BTreeSet<_>>();
    let mut order = Vec::with_capacity(components.len());
    while let Some(next) = ready.iter().next().copied() {
        ready.remove(&next);
        let (_, component) = next;
        order.push(component);
        for caller in &callers[component] {
            incoming[*caller] -= 1;
            if incoming[*caller] == 0 {
                ready.insert((components[*caller][0], *caller));
            }
        }
    }
    if order.len() != components.len() {
        return None;
    }
    let mut ordered_component_of = vec![usize::MAX; components.len()];
    for (ordinal, component) in order.iter().enumerate() {
        ordered_component_of[*component] = ordinal;
    }
    for (caller, callees) in graph.iter().enumerate() {
        for callee in callees {
            let caller_component = ordered_component_of[component_of[caller]];
            let callee_component = ordered_component_of[component_of[*callee]];
            if caller_component != callee_component && callee_component >= caller_component {
                return None;
            }
        }
    }

    let function_components = component_of
        .iter()
        .map(|component| {
            u32::try_from(ordered_component_of[*component])
                .expect("concrete call component count exceeds the u32 identity space")
        })
        .collect::<Vec<_>>();
    let mut outgoing = vec![Vec::new(); components.len()];
    for (caller, callees) in graph.iter().enumerate() {
        let caller_component = function_components[caller];
        for callee in callees {
            let callee_component = function_components[*callee];
            if caller_component != callee_component {
                outgoing[caller_component as usize].push(callee_component);
            }
        }
    }
    for callees in &mut outgoing {
        callees.sort_unstable();
        callees.dedup();
    }

    Some(PostconditionSchedule {
        components: order
            .into_iter()
            .enumerate()
            .map(|(ordinal, component)| PostconditionComponent {
                ordinal: u32::try_from(ordinal)
                    .expect("postcondition SCC count exceeds the u32 identity space"),
                functions: components[component]
                    .iter()
                    .map(|function| {
                        FunctionId(
                            u32::try_from(*function)
                                .expect("concrete function count exceeds the u32 identity space"),
                        )
                    })
                    .collect(),
                outgoing: outgoing[ordinal].clone(),
                summaries: Vec::new(),
            })
            .collect(),
        function_components,
        calls,
    })
}

/// Collects every concrete ordinary call occurrence inside one body, in
/// source order. This is call-graph shape only: no fact, disposition, or
/// derivation is read, so consumers outside the entailment engine may use it.
pub(super) fn collect_statement_calls(
    caller: FunctionId,
    statements: &[CheckedStatement],
    calls: &mut Vec<ConcreteCallOccurrence>,
) {
    for statement in statements {
        match statement {
            CheckedStatement::Proof(_) => {}
            CheckedStatement::Let { value, .. }
            | CheckedStatement::DestructuringLet { value, .. }
            | CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. }
            | CheckedStatement::Return { value, .. }
            | CheckedStatement::Give { value, .. } => {
                collect_expression_calls(caller, value, calls);
            }
            CheckedStatement::PropagateLet { scrutinee, .. } => {
                collect_expression_calls(caller, scrutinee, calls);
            }
            CheckedStatement::Set { target, value, .. } => {
                match target {
                    CheckedSetTarget::Place(_) => {}
                    CheckedSetTarget::RangeIndex(target) => {
                        for offset in target.offsets() {
                            collect_expression_calls(caller, offset, calls);
                        }
                    }
                    CheckedSetTarget::Storage(target) => {
                        for offset in target.offsets() {
                            collect_expression_calls(caller, offset, calls);
                        }
                    }
                }
                collect_expression_calls(caller, value, calls);
            }
            CheckedStatement::Match {
                scrutinee, arms, ..
            }
            | CheckedStatement::ValueMatchLet {
                scrutinee, arms, ..
            } => {
                collect_expression_calls(caller, scrutinee, calls);
                for arm in arms {
                    collect_statement_calls(caller, &arm.body, calls);
                }
            }
            CheckedStatement::Loop { body, .. } => {
                collect_statement_calls(caller, body, calls);
            }
            CheckedStatement::CountedRange {
                lower, upper, body, ..
            } => {
                collect_expression_calls(caller, lower, calls);
                collect_expression_calls(caller, upper, calls);
                collect_statement_calls(caller, body, calls);
            }
            CheckedStatement::Break { .. } => {}
        }
    }
}

fn collect_expression_calls(
    caller: FunctionId,
    expression: &CheckedExpression,
    calls: &mut Vec<ConcreteCallOccurrence>,
) {
    if let CheckedExpression::UserCall { function, call, .. } = expression {
        calls.push(ConcreteCallOccurrence {
            caller,
            node_path: call.clone(),
            callee: *function,
        });
    }
    for child in super::model::expression_children(expression) {
        collect_expression_calls(caller, child, calls);
    }
}

pub(super) fn strongly_connected_components(graph: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct Tarjan<'graph> {
        graph: &'graph [Vec<usize>],
        next_index: usize,
        indices: Vec<Option<usize>>,
        lowlinks: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        components: Vec<Vec<usize>>,
    }

    impl Tarjan<'_> {
        fn discover(&mut self, node: usize) {
            let index = self.next_index;
            self.next_index += 1;
            self.indices[node] = Some(index);
            self.lowlinks[node] = index;
            self.stack.push(node);
            self.on_stack[node] = true;
        }

        fn visit(&mut self, root: usize) {
            self.discover(root);
            // Retain the recursive walk's successor order and postorder,
            // without tying an FN-9 call chain to the native stack depth.
            let mut pending = vec![(root, 0)];
            while let Some((node, next_successor)) = pending.last_mut() {
                let node = *node;
                if let Some(&successor) = self.graph[node].get(*next_successor) {
                    *next_successor += 1;
                    if self.indices[successor].is_none() {
                        self.discover(successor);
                        pending.push((successor, 0));
                    } else if self.on_stack[successor] {
                        self.lowlinks[node] = self.lowlinks[node].min(
                            self.indices[successor]
                                .expect("on-stack node has a Tarjan discovery index"),
                        );
                    }
                    continue;
                }

                pending.pop();
                if Some(self.lowlinks[node]) == self.indices[node] {
                    let mut component = Vec::new();
                    loop {
                        let member = self
                            .stack
                            .pop()
                            .expect("Tarjan root retains its stack member");
                        self.on_stack[member] = false;
                        component.push(member);
                        if member == node {
                            break;
                        }
                    }
                    self.components.push(component);
                }
                if let Some(&(parent, _)) = pending.last() {
                    self.lowlinks[parent] = self.lowlinks[parent].min(self.lowlinks[node]);
                }
            }
        }
    }

    let mut tarjan = Tarjan {
        graph,
        next_index: 0,
        indices: vec![None; graph.len()],
        lowlinks: vec![0; graph.len()],
        stack: Vec::new(),
        on_stack: vec![false; graph.len()],
        components: Vec::new(),
    };
    for node in 0..graph.len() {
        if tarjan.indices[node].is_none() {
            tarjan.visit(node);
        }
    }
    tarjan.components
}

/// The engine's fragment-type gate: one member of the closed integer set
/// [OP-2], the only types terms may select [ENT-2].
const fn fragment_type(ty: CheckedType) -> Option<super::model::IntegerType> {
    match ty {
        CheckedType::Integer(ty) => Some(ty),
        _ => None,
    }
}

#[cfg(test)]
mod component_tests {
    use super::strongly_connected_components;

    #[test]
    fn a_deep_call_chain_and_cycle_do_not_use_the_native_stack() {
        let count = 20_000;
        let mut graph = (1..count).map(|next| vec![next]).collect::<Vec<_>>();
        graph.push(Vec::new());
        let expected = (0..count).rev().map(|node| vec![node]).collect::<Vec<_>>();
        assert_eq!(strongly_connected_components(&graph), expected);

        graph[count - 1].push(0);
        assert_eq!(
            strongly_connected_components(&graph),
            vec![(0..count).rev().collect::<Vec<_>>()]
        );
    }

    #[test]
    fn components_agree_with_mutual_reachability_for_every_three_node_graph() {
        // Reachability is an independent partition oracle, including self
        // edges, already-completed components and edges to active ancestors.
        for edges in 0..(1 << 9) {
            let graph = (0..3)
                .map(|from| {
                    (0..3)
                        .filter(|to| edges & (1 << (3 * from + to)) != 0)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let mut reachable = [[false; 3]; 3];
            for from in 0..3 {
                reachable[from][from] = true;
                for &to in &graph[from] {
                    reachable[from][to] = true;
                }
            }
            for via in 0..3 {
                for from in 0..3 {
                    for to in 0..3 {
                        reachable[from][to] |= reachable[from][via] && reachable[via][to];
                    }
                }
            }
            let components = strongly_connected_components(&graph);
            let mut component_of = [usize::MAX; 3];
            for (index, members) in components.iter().enumerate() {
                for &member in members {
                    assert_eq!(component_of[member], usize::MAX);
                    component_of[member] = index;
                }
            }
            for from in 0..3 {
                assert_ne!(component_of[from], usize::MAX);
                for to in 0..3 {
                    assert_eq!(
                        component_of[from] == component_of[to],
                        reachable[from][to] && reachable[to][from]
                    );
                }
            }
        }
    }
}
