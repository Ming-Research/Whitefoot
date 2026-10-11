//! The counted permission judgment [PAR-2]: whether the iterations of one
//! counted `for` may be executed with overlapping execution, and whether the
//! scalar and indexed values they carry may be recombined across them.
//!
//! The adjacency judgment next door reads two *statements*, so two executions
//! of one statement are never a pair and a counted loop gets nothing however
//! independent its iterations are. This judgment reads the loop itself. It
//! refuses nothing, changes no acceptance, and grants no lowering by itself.
//!
//! # What is judged
//!
//! The unit is one `for_stmt` L with body B, and every written, read, and
//! operand-read footprint of a statement of B is formed exactly as [PAR-1]
//! forms one. Writing an *iteration-own* place for one rooted in a binding B
//! itself introduces, permission holds exactly when all five conditions hold:
//!
//! 1. **One accumulator, or none.** "Among whole-place writes of B, at most
//!    one place is rooted in a binding declared outside L; that binding is
//!    L's accumulator, and every occurrence of it in B is one operand of one
//!    `set` statement whose target is that whole binding and whose right-hand
//!    side is one operation applied to that operand and to a second operand
//!    reaching the accumulator nowhere." The operation is from the admitted
//!    set, fixed for the accumulator across B. Indexed roots independently
//!    admit matching direct, copied-cell, temporary and summarized call updates,
//!    with fixed storage and length, discharged bounds and root-independent
//!    indices/contributions.
//! 2. **Every written place is admitted.** "Every place a footprint of B
//!    writes is iteration-own storage, the accumulator's whole place, an indexed
//!    accumulator cell, a place
//!    in one proved single-binder affine element, a place in one proved
//!    range reference, or a place in one certified element." Nothing else is
//!    admitted, and no injectivity argument is searched for.
//! 3. **Resolved footprints.** "A footprint element whose caller place the
//!    implementation does not resolve overlaps every place, so an unresolved
//!    element denies permission rather than granting it."
//! 4. **No exit edge.** "Every normal continuation of every statement of B
//!    reaches L's compiler-owned binder update, so no statement of B is a
//!    `return_stmt`, a `give_stmt`, a `break_stmt` resolved to L or a loop
//!    enclosing L, or a `let_stmt` selecting `propagate_let_rhs`."
//!
//! # Element and range families
//!
//! A **proved single-binder affine element** is one subscript of an `Array`,
//! a `Slots`, a `Paged`, the run either range kind names, a `Segments`, or
//! the pages of a `Paged`, rooted in an own binding
//! declared outside L, or reached through `^` of a reference parameter
//! whose row declares the write, and whose discharged [OP-4] bounds
//! obligation retains the offset's exact value `a*i + b` for L's binder with
//! `a` nonzero. Distinct binder values therefore select distinct elements.
//! A place is in that element when its path continues the element's by any
//! field, payload, `Box` content, index or range steps: aggregates hold only
//! owned values and a cell has one owner [TYPE-8, TYPE-9], so nothing below
//! element i is reachable from element j. A `set` target, an operand read and
//! a borrowed argument, whose callee row is projected onto the argument's
//! actual path [EFF-5], each reach such a place.
//! The `^` needs no rule of its own here: it selects the reference
//! root's resolved path [REF-1, TYPE-7], so `b^[a*i + c]` is recognized
//! exactly as an inline subscript is, which is what keeps every
//! runtime-capacity kernel in the family — [TYPE-9] admits a runtime-capacity
//! shape only as `Box` content [checker-facts].
//!
//! A `Ring` in an element-map position denies, "because a `Ring` subscript
//! selects the slot `(r.head + i) mod r.cap` [WIN-1], a wrapping map onto
//! storage rather than a linear offset".
//!
//! One affine map is admitted per resolved root. Every write to that root
//! must be in an element carrying the same `a` and `b`, and every read
//! through the same root must read a measure of that root [MSR-1] or be in an
//! element whose own discharged bounds result retains the same `a` and `b`;
//! a whole-root read, a different map, or an unavailable one denies. The
//! descriptor is disjoint from element storage [MSR-2], and the write
//! condition leaves it unchanged. That admits a same-index read-modify-write
//! and refuses a stencil, without any pairwise range search.
//!
//! A **proved range reference** is `&r[s*i+b..s*i+b+s]` [REF-4] passed as an
//! ordinary argument, whose discharged endpoint domain retains the exact
//! images `[s*i+b, s*i+b+s)` with `s` and `b` fixed throughout L and both
//! proved nonnegative, formed over an indexable place or range reference
//! declared outside B, so that every iteration's range is relative to one
//! origin. A range formed inside B from anything else is no proved range of
//! its own; it inherits one only by lying within it. For distinct indices
//! `i < j`, discreteness gives
//! `i+1 <= j` and nonnegative `s` gives `s*i+b+s <= s*j+b`, so the half-open
//! ranges do not overlap under [OWN-7]. Proved range references reached by
//! writes and whose origins overlap must name the same origin and carry
//! identical images. Every element access overlapping a written origin must
//! descend from a range with that partition; measure reads of that origin are
//! also admitted because the partition's element writes leave its disjoint
//! descriptor unchanged [MSR-2]. Read-only input origins may overlap across
//! iterations; they require no write partition.
//!
//! Forming a range reference reads its endpoints, reads no element content,
//! and authorizes no change to the origin's storage. There is no loan
//! condition in this judgment: [CAP-1] makes `own`, `&`, path overlap
//! [OWN-7] and the ordinary effect row the complete interference vocabulary,
//! so what a callee does through a reference is its declared row projected
//! onto the actual's path, which condition 2 already judges.
//!
//! # Why regrouping is admissible here and nowhere wider
//!
//! The adjacency rule never regroups anything: the combination tree of a pair
//! is written in the source, and permission only overlaps its two independent
//! halves. A loop rule is categorically stronger, because it lets the
//! implementation choose the tree. Two facts carry it:
//!
//! - Each admitted operation is a *total* function on its type's complete
//!   value set, carries no per-application obligation, and is associative and
//!   commutative there with a two-sided identity: `+wrap` and `*wrap` are the
//!   ring operations of the integers modulo two to the width, `iand`, `ior`,
//!   and `ixor` are the meet, join, and group operations of the bit vector,
//!   `imin` and `imax` are the meet and join of that type's total order, and
//!   `band`, `bor`, and `bxor` are the two-element cases of the same three.
//!   Fixing the leaf multiset therefore fixes the value of *every* binary
//!   tree over them. **No float operation is admitted.** `fadd.strict` is the
//!   pointed example: floating-point addition is not associative, so a
//!   schedule that regrouped it would move a published byte. `+`, `+defined`,
//!   `+checked`, and signed `+sat` are absent because each application carries
//!   an obligation, a `Result` route, or a clamp that regrouping moves.
//!   Unsigned `+sat` computes min(sum, max), with identity zero.
//! - No entailment fact established inside one counted iteration survives to
//!   a later head or to the continuation, so a regrouped accumulator can
//!   falsify no surviving proof.
//!
//! **Invariant.** This judgment consults typing, declared effect rows,
//! resolved places [REF-1, OWN-7], and the statement graph's exit edges. For
//! affine elements and proved range references it also consumes the [OP-4] and
//! [REF-4] dispositions and the exact value images already retained on the
//! checked function; it never repeats a bounds proof or reconstructs a value
//! from parser shape. Every form it has not classified refuses, and the
//! statement match is exhaustive for that reason: a missed statement would
//! contribute an empty footprint and *widen* permission.

use std::cell::RefCell;
use std::collections::BTreeMap;

use super::entailment::{
    ObligationFamily, ObligationOutcome, ProvedAffineIndexMap, ProvedRangePartition,
};
use super::model::{
    BindingId, CheckedArrayRoot, CheckedBooleanOperation, CheckedContainerRoot, CheckedExpression,
    CheckedFunction, CheckedIntegerOperation, CheckedLoopId, CheckedMeasure, CheckedPlaceStep,
    CheckedRangeElementPlace, CheckedRangeSource, CheckedSetTarget, CheckedStatement, CheckedType,
    CheckedValue, FunctionId, WindowShape, expression_children,
};
use super::permission::{
    CallProjection, Footprint, Program, argument_places, binding_read_projection, call_projection,
    collect_consumed_places, collect_operand_reads, container_steps, field_steps, match_referents,
    set_target_place, visit_read_bindings,
};
use super::places::{PlaceMap, PlaceRoot, PlaceStep, ResolvedPlace, UnprovedSeparations};
use super::range_facts::CheckedCertifiedLoop;
use crate::NodePath;

/// The judgment's outcome for one counted loop, and the advice that outlives
/// a refusal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoopPermission {
    /// The `for_stmt` node.
    pub(crate) statement: NodePath,
    pub(crate) verdict: LoopVerdict,
    /// The operations the body's accumulators combine under, in source order
    /// and without repeats. A loop with no carried value has none.
    pub(crate) combines: Vec<&'static str>,
    /// Whether a recursive split of this loop's index range, hand-written by
    /// the writer, would be eligible where the loop itself is refused. This
    /// is the advice the ledger prints for a refused loop; it is never true
    /// for a permitted one, which needs no rewrite.
    pub(crate) advises_split: bool,
    /// What actualizing this permission needs from the judgment, present for
    /// a permitted independent map or one-accumulator reduction.
    ///
    /// The judgment does not decide that anything is emitted: lowering reads
    /// this, applies its own emission conditions, and may still decline.
    pub(crate) actualization: Option<LoopActualization>,
    /// Resolved places written by the body, retained for capture permissions
    /// in the context executing an actualized chunk.
    pub(crate) written_places: Vec<ResolvedPlace>,
    /// Indexed roots live through the structured join; lengths are read at entry.
    pub(crate) indexed: Vec<IndexedReduction>,
}

impl LoopPermission {
    /// Retained writes conservatively overlap this resolved selection using
    /// the ordinary place relation; lowering adds no separation proof.
    pub(crate) fn writes_overlap(&self, place: &ResolvedPlace) -> bool {
        self.written_places
            .iter()
            .any(|written| super::places::places_overlap(&UnprovedSeparations, place, written))
    }
}

/// The two disjoint actualization shapes produced by the counted judgment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LoopActualization {
    IndependentMap,
    Reduction {
        accumulator: BindingId,
        combine: LoopCombine,
    },
}

/// A checked indexed family and its scalar cell projection. The root is
/// borrowed until the split joins; private cells acquire no cleanup authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndexedReduction {
    pub(crate) root: CheckedContainerRoot,
    pub(crate) fields: Vec<u32>,
    pub(crate) value_type: CheckedType,
    pub(crate) kind: IndexedFamilyKind,
    /// Calls updating this family; phase 2 uses their referent substitutions.
    pub(crate) calls: Vec<IndexedCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndexedCall {
    pub(crate) call: NodePath,
    pub(crate) function: FunctionId,
    pub(crate) argument: usize,
    /// This family's storage path relative to the callee parameter's referent.
    pub(crate) callee_root: CheckedContainerRoot,
    /// The actual reference's resolved prefix in the caller. Appending the
    /// callee root path yields the enclosing IndexedReduction's root; its
    /// fields are the unchanged callee-to-caller cell projection.
    pub(crate) actual: ResolvedPlace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum IndexedFamilyKind {
    Reduce { op: LoopCombine },
    Mark { constant: CheckedValue },
}

/// The closed set of operations an accumulator may be combined under: exactly
/// those [PAR-2] admits, named once so the judgment, the ledger, and the
/// emitted combination tree cannot hold three drifting copies of it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LoopCombine {
    AddWrap,
    AddSaturating,
    MultiplyWrap,
    BitAnd,
    BitOr,
    BitXor,
    Minimum,
    Maximum,
    And,
    Or,
    ExclusiveOr,
}

impl LoopCombine {
    /// The [OP-1] spelling the ledger prints.
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::AddWrap => "+wrap",
            Self::AddSaturating => "+sat",
            Self::MultiplyWrap => "*wrap",
            Self::BitAnd => "iand",
            Self::BitOr => "ior",
            Self::BitXor => "ixor",
            Self::Minimum => "imin",
            Self::Maximum => "imax",
            Self::And => "band",
            Self::Or => "bor",
            Self::ExclusiveOr => "bxor",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LoopVerdict {
    /// Permission holds, so the loop's iterations may be overlapped and its
    /// accumulator recombined.
    PermittedEligible,
    Denied(LoopDenial),
}

impl LoopVerdict {
    pub(crate) const fn is_permitted(&self) -> bool {
        matches!(self, Self::PermittedEligible)
    }

    /// The cited condition of a denial, or `None` for a permitted verdict.
    #[allow(dead_code)]
    pub(crate) const fn denied_condition(&self) -> Option<u8> {
        match self {
            Self::Denied(denial) => Some(denial.condition()),
            Self::PermittedEligible => None,
        }
    }
}

/// Why permission does not hold for one counted loop. Each variant names
/// exactly one condition of the judgment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LoopDenial {
    /// Condition 1: a write into storage outliving the iteration whose value
    /// is not that storage combined under an admitted operation.
    NotAReduction { statement: NodePath },
    /// Condition 1: the body carries more than one accumulator. The split
    /// advice survives this one: a hand-written recursion may return an
    /// aggregate, which this version does not synthesize.
    ManyAccumulators { accumulators: usize },
    /// Condition 1: the accumulator is read outside its own combine, so what
    /// a later iteration sees is the running total.
    AccumulatorRead { statement: NodePath, reads: usize },
    /// Condition 1: an indexed root violates its update-only contract.
    IndexedReduction {
        statement: NodePath,
        reason: &'static str,
    },
    /// Condition 2: a written or read place outside the families the rule
    /// admits.
    SharedWrite { argument: NodePath },
    /// Condition 3, fail closed: a place this judgment cannot resolve. An
    /// unresolved element overlaps every place, so it denies.
    UnresolvedWrite { argument: NodePath },
    /// Condition 2, fail closed: a body statement form whose footprint this
    /// judgment does not compute.
    BodyForm { form: &'static str },
    /// Condition 4: an edge leaves the loop.
    Exit { edge: &'static str },
    /// Condition 5: the body contains a waiting call [WAIT-1].
    WaitingCall { call: NodePath },
}

impl LoopDenial {
    /// The judgment condition this denial cites. The permission ledger prints
    /// it and the judgment tests assert it; acceptance never reads it.
    pub(crate) const fn condition(&self) -> u8 {
        match self {
            Self::NotAReduction { .. }
            | Self::ManyAccumulators { .. }
            | Self::AccumulatorRead { .. }
            | Self::IndexedReduction { .. } => 1,
            Self::SharedWrite { .. } | Self::BodyForm { .. } => 2,
            Self::UnresolvedWrite { .. } => 3,
            Self::Exit { .. } => 4,
            Self::WaitingCall { .. } => 5,
        }
    }
}

/// Whole-body summaries share the loop recognizer. A pending entry is a
/// cycle, never a provisional grant; both successes and failures are cached.
pub(super) struct IndexedSummaries<'check> {
    functions: &'check [CheckedFunction],
    entries: RefCell<BTreeMap<(u32, usize), IndexedSummaryState>>,
    #[cfg(test)]
    pub(super) computations: std::cell::Cell<usize>,
}

#[derive(Clone)]
enum IndexedSummaryState {
    Computing,
    Complete(Result<Vec<IndexedReduction>, IndexedSummaryFailure>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct IndexedSummaryFailure {
    pub(super) reason: &'static str,
    indexed: bool,
}

impl From<&'static str> for IndexedSummaryFailure {
    fn from(reason: &'static str) -> Self {
        Self {
            reason,
            indexed: false,
        }
    }
}

impl<'check> IndexedSummaries<'check> {
    pub(super) fn new(functions: &'check [CheckedFunction]) -> Self {
        Self {
            functions,
            entries: RefCell::new(BTreeMap::new()),
            #[cfg(test)]
            computations: std::cell::Cell::new(0),
        }
    }

    pub(super) fn get(
        &self,
        program: &Program<'check>,
        function: FunctionId,
        parameter: usize,
    ) -> Result<Vec<IndexedReduction>, IndexedSummaryFailure> {
        let key = (function.0, parameter);
        if let Some(entry) = self.entries.borrow().get(&key) {
            return match entry {
                IndexedSummaryState::Computing => Err(IndexedSummaryFailure {
                    reason: "indexed helper summaries require an acyclic chain of call-form updates",
                    indexed: true,
                }),
                IndexedSummaryState::Complete(result) => result.clone(),
            };
        }
        self.entries
            .borrow_mut()
            .insert(key, IndexedSummaryState::Computing);
        #[cfg(test)]
        self.computations.set(self.computations.get() + 1);
        let result = self.compute(program, function, parameter);
        self.entries
            .borrow_mut()
            .insert(key, IndexedSummaryState::Complete(result.clone()));
        result
    }

    fn compute(
        &self,
        program: &Program<'check>,
        function: FunctionId,
        parameter: usize,
    ) -> Result<Vec<IndexedReduction>, IndexedSummaryFailure> {
        let function = self
            .functions
            .get(function.0 as usize)
            .ok_or("the helper must have a checked body for an indexed summary")?;
        let parameter = function
            .parameters
            .get(parameter)
            .filter(|parameter| parameter.mode.is_reference())
            .ok_or("an indexed summary requires a reference parameter")?;
        let body = function
            .body
            .as_deref()
            .ok_or("the helper must have a checked body for an indexed summary")?;
        let places = PlaceMap::for_function(function);
        let mut survey = Survey::new(
            program,
            &places,
            &function.entailment.obligations,
            None,
            None,
            parameter.node_path.clone(),
            Vec::new(),
        );
        survey.summary_parameter = Some(parameter.binding);
        survey.introduce(body);
        survey.select_indexed_roots(body);
        survey.walk(body, 0);
        let indexed = !survey.indexed.is_empty() || survey.indexed_call_candidate;
        if let Some(LoopDenial::IndexedReduction { reason, .. }) = survey.indexed_state() {
            return Err(IndexedSummaryFailure { reason, indexed });
        }
        if survey.form.is_some() || survey.unresolved.is_some() {
            return Err(IndexedSummaryFailure {
                reason: "every footprint in an indexed helper summary must be resolved",
                indexed,
            });
        }
        Ok(survey.indexed_payload())
    }
}

/// The verdict of every counted loop of one function, in source order.
///
/// `eligible_pairs` are the statement paths of the adjacencies the [PAR-1]
/// judgment already found eligible in this function. A loop containing one of
/// them already has parallelism a writer can see, so it is never additionally
/// told to become a recursion. The *verdict* does not read them: [PAR-2]
/// judges the loop, not what a writer could put inside it.
pub(crate) fn judge_loops<'check>(
    program: &Program<'check>,
    places: &PlaceMap,
    function: &'check CheckedFunction,
    eligible_pairs: &[NodePath],
) -> Vec<LoopPermission> {
    let mut judged = Vec::new();
    collect(
        program,
        places,
        &function.entailment.obligations,
        &function.waiting.calls,
        &function.range_facts.certified,
        function.body.as_deref().unwrap_or_default(),
        &mut judged,
    );
    for loop_permission in &mut judged {
        if eligible_pairs
            .iter()
            .any(|pair| encloses(&loop_permission.statement, pair))
        {
            loop_permission.advises_split = false;
        }
    }
    judged
}

fn collect<'check>(
    program: &Program<'check>,
    places: &PlaceMap,
    obligations: &'check [ObligationOutcome],
    waiting: &'check [NodePath],
    certified: &'check [CheckedCertifiedLoop],
    statements: &'check [CheckedStatement],
    judged: &mut Vec<LoopPermission>,
) {
    for statement in statements {
        if let CheckedStatement::CountedRange {
            id,
            node_path,
            binder,
            body,
            ..
        } = statement
        {
            let mut loop_permission = judge(
                program,
                places,
                obligations,
                certified.iter().find(|certificate| certificate.id == *id),
                node_path.clone(),
                *id,
                *binder,
                body,
            );
            // [PAR-2] a body that waits has no overlap to offer whatever else
            // it does, so this refusal replaces any other. Every call node
            // lies inside the node of the loop holding it, so the body holds
            // exactly the waiting calls below the loop's node.
            if let Some(call) = waiting.iter().find(|call| encloses(node_path, call)) {
                loop_permission.verdict =
                    LoopVerdict::Denied(LoopDenial::WaitingCall { call: call.clone() });
                loop_permission.advises_split = false;
                loop_permission.actualization = None;
            }
            judged.push(loop_permission);
        }
        for nested in nested_bodies(statement) {
            collect(
                program,
                places,
                obligations,
                waiting,
                certified,
                nested,
                judged,
            );
        }
    }
}

/// Whether one statement lies inside another, by node path prefix.
fn encloses(outer: &NodePath, inner: &NodePath) -> bool {
    let outer = outer.components();
    let inner = inner.components();
    inner.len() > outer.len() && inner.starts_with(outer)
}

#[allow(clippy::too_many_arguments)]
fn judge<'check>(
    program: &Program<'check>,
    places: &PlaceMap,
    obligations: &'check [ObligationOutcome],
    certificate: Option<&'check CheckedCertifiedLoop>,
    statement: NodePath,
    id: CheckedLoopId,
    binder: BindingId,
    body: &'check [CheckedStatement],
) -> LoopPermission {
    let mut survey = Survey::new(
        program,
        places,
        obligations,
        certificate,
        Some(id),
        statement.clone(),
        vec![binder],
    );
    survey.introduce(body);
    survey.select_indexed_roots(body);
    // Selecting one family claims the entire root, including affine sibling
    // fields. Otherwise those siblings would still write shared storage.
    let selected = survey
        .indexed
        .iter()
        .filter(|root| {
            root.needs_reduction
                || survey.indexed.iter().any(|other| {
                    same_element_root(&other.origin, &root.origin) && other.map != root.map
                })
        })
        .flat_map(|root| survey.places.resolve(PlaceRoot::Binding(root.binding), &[]))
        .collect::<Vec<_>>();
    let remap = survey
        .indexed
        .iter()
        .enumerate()
        .filter(|(_, root)| {
            selected.iter().any(|origin| {
                survey
                    .places
                    .overlaps(&UnprovedSeparations, origin, &root.origin)
            })
        })
        .map(|(old, _)| old)
        .collect::<Vec<_>>();
    for call in &mut survey.indexed_calls {
        for (family, _) in &mut call.families {
            *family = remap
                .iter()
                .position(|old| old == family)
                .expect("call families require reduction");
        }
    }
    survey.indexed.retain(|root| {
        selected.iter().any(|origin| {
            survey
                .places
                .overlaps(&UnprovedSeparations, origin, &root.origin)
        })
    });
    survey.walk(body, 0);
    survey.finish(statement)
}

/// One write whose already-checked subscript value is `a*i+b`, where `i` is
/// the counted binder of the loop under judgment and `a != 0`. The successful
/// [OP-4] outcome establishes that the selected element is inside its
/// collection; the affine image establishes that two iterations select
/// distinct elements.
struct ProvenElementWrite {
    /// The resolved place above the index step: the mapped root.
    root: ResolvedPlace,
    statement: NodePath,
    map: ProvedAffineIndexMap,
    page: bool,
}

/// One already-proved element read whose exact offset is the same affine map.
/// This is consumer evidence only: permission uses it once to compare read
/// and write ranges, then lowering forgets it.
struct ProvenElementRead {
    root: ResolvedPlace,
    map: ProvedAffineIndexMap,
    page: bool,
}

/// One borrowed argument at or below one mapped element [PAR-2]: the callee's
/// row, projected onto the actual path [EFF-5], reaches only storage below
/// that element.
struct ProvenElementReference {
    /// The mapped root: the resolved place above the element's index step.
    root: ResolvedPlace,
    /// The element itself: the root extended by that index step.
    element: ResolvedPlace,
    map: ProvedAffineIndexMap,
    page: bool,
}

/// One range reference `&r[s*i+b..s*i+b+s]` whose endpoint images the [REF-4]
/// formation retained [PAR-2].
struct ProvenRangeReference {
    /// The resolved place above the range step: the origin.
    origin: ResolvedPlace,
    /// The complete resolved place including its range step, which is what a
    /// descendant access must be contained in.
    place: ResolvedPlace,
    argument: NodePath,
    map: ProvedRangePartition,
    /// Formation alone grants no write authority or independent-map work.
    /// The body's resolved write footprints select its actual partitions.
    written: bool,
}

/// One element write a holding certificate separates from every access of
/// every other iteration [RANGE-5]: the root is the resolved place above the
/// element's first index or range step.
struct CertifiedElementWrite {
    root: ResolvedPlace,
    statement: NodePath,
}

/// One source read occurrence and the places reached by that spelling.
///
/// The binding remains condition 1's accumulator identity. The resolved
/// places are condition 2's collection identity: sibling fields of one struct
/// are distinct, while a whole-parent or alias access overlaps a mapped root
/// and must be accounted for.
struct ReadOccurrence {
    binding: BindingId,
    places: Vec<ResolvedPlace>,
    /// The expression or call that reads, where it reads elements.
    carrier: Option<NodePath>,
    /// A measure expression without a certificate element-read carrier.
    /// Affine and range coverage also check its exact measured root [MSR-2].
    measure: bool,
    /// REF-4's page formation reads its owner's length, separate from a page map.
    page_descriptor: bool,
    /// A measure selected through a range element retains its carrier for
    /// the certified family, but can measure an affine map's own nested root.
    element_measure: bool,
}

impl ReadOccurrence {
    /// A measure of this exact mapped root or written range origin [PAR-2].
    /// Direct expressions retain the measured place; a projected helper row
    /// retains its final measure step. A measure below an element is still
    /// an access to that element and must satisfy its map or range coverage.
    fn is_root_measure(&self, place: &ResolvedPlace, root: &ResolvedPlace) -> bool {
        ((self.measure || self.element_measure) && same_element_root(place, root))
            || (matches!(place.path.last(), Some(PlaceStep::Measure(_)))
                && place.path.len() == root.path.len() + 1
                && root.contains(place))
    }
}

/// One accepted accumulate statement: `set a = a (+) e` with `(+)` admitted.
struct Accumulate {
    binding: BindingId,
    combine: LoopCombine,
    statement: NodePath,
}

/// A root requiring the indexed family rather than the existing affine map.
/// Bindings, paths and bounds come from ordinary checking, never from syntax
/// reconstruction. The whole binding's occurrences are checked at finish.
struct IndexedAccumulator {
    root: CheckedContainerRoot,
    binding: BindingId,
    origin: ResolvedPlace,
    map: Option<ProvedAffineIndexMap>,
    needs_reduction: bool,
    fields: Vec<u32>,
    value_type: CheckedType,
    kind: Option<IndexedFamilyKind>,
    updates: usize,
    statement: NodePath,
    calls: Vec<IndexedCall>,
}

struct IndexedCallUpdate {
    call: NodePath,
    argument: usize,
    families: Vec<(usize, IndexedFamilyKind)>,
}

/// A fresh initializer in the current block, with the footprint boundaries
/// after its evaluation. Only one operation temporary and one cell copy can
/// participate; retaining all initializers does not authorize a chain.
struct IndexedInitializer<'check> {
    binding: BindingId,
    value: &'check CheckedExpression,
    read_end: usize,
    write_end: usize,
}

struct Survey<'check, 'run> {
    program: &'run Program<'check>,
    places: &'run PlaceMap,
    /// The loop's certificate, when the range judgment found one to hold
    /// [RANGE-5].
    certificate: Option<&'check CheckedCertifiedLoop>,
    /// Element writes the certificate separates across iterations.
    certified_writes: Vec<CertifiedElementWrite>,
    /// Successful source obligations already computed by [ENT]. Permission
    /// consumes their disposition by source-node identity and never reruns
    /// the underlying proof.
    obligations: &'run [ObligationOutcome],
    /// The counted loop under judgment, so a `break` that closes it is told
    /// apart from one that closes a loop opened inside it.
    outer_loop: Option<CheckedLoopId>,
    summary_parameter: Option<BindingId>,
    indexed_calls: Vec<IndexedCallUpdate>,
    indexed_call_candidate: bool,
    /// The node a denial found while walking the current statement cites.
    /// It starts at L's own `for_stmt` and narrows to each statement that
    /// carries a node of its own, so every citation resolves.
    cite: NodePath,
    /// Bindings introduced anywhere inside the body, including the loop's own
    /// binder. Storage rooted in one of these is created fresh by every
    /// iteration and dies with it; everything else outlives the iteration.
    introduced: Vec<BindingId>,
    inner_loops: Vec<u32>,
    /// Every read occurrence, with multiplicity and resolved places.
    reads: Vec<ReadOccurrence>,
    accumulates: Vec<Accumulate>,
    indexed: Vec<IndexedAccumulator>,
    indexed_denial: Option<LoopDenial>,
    indexed_temporaries: Vec<(BindingId, NodePath, bool)>,
    /// Unlike the retained write set, this records repeated commits too.
    /// A repeated write between a temporary and its set still invalidates it.
    write_events: Vec<ResolvedPlace>,
    written_places: Vec<ResolvedPlace>,
    carried: Option<NodePath>,
    shared: Option<NodePath>,
    unresolved: Option<NodePath>,
    element_writes: Vec<ProvenElementWrite>,
    element_reads: Vec<ProvenElementRead>,
    range_references: Vec<ProvenRangeReference>,
    form: Option<&'static str>,
    /// A control edge that leaves the counted loop's iteration sequence.
    exit: Option<&'static str>,
}

impl<'check, 'run> Survey<'check, 'run> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        program: &'run Program<'check>,
        places: &'run PlaceMap,
        obligations: &'run [ObligationOutcome],
        certificate: Option<&'check CheckedCertifiedLoop>,
        id: Option<CheckedLoopId>,
        statement: NodePath,
        introduced: Vec<BindingId>,
    ) -> Self {
        Self {
            program,
            places,
            obligations,
            certificate,
            certified_writes: Vec::new(),
            outer_loop: id,
            summary_parameter: None,
            indexed_calls: Vec::new(),
            indexed_call_candidate: false,
            cite: statement,
            introduced,
            inner_loops: Vec::new(),
            reads: Vec::new(),
            accumulates: Vec::new(),
            indexed: Vec::new(),
            indexed_denial: None,
            indexed_temporaries: Vec::new(),
            write_events: Vec::new(),
            written_places: Vec::new(),
            carried: None,
            shared: None,
            unresolved: None,
            element_writes: Vec::new(),
            element_reads: Vec::new(),
            range_references: Vec::new(),
            form: None,
            exit: None,
        }
    }

    /// Records every binding the body introduces, and every loop it opens,
    /// before anything is judged against those sets.
    fn introduce(&mut self, statements: &'check [CheckedStatement]) {
        collect_introduced(statements, &mut self.introduced);
        collect_inner_loops(statements, &mut self.inner_loops);
    }

    /// Choose the indexed family only for roots outside the existing single
    /// affine map. This preserves map permission for ordinary same-index
    /// reads and stores while also admitting reductions over several maps.
    /// A holding certificate already owns its writes and requires no indexed
    /// update form; leave those writes with the certified-element family.
    fn select_indexed_roots(&mut self, statements: &[CheckedStatement]) {
        for statement in statements {
            if let CheckedStatement::Set {
                node_path,
                target: CheckedSetTarget::Storage(target),
                ..
            } = statement
                && !self
                    .certificate
                    .is_some_and(|certificate| certificate.writes.contains(node_path))
                && let Some((binding, origin)) = self.indexed_origin(target)
            {
                let (position, index, fields) =
                    indexed_parts(target).expect("indexed_origin established the cell projection");
                let map = self
                    .outermost_map(path_subscripts(&target.path))
                    .map(|(_, map)| map);
                let mut root = target.clone();
                root.path.truncate(position);
                root.ty = index.base_type;
                if let Some(position) = self.indexed.iter().position(|existing| {
                    same_element_root(&existing.origin, &origin) && existing.fields == fields
                }) {
                    let existing = &mut self.indexed[position];
                    existing.needs_reduction |= map.is_none() || existing.map != map;
                    self.retain_indexed_owner(position, root, binding);
                } else {
                    self.indexed.push(IndexedAccumulator {
                        root,
                        binding,
                        origin,
                        map,
                        needs_reduction: map.is_none(),
                        fields,
                        value_type: target.ty,
                        kind: None,
                        updates: 0,
                        statement: node_path.clone(),
                        calls: Vec::new(),
                    });
                }
            }
            if let Some(value) = statement_value(statement)
                && let Some(call) = call_projection(value)
            {
                self.select_indexed_call(&call);
            }
            for nested in nested_bodies(statement) {
                self.select_indexed_roots(nested);
            }
        }
    }

    fn select_indexed_call(&mut self, call: &CallProjection<'_>) {
        let Some(function) = self
            .program
            .indexed_summaries
            .functions
            .get(call.target.0 as usize)
        else {
            return;
        };
        if function.body.is_none() {
            return;
        }
        for (argument, parameter) in function.parameters.iter().enumerate() {
            if !parameter.mode.is_reference() {
                continue;
            }
            let Some(actual) = call.arguments.get(argument) else {
                continue;
            };
            let Some(actual_root) = reference_root(actual, self.places) else {
                continue;
            };
            let Some(binding) = actual_root.binding() else {
                continue;
            };
            if self.introduced.contains(&binding)
                || self
                    .summary_parameter
                    .is_some_and(|parameter| parameter != binding)
                || actual_root
                    .path
                    .iter()
                    .any(|step| matches!(step, CheckedPlaceStep::Subscript(_)))
            {
                continue;
            }
            let resolved = self
                .places
                .resolve(actual_root.root, &actual_root.place_path());
            let [actual_place] = resolved.as_slice() else {
                continue;
            };
            let summary = self
                .program
                .indexed_summaries
                .get(self.program, call.target, argument);
            let families = match summary {
                Ok(families) if !families.is_empty() => families,
                Ok(_) => continue,
                Err(failure) => {
                    // A non-indexed helper retains the ordinary element,
                    // range or shared-write diagnostic. Within a summary,
                    // every forwarded parameter must satisfy its contract.
                    let footprint = self.program.footprint(self.places, call);
                    if (failure.indexed || self.summary_parameter.is_some())
                        && footprint.writes.iter().any(|write| {
                            write.argument == call.argument_nodes[argument]
                                && self.places.overlaps(
                                    &UnprovedSeparations,
                                    actual_place,
                                    &write.place,
                                )
                        })
                    {
                        self.indexed_call_candidate |= failure.indexed;
                        self.indexed_denial
                            .get_or_insert(LoopDenial::IndexedReduction {
                                statement: call.call.clone(),
                                reason: failure.reason,
                            });
                    }
                    continue;
                }
            };
            self.indexed_call_candidate = true;
            let owners = self.places.resolve(PlaceRoot::Binding(binding), &[]);
            let footprint = self.program.footprint(self.places, call);
            let other_reaches = call.arguments.iter().enumerate().any(|(position, value)| {
                position != argument && {
                    let mut mentions = false;
                    visit_read_bindings(value, &mut |read| {
                        mentions |= self
                            .places
                            .resolve(PlaceRoot::Binding(read), &[])
                            .iter()
                            .any(|place| {
                                owners.iter().any(|owner| {
                                    self.places.overlaps(&UnprovedSeparations, owner, place)
                                })
                            });
                    });
                    mentions
                        || argument_places(self.places, value).is_some_and(|places| {
                            places.iter().any(|place| {
                                owners.iter().any(|owner| {
                                    self.places.overlaps(&UnprovedSeparations, owner, place)
                                })
                            })
                        })
                }
            }) || footprint
                .reads
                .iter()
                .chain(&footprint.writes)
                .chain(&footprint.operand_reads)
                .any(|access| {
                    access.argument != call.argument_nodes[argument]
                        && owners.iter().any(|owner| {
                            self.places
                                .overlaps(&UnprovedSeparations, owner, &access.place)
                        })
                });
            if other_reaches || footprint.unresolved.is_some() {
                self.indexed_denial.get_or_insert(LoopDenial::IndexedReduction {
                    statement: call.call.clone(),
                    reason: "a call-form update requires every other argument and its row accesses to be disjoint from the root's binding",
                });
                continue;
            }
            let mut mapped = Vec::new();
            for family in families {
                let mut root = actual_root.clone();
                root.path.extend_from_slice(&family.root.path);
                root.ty = family.root.ty;
                let mut origin = actual_place.clone();
                origin.path.extend(family.root.place_path());
                let mapping = IndexedCall {
                    call: call.call.clone(),
                    function: call.target,
                    argument,
                    callee_root: family.root,
                    actual: actual_place.clone(),
                };
                let position = self
                    .indexed
                    .iter()
                    .position(|existing| {
                        same_element_root(&existing.origin, &origin)
                            && existing.fields == family.fields
                    })
                    .unwrap_or_else(|| {
                        self.indexed.push(IndexedAccumulator {
                            root: root.clone(),
                            binding,
                            origin,
                            map: None,
                            needs_reduction: true,
                            fields: family.fields,
                            value_type: family.value_type,
                            kind: None,
                            updates: 0,
                            statement: call.call.clone(),
                            calls: Vec::new(),
                        });
                        self.indexed.len() - 1
                    });
                self.retain_indexed_owner(position, root, binding);
                self.indexed[position].needs_reduction = true;
                self.indexed[position].calls.push(mapping);
                mapped.push((position, family.kind));
            }
            self.indexed_calls.push(IndexedCallUpdate {
                call: call.call.clone(),
                argument,
                families: mapped,
            });
        }
    }

    /// Equal resolved families can be spelled through different inline
    /// ancestors. Retain the enclosing owner so a helper's reference reaches
    /// the actual private cells, not an inline copy in an argument wrapper.
    fn retain_indexed_owner(
        &mut self,
        position: usize,
        root: CheckedContainerRoot,
        binding: BindingId,
    ) {
        let owner = |root: &CheckedContainerRoot| {
            let prefix = root
                .path
                .iter()
                .rposition(|step| matches!(step, CheckedPlaceStep::BoxReferent(_)))
                .map_or(0, |index| index + 1);
            self.places.resolve(root.root, &root.place_path()[..prefix])
        };
        let current = owner(&self.indexed[position].root);
        let candidate = owner(&root);
        if matches!((current.as_slice(), candidate.as_slice()), ([current], [candidate])
            if candidate.path.len() < current.path.len() && candidate.contains(current))
        {
            self.indexed[position].root = root;
            self.indexed[position].binding = binding;
        }
    }

    fn record_indexed_kind(
        &mut self,
        position: usize,
        kind: IndexedFamilyKind,
        node: &NodePath,
        reads_cell: bool,
    ) {
        let root = &mut self.indexed[position];
        if root.kind.as_ref().is_some_and(|first| *first != kind) {
            self.indexed_denial.get_or_insert(LoopDenial::IndexedReduction {
                statement: node.clone(),
                reason: "one fixed operation or one constant is required per indexed family throughout the body",
            });
        }
        root.updates += usize::from(reads_cell && matches!(kind, IndexedFamilyKind::Reduce { .. }));
        root.kind = Some(kind);
    }

    fn indexed_origin(&self, target: &CheckedContainerRoot) -> Option<(BindingId, ResolvedPlace)> {
        let binding = target.binding()?;
        if self.introduced.contains(&binding)
            || self
                .summary_parameter
                .is_some_and(|parameter| parameter != binding)
        {
            return None;
        }
        let (position, index, _) = indexed_parts(target)?;
        let prefix = &target.path[..position];
        if !matches!(target.ty, CheckedType::Integer(_) | CheckedType::Bool)
            || !matches!(
                index.base_type,
                CheckedType::Array { .. }
                    | CheckedType::Buffer { .. }
                    | CheckedType::Window {
                        shape: WindowShape::Slots,
                        ..
                    }
            )
        {
            return None;
        }
        let resolved = self.places.resolve(
            target.root,
            &prefix
                .iter()
                .map(CheckedPlaceStep::place_step)
                .collect::<Vec<_>>(),
        );
        let [origin] = resolved.as_slice() else {
            return None;
        };
        Some((binding, origin.clone()))
    }

    fn indexed_overlaps(&self, root: &IndexedAccumulator, place: &ResolvedPlace) -> bool {
        self.places
            .resolve(PlaceRoot::Binding(root.binding), &[])
            .iter()
            .any(|owner| self.places.overlaps(&UnprovedSeparations, owner, place))
    }

    fn indexed_mentions(&self, root: &IndexedAccumulator, expression: &CheckedExpression) -> bool {
        let mut mentioned = false;
        visit_read_bindings(expression, &mut |binding| {
            mentioned |= binding == root.binding
                || self
                    .places
                    .resolve(PlaceRoot::Binding(binding), &[])
                    .iter()
                    .any(|place| self.indexed_overlaps(root, place));
        });
        mentioned
    }

    fn names_indexed_binding(&self, binding: BindingId) -> bool {
        self.indexed.iter().any(|root| {
            binding == root.binding
                || self
                    .places
                    .resolve(PlaceRoot::Binding(binding), &[])
                    .iter()
                    .any(|place| self.indexed_overlaps(root, place))
        }) || self.summary_parameter.is_some_and(|parameter| {
            binding == parameter
                || self
                    .places
                    .resolve(PlaceRoot::Binding(binding), &[])
                    .iter()
                    .any(|place| {
                        self.places.overlaps(
                            &UnprovedSeparations,
                            &ResolvedPlace::binding(parameter),
                            place,
                        )
                    })
        })
    }

    /// A selected root owns every one of its writes, including any malformed
    /// update or affine store mixed into the same binding.
    fn indexed_target(
        &mut self,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        node: &NodePath,
        initializers: &[IndexedInitializer<'check>],
    ) -> bool {
        if self.indexed.is_empty() {
            return false;
        }
        let mut footprint = Footprint::default();
        set_target_place(self.places, target, node, &mut footprint);
        let exact = match target {
            CheckedSetTarget::Storage(target) => self.indexed.iter().position(|root| {
                self.indexed_origin(target).is_some_and(|(_, origin)| {
                    same_element_root(&root.origin, &origin)
                        && indexed_parts(target).is_some_and(|(_, _, fields)| fields == root.fields)
                })
            }),
            _ => None,
        };
        let Some(position) = exact.or_else(|| {
            self.indexed.iter().position(|root| {
                footprint
                    .writes
                    .iter()
                    .any(|write| self.indexed_overlaps(root, &write.place))
            })
        }) else {
            return false;
        };
        let temporary = if let CheckedExpression::Binding { binding, .. } = value {
            initializers.iter().find(|entry| {
                entry.binding == *binding && operation_arguments(entry.value).is_some()
            })
        } else {
            None
        };
        let update = temporary.map_or(value, |entry| entry.value);
        let copy = operation_arguments(update).and_then(|arguments| {
            arguments.iter().find_map(|operand| {
                let CheckedExpression::Binding { binding, .. } = operand else {
                    return None;
                };
                initializers.iter().find(|entry| {
                    entry.binding == *binding
                        && matches!(target, CheckedSetTarget::Storage(target)
                        if self.same_indexed_operand(target, entry.value))
                })
            })
        });
        let root = &self.indexed[position];
        let result = self.indexed_combine(root, target, update, copy).and_then(|kind| {
            let mut support = Footprint::default();
            collect_operand_reads(self.places, update, node, &mut support);
            if let Some(copy) = copy {
                collect_operand_reads(self.places, copy.value, node, &mut support);
            }
            for entry in [copy, temporary].into_iter().flatten() {
                let root_read = self.reads[entry.read_end..].iter().any(|read| {
                    read.binding == root.binding || read.places.iter().any(|place| self.indexed_overlaps(root, place))
                });
                let changed = self.write_events[entry.write_end..].iter().any(|write| {
                    self.indexed_overlaps(root, write) || support.operand_reads.iter().any(|read| {
                        self.places.overlaps(&UnprovedSeparations, &read.place, write)
                    })
                });
                if root_read || changed || support.unresolved.is_some() {
                    return Err("a copied cell or single-use temporary requires no intervening root access or write to index/contribution support");
                }
            }
            Ok(kind)
        });
        match result {
            Ok(kind) => {
                for (entry, copied) in [(copy, true), (temporary, false)] {
                    if let Some(entry) = entry {
                        self.indexed_temporaries
                            .push((entry.binding, node.clone(), copied));
                    }
                }
                self.record_indexed_kind(position, kind, node, true);
            }
            Err(reason) => {
                self.indexed_denial
                    .get_or_insert(LoopDenial::IndexedReduction {
                        statement: node.clone(),
                        reason,
                    });
            }
        }
        for write in footprint.writes {
            self.record_written_place(&write.place);
        }
        if let Some(argument) = footprint.unresolved {
            self.unresolved.get_or_insert(argument);
        }
        match target {
            CheckedSetTarget::Storage(target) => {
                for offset in target.offsets() {
                    self.expression(offset);
                }
            }
            CheckedSetTarget::RangeIndex(target) => {
                for offset in target.offsets() {
                    self.expression(offset);
                }
            }
            CheckedSetTarget::Place(_) => {}
        }
        true
    }

    fn indexed_combine(
        &self,
        root: &IndexedAccumulator,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        copy: Option<&IndexedInitializer<'_>>,
    ) -> Result<IndexedFamilyKind, &'static str> {
        let CheckedSetTarget::Storage(target) = target else {
            return Err(
                "every write must update an indexed cell; the root and its length stay unchanged",
            );
        };
        let Some((_, origin)) = self.indexed_origin(target) else {
            return Err(
                "the target must be an integer or Bool cell of one outside Array or Slots root",
            );
        };
        if !same_element_root(&origin, &root.origin) {
            return Err(
                "indexed updates of overlapping outside bindings must use one fixed resolved storage path",
            );
        }
        let Some((_, index, fields)) = indexed_parts(target) else {
            return Err("the target must be a subscripted cell or record field");
        };
        if fields != root.fields {
            return Err("a field family cannot share its root with a whole-element write");
        }
        if !self.obligations.iter().any(|outcome| {
            outcome.family == ObligationFamily::Bounds
                && outcome.node_path == index.obligation
                && outcome.discharged
        }) {
            return Err("the indexed write requires its ordinary discharged OP-4 bound");
        }
        if self.indexed_mentions(root, &index.offset) {
            return Err("the subscript must read nothing of the indexed root");
        }
        if let CheckedExpression::Constant(constant)
        | CheckedExpression::NamedConstant {
            value: constant, ..
        } = value
            && matches!(
                constant,
                CheckedValue::Integer { .. } | CheckedValue::Bool(_)
            )
        {
            return Ok(IndexedFamilyKind::Mark {
                constant: constant.clone(),
            });
        }
        let (combine, arguments) = match value {
            CheckedExpression::IntegerOperation {
                operation,
                operand_type,
                arguments,
                ..
            } => (integer_combine(*operation, *operand_type), arguments),
            CheckedExpression::BooleanOperation {
                operation,
                arguments,
                ..
            } => (boolean_combine(*operation), arguments),
            _ => {
                return Err(
                    "each indexed write requires an admitted operation directly or through one fresh, unchanged, single-use temporary",
                );
            }
        };
        let Some(combine) = combine else {
            return Err(
                "the indexed operation must belong to the scalar accumulator's admitted set",
            );
        };
        let [left, right] = arguments.as_slice() else {
            return Err("the indexed operation must have two operands");
        };
        let Some((_, contribution)) = [(left, right), (right, left)]
            .into_iter()
            .find(|(operand, _)| self.same_indexed_operand(target, operand)
                || copy.is_some_and(|copy| matches!(operand, CheckedExpression::Binding { binding, .. } if *binding == copy.binding)))
        else {
            return Err("one operand must name exactly the target's subscripted place");
        };
        if self.indexed_mentions(root, contribution) {
            return Err("the contribution must read nothing of the indexed root");
        }
        Ok(IndexedFamilyKind::Reduce { op: combine })
    }

    fn same_indexed_operand(
        &self,
        target: &CheckedContainerRoot,
        operand: &CheckedExpression,
    ) -> bool {
        let CheckedExpression::ReadStorage { root, .. } = operand else {
            return self.same_legacy_indexed_operand(target, operand);
        };
        // GRAM-9 operands and subscript offsets are atoms, so no operation
        // can change a binding between the target and operand evaluations.
        // Captures retain different occurrence IDs, but identical typed paths
        // and offset atoms in this one statement select the same cell.
        let Some((written_position, _, _)) = indexed_parts(target) else {
            return false;
        };
        let Some((read_position, _, _)) = indexed_parts(root) else {
            return false;
        };
        let written = self
            .places
            .resolve(target.root, &target.place_path()[..written_position]);
        let read = self
            .places
            .resolve(root.root, &root.place_path()[..read_position]);
        same_update_path(
            &target.path[written_position..],
            &root.path[read_position..],
        ) && matches!((written.as_slice(), read.as_slice()), ([written], [read]) if same_element_root(written, read))
    }

    fn same_legacy_indexed_operand(
        &self,
        target: &CheckedContainerRoot,
        operand: &CheckedExpression,
    ) -> bool {
        let Some((CheckedPlaceStep::Subscript(index), prefix)) = target.path.split_last() else {
            return false;
        };
        let (binding, path, offset) = match operand {
            CheckedExpression::ArrayIndex {
                root: CheckedArrayRoot::Binding { binding, fields },
                offset,
                ..
            } => (*binding, field_steps(fields), offset.as_ref()),
            CheckedExpression::BufferIndex { root, offset, .. } => {
                (root.binding, root.place_path(), offset.as_ref())
            }
            _ => return false,
        };
        let same_offset = same_update_atom(&index.offset, offset);
        let written = self.places.resolve(
            target.root,
            &prefix
                .iter()
                .map(CheckedPlaceStep::place_step)
                .collect::<Vec<_>>(),
        );
        let read = self.places.resolve(PlaceRoot::Binding(binding), &path);
        same_offset
            && matches!((written.as_slice(), read.as_slice()), ([written], [read]) if same_element_root(written, read))
    }

    fn indexed_state(&self) -> Option<LoopDenial> {
        if let Some(denial) = &self.indexed_denial {
            return Some(denial.clone());
        }
        for (binding, statement, copied) in &self.indexed_temporaries {
            let uses = self
                .reads
                .iter()
                .filter(|read| read.binding == *binding)
                .count();
            let place = ResolvedPlace::binding(*binding);
            if uses != 1
                || self
                    .written_places
                    .iter()
                    .any(|written| self.places.overlaps(&UnprovedSeparations, &place, written))
            {
                return Some(LoopDenial::IndexedReduction {
                    statement: statement.clone(),
                    reason: if *copied {
                        "an indexed cell copy must be immutable and used exactly once as the accumulator operand"
                    } else {
                        "an indexed update temporary must be immutable and used exactly once by its set"
                    },
                });
            }
        }
        for (position, root) in self.indexed.iter().enumerate() {
            for other in &self.indexed[..position] {
                // Preserve the fixed storage path for one outside binding,
                // and apply it to references reaching that same owner too.
                if !self.indexed_overlaps(root, &other.origin)
                    && !self.indexed_overlaps(other, &root.origin)
                {
                    continue;
                }
                if !same_element_root(&root.origin, &other.origin) {
                    return Some(LoopDenial::IndexedReduction {
                        statement: root.statement.clone(),
                        reason: "indexed updates of overlapping outside bindings must use one fixed resolved storage path",
                    });
                }
                if root.fields.starts_with(&other.fields) || other.fields.starts_with(&root.fields)
                {
                    return Some(LoopDenial::IndexedReduction {
                        statement: root.statement.clone(),
                        reason: "indexed families require pairwise disjoint cell projections",
                    });
                }
            }
        }
        let mut bindings = self
            .indexed
            .iter()
            .map(|root| root.binding)
            .collect::<Vec<_>>();
        bindings.extend(self.summary_parameter);
        bindings.sort();
        bindings.dedup();
        for binding in bindings {
            let owners = self.places.resolve(PlaceRoot::Binding(binding), &[]);
            let families = self
                .indexed
                .iter()
                .filter(|root| {
                    owners.iter().any(|owner| {
                        self.places
                            .overlaps(&UnprovedSeparations, owner, &root.origin)
                    })
                })
                .collect::<Vec<_>>();
            let reads = self
                .reads
                .iter()
                .filter(|read| {
                    let measure = !read.places.is_empty()
                        && read.places.iter().all(|place| {
                            if self.summary_parameter == Some(binding) {
                                ((read.measure || read.element_measure)
                                    || matches!(place.path.last(), Some(PlaceStep::Measure(_))))
                                    && owners.iter().any(|owner| owner.contains(place))
                            } else {
                                families
                                    .iter()
                                    .any(|root| read.is_root_measure(place, &root.origin))
                            }
                        });
                    !measure
                        && (read.binding == binding
                            || read.places.iter().any(|place| {
                                owners.iter().any(|owner| {
                                    self.places.overlaps(&UnprovedSeparations, owner, place)
                                })
                            })
                            || self
                                .places
                                .resolve(PlaceRoot::Binding(read.binding), &[])
                                .iter()
                                .any(|place| {
                                    owners.iter().any(|owner| {
                                        self.places.overlaps(&UnprovedSeparations, owner, place)
                                    })
                                }))
                })
                .count();
            let expected_reads: usize = families.iter().map(|family| family.updates).sum();
            if reads != expected_reads {
                return Some(LoopDenial::IndexedReduction {
                    statement: families
                        .first()
                        .map_or_else(|| self.cite.clone(), |root| root.statement.clone()),
                    reason: "every occurrence of the indexed root must be a root measure or belong to its cell update; prefix reads, checks of cells and root-dependent subscripts or contributions are not permitted",
                });
            }
        }
        None
    }

    /// Walks one block, carrying how many value initializers the *body* opens
    /// around it.
    ///
    /// A `give` delivers to the innermost value initializer enclosing it
    /// [GIVE-1]. When that initializer is written inside B the `give` reaches
    /// a binding of this same iteration and leaves nothing; when the loop is
    /// written inside the initializer instead, the `give` leaves the loop
    /// *and* the initializer, and a combination tree over the whole range has
    /// no representation for that edge. The count tells the two apart, and it
    /// is reckoned from L's body, which is why judging a nested loop starts
    /// it again at zero.
    fn walk(&mut self, statements: &'check [CheckedStatement], initializers: usize) {
        let mut bindings = Vec::new();
        for statement in statements {
            if let Some(node) = statement_node(statement) {
                self.cite = node.clone();
            }
            self.statement(statement, initializers, &bindings);
            let inside = initializers
                + usize::from(matches!(statement, CheckedStatement::ValueMatchLet { .. }));
            for nested in nested_bodies(statement) {
                self.walk(nested, inside);
            }
            // Initializations are intervening writes too, but are not
            // subsequent mutations of the fresh copy/temporary itself.
            match statement {
                CheckedStatement::Let { binding, .. }
                | CheckedStatement::PropagateLet { binding, .. }
                | CheckedStatement::ValueMatchLet { binding, .. } => {
                    self.write_events.push(ResolvedPlace::binding(*binding));
                }
                CheckedStatement::DestructuringLet { bindings, .. } => {
                    self.write_events.extend(
                        bindings
                            .iter()
                            .map(|(binding, _, _)| ResolvedPlace::binding(*binding)),
                    );
                }
                _ => {}
            }
            if let CheckedStatement::Let { binding, value, .. } = statement {
                bindings.push(IndexedInitializer {
                    binding: *binding,
                    value,
                    read_end: self.reads.len(),
                    write_end: self.write_events.len(),
                });
            }
        }
    }

    /// One body statement. The match is exhaustive on purpose: every form is
    /// either given a footprint here or refused here.
    fn statement(
        &mut self,
        statement: &'check CheckedStatement,
        initializers: usize,
        bindings: &[IndexedInitializer<'check>],
    ) {
        match statement {
            CheckedStatement::Let {
                node_path,
                binding,
                value,
                ..
            } => {
                // A range reference bound inside B is the proved-range
                // family's own formation [REF-4, PAR-2]; every other `let`
                // binds iteration-own storage, which its binding being in
                // `introduced` already states.
                self.record_range_reference(*binding, value, node_path);
                self.moved_places(value, node_path);
                self.expression(value);
            }
            // [GRAM-4] a binder list over a call's ordered result list
            // [CALL-4], or a destructuring consume of `move place`: each
            // binder is a new binding of this iteration, as a `let`'s is,
            // which `introduced` already states, and the statement's
            // footprint is its right-hand side's: the call's projected row
            // and operand reads, and the places it consumes, whose fields a
            // final `..` releases with them.
            CheckedStatement::DestructuringLet {
                node_path, value, ..
            } => {
                self.moved_places(value, node_path);
                self.expression(value);
            }
            CheckedStatement::Set {
                node_path,
                target,
                value,
                ..
            } => {
                // A reference rebinding writes no storage, but its current
                // origin is flow-sensitive and may be carried from a prior
                // iteration. This static permission survey must not resolve
                // it through the binding's initial path and mistake that
                // path for iteration-own storage. Refuse the loop rather
                // than widen permission; ordinary sequential acceptance is
                // unaffected.
                if matches!(target, CheckedSetTarget::Place(place)
                    if place.fields.is_empty() && self.places.is_reference(place.binding))
                {
                    if matches!(target, CheckedSetTarget::Place(place)
                        if self.summary_parameter == Some(place.binding))
                    {
                        self.indexed_denial
                            .get_or_insert(LoopDenial::IndexedReduction {
                                statement: node_path.clone(),
                                reason: "an indexed helper parameter must retain its referent",
                            });
                    }
                    self.shared.get_or_insert(node_path.clone());
                    self.moved_places(value, node_path);
                    self.expression(value);
                    return;
                }
                if self.indexed_target(target, value, node_path, bindings) {
                    self.moved_places(value, node_path);
                    self.expression(value);
                    return;
                }
                let combine = match target {
                    CheckedSetTarget::Place(place) if place.fields.is_empty() => {
                        combine_of(place.binding, value)
                    }
                    _ => None,
                };
                self.written_target(target, node_path, combine);
                self.moved_places(value, node_path);
                self.expression(value);
            }
            // A source proof is checked before permission and erased before
            // lowering. It has no runtime footprint or exit edge.
            CheckedStatement::Proof(_) => {}
            CheckedStatement::Return {
                node_path, value, ..
            } => {
                if self.summary_parameter.is_some() {
                    self.moved_places(value, node_path);
                    self.expression(value);
                } else {
                    self.leaves("a return");
                }
            }
            CheckedStatement::Give {
                node_path, value, ..
            } => {
                if initializers == 0 {
                    self.leaves("a give");
                } else {
                    self.moved_places(value, node_path);
                    self.expression(value);
                }
            }
            CheckedStatement::PropagateLet {
                node_path,
                scrutinee,
                ..
            } => {
                if self.summary_parameter.is_some() {
                    self.moved_places(scrutinee, node_path);
                    self.expression(scrutinee);
                } else {
                    self.leaves("a propagate");
                }
            }
            CheckedStatement::Continue { target, .. } => {
                if Some(*target) != self.outer_loop && !self.inner_loops.contains(&target.0) {
                    self.leaves("a continue to an enclosing loop");
                }
            }
            CheckedStatement::Break { target, .. } => {
                if !self.inner_loops.contains(&target.0) || Some(*target) == self.outer_loop {
                    self.leaves("a break");
                }
            }
            CheckedStatement::Match { scrutinee, .. }
            | CheckedStatement::ValueMatchLet { scrutinee, .. } => {
                self.expression(scrutinee);
                self.record_match_dispatch_reads(scrutinee);
            }
            // A nested loop is judged on its own terms elsewhere; here its
            // endpoint atoms are two ordinary reads this iteration performs.
            // No rule joins two index ranges into one iteration space.
            CheckedStatement::CountedRange { lower, upper, .. } => {
                self.expression(lower);
                self.expression(upper);
            }
            CheckedStatement::Loop { .. } => {}
            // [SHARE-2] an atomic statement counts as a waiting call, which
            // condition 5 already refuses by its node.
            CheckedStatement::Atomic { .. } => self.refuse_form("an atomic statement"),
            // [GRAM-4] an expression statement is one call whose result is
            // discarded, judged exactly as a `let` binding that call is: its
            // row's projection, its operand reads, and its by-value
            // consumptions. It introduces no binding, and the release a
            // discarded affine result runs contributes no path [STOR-8].
            CheckedStatement::Evaluate { node_path, value }
            | CheckedStatement::DropExpression {
                node_path, value, ..
            } => {
                self.moved_places(value, node_path);
                self.expression(value);
            }
        }
    }

    /// The place one `set` writes, against condition 2 and, when the target is
    /// a whole binding of an enclosing scope, condition 1.
    fn written_target(
        &mut self,
        target: &'check CheckedSetTarget,
        node: &NodePath,
        combine: Option<LoopCombine>,
    ) {
        // The target's place is formed exactly as the adjacency judgment
        // forms one, so both judgments read one place relation.
        let mut footprint = Footprint::default();
        set_target_place(self.places, target, node, &mut footprint);
        if let Some(argument) = &footprint.unresolved {
            self.unresolved.get_or_insert(argument.clone());
        }
        let affine_map = self.proven_affine_map(target);
        for write in &footprint.writes {
            self.record_written_place(&write.place);
            self.reject_summary_write(&write.place, node);
            if self.is_iteration_own(&write.place) {
                continue;
            }
            if self.record_certified_write(&write.place, node) {
                continue;
            }
            if self.record_range_write(&write.place) {
                continue;
            }
            if let Some((after, map)) = affine_map
                && let Some((root, element)) = element_prefix(&write.place, after)
            {
                let page = matches!(element.path.last(), Some(PlaceStep::Page(_)));
                self.record_element_write(root, node.clone(), map, page);
                continue;
            }
            self.enclosing_write(target, &write.place, node, combine);
        }
        // [GRAM-9] makes a subscript an atom, so the offset reads storage and
        // calls nothing; it is walked anyway, because a read of the running
        // total spelled in a subscript is a read like any other.
        match target {
            CheckedSetTarget::Place(_) => {}
            CheckedSetTarget::RangeIndex(target) => {
                for offset in target.offsets() {
                    self.expression(offset);
                }
            }
            CheckedSetTarget::Storage(target) => {
                for offset in target.offsets() {
                    self.expression(offset);
                }
            }
        }
    }

    /// The outermost subscript of a written target whose exact
    /// single-binder affine image [ENT] was retained beside its successful
    /// [OP-4] outcome, with the number of subscripts written below it; a
    /// `Ring` base is refused that position.
    ///
    /// [PAR-2]'s element family is every access at or below one mapped
    /// element: everything below element i is storage element i alone owns
    /// [TYPE-8, TYPE-9], so the steps written after the mapped subscript are
    /// free. Permission neither evaluates the source expression nor reruns
    /// proof: absence of this checked evidence fails closed. Whether the
    /// subscript's root is an own binding or a reference parameter whose row
    /// declares the write was already decided when [SET-1] formed a writable
    /// target.
    fn proven_affine_map(
        &self,
        target: &CheckedSetTarget,
    ) -> Option<(usize, ProvedAffineIndexMap)> {
        match target {
            // [REF-4] the outer position selects from a range.
            CheckedSetTarget::RangeIndex(target) => {
                self.outermost_map(range_element_subscripts(target))
            }
            CheckedSetTarget::Storage(target) => self.outermost_map(path_subscripts(&target.path)),
            // A whole-place target is no element.
            CheckedSetTarget::Place(_) => None,
        }
    }

    /// The first subscript, outermost first, whose retained map is affine in
    /// this loop's binder, with the number of subscripts after it.
    fn outermost_map(
        &self,
        subscripts: Vec<(&NodePath, bool)>,
    ) -> Option<(usize, ProvedAffineIndexMap)> {
        let count = subscripts.len();
        subscripts
            .into_iter()
            .enumerate()
            .find_map(|(position, (obligation, ring))| {
                (!ring)
                    .then(|| self.proven_affine_map_at(obligation))
                    .flatten()
                    .map(|map| (count - 1 - position, map))
            })
    }

    /// Reads the retained [OP-4] map for one subscript occurrence.
    fn proven_affine_map_at(&self, obligation: &NodePath) -> Option<ProvedAffineIndexMap> {
        self.obligations
            .iter()
            .find(|outcome| {
                outcome.family == ObligationFamily::Bounds
                    && outcome.node_path == *obligation
                    && outcome.discharged
            })?
            .affine_index_maps
            .iter()
            .copied()
            .find(|map| Some(map.loop_id) == self.outer_loop && map.coefficient != 0)
    }

    /// One `let` that binds a range reference [REF-4], against [PAR-2]'s
    /// proved-range family.
    ///
    /// The formation's endpoint images are retained beside its discharged
    /// [REF-4] obligation, keyed by the capture the range step carries. Where
    /// no such image exists the reference is an ordinary binding and denies
    /// nothing by itself; what it is passed to is judged by that call's row.
    fn record_range_reference(
        &mut self,
        binding: BindingId,
        value: &CheckedExpression,
        node: &NodePath,
    ) {
        if !matches!(value, CheckedExpression::RangeOf { .. }) {
            return;
        }
        let resolved = self.places.resolve(PlaceRoot::Binding(binding), &[]);
        self.record_range_formation(&resolved, value, node);
    }

    /// Every range one call forms at its own arguments [REF-4, PAR-2].
    ///
    /// "A range reference `&r[s*i+b..s*i+b+s]` passed as an ordinary
    /// argument" is the family's formation whether a `let` names it first or
    /// the argument forms it at the call; the argument then names the same
    /// path the bound reference would [EFF-5]. An argument whose places do
    /// not resolve records nothing here: the call's own footprint carries it
    /// as unresolved, which is condition 3's denial.
    fn record_range_arguments(&mut self, arguments: &[CheckedExpression]) {
        for argument in arguments {
            if !matches!(argument, CheckedExpression::RangeOf { .. }) {
                continue;
            }
            let Some(resolved) = argument_places(self.places, argument) else {
                continue;
            };
            let node = self.cite.clone();
            self.record_range_formation(&resolved, argument, &node);
        }
    }

    /// One range formation whose resolved places are `resolved`, against the
    /// proved-range family.
    ///
    /// "The indexable place or range reference it is formed from is declared
    /// outside B and retains its resolved origin" [PAR-2]. The partition
    /// `[s*i+b, s*i+b+s)` is relative to that origin, so only an origin fixed
    /// throughout L makes two iterations' ranges disjoint: a source bound
    /// inside B may carry a range step whose endpoints change with i, and
    /// [OWN-7] leaves two different frames overlapping whatever lies below
    /// them. A formation over such a source is recorded as nothing of its
    /// own. "A range reference formed inside B instead inherits an existing
    /// proved range reference only when its complete origin path is a
    /// descendant of that range reference", which is the containment
    /// `record_range_write` asks of every write; anything else it writes is
    /// a shared write.
    fn record_range_formation(
        &mut self,
        resolved: &[ResolvedPlace],
        value: &CheckedExpression,
        node: &NodePath,
    ) {
        let CheckedExpression::RangeOf {
            source,
            obligation,
            captured,
            ..
        } = value
        else {
            return;
        };
        if source
            .binding()
            .is_some_and(|binding| self.introduced.contains(&binding))
        {
            return;
        }
        let [place] = resolved else {
            self.shared.get_or_insert(node.clone());
            return;
        };
        let Some(PlaceStep::Range(range)) = place.path.last() else {
            return;
        };
        let origin = ResolvedPlace {
            atomic_aliases: place.atomic_aliases.clone(),
            root: place.root,
            path: place.path[..place.path.len() - 1].to_vec(),
        };
        let Some(map) = self
            .obligations
            .iter()
            .filter(|outcome| {
                outcome.discharged
                    && outcome.family == ObligationFamily::RangeFormation
                    && outcome.node_path == *obligation
            })
            .flat_map(|outcome| &outcome.range_partitions)
            .find(|partition| {
                Some(partition.loop_id) == self.outer_loop
                    && partition.range == captured.start.capture
                    && partition.range == range.start.capture
            })
            .cloned()
        else {
            return;
        };
        // Compare partitions only after the complete body's footprints have
        // selected written origins. Overlapping read-only input ranges need
        // no common map, and forming a range reads no element content.
        self.range_references.push(ProvenRangeReference {
            origin,
            place: place.clone(),
            argument: obligation.clone(),
            map,
            written: false,
        });
    }

    /// The proved range reference whose extent contains this place, when one
    /// does. A descendant of a proved range inherits its per-iteration
    /// extent; nothing else does.
    fn record_range_write(&mut self, place: &ResolvedPlace) -> bool {
        let Some(reference) = self
            .range_references
            .iter_mut()
            .find(|reference| reference.place.contains(place))
        else {
            return false;
        };
        reference.written = true;
        true
    }

    /// One element write the loop's certificate separates [RANGE-5]: a
    /// write by a `set` statement or a call the certificate names, of a
    /// place with an index step. Its root is the place above the first
    /// index or range step, which coverage then holds every read of to the
    /// certificate's reads.
    fn record_certified_write(&mut self, place: &ResolvedPlace, node: &NodePath) -> bool {
        let Some(certificate) = self.certificate else {
            return false;
        };
        if !certificate.writes.contains(node) {
            return false;
        }
        let Some(first) = place
            .path
            .iter()
            .position(|step| matches!(step, PlaceStep::Index(_) | PlaceStep::Range(_)))
        else {
            return false;
        };
        if !matches!(place.path[first], PlaceStep::Index(_))
            && !place.path[first..]
                .iter()
                .any(|step| matches!(step, PlaceStep::Index(_) | PlaceStep::Page(_)))
        {
            return false;
        }
        self.certified_writes.push(CertifiedElementWrite {
            root: ResolvedPlace {
                atomic_aliases: place.atomic_aliases.clone(),
                root: place.root,
                path: place.path[..first].to_vec(),
            },
            statement: node.clone(),
        });
        true
    }

    /// One write into storage that outlives the iteration.
    ///
    /// An accumulator is a whole binding of an enclosing scope, named
    /// directly. A target spelled through a reference resolves to the storage
    /// the reference names, and this judgment counts reads by binding, so
    /// such a target is refused rather than accumulated. That refusal is
    /// deliberate and not an artifact of the combine test.
    fn enclosing_write(
        &mut self,
        target: &CheckedSetTarget,
        place: &ResolvedPlace,
        node: &NodePath,
        combine: Option<LoopCombine>,
    ) {
        let named = match target {
            CheckedSetTarget::Place(target) if target.fields.is_empty() => Some(target.binding),
            // Every element form not consumed as an affine map above, and
            // every field of enclosing storage, remains one shared place and
            // fails closed.
            _ => None,
        };
        let Some(binding) = named.filter(|binding| {
            !self.places.is_reference(*binding) && *place == ResolvedPlace::binding(*binding)
        }) else {
            self.shared.get_or_insert(node.clone());
            return;
        };
        match combine {
            Some(combine) => self.accumulates.push(Accumulate {
                binding,
                combine,
                statement: node.clone(),
            }),
            None => {
                self.carried.get_or_insert(node.clone());
            }
        }
    }

    /// Whether a resolved place is storage this iteration introduced. Storage
    /// rooted in a binding the body opens is created and released inside the
    /// iteration, so no two iterations reach one of them.
    fn is_iteration_own(&self, place: &ResolvedPlace) -> bool {
        match place.root {
            PlaceRoot::Binding(binding) => self.introduced.contains(&binding),
            // A named const [CONST-2] is enclosing storage. Nothing writes
            // one, so this arm exists to keep the classification total.
            PlaceRoot::Constant(_) => false,
        }
    }

    /// Retains every ordinary source read occurrence and the proved-map
    /// detail of each direct element read.
    ///
    /// One occurrence is recorded per expression node, because condition 1
    /// counts the accumulator's read occurrences and a deduplicated or
    /// per-statement count cannot tell one read from two. The two records are
    /// emitted by this one recursive walk, so a proved element read can never
    /// exist without its ordinary occurrence and the two counts
    /// `element_map_coverage` compares stay in step.
    ///
    /// The match is exhaustive on purpose: an expression form that reads
    /// caller storage and is not classified here would leave the read out of
    /// condition 2 and *widen* permission.
    fn record_reads(&mut self, expression: &CheckedExpression) {
        let (expression, projection) = binding_read_projection(expression);
        if let Some(call) = call_projection(expression)
            && self
                .indexed_calls
                .iter()
                .any(|update| update.call == *call.call)
        {
            for (position, argument) in call.arguments.iter().enumerate() {
                if !self
                    .indexed_calls
                    .iter()
                    .any(|update| update.call == *call.call && update.argument == position)
                {
                    self.record_reads(argument);
                }
            }
            return;
        }
        // Reference formation is also an occurrence for an indexed root,
        // even though it reads no element and contributes no map footprint.
        if let CheckedExpression::RangeOf { source, .. } = expression
            && let Some(binding) = source.binding()
            && self.names_indexed_binding(binding)
        {
            self.reads.push(ReadOccurrence {
                binding,
                places: Vec::new(),
                carrier: None,
                measure: false,
                page_descriptor: false,
                element_measure: false,
            });
        }
        let occurrence = match expression {
            // Forming a reference reads no content, but naming an
            // accumulator outside its one combine operand still violates
            // PAR-2's occurrence restriction. Keep the occurrence without
            // inventing an element-read footprint for address formation.
            CheckedExpression::BorrowSegment { root, segment, .. } => {
                if matches!(segment, super::CheckedSegmentSelect::Page(_)) {
                    let (spelling, mut path) = root.place();
                    path.push(PlaceStep::Measure(CheckedMeasure::Length));
                    let places = self.places.resolve(spelling, &path);
                    let subscripts = match root {
                        super::CheckedSegmentSource::Storage(root) => path_subscripts(&root.path),
                        super::CheckedSegmentSource::Element(place) => range_element_subscripts(place),
                    };
                    self.record_element_reads(subscripts, &places);
                    root.binding().map(|binding| (binding, places))
                } else {
                    if let Some(binding) = root.binding() {
                        self.reads.push(ReadOccurrence {
                            binding,
                            places: Vec::new(),
                            carrier: None,
                            measure: false,
                            page_descriptor: false,
                            element_measure: false,
                        });
                    }
                    None
                }
            }
            CheckedExpression::BorrowAddressed { root, .. } => {
                if let Some(binding) = root.binding() {
                    self.reads.push(ReadOccurrence {
                        binding,
                        places: Vec::new(),
                        carrier: None,
                        measure: false,
                        page_descriptor: false,
                        element_measure: false,
                    });
                }
                None
            }
            CheckedExpression::BorrowRangeIndex { place, .. } => {
                self.reads.push(ReadOccurrence {
                    binding: place.root.binding,
                    places: Vec::new(),
                    carrier: None,
                    measure: false,
                    page_descriptor: false,
                    element_measure: false,
                });
                None
            }
            // A subscripted storage read or measure: the discharged [OP-4]
            // image of its outermost mapped subscript is what puts it in the
            // element family, and a `Ring` base is refused that position
            // [WIN-1].
            CheckedExpression::ContainerMeasure { root, .. }
            | CheckedExpression::ReadStorage { root, .. } => {
                let places = self.places.resolve(root.root, &container_steps(root));
                self.record_element_reads(path_subscripts(&root.path), &places);
                root.binding().map(|binding| (binding, places))
            }
            // Copying a reference reads its name, not the storage it names
            // [REF-1, TYPE-7]. A call's projected row records any referent
            // read; reference rebinding is separately refused by statement.
            CheckedExpression::Binding { binding, .. }
                if self.places.is_reference(*binding)
                    && !self.names_indexed_binding(*binding) => None,
            CheckedExpression::Binding { binding, .. }
            | CheckedExpression::DerefAddressed { binding, .. } => Some((
                *binding,
                self.places.resolve(PlaceRoot::Binding(*binding), &projection),
            )),
            // [REF-4, MSR-2] a read through a range reference reads the path
            // the reference names; its own offset is this node's child.
            CheckedExpression::RangeMeasure { root, .. } => {
                let places = self.places.resolve(PlaceRoot::Binding(root.binding), &root.place_path());
                self.record_element_reads(range_root_subscripts(root), &places);
                Some((root.binding, places))
            },
            CheckedExpression::RangeElementMeasure { place, .. } => {
                let places = self.places.resolve(
                    PlaceRoot::Binding(place.root.binding),
                    &place.place_path(),
                );
                self.record_element_reads(range_element_subscripts(place), &places);
                Some((place.root.binding, places))
            }
            // A read through a range first selects its outer element and may
            // then select a nested element; the outermost mapped subscript
            // puts it in the element family.
            CheckedExpression::RangeIndex { place, .. } => {
                let places = self.places.resolve(
                    PlaceRoot::Binding(place.root.binding),
                    &place.place_path(),
                );
                self.record_element_reads(range_element_subscripts(place), &places);
                Some((place.root.binding, places))
            }
            CheckedExpression::Project {
                binding, fields, ..
            } => Some((
                *binding,
                self.places
                    .resolve(PlaceRoot::Binding(*binding), &field_steps(fields)),
            )),
            // A direct subscript of a constant-capacity `Array` [TYPE-9]. Its
            // base is an `Array` by construction, so no `Ring` reaches here.
            CheckedExpression::ArrayIndex {
                root: CheckedArrayRoot::Binding { binding, fields },
                obligation,
                ..
            } => {
                let places = self
                    .places
                    .resolve(PlaceRoot::Binding(*binding), &field_steps(fields));
                if let Some(map) = self.proven_affine_map_at(obligation) {
                    for root in places.iter().cloned() {
                        self.element_reads.push(ProvenElementRead { root, map, page: false });
                    }
                }
                Some((*binding, places))
            }
            // A direct subscript of a runtime-capacity `Array` [TYPE-9]. Its
            // checked buffer root is the complete place above the selected
            // element, so it participates in the same retained affine-map
            // family as a constant-capacity `Array` subscript.
            CheckedExpression::BufferIndex {
                root, obligation, ..
            } => {
                let places = self
                    .places
                    .resolve(PlaceRoot::Binding(root.binding), &root.place_path());
                if let Some(map) = self.proven_affine_map_at(obligation) {
                    for root in places.iter().cloned() {
                        self.element_reads.push(ProvenElementRead { root, map, page: false });
                    }
                }
                Some((root.binding, places))
            }
            CheckedExpression::ArrayMeasure {
                root: CheckedArrayRoot::Binding { binding, fields },
                ..
            } => Some((
                *binding,
                self.places
                    .resolve(PlaceRoot::Binding(*binding), &field_steps(fields)),
            )),
            // A named const [CONST-2] is enclosing storage nothing writes,
            // and these forms read no caller storage of their own.
            CheckedExpression::ArrayMeasure {
                root: CheckedArrayRoot::Constant(_),
                ..
            }
            | CheckedExpression::ArrayIndex {
                root: CheckedArrayRoot::Constant(_),
                ..
            }
            | CheckedExpression::Constant(_)
            | CheckedExpression::NamedConstant { .. }
            | CheckedExpression::UserCall { .. }
            | CheckedExpression::IntegerOperation { .. }
            | CheckedExpression::FloatOperation { .. }
            | CheckedExpression::NumericConversion { .. }
            | CheckedExpression::Reinterpret { .. }
            | CheckedExpression::BooleanOperation { .. }
            | CheckedExpression::ValueEquality { .. }
            | CheckedExpression::ConstructStruct { .. }
            | CheckedExpression::ConstructEnum { .. }
            // Naming a path reads no element content [REF-1, REF-4]; the
            // endpoints are this node's children.
            | CheckedExpression::RangeOf { .. }
            | CheckedExpression::ProjectValue { .. } => None,
            CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::BoxDeref { .. }
            | CheckedExpression::BoxTake { .. } => {
                self.refuse_form("an expression form this version no longer writes");
                None
            }
        };
        // The carrier is what a certificate names an element read by; a
        // measure read selects descriptor storage no element write changes.
        let (carrier, measure) = match expression {
            CheckedExpression::ReadStorage { carrier, .. }
            | CheckedExpression::RangeIndex { carrier, .. }
            | CheckedExpression::RangeElementMeasure { carrier, .. }
            | CheckedExpression::ArrayIndex { carrier, .. }
            | CheckedExpression::BufferIndex { carrier, .. }
            | CheckedExpression::DerefAddressed { carrier, .. } => (Some(carrier.clone()), false),
            CheckedExpression::ContainerMeasure { .. }
            | CheckedExpression::RangeMeasure { .. }
            | CheckedExpression::ArrayMeasure { .. } => (None, true),
            CheckedExpression::BorrowSegment {
                segment: super::CheckedSegmentSelect::Page(_),
                ..
            } => (None, true),
            _ => (None, false),
        };
        if let Some((binding, places)) = occurrence {
            if places.is_empty() {
                self.unresolved.get_or_insert(self.cite.clone());
            } else {
                self.reads.push(ReadOccurrence {
                    binding,
                    places,
                    carrier,
                    measure,
                    page_descriptor: matches!(
                        expression,
                        CheckedExpression::BorrowSegment {
                            segment: super::CheckedSegmentSelect::Page(_),
                            ..
                        }
                    ),
                    element_measure: matches!(
                        expression,
                        CheckedExpression::RangeElementMeasure { .. }
                    ),
                });
            }
        }
        for child in expression_children(expression) {
            self.record_reads(child);
        }
    }

    /// Dispatch reads every resolved referent's tag, including its element
    /// map when the borrowed selection carries one [EFF-2, OWN-13, PAR-2].
    fn record_match_dispatch_reads(&mut self, scrutinee: &CheckedExpression) {
        let Some(referents) = match_referents(self.places, scrutinee) else {
            return;
        };
        if referents.is_empty() {
            self.unresolved.get_or_insert(self.cite.clone());
        }
        for element in self.element_arguments(std::slice::from_ref(scrutinee)) {
            self.element_reads.push(ProvenElementRead {
                root: element.root,
                map: element.map,
                page: element.page,
            });
        }
        for place in referents {
            let PlaceRoot::Binding(binding) = place.root else {
                continue;
            };
            self.reads.push(ReadOccurrence {
                binding,
                places: vec![place],
                carrier: scrutinee.carrier().cloned(),
                measure: false,
                page_descriptor: false,
                element_measure: false,
            });
        }
    }

    /// One proved element read per resolved place, when a subscript of the
    /// read carries a map affine in this loop's binder.
    fn record_element_reads(
        &mut self,
        subscripts: Vec<(&NodePath, bool)>,
        places: &[ResolvedPlace],
    ) {
        if let Some((after, map)) = self.outermost_map(subscripts) {
            for place in places {
                if let Some((root, element)) = element_prefix(place, after) {
                    let page = matches!(element.path.last(), Some(PlaceStep::Page(_)));
                    self.element_reads
                        .push(ProvenElementRead { root, map, page });
                }
            }
        }
    }

    /// The borrowed arguments of one call that lie at or below one mapped
    /// element [PAR-2]: `&a^[i]`, `&cells.inner[i].field` or a range formed
    /// below such an element. The call's projected row is then judged against
    /// these elements.
    fn element_arguments(&self, arguments: &[CheckedExpression]) -> Vec<ProvenElementReference> {
        let mut references = Vec::new();
        for argument in arguments {
            let subscripts = match argument {
                CheckedExpression::BorrowRangeIndex { place, .. } => {
                    range_element_subscripts(place)
                }
                CheckedExpression::BorrowAddressed { root, .. } => path_subscripts(&root.path),
                // [TYPE-9] distinct segments are distinct storage, so a
                // segment borrow is an element of its `Segments` place.
                CheckedExpression::BorrowSegment { root, segment, .. } => {
                    let mut subscripts = match root {
                        super::CheckedSegmentSource::Storage(root) => path_subscripts(&root.path),
                        super::CheckedSegmentSource::Element(place) => {
                            range_element_subscripts(place)
                        }
                    };
                    if let crate::semantic::CheckedSegmentSelect::One(index)
                    | crate::semantic::CheckedSegmentSelect::Page(index) = segment
                    {
                        subscripts.push((&index.obligation, false));
                    }
                    subscripts
                }
                CheckedExpression::RangeOf { source, .. } => match source {
                    CheckedRangeSource::Storage(root) => path_subscripts(&root.path),
                    CheckedRangeSource::Element(place) => range_element_subscripts(place),
                    CheckedRangeSource::Range(_) => continue,
                },
                _ => continue,
            };
            let Some((after, map)) = self.outermost_map(subscripts) else {
                continue;
            };
            let Some(resolved) = argument_places(self.places, argument) else {
                continue;
            };
            // A range formed below the element adds one range step, never an
            // index step, so the count of subscripts after the mapped one
            // locates it in every resolved place.
            for place in &resolved {
                if let Some((root, element)) = element_prefix(place, after) {
                    let page = matches!(element.path.last(), Some(PlaceStep::Page(_)));
                    references.push(ProvenElementReference {
                        root,
                        element,
                        map,
                        page,
                    });
                }
            }
        }
        references
    }

    /// The caller places one expression transfers away by consuming an `own`
    /// value. A move out of enclosing storage is a write of it [PAR-1].
    fn moved_places(&mut self, value: &CheckedExpression, node: &NodePath) {
        let mut footprint = Footprint::default();
        collect_consumed_places(self.places, value, node, &mut footprint);
        self.record_writes(&footprint, &[], None);
    }

    /// One expression tree: every read it performs, and every call it makes.
    ///
    /// The two walks are separate because the read walk descends the whole
    /// tree itself. Calling it at every level as well would count one read of
    /// an accumulator as several, and condition 1 reads that count.
    fn expression(&mut self, expression: &CheckedExpression) {
        self.record_reads(expression);
        self.calls(expression);
    }

    /// Every call one expression tree makes, with its [EFF-5] projection.
    ///
    /// The walk reaches a call wherever it is written rather than only as the
    /// whole right-hand side of a `let`, so no call's row escapes the
    /// footprint by the shape of the statement that holds it.
    fn calls(&mut self, expression: &CheckedExpression) {
        if let Some(projection) = call_projection(expression) {
            self.record_range_arguments(projection.arguments);
            let elements = self.element_arguments(projection.arguments);
            let mut footprint = self.program.footprint(self.places, &projection);
            let updates = self
                .indexed_calls
                .iter()
                .filter(|update| update.call == *projection.call)
                .map(|update| (update.argument, update.families.clone()))
                .collect::<Vec<_>>();
            for (argument, families) in updates {
                if let Some(node) = projection.argument_nodes.get(argument) {
                    footprint.reads.retain(|read| read.argument != *node);
                    footprint.writes.retain(|write| write.argument != *node);
                }
                for (position, kind) in families {
                    self.record_indexed_kind(position, kind, projection.call, false);
                    let origin = self.indexed[position].origin.clone();
                    self.record_written_place(&origin);
                }
            }
            self.record_writes(&footprint, &elements, Some(projection.call));
        }
        for child in expression_children(expression) {
            self.calls(child);
        }
    }

    /// Every write of one projected footprint, against condition 2.
    ///
    /// A call's reads are retained beside them: "the [EFF-2] projection of a
    /// helper's declared row counts as an access on its actual range", so a
    /// read reaching a mapped root or a proved origin is judged exactly as a
    /// source read occurrence is.
    fn record_writes(
        &mut self,
        footprint: &Footprint,
        elements: &[ProvenElementReference],
        call: Option<&NodePath>,
    ) {
        if let Some(argument) = &footprint.unresolved {
            self.unresolved.get_or_insert(argument.clone());
        }
        // The declared row's reads only: this statement's own operand
        // expressions are walked by `record_reads`, and counting them again
        // here would double every source read occurrence.
        for read in &footprint.reads {
            if let Some(element) = elements
                .iter()
                .find(|element| element.element.contains(&read.place))
            {
                self.element_reads.push(ProvenElementRead {
                    root: element.root.clone(),
                    map: element.map,
                    page: element.page,
                });
            }
            self.reads.push(ReadOccurrence {
                binding: match read.place.root {
                    PlaceRoot::Binding(binding) => binding,
                    // A const root is never an accumulator; the place still
                    // takes part in condition 2.
                    PlaceRoot::Constant(_) => continue,
                },
                places: vec![read.place.clone()],
                carrier: call.cloned(),
                measure: false,
                page_descriptor: false,
                element_measure: false,
            });
        }
        for write in &footprint.writes {
            self.record_written_place(&write.place);
            self.reject_summary_write(&write.place, &write.argument);
            if self
                .indexed
                .iter()
                .any(|root| self.indexed_overlaps(root, &write.place))
            {
                self.indexed_denial.get_or_insert(LoopDenial::IndexedReduction {
                    statement: write.argument.clone(),
                    reason: "every write must be a matching cell update; unsummarized calls, moves and length changes are not updates",
                });
            }
            if self.is_iteration_own(&write.place) {
                continue;
            }
            if let Some(call) = call
                && self.record_certified_write(&write.place, call)
            {
                continue;
            }
            if self.record_range_write(&write.place) {
                continue;
            }
            // [PAR-2] a row projected onto a borrow at or below one mapped
            // element writes only that element's storage.
            if let Some(element) = elements
                .iter()
                .find(|element| element.element.contains(&write.place))
            {
                self.record_element_write(
                    element.root.clone(),
                    write.argument.clone(),
                    element.map,
                    element.page,
                );
                continue;
            }
            self.shared.get_or_insert(write.argument.clone());
        }
    }

    fn record_written_place(&mut self, place: &ResolvedPlace) {
        self.write_events.push(place.clone());
        if !self.written_places.contains(place) {
            self.written_places.push(place.clone());
        }
    }

    /// One proved element write, refusing a second map on an overlapping
    /// root.
    fn record_element_write(
        &mut self,
        root: ResolvedPlace,
        statement: NodePath,
        map: ProvedAffineIndexMap,
        page: bool,
    ) {
        // [PAR-2] "Every write by B to one mapped root must be to a place in
        // a proved single-binder affine element of it carrying exactly the
        // same a and b". Two different maps on one root can cross between
        // iterations even where each is injective by itself, and so can two
        // roots one of which lies inside the other: `a` mapped by `a[i]` and
        // `a[j].xs` mapped by `a[j].xs[i]` both reach `a[j].xs`. Roots are
        // compared by the ordinary overlap relation [OWN-7]; this fixed rule
        // performs no pairwise range search.
        let oracle = UnprovedSeparations;
        if self.element_writes.iter().any(|written| {
            self.places.overlaps(&oracle, &written.root, &root)
                && !(same_element_root(&written.root, &root)
                    && written.map == map
                    && written.page == page)
        }) {
            self.shared.get_or_insert(statement.clone());
        }
        self.element_writes.push(ProvenElementWrite {
            root,
            statement,
            map,
            page,
        });
    }

    fn leaves(&mut self, edge: &'static str) {
        self.exit.get_or_insert(edge);
    }

    fn refuse_form(&mut self, form: &'static str) {
        self.form.get_or_insert(form);
    }

    /// The four conditions in their numbered order, then eligibility.
    ///
    /// A form refusal is reported ahead of all four: a statement whose
    /// footprint this judgment does not compute has no condition-1 or
    /// condition-2 answer to give, so a loop with several defects reports the
    /// unclassified form first, which is the honest report.
    fn finish(self, statement: NodePath) -> LoopPermission {
        let mut carried = Vec::new();
        for accumulate in &self.accumulates {
            if !carried.contains(&accumulate.combine) {
                carried.push(accumulate.combine);
            }
        }
        for root in &self.indexed {
            if let Some(IndexedFamilyKind::Reduce { op: combine }) = &root.kind
                && !carried.contains(combine)
            {
                carried.push(*combine);
            }
        }
        let combines = carried.iter().map(|combine| combine.spelling()).collect();
        let denial = self.denial();
        // The advice outlives exactly one refusal: a loop this version
        // declines only because it carries several accumulators is still one
        // a hand-written recursion returning an aggregate can split. Every
        // other refusal is a reason the split would be refused too.
        let advises_split =
            matches!(denial, Some(LoopDenial::ManyAccumulators { .. })) && !carried.is_empty();
        // A stateless loop is not a map merely because it is permitted. An
        // admitted element family is the positive witness which selects
        // IndependentMap; an accumulator selects Reduction, including a
        // reduction whose body also contains independent element maps.
        let actualization = if denial.is_some() {
            None
        } else if let Some(accumulate) = self.accumulates.first() {
            Some(LoopActualization::Reduction {
                accumulator: accumulate.binding,
                combine: accumulate.combine,
            })
        } else if self.indexed.is_empty()
            && self.element_writes.is_empty()
            && self.certified_writes.is_empty()
            && !self.range_references.iter().any(|range| range.written)
        {
            None
        } else {
            Some(LoopActualization::IndependentMap)
        };
        let permitted = denial.is_none();
        let verdict = match denial {
            Some(denial) => LoopVerdict::Denied(denial),
            None => LoopVerdict::PermittedEligible,
        };
        LoopPermission {
            statement,
            verdict,
            combines,
            advises_split,
            actualization,
            indexed: if permitted {
                self.indexed_payload()
            } else {
                Vec::new()
            },
            written_places: self.written_places,
        }
    }

    fn indexed_payload(&self) -> Vec<IndexedReduction> {
        self.indexed
            .iter()
            .filter_map(|root| {
                root.kind.clone().map(|kind| IndexedReduction {
                    root: root.root.clone(),
                    fields: root.fields.clone(),
                    value_type: root.value_type,
                    kind,
                    calls: root.calls.clone(),
                })
            })
            .collect()
    }

    fn reject_summary_write(&mut self, place: &ResolvedPlace, node: &NodePath) {
        if self.summary_parameter.is_some_and(|parameter| {
            self.places.overlaps(
                &UnprovedSeparations,
                &ResolvedPlace::binding(parameter),
                place,
            )
        }) {
            self.indexed_denial.get_or_insert(LoopDenial::IndexedReduction {
                statement: node.clone(),
                reason: "an indexed helper writes its parameter only through matching cell updates or summarized calls",
            });
        }
    }

    fn denial(&self) -> Option<LoopDenial> {
        if let Some(form) = self.form {
            return Some(LoopDenial::BodyForm { form });
        }
        if let Some(denial) = self.carried_state() {
            return Some(denial);
        }
        if let Some(denial) = self.indexed_state() {
            return Some(denial);
        }
        if let Some(argument) = &self.shared {
            return Some(LoopDenial::SharedWrite {
                argument: argument.clone(),
            });
        }
        if let Some(denial) = self.range_reference_coverage() {
            return Some(denial);
        }
        if let Some(denial) = self.element_map_coverage() {
            return Some(denial);
        }
        if let Some(denial) = self.certified_coverage() {
            return Some(denial);
        }
        if let Some(argument) = &self.unresolved {
            return Some(LoopDenial::UnresolvedWrite {
                argument: argument.clone(),
            });
        }
        self.exit.map(|edge| LoopDenial::Exit { edge })
    }

    /// Written origins must have one map. Reads of an origin's measures
    /// reach its unchanged descriptor; every element access must stay in
    /// that map's current-iteration extent. Read-only origins carry no
    /// cross-iteration write conflict.
    fn range_reference_coverage(&self) -> Option<LoopDenial> {
        let oracle = UnprovedSeparations;
        for reference in self.range_references.iter().filter(|range| range.written) {
            let incompatible_write = self.range_references.iter().any(|other| {
                other.written
                    && self
                        .places
                        .overlaps(&oracle, &reference.origin, &other.origin)
                    && (reference.origin != other.origin
                        || reference.map.stride != other.map.stride
                        || reference.map.base != other.map.base)
            });
            let covered = |place: &ResolvedPlace| {
                self.range_references.iter().any(|assigned| {
                    assigned.origin == reference.origin
                        && assigned.map.stride == reference.map.stride
                        && assigned.map.base == reference.map.base
                        && assigned.place.contains(place)
                })
            };
            let uncovered_read = self.reads.iter().any(|read| {
                read.places.iter().any(|place| {
                    self.places.overlaps(&oracle, &reference.origin, place)
                        && !read.is_root_measure(place, &reference.origin)
                        && !covered(place)
                })
            });
            let mixed_element_map = self.element_writes.iter().any(|written| {
                self.places
                    .overlaps(&oracle, &reference.origin, &written.root)
            });
            if incompatible_write || uncovered_read || mixed_element_map {
                return Some(LoopDenial::SharedWrite {
                    argument: reference.argument.clone(),
                });
            }
        }
        None
    }

    /// Every read through a mapped root is a measure of that root or a place
    /// in an element carrying the write's same a and b [PAR-2]. Measure reads
    /// reach descriptor storage that the admitted writes leave unchanged
    /// [MSR-2]. Every other read must match the element map; a whole-root
    /// read, different image, or unproved subscript leaves the counts unequal.
    fn element_map_coverage(&self) -> Option<LoopDenial> {
        let oracle = UnprovedSeparations;
        self.element_writes
            .iter()
            .find(|written| {
                let mut descriptor = written.root.clone();
                descriptor
                    .path
                    .push(PlaceStep::Measure(CheckedMeasure::Length));
                let reads = self
                    .reads
                    .iter()
                    .flat_map(|read| read.places.iter().map(move |place| (read, place)))
                    .filter(|(read, place)| {
                        !read.is_root_measure(place, &written.root)
                            && self.places.overlaps(&oracle, place, &written.root)
                            && !(written.page
                                && read.page_descriptor
                                && same_element_root(place, &descriptor))
                    })
                    .count();
                let matching = self
                    .element_reads
                    .iter()
                    .filter(|read| {
                        same_element_root(&read.root, &written.root)
                            && read.map == written.map
                            && read.page == written.page
                    })
                    .count();
                reads != matching
            })
            .map(|written| LoopDenial::SharedWrite {
                argument: written.statement.clone(),
            })
    }

    /// [RANGE-5] every read reaching a certified root is a measure or one of
    /// the element reads the certificate separated, and no other family
    /// writes there: the certificate compared exactly those accesses. The
    /// certificate's walk records every access it reaches, but it skips an
    /// arm its entry state excludes, such as the body of `if off` where
    /// `off` is a constant false, while this survey still sees that arm's
    /// accesses; such a loop stays sequential.
    fn certified_coverage(&self) -> Option<LoopDenial> {
        let certificate = self.certificate?;
        let oracle = UnprovedSeparations;
        for written in &self.certified_writes {
            let uncovered = self.reads.iter().any(|read| {
                !read.measure
                    && read
                        .places
                        .iter()
                        .any(|place| self.places.overlaps(&oracle, place, &written.root))
                    && !read.carrier.as_ref().is_some_and(|carrier| {
                        certificate.reads.contains(carrier) || certificate.writes.contains(carrier)
                    })
            });
            let mixed = self
                .element_writes
                .iter()
                .any(|other| self.places.overlaps(&oracle, &other.root, &written.root))
                || self.range_references.iter().any(|range| {
                    range.written && self.places.overlaps(&oracle, &range.origin, &written.root)
                });
            if uncovered || mixed {
                return Some(LoopDenial::SharedWrite {
                    argument: written.statement.clone(),
                });
            }
        }
        None
    }

    /// Condition 1: the body carries at most one value across iterations, and
    /// that value is a reduction.
    ///
    /// The read count is per body rather than per accumulate, so a loop that
    /// combines one accumulator under two branches is refused although its
    /// combines are admitted. That is a conservatism, not an unsoundness, and
    /// it is what makes a single accumulate statement, and therefore a single
    /// fixed combine per accumulator, an invariant here.
    fn carried_state(&self) -> Option<LoopDenial> {
        if let Some(statement) = &self.carried {
            return Some(LoopDenial::NotAReduction {
                statement: statement.clone(),
            });
        }
        let mut bindings: Vec<BindingId> = Vec::new();
        for accumulate in &self.accumulates {
            if !bindings.contains(&accumulate.binding) {
                bindings.push(accumulate.binding);
            }
        }
        let [accumulator] = bindings.as_slice() else {
            return (bindings.len() > 1).then_some(LoopDenial::ManyAccumulators {
                accumulators: bindings.len(),
            });
        };
        let first = self
            .accumulates
            .iter()
            .find(|accumulate| accumulate.binding == *accumulator)
            .expect("the accumulator came from one of these");
        let reads = self
            .reads
            .iter()
            .filter(|read| read.binding == *accumulator)
            .count();
        (reads != 1).then(|| LoopDenial::AccumulatorRead {
            statement: first.statement.clone(),
            reads,
        })
    }
}

/// The node one body statement cites, where it carries one of its own.
const fn statement_node(statement: &CheckedStatement) -> Option<&NodePath> {
    match statement {
        CheckedStatement::Let { node_path, .. }
        | CheckedStatement::DestructuringLet { node_path, .. }
        | CheckedStatement::PropagateLet { node_path, .. }
        | CheckedStatement::Set { node_path, .. }
        | CheckedStatement::Return { node_path, .. }
        | CheckedStatement::ValueMatchLet { node_path, .. }
        | CheckedStatement::Give { node_path, .. }
        | CheckedStatement::CountedRange { node_path, .. }
        | CheckedStatement::Evaluate { node_path, .. }
        | CheckedStatement::DropExpression { node_path, .. }
        | CheckedStatement::Atomic { node_path, .. } => Some(node_path),
        CheckedStatement::Proof(proof) => Some(&proof.node_path),
        CheckedStatement::Match { .. }
        | CheckedStatement::Loop { .. }
        | CheckedStatement::Break { .. }
        | CheckedStatement::Continue { .. } => None,
    }
}

/// The mapped root and the mapped element of a resolved place whose mapped
/// index step has `after` index steps below it: the prefix above that step,
/// and the prefix through it. Steps below the element are that element's own
/// storage [TYPE-8, TYPE-9].
fn element_prefix(place: &ResolvedPlace, after: usize) -> Option<(ResolvedPlace, ResolvedPlace)> {
    let mut remaining = after;
    let position = place
        .path
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, step)| matches!(step, PlaceStep::Index(_) | PlaceStep::Page(_)))
        .find_map(|(position, _)| {
            if remaining == 0 {
                Some(position)
            } else {
                remaining -= 1;
                None
            }
        })?;
    Some((
        ResolvedPlace {
            atomic_aliases: place.atomic_aliases.clone(),
            root: place.root,
            path: place.path[..position].to_vec(),
        },
        ResolvedPlace {
            atomic_aliases: place.atomic_aliases.clone(),
            root: place.root,
            path: place.path[..=position].to_vec(),
        },
    ))
}

/// The subscripts of one typed storage path, outermost first, each with
/// whether its base is a `Ring` [WIN-1].
fn path_subscripts(path: &[CheckedPlaceStep]) -> Vec<(&NodePath, bool)> {
    path.iter()
        .filter_map(|step| match step {
            CheckedPlaceStep::Subscript(index) => {
                Some((&index.obligation, checked_type_is_ring(index.base_type)))
            }
            CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
        })
        .collect()
}

/// The subscripts of one element place through a range, outermost first:
/// the range's own offset, then the nested subscripts below the element.
fn range_root_subscripts(root: &super::model::CheckedRangeRoot) -> Vec<(&NodePath, bool)> {
    match root.formation.as_deref() {
        Some(CheckedExpression::BorrowSegment { root, segment, .. }) => {
            let mut result = match root {
                super::CheckedSegmentSource::Storage(root) => path_subscripts(&root.path),
                super::CheckedSegmentSource::Element(place) => range_element_subscripts(place),
            };
            if let super::CheckedSegmentSelect::One(index)
            | super::CheckedSegmentSelect::Page(index) = segment
            {
                result.push((&index.obligation, false));
            }
            result
        }
        _ => Vec::new(),
    }
}

fn range_element_subscripts(place: &CheckedRangeElementPlace) -> Vec<(&NodePath, bool)> {
    range_root_subscripts(&place.root)
        .into_iter()
        .chain(std::iter::once((&place.obligation, false)))
        .chain(path_subscripts(&place.path))
        .collect()
}

/// Whether two mapped roots positively name the same storage.
///
/// Raw captured-value equality includes the source occurrence even for
/// literals and named constants. [OWN-7]'s positive identity deliberately
/// ignores that occurrence for value-determined indices while retaining it
/// for binding and opaque captures, which is the distinction `contains`
/// already implements. Equal depth keeps this from treating a prefix as the
/// same mapped collection.
fn same_element_root(left: &ResolvedPlace, right: &ResolvedPlace) -> bool {
    left.path.len() == right.path.len() && left.contains(right)
}

/// Whether a checked type is a `Ring` [TYPE-9].
///
/// A `Ring` subscript selects the slot `(r.head + i) mod r.cap` [WIN-1], a
/// wrapping map onto storage rather than a linear offset, so [PAR-2] denies
/// it an element-map position. The checked window shape retains that
/// distinction for both constant and runtime capacities.
const fn checked_type_is_ring(ty: CheckedType) -> bool {
    matches!(
        ty,
        CheckedType::Window {
            shape: WindowShape::Ring,
            ..
        }
    )
}

/// Typed atom identity within a single set statement. Unlike a stored
/// reference capture, both evaluations occur before the same commit and
/// GRAM-9 admits no intervening call. This is not an index injectivity proof.
fn same_update_atom(left: &CheckedExpression, right: &CheckedExpression) -> bool {
    match (left, right) {
        (
            CheckedExpression::Binding { binding: a, .. },
            CheckedExpression::Binding { binding: b, .. },
        ) => a == b,
        (CheckedExpression::Constant(a), CheckedExpression::Constant(b)) => a == b,
        (
            CheckedExpression::DerefAddressed { binding: a, .. },
            CheckedExpression::DerefAddressed { binding: b, .. },
        ) => a == b,
        (
            CheckedExpression::ContainerMeasure {
                measure: am,
                root: a,
            },
            CheckedExpression::ContainerMeasure {
                measure: bm,
                root: b,
            },
        ) => am == bm && a.root == b.root && same_update_path(&a.path, &b.path),
        (
            CheckedExpression::ArrayMeasure {
                measure: am,
                root: a,
                ..
            },
            CheckedExpression::ArrayMeasure {
                measure: bm,
                root: b,
                ..
            },
        ) => am == bm && a == b,
        (
            CheckedExpression::RangeMeasure {
                measure: am,
                root: a,
            },
            CheckedExpression::RangeMeasure {
                measure: bm,
                root: b,
            },
        ) => am == bm && a == b,
        (
            CheckedExpression::BufferMeasure {
                measure: am,
                root: a,
            },
            CheckedExpression::BufferMeasure {
                measure: bm,
                root: b,
            },
        ) => am == bm && a.binding == b.binding && same_update_path(&a.path, &b.path),
        (
            CheckedExpression::RangeElementMeasure {
                measure: am,
                place: a,
                ..
            },
            CheckedExpression::RangeElementMeasure {
                measure: bm,
                place: b,
                ..
            },
        ) => {
            am == bm
                && a.root == b.root
                && same_update_atom(&a.offset, &b.offset)
                && same_update_path(&a.path, &b.path)
        }
        (
            CheckedExpression::NamedConstant { declaration: a, .. },
            CheckedExpression::NamedConstant { declaration: b, .. },
        ) => a == b,
        (
            CheckedExpression::Project {
                binding: a,
                fields: ap,
                ..
            },
            CheckedExpression::Project {
                binding: b,
                fields: bp,
                ..
            },
        ) => a == b && ap == bp,
        (
            CheckedExpression::ReadStorage { root: a, .. },
            CheckedExpression::ReadStorage { root: b, .. },
        ) => a.root == b.root && same_update_path(&a.path, &b.path),
        (
            CheckedExpression::ArrayIndex {
                root: a,
                offset: ai,
                ..
            },
            CheckedExpression::ArrayIndex {
                root: b,
                offset: bi,
                ..
            },
        ) => a == b && same_update_atom(ai, bi),
        (
            CheckedExpression::BufferIndex {
                root: a,
                offset: ai,
                ..
            },
            CheckedExpression::BufferIndex {
                root: b,
                offset: bi,
                ..
            },
        ) => {
            a.binding == b.binding && same_update_path(&a.path, &b.path) && same_update_atom(ai, bi)
        }
        (
            CheckedExpression::RangeIndex { place: a, .. },
            CheckedExpression::RangeIndex { place: b, .. },
        ) => {
            a.root == b.root
                && same_update_atom(&a.offset, &b.offset)
                && same_update_path(&a.path, &b.path)
        }
        _ => false,
    }
}

fn same_update_path(left: &[CheckedPlaceStep], right: &[CheckedPlaceStep]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| match (a, b) {
            (CheckedPlaceStep::Subscript(a), CheckedPlaceStep::Subscript(b)) => {
                a.base_type == b.base_type && same_update_atom(&a.offset, &b.offset)
            }
            _ => a == b,
        })
}

/// The value evaluated by a statement; nested blocks are surveyed separately.
fn statement_value(statement: &CheckedStatement) -> Option<&CheckedExpression> {
    match statement {
        CheckedStatement::Let { value, .. }
        | CheckedStatement::DestructuringLet { value, .. }
        | CheckedStatement::Set { value, .. }
        | CheckedStatement::Evaluate { value, .. }
        | CheckedStatement::DropExpression { value, .. }
        | CheckedStatement::Return { value, .. }
        | CheckedStatement::Give { value, .. } => Some(value),
        CheckedStatement::PropagateLet { scrutinee, .. }
        | CheckedStatement::Match { scrutinee, .. }
        | CheckedStatement::ValueMatchLet { scrutinee, .. } => Some(scrutinee),
        CheckedStatement::Proof(_)
        | CheckedStatement::Continue { .. }
        | CheckedStatement::Break { .. }
        | CheckedStatement::CountedRange { .. }
        | CheckedStatement::Loop { .. }
        | CheckedStatement::Atomic { .. } => None,
    }
}

fn reference_root(value: &CheckedExpression, places: &PlaceMap) -> Option<CheckedContainerRoot> {
    match value {
        CheckedExpression::BorrowAddressed { root, .. } => Some(root.clone()),
        CheckedExpression::Binding { binding, ty, .. } if places.is_reference(*binding) => {
            Some(CheckedContainerRoot {
                root: PlaceRoot::Binding(*binding),
                path: Vec::new(),
                ty: *ty,
                proof_base: None,
            })
        }
        _ => None,
    }
}

fn operation_arguments(value: &CheckedExpression) -> Option<&[CheckedExpression]> {
    match value {
        CheckedExpression::IntegerOperation { arguments, .. }
        | CheckedExpression::BooleanOperation { arguments, .. } => Some(arguments),
        _ => None,
    }
}

/// The combine of `set acc = <op>(acc, rest)`, when `op` is one of those
/// [PAR-2] admits and `rest` reaches `acc` nowhere.
fn combine_of(accumulator: BindingId, value: &CheckedExpression) -> Option<LoopCombine> {
    let (combine, arguments) = match value {
        CheckedExpression::IntegerOperation {
            operation,
            operand_type,
            arguments,
            ..
        } => (integer_combine(*operation, *operand_type)?, arguments),
        CheckedExpression::BooleanOperation {
            operation,
            arguments,
            ..
        } => (boolean_combine(*operation)?, arguments),
        _ => return None,
    };
    let [left, right] = arguments.as_slice() else {
        return None;
    };
    // Exactly one operand is the accumulator read, and the other reaches it
    // nowhere: `acc = acc + f(acc)` is not a reduction. Both operand
    // positions are accepted, which is sound only because every admitted
    // operation is commutative as well as associative; admitting a
    // non-commutative associative operation would silently turn a right fold
    // into a left fold and must fix the position instead.
    let carried = [(left, right), (right, left)]
        .into_iter()
        .find(|(operand, _)| reads_only(operand, accumulator))?;
    let mut mentioned = false;
    visit_read_bindings(carried.1, &mut |binding| {
        mentioned |= binding == accumulator;
    });
    if mentioned {
        return None;
    }
    Some(combine)
}

/// Whether one operand is exactly a read of this binding.
fn reads_only(operand: &CheckedExpression, binding: BindingId) -> bool {
    matches!(operand, CheckedExpression::Binding { binding: read, .. } if *read == binding)
}

/// The integer operations [PAR-2] admits at this operand type.
///
/// The list is closed and every entry is here for the same stated reason:
/// each is total, associative, and commutative on the complete value set of
/// its type and carries a two-sided identity, so regrouping its applications
/// produces the same bits. `+`, `+defined`, and `+checked` are associative in
/// Z and are still absent, because each application attaches a domain
/// obligation or a `Result` route that regrouping moves. Unsigned `+sat`
/// computes min(sum, max); signed `+sat` can move its clamp under regrouping.
const fn integer_combine(
    operation: CheckedIntegerOperation,
    ty: CheckedType,
) -> Option<LoopCombine> {
    Some(match operation {
        CheckedIntegerOperation::AddWrap => LoopCombine::AddWrap,
        CheckedIntegerOperation::AddSaturating if matches!(ty, CheckedType::Integer(integer) if !integer.signed()) => {
            LoopCombine::AddSaturating
        }
        CheckedIntegerOperation::MultiplyWrap => LoopCombine::MultiplyWrap,
        CheckedIntegerOperation::BitAnd => LoopCombine::BitAnd,
        CheckedIntegerOperation::BitOr => LoopCombine::BitOr,
        CheckedIntegerOperation::BitXor => LoopCombine::BitXor,
        CheckedIntegerOperation::Minimum => LoopCombine::Minimum,
        CheckedIntegerOperation::Maximum => LoopCombine::Maximum,
        _ => return None,
    })
}

/// The three boolean operations [PAR-2] admits. `not` is unary and no
/// combine.
const fn boolean_combine(operation: CheckedBooleanOperation) -> Option<LoopCombine> {
    Some(match operation {
        CheckedBooleanOperation::And => LoopCombine::And,
        CheckedBooleanOperation::Or => LoopCombine::Or,
        CheckedBooleanOperation::ExclusiveOr => LoopCombine::ExclusiveOr,
        CheckedBooleanOperation::Not => return None,
    })
}

/// Every binding one block and its nested blocks introduce, appended in
/// source order.
fn collect_introduced(statements: &[CheckedStatement], out: &mut Vec<BindingId>) {
    for statement in statements {
        match statement {
            CheckedStatement::Let { binding, .. }
            | CheckedStatement::PropagateLet { binding, .. }
            | CheckedStatement::ValueMatchLet { binding, .. } => out.push(*binding),
            CheckedStatement::DestructuringLet { bindings, .. } => {
                out.extend(bindings.iter().map(|(binding, _, _)| *binding));
            }
            CheckedStatement::CountedRange { binder, .. } => out.push(*binder),
            CheckedStatement::Atomic { targets, .. } => {
                out.extend(targets.iter().map(|t| t.binding))
            }
            _ => {}
        }
        if let CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } =
            statement
        {
            for arm in arms {
                for arm_binder in &arm.binders {
                    out.push(arm_binder.binding);
                }
            }
        }
        for nested in nested_bodies(statement) {
            collect_introduced(nested, out);
        }
    }
}

/// Every loop one block opens inside itself, by loop identity.
fn collect_inner_loops(statements: &[CheckedStatement], out: &mut Vec<u32>) {
    for statement in statements {
        if let CheckedStatement::CountedRange { id, .. } | CheckedStatement::Loop { id, .. } =
            statement
        {
            out.push(id.0);
        }
        for nested in nested_bodies(statement) {
            collect_inner_loops(nested, out);
        }
    }
}

/// Every block a statement owns.
fn nested_bodies(statement: &CheckedStatement) -> Vec<&[CheckedStatement]> {
    match statement {
        CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
            arms.iter().map(|arm| arm.body.as_slice()).collect()
        }
        CheckedStatement::Loop { body, .. }
        | CheckedStatement::CountedRange { body, .. }
        | CheckedStatement::Atomic { body, .. } => vec![body.as_slice()],
        _ => Vec::new(),
    }
}

/// Exactly one subscript, followed only by record fields. Keeping the root
/// separate from its projection makes sibling families share one read policy.
fn indexed_parts(
    target: &CheckedContainerRoot,
) -> Option<(usize, &super::model::CheckedPlaceSubscript, Vec<u32>)> {
    let position = target
        .path
        .iter()
        .position(|step| matches!(step, CheckedPlaceStep::Subscript(_)))?;
    let CheckedPlaceStep::Subscript(index) = &target.path[position] else {
        return None;
    };
    let fields = target.path[position + 1..]
        .iter()
        .map(|step| match step {
            CheckedPlaceStep::Field(field) => Some(*field),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some((position, index, fields))
}
